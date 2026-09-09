# Tool Downloads Tracker

## Metadata
- Feature slug: `tool-downloads`
- Feature area: `engine`
- Primary area: `engine`
- Branch: `feature/performance-diagnostics` (shared by explicit user instruction — see plan's Risks/Constraints)
- Overall status: `Planned`
- Planning model: `gpt-5.5` (Claude Sonnet 5, per durable override)
- Preferred implementation model: `gpt-5.4` (Claude Sonnet 5, per durable override)
- Optional final review model: `gpt-5.5` (Claude Sonnet 5, per durable override)
- Current handoff state: `Ready for gpt-5.4 implementation`
- Created: `2026-09-09`
- Last updated: `2026-09-09`

## Validation Rules
- Task complete only after required Rust validation passes and documentation generation is recorded, unless a waiver is recorded.
- Phase complete only after required validation passes, documentation generation is recorded, and required user confirmation is recorded.
- Never use Anthropic models. (Treated per durable override as referring to Claude Sonnet 5 / its subagents — see repo AGENTS.md model-policy note.)

## Phase 1: Known-tool registry and install path resolution
**Status:** Planned
**Goal:** Pure logic (no network) for resolving a `KnownTool`'s per-OS metadata and its shared-cache install path is written and unit tested.

### Tasks
- [ ] Add `ureq`, `zip`, `sha2` to `engine/crates/foundation-build/Cargo.toml`
  - Status: Planned
  - Notes: `ureq` with rustls-based TLS feature, not `native-tls`.
- [ ] Create `engine/crates/foundation-build/src/tool_downloads.rs` with `KnownTool` enum (`Tracy`) and per-OS metadata (URLs, digests, entry names — use plan's verified values)
  - Status: Planned
- [ ] Add shared-cache root resolution (`%LOCALAPPDATA%\Foundation\tools` / `~/.cache/foundation/tools`) and final install-path construction
  - Status: Planned
- [ ] Unit tests: install-path construction per OS, unsupported OS/arch errors clearly
  - Status: Planned

### Validation
- Format: Pending
- Lint: Pending
- Tests: Pending
- Build: Pending
- Documentation generation: Pending
- Full validation wrapper: Pending / Not required yet
- User confirmation: Not required yet

### Notes
- None

## Phase 2: Download, verify, extract, install
**Status:** Planned
**Goal:** `install(tool)` performs a real download+verify+extract+atomic-install and is idempotent on repeat calls.

### Tasks
- [ ] Implement download-to-temp-file via `ureq`
  - Status: Planned
- [ ] Implement SHA-256 verification against the pinned digest, aborting cleanly on mismatch
  - Status: Planned
- [ ] Implement single-entry zip extraction via the `zip` crate, preserving the Unix exec bit on Linux
  - Status: Planned
- [ ] Implement atomic rename into the final versioned install path
  - Status: Planned
- [ ] Implement idempotency short-circuit (return existing path without network if already installed)
  - Status: Planned
- [ ] Unit tests: idempotency short-circuit against a pre-populated fake install dir; digest-mismatch handling with a fake downloaded file
  - Status: Planned

### Validation
- Format: Pending
- Lint: Pending
- Tests: Pending
- Build: Pending
- Documentation generation: Pending
- Full validation wrapper: Pending / Not required yet
- User confirmation: Not required yet

### Notes
- None

## Phase 3: CLI surface and docs
**Status:** Planned
**Goal:** `foundation-build tools install <name>` / `tools list` work end to end from the command line; usage text updated.

### Tasks
- [ ] Add early `tools` dispatch branch in `lib.rs`'s `run()`, before game-project resolution
  - Status: Planned
- [ ] Implement `install <name>` and `list` verb parsing/dispatch
  - Status: Planned
- [ ] Update `print_usage()` with the new verbs/examples
  - Status: Planned
- [ ] Rustdoc comments on new public items
  - Status: Planned
- [ ] Manual end-to-end validation: real `tools install tracy` run on this machine, confirm printed path exists and the profiler launches; run again to confirm idempotent no-op
  - Status: Planned

### Validation
- Format: Pending
- Lint: Pending
- Tests: Pending
- Build: Pending
- Documentation generation: Pending
- Full validation wrapper: Pending / Not required yet
- User confirmation: Pending — required (manual end-to-end run should be confirmed working)

### Notes
- None

## Implementation / Review Handoff Notes
- None yet.

## Postponed Work
- macOS support: deferred — nothing else in this codebase targets macOS yet, and Tracy's macOS release is structurally different (a `.app` bundle tree vs. a flat executable), which would add real complexity for a platform not otherwise supported.
- `--version` override for `tools install`: deferred (YAGNI) until a second concrete need for a non-pinned version appears.
- Follow-up (separate, small, out of this tracker): update Last Beacon's root `docs/performance-profiling.md` to reference `foundation-build tools install tracy` once this ships, replacing the manual-download instructions.

## Progress Log
- `2026-09-09`: Brainstormed with user following the performance-diagnostics work. Ruled out git submodules (release binaries aren't part of a repo's git history) and package-manager wrapping (version drift, uneven Linux coverage) in favor of a direct-from-GitHub-releases download. Verified real Tracy v0.14.1 release asset layout and computed real SHA-256 digests for the Windows/Linux assets via `gh api`/`curl`/`sha256sum`. User confirmed engine-only scope (matches existing Windows+Linux-only platform support; macOS's `.app` bundle structure would add real complexity for no current benefit) and asked to keep this work on the existing `feature/performance-diagnostics` branch rather than opening a new one.
- `2026-09-09`: Plan and tracker created under `engine/docs/plans/tool-downloads/`.
