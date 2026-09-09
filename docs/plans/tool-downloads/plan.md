# Tool Downloads Plan

## Metadata
- Feature slug: `tool-downloads`
- Feature area: `engine`
- Primary area: `engine`
- Branch: `feature/performance-diagnostics` (shared branch by explicit user instruction — see Risks/Constraints below; not a dedicated branch per the usual gitflow-workflow rule)
- Status: `Planned`
- Planning model: `gpt-5.5` (Claude Sonnet 5, per durable override recorded 2026-07-20)
- Implementation model: `gpt-5.4` (Claude Sonnet 5, same override)
- Review model: `gpt-5.5` (Claude Sonnet 5, same override)
- Created: `2026-09-09`
- Last updated: `2026-09-09`

## User Request
Following on from the performance-diagnostics work, the user asked whether Tracy (the external profiler GUI) could be auto-downloaded as part of engine setup rather than requiring a manual download or a package manager. After ruling out git submodules (they only track source, not GitHub release binary attachments) and weighing package-manager wrapping (winget/brew) against a direct-from-GitHub-releases download, the user chose the direct-download approach, implemented as a `foundation-build` subcommand so it can grow to fetch other external tools in the future, and confirmed this belongs in the engine (not the game), since `foundation-build` is already the shared build tool used by every Foundation game.

## Feature Summary
Adds a `foundation-build tools install <name>` (and `tools list`) subcommand that downloads a known external developer tool's prebuilt release archive for the current OS, verifies its checksum, extracts the needed executable, and installs it into a shared per-user cache directory — idempotently, so repeat calls are instant no-ops. Ships with one known tool (`tracy`, the Tracy profiler GUI) but is structured so a second known tool is a small, additive change.

## Feature Area Classification
- Area: `engine`
- Primary area: `engine`
- Rationale: All code changes are inside `engine/crates/foundation-build`, which is Foundation's shared build/packaging tool used by every game via `scripts/foundation-build.cmd` (and each game's own thin wrapper, e.g. Last Beacon's `scripts/foundation-game.cmd`). No game-specific code is needed. A one-line follow-up to Last Beacon's `docs/performance-profiling.md` (pointing at the new command instead of a manual download) is out of this plan's scope and tracked as a follow-up in the tracker.

## Codebase Research
- `engine/crates/foundation-build/src/lib.rs` (1166 lines) is the entire crate: a hand-rolled argument parser (`BuildInvocation::parse`), a `BuildCommand` enum (`Build`/`Package`/`Run`), and the orchestration in `pub fn run()`. `run()` unconditionally resolves a game project (`find_game_project`) after parsing, before dispatching to build/package/run — a `tools` command has no game project, so `run()` needs a new early branch checked before that resolution, alongside the existing `help_requested` short-circuit.
- Current dependencies are minimal: `serde` and `toml` only (`engine/crates/foundation-build/Cargo.toml`). This feature needs to add an HTTP client, a zip reader, and a checksum hasher — kept to three additions, each with a single, narrow purpose.
- `scripts/foundation-game.cmd` (root) forwards all arguments straight through to `engine/scripts/foundation-build.cmd`, which itself is a thin wrapper around `cargo run -p foundation-build --`. No new wrapper scripts are needed for the new subcommand — both existing entry points already forward arbitrary arguments.
- `current_platform_alias()` in `lib.rs` already establishes the pattern of matching `(std::env::consts::OS, std::env::consts::ARCH)` and erroring clearly on unsupported combinations (only `windows`/`x86_64` and `linux`/`x86_64` are recognized anywhere in this codebase — macOS is not a supported platform anywhere today). This feature follows the same restriction rather than introducing macOS support nothing else in the project has.
- `engine/crates/foundation-build/src/lib.rs` has an established plain `#[cfg(test)] mod tests` block (10 tests, no mocking framework) testing pure parsing/path logic directly — the new module follows the same style for its pure logic (asset/digest selection, install-path construction, idempotency check), without unit-testing the actual network call.

