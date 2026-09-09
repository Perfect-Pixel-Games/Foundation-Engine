//! Foundation build and packaging orchestration.
//!
//! This crate owns the stable local command interface used by developers and CI
//! agents. It deliberately reads game manifests from the workspace instead of
//! depending on concrete game crates, so every Foundation game can share the same
//! build flow.

use std::{
    ffi::OsStr,
    fmt, fs, io,
    path::{Path, PathBuf},
    process::Command,
};

use serde::Deserialize;

mod tool_downloads;

const GAME_MANIFEST_FILE_NAME: &str = "foundation.game.toml";
const GAMES_DIRECTORY_NAME: &str = "games";
const DEFAULT_OUTPUT_DIRECTORY: &str = "artifacts/packages";

/// Runs the Foundation build command using already-split command-line arguments.
pub fn run(arguments: impl IntoIterator<Item = String>) -> Result<(), String> {
    let mut argument_iterator = arguments.into_iter().peekable();

    // `tools` has nothing to do with any specific game, so it's dispatched
    // before the game-project resolution every other command needs.
    if argument_iterator.peek().map(String::as_str) == Some("tools") {
        argument_iterator.next();
        return tool_downloads::run_tools_command(argument_iterator);
    }

    let invocation = BuildInvocation::parse(argument_iterator)?;
    if invocation.help_requested {
        print_usage();
        return Ok(());
    }

    let invocation_directory = std::env::current_dir()
        .map_err(|error| format!("Failed to read current directory: {error}"))?;
    let engine_root = find_engine_root()?;
    let game_project = find_game_project(&engine_root, &invocation)?;
    let build_request = BuildRequest::new(invocation, invocation_directory, game_project)?;

    if build_request.command.builds_executable() {
        build_game(&engine_root, &build_request)?;
    }

    match build_request.command {
        BuildCommand::Build => {}
        BuildCommand::Package => package_game(&engine_root, &build_request)?,
        BuildCommand::Run => run_game(&build_request)?,
    }

    Ok(())
}

