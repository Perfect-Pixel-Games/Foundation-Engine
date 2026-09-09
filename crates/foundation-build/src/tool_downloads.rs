//! Downloads, verifies, and installs known external developer tools (e.g. the
//! Tracy profiler) from their GitHub release binaries into a shared per-user
//! cache, so Foundation games don't need to compile these tools themselves or
//! depend on a platform-specific package manager being installed.
//!
//! Only Windows and Linux are supported -- no other part of this codebase
//! builds for macOS yet, and Tracy's macOS release ships as a `.app` bundle
//! tree rather than a single flat executable, which would need real extra
//! extraction logic for a platform nothing else here targets.

use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

/// A developer tool this command knows how to fetch. Adding a second tool is
/// one more variant plus its [`KnownTool::platform_asset`] match arm -- the
/// download/verify/extract/install machinery below is fully shared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KnownTool {
    Tracy,
}

impl KnownTool {
    const ALL: [Self; 1] = [Self::Tracy];

    /// Parses a tool name from the command line, case-insensitively.
    fn parse(tool_name: &str) -> Result<Self, String> {
        match tool_name.to_ascii_lowercase().as_str() {
            "tracy" => Ok(Self::Tracy),
            unknown_tool_name => Err(format!(
                "Unknown tool `{unknown_tool_name}`. Known tools: {}.",
                Self::ALL
                    .iter()
                    .map(|known_tool| known_tool.name())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Tracy => "tracy",
        }
    }

    /// The pinned version installed for every platform. Bumping this also
    /// requires re-verifying and updating every digest in
    /// [`KnownTool::platform_asset`] -- see this feature's plan for how the
    /// current digests were computed.
    fn version(self) -> &'static str {
        match self {
            Self::Tracy => "0.14.1",
        }
    }

    /// This tool's release asset for the current host OS, or an error naming
    /// the unsupported platform.
    fn platform_asset(self) -> Result<PlatformAsset, String> {
        match (self, std::env::consts::OS) {
            (Self::Tracy, "windows") => Ok(PlatformAsset {
                download_url:
                    "https://github.com/wolfpld/tracy/releases/download/v0.14.1/windows-0.14.1.zip",
                expected_sha256:
                    "f7499d74914aa3ba94a2c1ce72f36477d7b61d9d0f7c9790e05274c258c97fb5",
                zip_entry_name: "tracy-profiler.exe",
                installed_relative_path: "tracy-profiler.exe",
            }),
            (Self::Tracy, "linux") => Ok(PlatformAsset {
                download_url:
                    "https://github.com/wolfpld/tracy/releases/download/v0.14.1/linux-0.14.1.zip",
                expected_sha256:
                    "4f57574337b206cac86758081ab21c37cf9c83fc12170d2e339e1f4ee94ff590",
                zip_entry_name: "tracy-profiler-x86_64.AppImage",
                installed_relative_path: "tracy-profiler-x86_64.AppImage",
            }),
            (_, unsupported_os) => Err(format!(
                "No {} download is available for `{unsupported_os}`. Only windows and linux are supported.",
                self.name()
            )),
        }
    }
}

/// One tool's release asset metadata for a specific host OS.
#[derive(Clone, Copy, Debug)]
struct PlatformAsset {
    download_url: &'static str,
    expected_sha256: &'static str,
    zip_entry_name: &'static str,
    installed_relative_path: &'static str,
}

/// Resolves the shared per-user cache root every known tool installs under:
/// `%LOCALAPPDATA%\Foundation\tools` on Windows, `$XDG_CACHE_HOME/foundation/tools`
/// (falling back to `~/.cache/foundation/tools`) on Linux. Shared across every
/// Foundation game checkout on the machine, since `foundation-build` itself is
/// shared the same way.
fn tools_cache_root() -> Result<PathBuf, String> {
    if cfg!(windows) {
        let local_app_data_directory = std::env::var("LOCALAPPDATA").map_err(|_| {
            "LOCALAPPDATA is not set; cannot resolve the shared tools cache directory.".to_string()
        })?;
        Ok(PathBuf::from(local_app_data_directory)
            .join("Foundation")
            .join("tools"))
    } else {
        let cache_home_directory = std::env::var("XDG_CACHE_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|home_directory| PathBuf::from(home_directory).join(".cache"))
            })
            .ok_or_else(|| {
                "Neither XDG_CACHE_HOME nor HOME is set; cannot resolve the shared tools cache directory."
                    .to_string()
            })?;
        Ok(cache_home_directory.join("foundation").join("tools"))
    }
}

/// The path `tool` is installed to (whether or not it exists yet). The
/// pinned version is baked into the path, so installing a new pinned version
/// later never collides with or silently replaces an older one.
fn installed_tool_path(tool: KnownTool, platform_asset: &PlatformAsset) -> Result<PathBuf, String> {
    Ok(tools_cache_root()?
        .join(tool.name())
        .join(tool.version())
        .join(platform_asset.installed_relative_path))
}