## External Research
Verified directly against Tracy's real GitHub releases (`github.com/wolfpld/tracy`), not assumed from training knowledge:
- Latest release at research time: `v0.14.1`.
- Confirmed via `gh api repos/wolfpld/tracy/releases/latest` that each platform ships a single zip asset: `windows-0.14.1.zip`, `linux-0.14.1.zip`, `macos-0.14.1.zip` (macOS out of scope per Feature Area Classification above).
- Downloaded and inspected each zip's contents directly:
  - `windows-0.14.1.zip`: flat, contains `tracy-profiler.exe` directly (plus other Tracy CLI tools not needed here).
  - `linux-0.14.1.zip`: flat, contains `tracy-profiler-x86_64.AppImage` directly.
  - `macos-0.14.1.zip`: **not** flat — contains a full `tracy-profiler.app/Contents/...` bundle tree, which would need directory-tree extraction and exec-bit preservation. This asymmetry is the concrete reason macOS was scoped out (see Feature Area Classification).
- Computed and verified real SHA-256 digests for the exact assets this feature will pin against:
  - `windows-0.14.1.zip`: `f7499d74914aa3ba94a2c1ce72f36477d7b61d9d0f7c9790e05274c258c97fb5`
  - `linux-0.14.1.zip`: `4f57574337b206cac86758081ab21c37cf9c83fc12170d2e339e1f4ee94ff590`
  - Download URLs: `https://github.com/wolfpld/tracy/releases/download/v0.14.1/windows-0.14.1.zip` and the `linux-0.14.1.zip` equivalent.

## Affected Files And Systems
- `engine/crates/foundation-build/Cargo.toml`: add `ureq` (HTTP GET; rustls-based TLS feature, to avoid an OpenSSL system dependency on Linux), `zip` (pure-Rust zip reading, exposes Unix permission bits via `unix_mode()`), `sha2` (SHA-256 verification).
- `engine/crates/foundation-build/src/tool_downloads.rs` (new): the `KnownTool` registry, per-OS asset metadata, install-path resolution, and the install/list logic.
- `engine/crates/foundation-build/src/lib.rs`: new early dispatch branch in `run()` for a `tools` top-level command; updated `print_usage()`.

## Proposed Implementation Approach
1. Add the `KnownTool` enum (`Tracy` only) in `tool_downloads.rs`, each variant exposing: display name, pinned version, and a per-`(target_os, target_arch)` table of `{ download_url, expected_sha256, zip_entry_name, installed_relative_path }`. For both `Tracy` platforms today, `zip_entry_name == installed_relative_path` since both ship flat.
2. Add a pure function resolving the shared cache root: `%LOCALAPPDATA%\Foundation\tools` on Windows (via `std::env::var("LOCALAPPDATA")`), `$XDG_CACHE_HOME/foundation/tools` falling back to `~/.cache/foundation/tools` on Linux (via `std::env::var("HOME")`). The final install path is `<cache_root>/<tool_name>/<version>/<installed_relative_path>`.
3. `install(tool: KnownTool) -> Result<PathBuf, String>`:
   - Resolve the target's install path; if it already exists, return it immediately (idempotent, no network).
   - Download the platform's zip to a temp file inside the same cache root (so the final rename is same-filesystem and atomic) via a blocking `ureq::get(...)` streamed to disk.
   - Hash the downloaded file with `sha2`; compare against the pinned digest; on mismatch, delete the temp file and return an error naming both digests.
   - Open the zip with the `zip` crate, extract only the one needed entry to a temp path, and on Linux set the executable bit explicitly (`std::os::unix::fs::PermissionsExt`) using the entry's `unix_mode()` (falling back to `0o755` if the zip didn't record a mode).
   - Create the versioned parent directory and atomically rename the extracted temp file into its final path.
   - Return the final path.
4. `list_known_tools() -> Vec<(&'static str, &'static str, bool)>` (name, version, is-installed) for the `tools list` verb, reusing the same install-path resolution to check installed status without downloading.
5. In `lib.rs`'s `run()`, check for a leading `"tools"` argument before the existing `BuildInvocation::parse`/game-project-resolution path; parse `install <name>` / `list` there and delegate to `tool_downloads`. Update `print_usage()` with the new verbs and examples.
6. Follow-up (tracked, not part of this plan): once merged, update Last Beacon's `docs/performance-profiling.md` to replace the manual Tracy download instructions with `foundation-build tools install tracy` (via `scripts\foundation-game.cmd tools install tracy` from the game repo root).

