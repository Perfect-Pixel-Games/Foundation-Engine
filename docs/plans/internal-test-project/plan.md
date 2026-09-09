# Internal Test Project Plan

## Metadata
- Feature slug: `internal-test-project`
- Feature area: `engine`
- Primary area: `engine`
- Branch: `feature/internal-test-project`
- Status: `Planned`
- Planning model: `gpt-5.5`
- Implementation model: `gpt-5.4`
- Review model: `gpt-5.5`
- Created: `2026-09-09`
- Last updated: `2026-09-09`

## User Request
The user wants to stop using the external `template-game` repository as the engine's CI build/package target. Complex engine changes sometimes break `template-game` or `last-beacon` in ways that are hard to propagate a fix for, because the engine's own CI is coupled to another repository's `dev` branch state. Instead, add a nested `test-project` directory inside the Foundation engine repository itself, update `foundation-build.yml` to build/package against that internal project, and remove the `template-game` checkout/junction steps from the engine workflow. The engine must remain a git submodule consumed by `template-game` and `last-beacon`; only the engine's own CI stops depending on `template-game`.

## Feature Summary
Add `engine/test-project/` as a self-contained Foundation game project (its own `Cargo.toml`, `foundation.game.toml`, `src/`, `assets/`, `tests/`) that exists purely to exercise engine features in CI and local validation. Point `foundation-build.yml` at it via the existing `foundation-build -- package --project <path>` mechanism instead of checking out and junction-linking the external `template-game` repository. This decouples engine CI from `template-game`'s branch state while still validating a full game build/package/asset flow on every push and pull request.

## Feature Area Classification
- Area: `engine`
- Primary area: `engine`
- Rationale: This is purely engine-repository tooling/CI and a new internal fixture project; it does not change `template-game` or `last-beacon` beyond continuing to consume `engine` as a submodule exactly as before.