/// Downloads, verifies, and installs `tool` if it isn't already installed,
/// returning the path to its executable either way.
fn install(tool: KnownTool) -> Result<PathBuf, String> {
    let platform_asset = tool.platform_asset()?;
    let installed_path = installed_tool_path(tool, &platform_asset)?;

    // Idempotent: the pinned version is baked into the install path, so an
    // existing file at that exact path is already the right version -- no
    // network round-trip or re-verification needed.
    if installed_path.is_file() {
        return Ok(installed_path);
    }

    let installed_directory = installed_path.parent().ok_or_else(|| {
        format!(
            "Installed tool path `{}` has no parent directory.",
            installed_path.display()
        )
    })?;
    fs::create_dir_all(installed_directory).map_err(|create_directory_error| {
        format!(
            "Failed to create `{}`: {create_directory_error}",
            installed_directory.display()
        )
    })?;

    let downloaded_zip_path = installed_directory.join(format!("{}.download.zip", tool.name()));
    download_to_file(platform_asset.download_url, &downloaded_zip_path)?;

    let downloaded_zip_sha256 = sha256_of_file(&downloaded_zip_path)?;
    if downloaded_zip_sha256 != platform_asset.expected_sha256 {
        let _ = fs::remove_file(&downloaded_zip_path);
        return Err(format!(
            "Downloaded {} archive's SHA-256 (`{downloaded_zip_sha256}`) does not match the \
             expected digest (`{}`). Refusing to install an unverified binary.",
            tool.name(),
            platform_asset.expected_sha256
        ));
    }

    let extracting_path = installed_directory.join(format!("{}.extracting", tool.name()));
    extract_single_entry(
        &downloaded_zip_path,
        platform_asset.zip_entry_name,
        &extracting_path,
    )?;
    let _ = fs::remove_file(&downloaded_zip_path);

    // Renaming within the same directory is atomic on both Windows and
    // Linux, so a crash between extraction and this point never leaves a
    // partially written file sitting at the path the idempotency check
    // above looks for.
    fs::rename(&extracting_path, &installed_path).map_err(|rename_error| {
        format!(
            "Failed to install `{}`: {rename_error}",
            installed_path.display()
        )
    })?;

    Ok(installed_path)
}

/// A blocking HTTP agent configured to verify server certificates against
/// the OS's own trust store (`RootCerts::PlatformVerifier`) rather than
/// `ureq`'s bundled Mozilla root list -- machines behind a corporate TLS-
/// inspecting proxy (whose root CA is only trusted by the OS, not by
/// `ureq`'s default roots) would otherwise fail every download with an
/// "unknown issuer" certificate error.
fn download_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .new_agent()
}

fn download_to_file(download_url: &str, destination_path: &Path) -> Result<(), String> {
    let response = download_agent()
        .get(download_url)
        .call()
        .map_err(|request_error| format!("Failed to download `{download_url}`: {request_error}"))?;
    let mut response_body_reader = response.into_body().into_reader();
    let mut destination_file = fs::File::create(destination_path).map_err(|create_file_error| {
        format!(
            "Failed to create `{}`: {create_file_error}",
            destination_path.display()
        )
    })?;
    io::copy(&mut response_body_reader, &mut destination_file).map_err(|copy_error| {
        format!(
            "Failed to write `{}`: {copy_error}",
            destination_path.display()
        )
    })?;
    Ok(())
}

fn sha256_of_file(file_path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(file_path)
        .map_err(|open_error| format!("Failed to open `{}`: {open_error}", file_path.display()))?;
    let mut hasher = Sha256::new();
    // `Sha256` doesn't implement `io::Write`, so feed it in chunks read
    // manually rather than via `io::copy`.
    let mut read_buffer = [0_u8; 64 * 1024];
    loop {
        let bytes_read = file.read(&mut read_buffer).map_err(|read_error| {
            format!("Failed to hash `{}`: {read_error}", file_path.display())
        })?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&read_buffer[..bytes_read]);
    }
    let digest_bytes = hasher.finalize();
    Ok(digest_bytes
        .iter()
        .map(|digest_byte| format!("{digest_byte:02x}"))
        .collect())
}

/// Extracts exactly one named entry from a zip archive to `destination_path`,
/// preserving the entry's Unix executable bit on Linux.
fn extract_single_entry(
    zip_path: &Path,
    zip_entry_name: &str,
    destination_path: &Path,
) -> Result<(), String> {
    let zip_file = fs::File::open(zip_path)
        .map_err(|open_error| format!("Failed to open `{}`: {open_error}", zip_path.display()))?;
    let mut archive = zip::ZipArchive::new(zip_file).map_err(|zip_error| {
        format!(
            "Failed to read `{}` as a zip archive: {zip_error}",
            zip_path.display()
        )
    })?;
    let mut zip_entry = archive.by_name(zip_entry_name).map_err(|zip_error| {
        format!(
            "`{}` does not contain `{zip_entry_name}`: {zip_error}",
            zip_path.display()
        )
    })?;
    let zip_entry_unix_mode = zip_entry.unix_mode();

    let mut destination_file = fs::File::create(destination_path).map_err(|create_file_error| {
        format!(
            "Failed to create `{}`: {create_file_error}",
            destination_path.display()
        )
    })?;
    io::copy(&mut zip_entry, &mut destination_file)
        .map_err(|copy_error| format!("Failed to extract `{zip_entry_name}`: {copy_error}"))?;

    // Zip doesn't restore Unix executable bits on its own. Tracy's Linux
    // release marks its AppImage executable in the archive's stored mode, so
    // apply that mode explicitly (falling back to a normal executable mode
    // if the archive didn't record one) -- otherwise the extracted file
    // can't be run.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let executable_mode = zip_entry_unix_mode.unwrap_or(0o755);
        fs::set_permissions(
            destination_path,
            fs::Permissions::from_mode(executable_mode),
        )
        .map_err(|permissions_error| {
            format!(
                "Failed to set permissions on `{}`: {permissions_error}",
                destination_path.display()
            )
        })?;
    }
    #[cfg(not(unix))]
    {
        let _ = zip_entry_unix_mode;
    }

    Ok(())
}