## Alternatives Considered
- **Package-manager wrapping (winget/brew/apt)**: rejected in earlier conversation — version drift risk between platforms/machines, no single deterministic pin, and no uniform coverage across Linux distros.
- **`reqwest` instead of `ureq`**: rejected — `reqwest`, even in "blocking" mode, pulls in an internal async runtime and a much larger dependency tree for what is here a single synchronous GET; `ureq` is a better fit for this crate's minimal-dependency style.
- **Including macOS now**: rejected for this round — real added complexity (bundle-tree extraction, exec-bit preservation across a directory) for a platform nothing else in this codebase builds for yet (see External Research: the macOS asset is structurally different, not just a naming variant).

## Risks, Constraints, And Assumptions
- **Shared branch deviation**: the user explicitly asked to keep this work on the existing `feature/performance-diagnostics` branch (both root and engine) rather than opening a new dedicated branch, overriding this repo's usual "every feature gets its own branch" gitflow rule. Recorded here so the eventual PR description and reviewers understand why two unrelated-sounding features share one branch/PR.
- Assumption: Tracy's release asset naming (`<platform>-<version>.zip`) and flat/bundle structure per OS remain stable across future version bumps; if Tracy changes its release layout, only `tool_downloads.rs`'s `KnownTool::Tracy` entry needs updating, not the general install machinery.
- Risk: pinning an exact SHA-256 means every future Tracy version bump requires a manual digest update in this repo; accepted deliberately, since it's what makes the download verifiable/reproducible rather than trusting whatever's live on GitHub at install time.
- Constraint: `ureq`'s TLS backend must not require a system OpenSSL install (Linux CI runners shouldn't need extra setup) — use its `rustls`-based feature, not `native-tls`.

## Open Questions
- None blocking. Whether to extend `tools list`/`tools install` with a `--version` override (installing something other than the pinned version) is left for if/when a second real need for it appears (YAGNI for now).

## Documentation Expectations
- Public items in `tool_downloads.rs` (the `KnownTool` enum, `install`, `list_known_tools`) get Rustdoc comments consistent with this crate's existing density.
- `print_usage()`'s new `tools` lines are the primary end-user-facing documentation for the command itself.
- The Last Beacon `docs/performance-profiling.md` follow-up (see Proposed Implementation Approach, step 6) is explicitly out of scope for this plan/tracker and will be its own small follow-up commit once this ships.

## Implementation Handoff Notes
- Implementer: Claude Sonnet 5 (standing in for `gpt-5.4`).
- Keep `tool_downloads.rs` fully separate from the existing `BuildCommand`/`BuildInvocation` machinery in `lib.rs` — the only touch point should be the new early-dispatch branch in `run()` and the `print_usage()` text, to avoid any regression risk to the existing build/package/run commands.
- Use real values from this plan's External Research section (URLs, digests, entry names) rather than re-deriving them; if the implementer bumps the pinned Tracy version, re-verify the digests the same way (download + `sha256sum`) rather than trusting a value from memory.
- Match `rust-coding-standards`: descriptive names (no `path`/`url` shorthand — e.g. `zip_entry_name`, `installed_relative_path`), named values before calls, frequent why-comments especially around the atomic-rename and exec-bit logic.

## Optional Review Focus Areas
- Reviewer: Claude Sonnet 5 (standing in for `gpt-5.5`).
- Confirm the download-then-verify-then-extract-then-atomic-rename sequence has no window where a partially-downloaded or checksum-failed file could be mistaken for a valid install by the idempotency check.
- Confirm the new `tools` dispatch branch in `run()` doesn't change any existing `build`/`package`/`run` behavior or error messages.

## Success Criteria
- `cargo run -p foundation-build -- tools install tracy` (and the equivalent via `scripts\foundation-game.cmd tools install tracy` from the Last Beacon root) downloads, verifies, and installs Tracy on a machine that doesn't have it, and prints the final executable path.
- Running the same command again is near-instant and makes no network request (idempotent).
- `cargo run -p foundation-build -- tools list` shows `tracy` and whether it's installed.
- A deliberately corrupted/mismatched digest causes install to fail loudly rather than installing an unverified binary.

## Testing Methodology
- `engine\scripts\format-project.cmd`
- `engine\scripts\lint-project.cmd`
- `engine\scripts\test-project.cmd`
- `engine\scripts\compile-project.cmd`
- `engine\scripts\doc-project.cmd`
- `engine\scripts\validate-project.cmd`
- Manual validation: run `tools install tracy` for real on this machine (network access confirmed available in this session), confirm the printed path exists and launches, then run it again to confirm the idempotent no-op path.