fn print_usage() {
    println!("Foundation build tool");
    println!("Usage:");
    println!("  cargo run -p foundation-build -- package (--game <name>|--project <path>) [--platform <alias>] [--configuration <debug|test|shipping>] [--target <game|game-editor>] [--output <directory>]");
    println!("  cargo run -p foundation-build -- build   (--game <name>|--project <path>) [--platform <alias>] [--configuration <debug|test|shipping>] [--target <game|game-editor>]");
    println!("  cargo run -p foundation-build -- run     (--game <name>|--project <path>) [--platform <alias>] [--configuration <debug|test|shipping>] [--target <game|game-editor>] [-- <game arguments>]");
    println!("  cargo run -p foundation-build -- tools install <tool-name>");
    println!("  cargo run -p foundation-build -- tools list");
    println!("  cargo run -p foundation-build -- tools run <tool-name>");
    println!("Examples:");
    println!("  cargo run -p foundation-build -- run --game template-game");
    println!("  cargo run -p foundation-build -- run --project ../template-game/game");
    println!("  cargo run -p foundation-build -- run --project ../template-game/game --platform windows-x64 --configuration debug --target game-editor");
    println!("  cargo run -p foundation-build -- package --project ../template-game/game --platform windows-x64 --configuration test --target game");
    println!("  cargo run -p foundation-build -- package --project ../template-game/game --platform linux-x64 --configuration shipping --target game");
    println!("  cargo run -p foundation-build -- tools install tracy");
    println!("  cargo run -p foundation-build -- tools list");
    println!("  cargo run -p foundation-build -- tools run tracy");
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BuildCommand {
    Build,
    Package,
    Run,
}

impl BuildCommand {
    fn parse(command_text: &str) -> Result<Self, String> {
        match command_text {
            "build" => Ok(Self::Build),
            "package" => Ok(Self::Package),
            "run" => Ok(Self::Run),
            unknown_command => Err(format!(
                "Unknown Foundation build command `{unknown_command}`. Expected `build`, `package`, or `run`."
            )),
        }
    }

    fn builds_executable(self) -> bool {
        match self {
            Self::Build | Self::Package | Self::Run => true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BuildConfiguration {
    Debug,
    Test,
    Shipping,
}

impl BuildConfiguration {
    fn parse(configuration_text: &str) -> Result<Self, String> {
        match configuration_text.to_ascii_lowercase().as_str() {
            "debug" => Ok(Self::Debug),
            "test" => Ok(Self::Test),
            "shipping" => Ok(Self::Shipping),
            unknown_configuration => Err(format!(
                "Unknown build configuration `{unknown_configuration}`. Expected `debug`, `test`, or `shipping`."
            )),
        }
    }

    fn as_output_segment(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Test => "test",
            Self::Shipping => "shipping",
        }
    }

    fn cargo_profile(self) -> Option<&'static str> {
        match self {
            Self::Debug => None,
            Self::Test => Some("foundation-test"),
            Self::Shipping => Some("foundation-shipping"),
        }
    }

    fn enables_dev_tools(self) -> bool {
        match self {
            Self::Debug | Self::Test => true,
            Self::Shipping => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TargetKind {
    Game,
    GameEditor,
}

impl TargetKind {
    fn parse(target_kind_text: &str) -> Result<Self, String> {
        let normalized_target_kind = target_kind_text.to_ascii_lowercase().replace('_', "-");
        match normalized_target_kind.as_str() {
            "game" => Ok(Self::Game),
            "game-editor" | "gameeditor" | "editor" => Ok(Self::GameEditor),
            _ => Err(format!(
                "Unknown target `{target_kind_text}`. Expected `game` or `game-editor`."
            )),
        }
    }

    fn as_output_segment(self) -> &'static str {
        match self {
            Self::Game => "game",
            Self::GameEditor => "game-editor",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TargetPlatform {
    alias: String,
    rust_target_triple: String,
    executable_suffix: String,
}

impl TargetPlatform {
    fn parse(platform_text: &str) -> Result<Self, String> {
        let normalized_platform = platform_text.to_ascii_lowercase();
        let (rust_target_triple, executable_suffix) = match normalized_platform.as_str() {
            "windows" | "windows-x64" | "win64" => ("x86_64-pc-windows-msvc", ".exe"),
            "linux" | "linux-x64" => ("x86_64-unknown-linux-gnu", ""),
            _ if platform_text.contains('-') => {
                (platform_text, executable_suffix_for_target(platform_text))
            }
            _ => {
                return Err(format!(
                    "Unknown platform `{platform_text}`. Expected `windows-x64`, `linux-x64`, or a Rust target triple."
                ));
            }
        };

        Ok(Self {
            alias: normalized_platform,
            rust_target_triple: rust_target_triple.to_string(),
            executable_suffix: executable_suffix.to_string(),
        })
    }
}

fn executable_suffix_for_target(rust_target_triple: &str) -> &'static str {
    if rust_target_triple.contains("windows") {
        ".exe"
    } else {
        ""
    }
}

fn current_platform_alias() -> Result<String, String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Ok("windows-x64".to_string()),
        ("linux", "x86_64") => Ok("linux-x64".to_string()),
        (current_os, current_arch) => Err(format!(
            "No default Foundation platform alias is configured for {current_os}/{current_arch}. Pass `--platform <alias-or-target-triple>` explicitly."
        )),
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct BuildInvocation {
    command: Option<BuildCommand>,
    game_name: String,
    project_path: Option<PathBuf>,
    platform_text: String,
    configuration: Option<BuildConfiguration>,
    target_kind: Option<TargetKind>,
    output_directory: Option<PathBuf>,
    runtime_arguments: Vec<String>,
    help_requested: bool,
}

impl BuildInvocation {
    fn parse(arguments: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut invocation = Self::default();
        let mut argument_iterator = arguments.into_iter();

        let Some(command_or_help) = argument_iterator.next() else {
            return Err(
                "Expected `build`, `package`, or `run`. Use `--help` for usage.".to_string(),
            );
        };

        if command_or_help == "--help" || command_or_help == "-h" {
            invocation.help_requested = true;
            return Ok(invocation);
        }

        invocation.command = Some(BuildCommand::parse(&command_or_help)?);

        while let Some(argument_name) = argument_iterator.next() {
            if argument_name == "--" {
                invocation.runtime_arguments.extend(argument_iterator);
                break;
            }

            match argument_name.as_str() {
                "--game" => {
                    invocation.game_name = required_value("--game", &mut argument_iterator)?
                }
                "--project" => {
                    let project_path_text = required_value("--project", &mut argument_iterator)?;
                    invocation.project_path = Some(PathBuf::from(project_path_text));
                }
                "--platform" => {
                    invocation.platform_text =
                        required_value("--platform", &mut argument_iterator)?;
                }
                "--configuration" => {
                    let configuration_text =
                        required_value("--configuration", &mut argument_iterator)?;
                    invocation.configuration =
                        Some(BuildConfiguration::parse(&configuration_text)?);
                }
                "--target" => {
                    let target_text = required_value("--target", &mut argument_iterator)?;
                    invocation.target_kind = Some(TargetKind::parse(&target_text)?);
                }
                "--output" => {
                    let output_directory_text = required_value("--output", &mut argument_iterator)?;
                    invocation.output_directory = Some(PathBuf::from(output_directory_text));
                }
                "--help" | "-h" => {
                    invocation.help_requested = true;
                    return Ok(invocation);
                }
                unknown_argument => {
                    return Err(format!(
                        "Unknown Foundation build argument `{unknown_argument}`."
                    ));
                }
            }
        }

        invocation.validate_required_arguments()?;
        Ok(invocation)
    }

    fn validate_required_arguments(&self) -> Result<(), String> {
        let has_game_name = !self.game_name.trim().is_empty();
        let has_project_path = self.project_path.is_some();

        match (has_game_name, has_project_path) {
            (true, true) => {
                Err("Expected either `--game <name>` or `--project <path>`, not both.".to_string())
            }
            (false, false) => Err("Expected `--game <name>` or `--project <path>`.".to_string()),
            _ => Ok(()),
        }
    }
}

fn required_value(
    argument_name: &str,
    argument_iterator: &mut impl Iterator<Item = String>,
) -> Result<String, String> {
    argument_iterator
        .next()
        .filter(|argument_value| !argument_value.starts_with("--"))
        .ok_or_else(|| format!("Expected a value after `{argument_name}`."))
}

#[derive(Clone, Debug)]
struct BuildRequest {
    command: BuildCommand,
    game_name: String,
    package_name: String,
    executable_name: String,
    asset_roots: Vec<PathBuf>,
    game_directory: PathBuf,
    manifest_path: PathBuf,
    cargo_manifest_path: PathBuf,
    cargo_working_directory: PathBuf,
    default_target_directory: PathBuf,
    invocation_directory: PathBuf,
    platform: TargetPlatform,
    configuration: BuildConfiguration,
    target_kind: TargetKind,
    output_directory: PathBuf,
    runtime_arguments: Vec<String>,
    uses_workspace_package: bool,
}

impl BuildRequest {
    fn new(
        invocation: BuildInvocation,
        invocation_directory: PathBuf,
        game_project: GameProject,
    ) -> Result<Self, String> {
        let command = invocation
            .command
            .expect("command is validated before request creation");
        let configuration = invocation.configuration.unwrap_or(BuildConfiguration::Test);
        let target_kind = invocation.target_kind.unwrap_or(TargetKind::Game);

        if configuration == BuildConfiguration::Shipping && target_kind == TargetKind::GameEditor {
            return Err(
                "Invalid Foundation build matrix: `shipping` cannot be combined with `game-editor`."
                    .to_string(),
            );
        }

        let platform_text = if invocation.platform_text.trim().is_empty() {
            current_platform_alias()?
        } else {
            invocation.platform_text
        };
        let platform = TargetPlatform::parse(&platform_text)?;
        let package = game_project.manifest.launch.package.clone();
        let package_metadata = game_project.manifest.package.clone().unwrap_or_default();
        let executable_name = package_metadata
            .executable_name
            .unwrap_or_else(|| game_project.manifest.game.name.clone());
        let asset_roots = package_metadata
            .asset_roots
            .unwrap_or_else(|| vec!["assets".to_string()])
            .into_iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        let output_directory = invocation
            .output_directory
            .unwrap_or_else(|| PathBuf::from(DEFAULT_OUTPUT_DIRECTORY));

        Ok(Self {
            command,
            game_name: game_project.manifest.game.name,
            package_name: package,
            executable_name,
            asset_roots,
            game_directory: game_project.game_directory,
            manifest_path: game_project.manifest_path,
            cargo_manifest_path: game_project.cargo_manifest_path,
            cargo_working_directory: game_project.cargo_working_directory,
            default_target_directory: game_project.default_target_directory,
            invocation_directory,
            platform,
            configuration,
            target_kind,
            output_directory,
            runtime_arguments: invocation.runtime_arguments,
            uses_workspace_package: game_project.uses_workspace_package,
        })
    }

    fn cargo_feature_arguments(&self) -> Vec<String> {
        let mut feature_names = Vec::new();
        if self.configuration.enables_dev_tools() {
            feature_names.push("dev-tools");
        }
        if self.target_kind == TargetKind::GameEditor {
            feature_names.push("editor");
        }

        if feature_names.is_empty() {
            vec!["--no-default-features".to_string()]
        } else {
            vec![
                "--no-default-features".to_string(),
                "--features".to_string(),
                feature_names.join(","),
            ]
        }
    }

    fn cargo_profile_directory(&self) -> &'static str {
        match self.configuration {
            BuildConfiguration::Debug => "debug",
            BuildConfiguration::Test => "foundation-test",
            BuildConfiguration::Shipping => "foundation-shipping",
        }
    }

    fn package_directory(&self) -> PathBuf {
        let output_directory = if self.output_directory.is_absolute() {
            self.output_directory.clone()
        } else {
            self.invocation_directory.join(&self.output_directory)
        };

        output_directory
            .join(&self.game_name)
            .join(&self.platform.alias)
            .join(self.configuration.as_output_segment())
            .join(self.target_kind.as_output_segment())
    }

    fn built_executable_path(&self) -> PathBuf {
        let cargo_target_directory_override =
            std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from);
        self.built_executable_path_with_target_directory(cargo_target_directory_override.as_deref())
    }

    fn built_executable_path_with_target_directory(
        &self,
        cargo_target_directory_override: Option<&Path>,
    ) -> PathBuf {
        let executable_file_name =
            format!("{}{}", self.package_name, self.platform.executable_suffix);
        cargo_target_directory_override
            .unwrap_or(&self.default_target_directory)
            .join(&self.platform.rust_target_triple)
            .join(self.cargo_profile_directory())
            .join(executable_file_name)
    }

    fn packaged_executable_path(&self, package_directory: &Path) -> PathBuf {
        let executable_file_name = format!(
            "{}{}",
            self.executable_name, self.platform.executable_suffix
        );
        package_directory.join(executable_file_name)
    }
}

#[derive(Clone, Debug)]
struct GameProject {
    manifest: FoundationGameManifest,
    game_directory: PathBuf,
    manifest_path: PathBuf,
    cargo_manifest_path: PathBuf,
    cargo_working_directory: PathBuf,
    default_target_directory: PathBuf,
    uses_workspace_package: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct FoundationGameManifest {
    game: FoundationGameManifestGame,
    launch: FoundationGameManifestLaunch,
    package: Option<FoundationGameManifestPackage>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct FoundationGameManifestGame {
    name: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct FoundationGameManifestLaunch {
    package: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
struct FoundationGameManifestPackage {
    executable_name: Option<String>,
    asset_roots: Option<Vec<String>>,
}

fn find_engine_root() -> Result<PathBuf, String> {
    let build_crate_directory = Path::new(env!("CARGO_MANIFEST_DIR"));
    build_crate_directory
        .ancestors()
        .find(|candidate_directory| candidate_directory.join("Cargo.toml").is_file())
        .map(Path::to_path_buf)
        .ok_or_else(|| "Could not find the Foundation engine root.".to_string())
}

fn find_game_project(
    engine_root: &Path,
    invocation: &BuildInvocation,
) -> Result<GameProject, String> {
    if let Some(project_path) = &invocation.project_path {
        return find_external_game_project(project_path);
    }

    find_workspace_game_project(engine_root, &invocation.game_name)
}

fn find_external_game_project(project_path: &Path) -> Result<GameProject, String> {
    let manifest_path = external_manifest_path(project_path)?;
    let manifest = read_game_manifest(&manifest_path)?;
    let game_directory = manifest_path
        .parent()
        .ok_or_else(|| {
            format!(
                "Game manifest {} has no parent directory.",
                manifest_path.display()
            )
        })?
        .to_path_buf();
    let cargo_manifest_path = game_directory.join("Cargo.toml");
    if !cargo_manifest_path.is_file() {
        return Err(format!(
            "External game project {} is missing Cargo.toml.",
            game_directory.display()
        ));
    }

    Ok(GameProject {
        manifest,
        game_directory: game_directory.clone(),
        manifest_path,
        cargo_manifest_path,
        cargo_working_directory: game_directory.clone(),
        default_target_directory: game_directory.join("target"),
        uses_workspace_package: false,
    })
}

fn external_manifest_path(project_path: &Path) -> Result<PathBuf, String> {
    let absolute_project_path = if project_path.is_absolute() {
        project_path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("Failed to read current directory: {error}"))?
            .join(project_path)
    };

    let manifest_path = if absolute_project_path.is_dir() {
        absolute_project_path.join(GAME_MANIFEST_FILE_NAME)
    } else {
        absolute_project_path
    };

    if !manifest_path.is_file() {
        return Err(format!(
            "External game project manifest {} does not exist.",
            manifest_path.display()
        ));
    }

    manifest_path.canonicalize().map_err(|error| {
        format!(
            "Failed to canonicalize {}: {error}",
            manifest_path.display()
        )
    })
}

fn find_workspace_game_project(
    engine_root: &Path,
    requested_game_name: &str,
) -> Result<GameProject, String> {
    let games_directory = engine_root.join(GAMES_DIRECTORY_NAME);
    let mut available_game_names = Vec::new();

    for directory_entry_result in fs::read_dir(&games_directory)
        .map_err(|error| format!("Failed to read {}: {error}", games_directory.display()))?
    {
        let directory_entry = directory_entry_result
            .map_err(|error| format!("Failed to inspect game directory entry: {error}"))?;
        let manifest_path = directory_entry.path().join(GAME_MANIFEST_FILE_NAME);
        if !manifest_path.is_file() {
            continue;
        }

        let game_manifest = read_game_manifest(&manifest_path)?;
        available_game_names.push(game_manifest.game.name.clone());
        if game_manifest
            .game
            .name
            .eq_ignore_ascii_case(requested_game_name)
        {
            return Ok(GameProject {
                manifest: game_manifest,
                game_directory: directory_entry.path(),
                manifest_path,
                cargo_manifest_path: engine_root.join("Cargo.toml"),
                cargo_working_directory: engine_root.to_path_buf(),
                default_target_directory: engine_root.join("target"),
                uses_workspace_package: true,
            });
        }
    }

    available_game_names.sort();
    Err(format!(
        "Unknown Foundation game `{requested_game_name}`. Available games: {}",
        available_game_names.join(", ")
    ))
}

fn read_game_manifest(manifest_path: &Path) -> Result<FoundationGameManifest, String> {
    let manifest_text = fs::read_to_string(manifest_path).map_err(|error| {
        format!(
            "Failed to read game manifest {}: {error}",
            manifest_path.display()
        )
    })?;
    toml::from_str::<FoundationGameManifest>(&manifest_text).map_err(|error| {
        format!(
            "Failed to parse game manifest {}: {error}",
            manifest_path.display()
        )
    })
}

fn build_game(_engine_root: &Path, build_request: &BuildRequest) -> Result<(), String> {
    let mut cargo_command = Command::new("cargo");
    cargo_command.current_dir(&build_request.cargo_working_directory);
    cargo_command.arg("build");

    if build_request.uses_workspace_package {
        cargo_command.args(["-p", build_request.package_name.as_str()]);
    } else {
        cargo_command.args([
            "--manifest-path",
            build_request
                .cargo_manifest_path
                .as_os_str()
                .to_string_lossy()
                .as_ref(),
        ]);
    }

    cargo_command.args([
        "--target",
        build_request.platform.rust_target_triple.as_str(),
    ]);

    if let Some(cargo_profile) = build_request.configuration.cargo_profile() {
        cargo_command.args(["--profile", cargo_profile]);
    }

    cargo_command.args(build_request.cargo_feature_arguments());

    println!(
        "Building {} for {} / {} / {}.",
        build_request.game_name,
        build_request.platform.alias,
        build_request.configuration.as_output_segment(),
        build_request.target_kind.as_output_segment()
    );

    let build_status = cargo_command
        .status()
        .map_err(|error| format!("Failed to start cargo build: {error}"))?;
    if !build_status.success() {
        return Err(format!("Cargo build failed with status {build_status}."));
    }

    Ok(())
}

fn run_game(build_request: &BuildRequest) -> Result<(), String> {
    let built_executable_path = build_request.built_executable_path();
    let mut game_command = Command::new(&built_executable_path);
    game_command.current_dir(&build_request.game_directory);

    if let Some(first_asset_root) = build_request.asset_roots.first() {
        let local_asset_root = build_request.game_directory.join(first_asset_root);
        game_command.env("FOUNDATION_ASSET_ROOT", local_asset_root);
    }

    if build_request.target_kind == TargetKind::GameEditor {
        game_command.arg("--editor");
    }
    game_command.args(&build_request.runtime_arguments);

    println!(
        "Running {} for {} / {} / {}.",
        build_request.game_name,
        build_request.platform.alias,
        build_request.configuration.as_output_segment(),
        build_request.target_kind.as_output_segment()
    );

    let game_status = game_command.status().map_err(|error| {
        format!(
            "Failed to run built game executable {}: {error}",
            built_executable_path.display()
        )
    })?;
    if !game_status.success() {
        return Err(format!("Game exited with status {game_status}."));
    }

    Ok(())
}

fn package_game(engine_root: &Path, build_request: &BuildRequest) -> Result<(), String> {
    let package_directory = build_request.package_directory();
    if package_directory.exists() {
        fs::remove_dir_all(&package_directory).map_err(|error| {
            format!(
                "Failed to clean existing package directory {}: {error}",
                package_directory.display()
            )
        })?;
    }
    fs::create_dir_all(&package_directory).map_err(|error| {
        format!(
            "Failed to create package directory {}: {error}",
            package_directory.display()
        )
    })?;

    let built_executable_path = build_request.built_executable_path();
    let packaged_executable_path = build_request.packaged_executable_path(&package_directory);
    copy_file(&built_executable_path, &packaged_executable_path)?;

    for asset_root in &build_request.asset_roots {
        let source_asset_root = build_request.game_directory.join(asset_root);
        if !source_asset_root.exists() {
            continue;
        }
        let destination_asset_root = package_directory.join(asset_root);
        copy_directory(&source_asset_root, &destination_asset_root)?;
    }

    let engine_asset_root = engine_root.join("assets");
    if engine_asset_root.is_dir() {
        let destination_engine_asset_root = package_directory.join("assets").join("engine");
        copy_directory(&engine_asset_root, &destination_engine_asset_root)?;
    }

    let manifest_destination_path = package_directory.join(GAME_MANIFEST_FILE_NAME);
    copy_file(&build_request.manifest_path, &manifest_destination_path)?;
    write_package_metadata(&package_directory, build_request)?;
    create_archive(&package_directory, build_request)?;

    println!("Packaged game at {}.", package_directory.display());
    Ok(())
}

fn copy_file(source_file_path: &Path, destination_file_path: &Path) -> Result<(), String> {
    let destination_parent = destination_file_path.parent().ok_or_else(|| {
        format!(
            "Destination file path {} has no parent directory.",
            destination_file_path.display()
        )
    })?;
    fs::create_dir_all(destination_parent).map_err(|error| {
        format!(
            "Failed to create destination directory {}: {error}",
            destination_parent.display()
        )
    })?;
    fs::copy(source_file_path, destination_file_path).map_err(|error| {
        format!(
            "Failed to copy {} to {}: {error}",
            source_file_path.display(),
            destination_file_path.display()
        )
    })?;
    Ok(())
}

fn copy_directory(source_directory: &Path, destination_directory: &Path) -> Result<(), String> {
    fs::create_dir_all(destination_directory).map_err(|error| {
        format!(
            "Failed to create directory {}: {error}",
            destination_directory.display()
        )
    })?;

    for directory_entry_result in fs::read_dir(source_directory).map_err(|error| {
        format!(
            "Failed to read source directory {}: {error}",
            source_directory.display()
        )
    })? {
        let directory_entry = directory_entry_result
            .map_err(|error| format!("Failed to inspect directory entry: {error}"))?;
        let source_path = directory_entry.path();
        let destination_path = destination_directory.join(directory_entry.file_name());
        let file_type = directory_entry
            .file_type()
            .map_err(|error| format!("Failed to inspect {}: {error}", source_path.display()))?;

        if file_type.is_dir() {
            copy_directory(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            copy_file(&source_path, &destination_path)?;
        }
    }

    Ok(())
}

fn write_package_metadata(
    package_directory: &Path,
    build_request: &BuildRequest,
) -> Result<(), String> {
    let metadata_path = package_directory.join("foundation.package.toml");
    let metadata_text = format!(
        "[package]\nname = \"{}\"\nconfiguration = \"{}\"\ntarget = \"{}\"\nplatform = \"{}\"\nrust-target = \"{}\"\n",
        build_request.game_name,
        build_request.configuration.as_output_segment(),
        build_request.target_kind.as_output_segment(),
        build_request.platform.alias,
        build_request.platform.rust_target_triple
    );
    fs::write(&metadata_path, metadata_text)
        .map_err(|error| format!("Failed to write {}: {error}", metadata_path.display()))?;
    Ok(())
}

fn create_archive(package_directory: &Path, build_request: &BuildRequest) -> Result<(), String> {
    let archive_file_name = format!(
        "{}-{}-{}-{}.tar.gz",
        build_request.game_name,
        build_request.platform.alias,
        build_request.configuration.as_output_segment(),
        build_request.target_kind.as_output_segment()
    );
    let archive_path = package_directory
        .parent()
        .ok_or_else(|| {
            format!(
                "Package directory {} has no parent.",
                package_directory.display()
            )
        })?
        .join(archive_file_name);

    let package_parent = package_directory.parent().ok_or_else(|| {
        format!(
            "Package directory {} has no parent.",
            package_directory.display()
        )
    })?;
    let package_directory_name = package_directory
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| {
            format!(
                "Package directory {} has no file name.",
                package_directory.display()
            )
        })?;

    let archive_status = Command::new("tar")
        .current_dir(package_parent)
        .args([
            "-czf",
            archive_path.as_os_str().to_string_lossy().as_ref(),
            package_directory_name,
        ])
        .status();

    match archive_status {
        Ok(status) if status.success() => {
            println!("Created archive {}.", archive_path.display());
            Ok(())
        }
        Ok(status) => Err(format!("Archive creation failed with status {status}.")),
        Err(error) if is_missing_executable(&error) => Err(format!(
            "Could not create package archive because `tar` is unavailable. Package directory is still available at {}.",
            package_directory.display()
        )),
        Err(error) => Err(format!("Failed to start archive command: {error}")),
    }
}

fn is_missing_executable(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::NotFound
}

impl fmt::Display for BuildConfiguration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_output_segment())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipping_game_editor_is_rejected() {
        let invocation = BuildInvocation {
            command: Some(BuildCommand::Package),
            game_name: "template-game".to_string(),
            project_path: None,
            platform_text: "windows-x64".to_string(),
            configuration: Some(BuildConfiguration::Shipping),
            target_kind: Some(TargetKind::GameEditor),
            output_directory: None,
            runtime_arguments: Vec::new(),
            help_requested: false,
        };
        let game_project = template_game_project();
        let invocation_directory = PathBuf::from("C:/workspace/Foundation");

        let request_error = BuildRequest::new(invocation, invocation_directory, game_project)
            .expect_err("shipping game-editor builds must be invalid");

        assert!(request_error.contains("shipping"));
        assert!(request_error.contains("game-editor"));
    }

    #[test]
    fn test_game_editor_enables_dev_and_editor_features() {
        let invocation = BuildInvocation {
            command: Some(BuildCommand::Package),
            game_name: "template-game".to_string(),
            project_path: None,
            platform_text: "linux-x64".to_string(),
            configuration: Some(BuildConfiguration::Test),
            target_kind: Some(TargetKind::GameEditor),
            output_directory: None,
            runtime_arguments: Vec::new(),
            help_requested: false,
        };
        let game_project = template_game_project();
        let invocation_directory = PathBuf::from("C:/workspace/Foundation");
        let build_request = BuildRequest::new(invocation, invocation_directory, game_project)
            .expect("request should build");

        assert_eq!(
            build_request.cargo_feature_arguments(),
            ["--no-default-features", "--features", "dev-tools,editor"]
        );
    }

    #[test]
    fn shipping_game_disables_default_features() {
        let invocation = BuildInvocation {
            command: Some(BuildCommand::Package),
            game_name: "template-game".to_string(),
            project_path: None,
            platform_text: "windows-x64".to_string(),
            configuration: Some(BuildConfiguration::Shipping),
            target_kind: Some(TargetKind::Game),
            output_directory: None,
            runtime_arguments: Vec::new(),
            help_requested: false,
        };
        let game_project = template_game_project();
        let invocation_directory = PathBuf::from("C:/workspace/Foundation");
        let build_request = BuildRequest::new(invocation, invocation_directory, game_project)
            .expect("request should build");

        assert_eq!(
            build_request.cargo_feature_arguments(),
            ["--no-default-features"]
        );
    }

    #[test]
    fn omitted_configuration_and_target_default_to_test_game() {
        let invocation = BuildInvocation {
            command: Some(BuildCommand::Run),
            game_name: "template-game".to_string(),
            project_path: None,
            platform_text: "windows-x64".to_string(),
            configuration: None,
            target_kind: None,
            output_directory: None,
            runtime_arguments: Vec::new(),
            help_requested: false,
        };
        let game_project = template_game_project();
        let invocation_directory = PathBuf::from("C:/workspace/Foundation");
        let build_request = BuildRequest::new(invocation, invocation_directory, game_project)
            .expect("request should build");

        assert_eq!(build_request.configuration, BuildConfiguration::Test);
        assert_eq!(build_request.target_kind, TargetKind::Game);
        assert_eq!(build_request.platform.alias, "windows-x64");
    }

    #[test]
    fn run_command_preserves_runtime_arguments() {
        let arguments = [
            "run",
            "--game",
            "template-game",
            "--platform",
            "windows-x64",
            "--configuration",
            "debug",
            "--target",
            "game-editor",
            "--",
            "--custom-game-argument",
            "--log",
        ]
        .map(str::to_string);

        let invocation = BuildInvocation::parse(arguments).expect("run arguments should parse");

        assert_eq!(invocation.command, Some(BuildCommand::Run));
        assert_eq!(
            invocation.runtime_arguments,
            ["--custom-game-argument", "--log"]
        );
    }

    #[test]
    fn project_argument_accepts_external_game_directory() {
        let arguments = [
            "build",
            "--project",
            "../template-game/game",
            "--platform",
            "windows-x64",
        ]
        .map(str::to_string);

        let invocation = BuildInvocation::parse(arguments).expect("project arguments should parse");

        assert_eq!(
            invocation.project_path,
            Some(PathBuf::from("../template-game/game"))
        );
        assert!(invocation.game_name.is_empty());
    }

    #[test]
    fn game_and_project_arguments_conflict() {
        let arguments = [
            "build",
            "--game",
            "template-game",
            "--project",
            "../template-game/game",
        ]
        .map(str::to_string);

        let parse_error = BuildInvocation::parse(arguments)
            .expect_err("game name and project path should conflict");

        assert!(parse_error.contains("either `--game <name>` or `--project <path>`"));
    }

    #[test]
    fn external_project_uses_game_target_directory_by_default() {
        let invocation = BuildInvocation {
            command: Some(BuildCommand::Package),
            game_name: String::new(),
            project_path: Some(PathBuf::from("C:/workspace/template-game/game")),
            platform_text: "windows-x64".to_string(),
            configuration: Some(BuildConfiguration::Test),
            target_kind: Some(TargetKind::Game),
            output_directory: None,
            runtime_arguments: Vec::new(),
            help_requested: false,
        };
        let mut game_project = template_game_project();
        game_project.game_directory = PathBuf::from("C:/workspace/template-game/game");
        game_project.cargo_manifest_path = game_project.game_directory.join("Cargo.toml");
        game_project.cargo_working_directory = game_project.game_directory.clone();
        game_project.default_target_directory = game_project.game_directory.join("target");
        game_project.uses_workspace_package = false;
        let invocation_directory = PathBuf::from("C:/workspace/template-game");
        let build_request = BuildRequest::new(invocation, invocation_directory, game_project)
            .expect("request should build");

        assert_eq!(
            build_request.built_executable_path_with_target_directory(None),
            PathBuf::from(
                "C:/workspace/template-game/game/target/x86_64-pc-windows-msvc/foundation-test/template-game.exe"
            )
        );
    }

    #[test]
    fn built_executable_path_respects_cargo_target_dir() {
        let invocation = BuildInvocation {
            command: Some(BuildCommand::Package),
            game_name: "template-game".to_string(),
            project_path: None,
            platform_text: "windows-x64".to_string(),
            configuration: Some(BuildConfiguration::Test),
            target_kind: Some(TargetKind::Game),
            output_directory: None,
            runtime_arguments: Vec::new(),
            help_requested: false,
        };
        let game_project = template_game_project();
        let invocation_directory = PathBuf::from("C:/workspace/Foundation");
        let build_request = BuildRequest::new(invocation, invocation_directory, game_project)
            .expect("request should build");
        let cargo_target_directory_override =
            Path::new("C:/actions-runner/cargo-target/Foundation");
        let built_executable_path = build_request
            .built_executable_path_with_target_directory(Some(cargo_target_directory_override));

        assert_eq!(
            built_executable_path,
            PathBuf::from(
                "C:/actions-runner/cargo-target/Foundation/x86_64-pc-windows-msvc/foundation-test/template-game.exe"
            )
        );
    }

    #[test]
    fn platform_aliases_map_to_rust_targets() {
        let windows_platform = TargetPlatform::parse("windows-x64").expect("windows alias parses");
        let linux_platform = TargetPlatform::parse("linux-x64").expect("linux alias parses");

        assert_eq!(
            windows_platform.rust_target_triple,
            "x86_64-pc-windows-msvc"
        );
        assert_eq!(windows_platform.executable_suffix, ".exe");
        assert_eq!(
            linux_platform.rust_target_triple,
            "x86_64-unknown-linux-gnu"
        );
        assert_eq!(linux_platform.executable_suffix, "");
    }

    fn template_game_project() -> GameProject {
        let game_directory = PathBuf::from("C:/workspace/Foundation/games/template-game");
        GameProject {
            manifest: FoundationGameManifest {
                game: FoundationGameManifestGame {
                    name: "template-game".to_string(),
                },
                launch: FoundationGameManifestLaunch {
                    package: "template-game".to_string(),
                },
                package: Some(FoundationGameManifestPackage {
                    executable_name: Some("template-game".to_string()),
                    asset_roots: Some(vec!["assets".to_string()]),
                }),
            },
            game_directory: game_directory.clone(),
            manifest_path: game_directory.join(GAME_MANIFEST_FILE_NAME),
            cargo_manifest_path: PathBuf::from("C:/workspace/Foundation/Cargo.toml"),
            cargo_working_directory: PathBuf::from("C:/workspace/Foundation"),
            default_target_directory: PathBuf::from("C:/workspace/Foundation/target"),
            uses_workspace_package: true,
        }
    }
}