## Codebase Research
- `engine/.github/workflows/foundation-build.yml` currently checks out `Perfect-Pixel-Games/template-game@dev` into `external/template-game`, junction-links the current engine checkout into `external/template-game/engine` (`mklink /J`), then runs `cargo run -p foundation-build -- package --project "%FOUNDATION_GAME_PROJECT%" ...` where `FOUNDATION_GAME_PROJECT=external\template-game\game` and `FOUNDATION_GAME=template-game`. The "Stage release assets" step copies `artifacts\packages\%FOUNDATION_GAME%\windows-x64\<configuration>\*.tar.gz`.
- `crates/foundation-build/src/lib.rs` already fully supports building an out-of-workspace project via `--project <path>` (`find_external_game_project`): it resolves/canonicalizes the manifest path, requires a sibling `Cargo.toml`, and treats the project as a standalone Cargo manifest (`--manifest-path`, not `-p` in the engine workspace), using `<project>/target` as the default Cargo target directory (overridable by the `CARGO_TARGET_DIR` env var, which the workflow already sets to a fixed self-hosted-runner path). `engine_root` is found by walking up from the `foundation-build` crate's own `CARGO_MANIFEST_DIR`, so it always resolves to `engine/` regardless of where `--project` points. **No changes to `foundation-build` are required** — this feature only adds a new project directory and repoints the workflow.
- `engine/Cargo.toml` workspace `members` currently lists only `crates/*`; there is no in-workspace `games/` member, matching the `--project` (external, non-member) build path already used for `template-game`.
- `/e/GameDev/template-game/game/` (sibling checkout on this machine) is the concrete shape to mirror: `Cargo.toml` (package with an empty `[workspace]` table so Cargo does not try to unify it with a parent workspace, dependencies on `foundation-runtime-library`/`foundation-editor-library` via relative path, `dev-tools`/`editor` features, `foundation-test`/`foundation-shipping` profiles matching `engine/Cargo.toml`'s profile names), `foundation.game.toml` (`[game].name`, `[launch].package`, `[package].executable-name`/`asset-roots`), `assets/` (fonts, `.bsn` scenes, `credits.json`), `src/lib.rs` (Bevy `App` setup, `FoundationPlugin`, asset-root resolution, a couple of example console commands and a `SpinningCube` component/system, unit tests), `src/main.rs` (thin binary wrapper), `tests/bsn_asset_flow.rs`. Because `test-project` lives one level *inside* `engine/` (not as a sibling repo), its relative dependency paths are `../crates/...` instead of template-game's `../engine/crates/...`.
- `engine/docs/build-packaging.md` documents the `--project` flow and states "packages the external TemplateGame reference project on pushes and pull requests" — needs updating to describe the internal `test-project` instead.
- `engine/docs/branch-protection.md` lists required status check names (`Main source branch policy`, `Validate workspace on Windows`, `Package windows-x64 test on Windows`, `Package windows-x64 shipping on Windows`) — none of these names reference `template-game`, so branch-protection required-check configuration does not need to change.
- `engine/docs/plans/external-template-game/plan.md` (2026-07-15) previously made the deliberate decision to move `template-game` out of the engine repo entirely and validate the engine only against the external repo; its "Open Questions" section explicitly asked "Should Foundation keep a tiny internal smoke-test game after moving TemplateGame out?" and left it unresolved. This feature answers that question: yes, for CI decoupling reasons.
- `engine/docs/plans/foundation-build-packaging/plan.md` (2026-07-15) defined the `Debug`/`Test`/`Shipping` configuration and `Game`/`GameEditor` target vocabulary that `test-project` must remain compatible with (in particular: shipping excludes `dev-tools`/`editor` features, and `Shipping + GameEditor` is rejected).

## External Research
No external online research was performed because this is repository-internal restructuring using an already-implemented `foundation-build --project` code path; no new third-party APIs or libraries are involved.

## Affected Files And Systems
- `engine/test-project/` (new): `Cargo.toml`, `foundation.game.toml`, `src/lib.rs`, `src/main.rs`, `assets/` (minimal fixture assets), `tests/` — a new self-contained Foundation game project used only for engine CI/local validation.
- `engine/.github/workflows/foundation-build.yml`: remove the "Check out TemplateGame reference project" and "Use current Foundation checkout as TemplateGame engine" (`mklink /J`) steps; change `FOUNDATION_GAME`/`FOUNDATION_GAME_PROJECT` env vars to point at `test-project`; the "Package Foundation game" and "Stage release assets" steps otherwise stay structurally the same.
- `engine/docs/build-packaging.md`: update the "External Game Layout" / "CI Usage" prose that currently names `template-game` as the CI target, and keep `template-game` only as a *usage example* of the general `--project` flow for real downstream games.
- `engine/docs/plans/external-template-game/plan.md`: not edited by this feature (historical record), but this plan's research section above cross-references and resolves its open question.
- No changes expected to `crates/foundation-build`, `crates/foundation`, `crates/foundation-runtime-library`, or `crates/foundation-editor-library`, since `--project` already does everything needed.
- No changes to `last-beacon` or `template-game` repositories: both continue to consume `engine` as an unmodified git submodule.

## Proposed Implementation Approach
1. Scaffold `engine/test-project/` mirroring the shape of `template-game/game/`, adapted for its new nested location:
   - `Cargo.toml`: package name `test-project` (or similar), empty `[workspace]` table, `dev-tools`/`editor` features, dependencies on `foundation-runtime-library`/`foundation-editor-library` via `../crates/...` relative paths, and the same `foundation-test`/`foundation-shipping` profile names used by `engine/Cargo.toml` (`profile.*` sections must be duplicated locally since the project is not a workspace member and cannot inherit them).
   - `foundation.game.toml`: `[game].name`, `[launch].package`, `[package].executable-name`, `[package].asset-roots = ["assets"]`.
   - `src/lib.rs` / `src/main.rs`: minimal Bevy app wiring `FoundationPlugin`, enough scene/asset/console-command surface to exercise the engine features the test suite cares about (can start as a trimmed-down copy of `template-game`'s wiring rather than a byte-for-byte copy).
   - `assets/`: minimal fixture assets (at least one `.bsn` scene) sufficient for a packaging smoke test; avoid copying `template-game`'s real game content.
   - `tests/`: a basic asset/scene-flow smoke test analogous to `template-game/game/tests/bsn_asset_flow.rs`.
2. Update `engine/.github/workflows/foundation-build.yml`:
   - Remove the "Check out TemplateGame reference project" `actions/checkout` step and the "Use current Foundation checkout as TemplateGame engine" `mklink /J` step from the `package` job.
   - Change `env.FOUNDATION_GAME` to the new project's game name and `env.FOUNDATION_GAME_PROJECT` to `test-project` (relative to the engine repo root, which is the checkout root in this workflow).
   - Leave the rest of the `package` job (build/package invocation, artifact staging/upload, `dev-tag`/`main-tag` jobs) structurally unchanged.
3. Update `engine/docs/build-packaging.md` to describe `test-project` as the engine's internal CI/validation fixture, keeping `template-game` in the doc only as an example of using `--project` against a real external game.
4. Validate locally: run `scripts\foundation-build.cmd package --project test-project --platform windows-x64 --configuration test --target game` (and `shipping`) from the engine root, confirming a package directory and archive are produced without any external checkout.
5. Run full engine validation (`scripts\validate-project.cmd`) and push the branch; open a PR into `dev` and confirm the `Foundation Build` workflow's `validate` and `package` jobs pass without checking out `template-game`.

## Alternatives Considered
- Keep testing against `template-game` but pin it to a specific tag/commit instead of `dev`: rejected because it still requires maintaining a second repository in lockstep and does not solve the "breaks all three projects" propagation problem the user described.
- Add the test fixture as an in-workspace `games/` member again (reverting the `external-template-game` decision) instead of using `--project`: rejected because `--project` already exists, is exercised by existing tests, and keeping the fixture out of the workspace keeps engine-only `cargo` commands (`clippy --workspace`, etc.) from being affected by fixture-only code.
- Byte-for-byte copy `template-game/game` into `engine/test-project`: rejected as unnecessarily heavy; the internal fixture only needs to be representative enough to catch integration breakage, not a full reference game.

## Risks, Constraints, And Assumptions
- Assumes `foundation-build --project` behavior is stable as researched above; no engine crate changes are planned, so this is low-risk from a code-behavior standpoint.
- `test-project`'s `Cargo.toml` must duplicate the `foundation-test`/`foundation-shipping` profile definitions from `engine/Cargo.toml` (profiles are workspace-level and are not inherited by an out-of-workspace manifest); if the engine's profiles change, `test-project`'s copies must be kept in sync manually. Document this coupling clearly in a comment.
- Removing the `template-game` checkout from CI means engine CI no longer catches breakage in `template-game` itself; `template-game`'s own CI (checking out `engine` as a submodule) remains the place that catches that, so this is an intentional decoupling, not a coverage loss for the engine's own correctness.
- Assumes the self-hosted Windows runner has no leftover `external/template-game` state from previous runs that could mask removal issues; safe to ignore since the checkout step is simply deleted.

## Open Questions
- None outstanding; resolves the open question left by `docs/plans/external-template-game/plan.md` about whether Foundation should keep an internal smoke-test game.

## Documentation Expectations
- `test-project` is a test/CI fixture, not a public API; no Rustdoc requirements beyond normal project code-comment standards (`rust-coding-standards`).
- Update `docs/build-packaging.md` so its CI-usage description matches the new internal fixture.
- Generated documentation (`cargo doc --workspace --all-features --no-deps`) is unaffected since `test-project` is not a workspace member.

## Implementation Handoff Notes
- Use `gpt-5.4` for implementation.
- Never use Anthropic models.
- Confirm the active engine branch is `feature/internal-test-project`, created from `dev` (verified at plan time: local `HEAD` matched `origin/dev` before branching).
- Do not modify `crates/foundation-build` or other engine crates; this feature is scaffolding + workflow only.
- Keep `test-project` deliberately small; it is a fixture, not a second reference game.
- Preserve `foundation-test`/`foundation-shipping` profile parity with `engine/Cargo.toml` and leave a comment noting the duplication reason.

## Optional Review Focus Areas
- Use `gpt-5.5` for review.
- Verify the workflow no longer references `external/template-game` or `mklink` anywhere.
- Verify `scripts\foundation-build.cmd package --project test-project ...` succeeds locally for both `test` and `shipping` configurations without network/external-repo access.
- Verify `docs/build-packaging.md` no longer implies the engine's CI depends on the external `template-game` repository.

## Success Criteria
- `engine/test-project/` builds, runs, and packages successfully via `foundation-build --project test-project` for both `test` and `shipping` configurations.
- `foundation-build.yml` no longer checks out or junction-links `template-game`; `FOUNDATION_GAME`/`FOUNDATION_GAME_PROJECT` point at `test-project`.
- A pull request against `dev` shows the `Foundation Build` workflow's `validate` and `package` jobs passing using only the internal fixture.
- `template-game` and `last-beacon` continue to consume `engine` as an unmodified git submodule; nothing in either of those repositories needs to change.

## Testing Methodology
- `scripts/format-project.cmd`
- `scripts/lint-project.cmd`
- `scripts/test-project.cmd`
- `scripts/compile-project.cmd`
- `scripts/doc-project.cmd`
- `scripts/validate-project.cmd`
- Local: `scripts\foundation-build.cmd package --project test-project --platform windows-x64 --configuration test --target game`
- Local: `scripts\foundation-build.cmd package --project test-project --platform windows-x64 --configuration shipping --target game`
- GitHub PR workflow run against `dev` confirming `validate` and `package` jobs pass without an external checkout.
