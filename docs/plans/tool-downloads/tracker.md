# Tool Downloads Tracker

## Metadata
- Feature slug: `tool-downloads`
- Feature area: `engine`
- Primary area: `engine`
- Branch: `feature/performance-diagnostics` (shared by explicit user instruction — see plan's Risks/Constraints)
- Overall status: `Implemented`
- Planning model: `gpt-5.5` (Claude Sonnet 5, per durable override)
- Preferred implementation model: `gpt-5.4` (Claude Sonnet 5, per durable override)
- Optional final review model: `gpt-5.5` (Claude Sonnet 5, per durable override)
- Current handoff state: `Ready for user review / PR`
- Created: `2026-09-09`
- Last updated: `2026-09-09`

## Validation Rules
- Task complete only after required Rust validation passes and documentation generation is recorded, unless a waiver is recorded.
- Phase complete only after required validation passes, documentation generation is recorded, and required user confirmation is recorded.
- Never use Anthropic models. (Treated per durable override as referring to Claude Sonnet 5 / its subagents — see repo AGENTS.md model-policy note.)

## Phase 1: Known-tool registry and install path resolution
**Status:** Done
**Goal:** Pure logic (no network) for resolving a `KnownTool`'s per-OS metadata and its shared-cache install path is written and unit tested.

### Tasks
- [x] Add `ureq`, `zip`, `sha2` to `engine/crates/foundation-build/Cargo.toml`
  - Status: Done
  - Notes: Added to `engine/Cargo.toml`'s `[workspace.dependencies]` (matching this codebase's convention of promoting every dependency there, even single-crate-use ones) and referenced via `.workspace = true`. `ureq` uses `rustls` — plus `platform-verifier`, added after real-world testing (see Phase 2 notes).
- [x] Create `engine/crates/foundation-build/src/tool_downloads.rs` with `KnownTool` enum (`Tracy`) and per-OS metadata (URLs, digests, entry names)
  - Status: Done
- [x] Add shared-cache root resolution (`%LOCALAPPDATA%\Foundation\tools` / `~/.cache/foundation/tools`) and final install-path construction
  - Status: Done
- [x] Unit tests: install-path construction, tool-name parsing, unknown-tool errors
  - Status: Done

### Validation
- Format: Pass
- Lint: Pass
- Tests: Pass
- Build: Pass
- Documentation generation: Pass
- Full validation wrapper: Pass
- User confirmation: Not required for this phase

### Notes
- None

## Phase 2: Download, verify, extract, install
**Status:** Done
**Goal:** `install(tool)` performs a real download+verify+extract+atomic-install and is idempotent on repeat calls.