/// Reports each known tool's name, pinned version, and whether it's already
/// installed on this machine, without downloading anything. A tool that has
/// no release for the current platform is reported as not installed rather
/// than as an error, since this command is meant to be a quick status check.
fn list_known_tools() -> Vec<(&'static str, &'static str, bool)> {
    KnownTool::ALL
        .iter()
        .map(|&known_tool| {
            let is_installed = known_tool
                .platform_asset()
                .and_then(|platform_asset| installed_tool_path(known_tool, &platform_asset))
                .map(|installed_path| installed_path.is_file())
                .unwrap_or(false);
            (known_tool.name(), known_tool.version(), is_installed)
        })
        .collect()
}

/// Entry point for the `tools` command family (`tools install <name>`,
/// `tools list`), called from [`crate::run`] before any game-project
/// resolution happens -- installing a tool has nothing to do with any
/// specific game.
pub(crate) fn run_tools_command(mut arguments: impl Iterator<Item = String>) -> Result<(), String> {
    let Some(tools_subcommand) = arguments.next() else {
        return Err(
            "Expected `install <tool-name>` or `list` after `tools`. Use `--help` for usage."
                .to_string(),
        );
    };

    match tools_subcommand.as_str() {
        "install" => {
            let tool_name = arguments.next().ok_or_else(|| {
                "Expected a tool name after `tools install`. Use `--help` for usage.".to_string()
            })?;
            let tool = KnownTool::parse(&tool_name)?;
            let installed_path = install(tool)?;
            println!("{}", installed_path.display());
            Ok(())
        }
        "list" => {
            for (tool_name, tool_version, is_installed) in list_known_tools() {
                let installed_marker = if is_installed {
                    "installed"
                } else {
                    "not installed"
                };
                println!("{tool_name} {tool_version} ({installed_marker})");
            }
            Ok(())
        }
        unknown_tools_subcommand => Err(format!(
            "Unknown `tools` subcommand `{unknown_tools_subcommand}`. Expected `install` or `list`."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_known_tool_names_case_insensitively() {
        assert_eq!(KnownTool::parse("tracy"), Ok(KnownTool::Tracy));
        assert_eq!(KnownTool::parse("TRACY"), Ok(KnownTool::Tracy));
        assert_eq!(KnownTool::parse("Tracy"), Ok(KnownTool::Tracy));
    }

    #[test]
    fn parse_rejects_unknown_tool_names_and_lists_known_ones() {
        let parse_error = KnownTool::parse("renderdoc").unwrap_err();
        assert!(parse_error.contains("renderdoc"));
        assert!(parse_error.contains("tracy"));
    }

    #[test]
    fn installed_tool_path_includes_the_pinned_version_and_relative_path() {
        let platform_asset = PlatformAsset {
            download_url: "https://example.invalid/tool.zip",
            expected_sha256: "0000000000000000000000000000000000000000000000000000000000000000",
            zip_entry_name: "tool.exe",
            installed_relative_path: "tool.exe",
        };

        let installed_path = installed_tool_path(KnownTool::Tracy, &platform_asset).unwrap();

        assert!(installed_path.ends_with("tracy/0.14.1/tool.exe"));
    }

    #[test]
    fn list_known_tools_reports_every_known_tool() {
        let known_tools = list_known_tools();
        assert_eq!(known_tools.len(), KnownTool::ALL.len());
        assert!(known_tools.iter().any(|(name, _, _)| *name == "tracy"));
    }

    #[test]
    fn run_tools_command_rejects_an_unknown_subcommand() {
        let result = run_tools_command(vec!["frobnicate".to_string()].into_iter());
        let error_message = result.unwrap_err();
        assert!(error_message.contains("frobnicate"));
    }

    #[test]
    fn run_tools_command_rejects_install_without_a_tool_name() {
        let result = run_tools_command(vec!["install".to_string()].into_iter());
        assert!(result.unwrap_err().contains("tool name"));
    }

    #[test]
    fn run_tools_command_rejects_install_with_an_unknown_tool_name() {
        let result =
            run_tools_command(vec!["install".to_string(), "renderdoc".to_string()].into_iter());
        assert!(result.unwrap_err().contains("renderdoc"));
    }
}