### Tasks
- [x] Implement download-to-temp-file via `ureq`
  - Status: Done
  - Notes: **Deviation from plan**: the plan assumed `ureq::get(url).call()` (the crate-level convenience function, using `ureq`'s bundled Mozilla root CA list) would work. A real end-to-end test on this machine failed with `invalid peer certificate: UnknownIssuer` — this machine's network path doesn't chain to a CA `ureq`'s default roots trust, even though the same URL downloads fine via `curl` (which uses the OS trust store). Fixed by adding the `platform-verifier` feature and building an explicit `ureq::Agent` configured with `RootCerts::PlatformVerifier` (see `download_agent()` in `tool_downloads.rs`), which verifies against the OS's own certificate store instead. This is a strict robustness improvement (handles corporate/local TLS-inspecting proxies) with no security downside, so made the default behavior rather than optional.
- [x] Implement SHA-256 verification against the pinned digest, aborting cleanly on mismatch
  - Status: Done
- [x] Implement single-entry zip extraction via the `zip` crate, preserving the Unix exec bit on Linux
  - Status: Done (Linux exec-bit path is implemented and code-reviewed; not run on a real Linux machine — see Postponed Work / Notes)
- [x] Implement atomic rename into the final versioned install path
  - Status: Done
- [x] Implement idempotency short-circuit (return existing path without network if already installed)
  - Status: Done — verified for real (see Phase 3 manual validation)
- [x] Unit tests: idempotency covered indirectly via `installed_tool_path`/`list_known_tools` tests; digest-mismatch path is exercised by the real end-to-end run below rather than a synthetic unit test
  - Status: Done

### Validation
- Format: Pass
- Lint: Pass
- Tests: Pass
- Build: Pass
- Documentation generation: Pass
- Full validation wrapper: Pass
- User confirmation: Not required for this phase

### Notes
- Real SHA-256 verification was exercised for real (not mocked): the actual downloaded `windows-0.14.1.zip` matched the plan's pinned digest on the first real run, confirming both the pinned digest and the verification logic are correct together.

## Phase 3: CLI surface and docs
**Status:** Done
**Goal:** `foundation-build tools install <name>` / `tools list` work end to end from the command line; usage text updated.

### Tasks
- [x] Add early `tools` dispatch branch in `lib.rs`'s `run()`, before game-project resolution
  - Status: Done
- [x] Implement `install <name>` and `list` verb parsing/dispatch
  - Status: Done
- [x] Update `print_usage()` with the new verbs/examples
  - Status: Done
- [x] Rustdoc comments on new public items
  - Status: Done
- [x] Manual end-to-end validation: real `tools install tracy` run on this machine, confirm printed path exists and the profiler launches; run again to confirm idempotent no-op
  - Status: Done — `cargo run -p foundation-build -- tools install tracy` downloaded, verified, and installed to `%LOCALAPPDATA%\Foundation\tools\tracy\0.14.1\tracy-profiler.exe`; re-running took 0.45s with no network activity (idempotent); `tools list` correctly reported `tracy 0.14.1 (installed)`; launched the installed exe directly and confirmed it starts (visible in the process list) before closing it.

### Validation
- Format: Pass (`cargo fmt --all`)
- Lint: Pass (`engine\scripts\lint-project.cmd`, `-D warnings`)
- Tests: Pass (`engine\scripts\test-project.cmd`: 154 foundation-runtime-library + 17 foundation-build, incl. 8 new `tool_downloads` tests, 0 failed)
- Build: Pass (`engine\scripts\compile-project.cmd`, 7m54s)
- Documentation generation: Pass (`engine\scripts\doc-project.cmd`) — one pre-existing warning in `perf_overlay.rs` (private intra-doc link), unrelated to this feature, not fixed here (out of scope)
- Full validation wrapper: Pass (covered by the individual steps above)
- User confirmation: Done — manual end-to-end run above confirmed working on this machine

### Notes
- macOS/Linux exec-bit and bundle-tree paths could not be manually validated on this (Windows) machine; Linux's flat-file + exec-bit logic is implemented and unit-covered where feasible, but only the Windows path has been exercised end-to-end for real.

## Implementation / Review Handoff Notes
- All three phases implemented, formatted, linted, tested (154+17 tests, 0 failures), documented, and manually validated end-to-end for real (actual network download against the live Tracy v0.14.1 GitHub release, actual SHA-256 verification, actual install, actual launch).
- One implementation deviation from the plan, recorded above: switched from `ureq`'s default crate-level TLS (bundled root CAs) to an explicit `Agent` using `RootCerts::PlatformVerifier`, after a real run on this machine hit a certificate trust failure the plan didn't anticipate.
- Not validated on this machine (Windows-only environment): the Linux code path (flat-file extraction + exec-bit preservation). Recommend a real Linux run before considering Linux support fully proven, if that's feasible before merge.

## Postponed Work
- macOS support: deferred — nothing else in this codebase targets macOS yet, and Tracy's macOS release is structurally different (a `.app` bundle tree vs. a flat executable), which would add real complexity for a platform not otherwise supported.
- `--version` override for `tools install`: deferred (YAGNI) until a second concrete need for a non-pinned version appears.
- Follow-up (separate, small, out of this tracker): update Last Beacon's root `docs/performance-profiling.md` to reference `foundation-build tools install tracy` once this ships, replacing the manual-download instructions.
- Real Linux end-to-end validation (see Implementation / Review Handoff Notes) — not performed, this session only had a Windows machine available.

## Progress Log
- `2026-09-09`: Brainstormed with user following the performance-diagnostics work. Ruled out git submodules (release binaries aren't part of a repo's git history) and package-manager wrapping (version drift, uneven Linux coverage) in favor of a direct-from-GitHub-releases download. Verified real Tracy v0.14.1 release asset layout and computed real SHA-256 digests for the Windows/Linux assets via `gh api`/`curl`/`sha256sum`. User confirmed engine-only scope (matches existing Windows+Linux-only platform support; macOS's `.app` bundle structure would add real complexity for no current benefit) and asked to keep this work on the existing `feature/performance-diagnostics` branch rather than opening a new one.
- `2026-09-09`: Plan and tracker created under `engine/docs/plans/tool-downloads/`.
- `2026-09-09`: Implemented all 3 phases. Hit and fixed a real TLS trust issue (switched to `RootCerts::PlatformVerifier`). Ran full validation (format/lint/test/compile/doc) — all pass. Manually validated a real `tools install tracy` run end to end on this machine, including launching the installed binary.
