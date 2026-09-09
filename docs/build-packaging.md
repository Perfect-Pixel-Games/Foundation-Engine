# Foundation Build And Packaging

Foundation games should be built, run, and packaged through the Foundation build tool.

The Foundation repository can still build an in-repo game by name when one exists:

```cmd
scripts\foundation-build.cmd run --game example-game
```

External game projects are the preferred model for real games and reference games:

```cmd
scripts\foundation-build.cmd run --project ..\template-game\game
scripts\foundation-build.cmd package --project ..\template-game\game --platform windows-x64 --configuration test --target game
```

`--project` accepts either a directory containing `foundation.game.toml` or a direct path to a `foundation.game.toml` file. Relative paths resolve from the caller's current directory, so game repositories can call the engine script from their own wrapper scripts.

## Internal Test Fixture

`engine/test-project/` is a minimal Foundation game project that lives inside the engine repository itself, built the same way as an external project:

```cmd
scripts\foundation-build.cmd package --project test-project --platform windows-x64 --configuration test --target game
```

It exists only so engine CI and local validation can exercise a full build/package/asset-loading flow without checking out or depending on the external `template-game` repository's branch state. It is not a reference game; keep it deliberately small and do not add real game content to it. Real games and reference games should keep using their own repository with `--project <path>` as shown above.

## Build Configurations

| Configuration | Meaning | Dev tools | Editor target | Typical use |
| --- | --- | --- | --- | --- |
| `debug` | Full debugging with no optimization-focused profile. | Enabled | Allowed | Local debugging |
| `test` | Optimized development/QA build with diagnostics. | Enabled | Allowed | Internal test builds |
| `shipping` | Store/share build containing only the game and assets. | Disabled | Not allowed | Public distribution |

`shipping` builds pass `--no-default-features` to the game crate and use the `foundation-shipping` Cargo profile. This is the configuration that must exclude console commands, log windows, editor plugins, and other dev-only systems.

## Targets

- `game`: builds the standalone game.
- `game-editor`: builds the game with Foundation editor support. This target is rejected for `shipping`.

## Platforms

Initial aliases:

| Alias | Rust target triple |
| --- | --- |
| `windows-x64` | `x86_64-pc-windows-msvc` |
| `linux-x64` | `x86_64-unknown-linux-gnu` |

A Rust target triple can also be passed directly. If `--platform` is omitted, Foundation defaults to the current host platform when it has a known alias.

## Defaults

If omitted, the build tool uses:

- `--configuration test`
- `--target game`
- `--platform` matching the current host, currently `windows-x64` on x86_64 Windows or `linux-x64` on x86_64 Linux

## External Game Layout

The recommended game repository layout is:

```text
template-game/
  engine/   # Foundation submodule, checkout, junction, or symlink
  game/     # Cargo package, source, foundation.game.toml, and game assets
  scripts/  # Game-facing wrappers around engine/scripts/foundation-build.cmd
```

Game-owned assets live under `game/assets`. Foundation-owned assets may live under `engine/assets`. When packaging, game asset roots are copied as configured by the game manifest, and engine assets are copied to `assets/engine` when present.

## Game Manifest Metadata

Each game declares build/package metadata in `foundation.game.toml`:

```toml
[game]
name = "template-game"

[launch]
package = "template-game"

[package]
executable-name = "template-game"
asset-roots = ["assets"]
```

- `game.name`: Foundation game identifier used in output paths and metadata.
- `launch.package`: Cargo package/executable to build.
- `package.executable-name`: file name used for the packaged executable.
- `package.asset-roots`: game-relative directories copied into the package.

## Local Examples

```cmd
scripts\foundation-build.cmd run --project ..\template-game\game
scripts\foundation-build.cmd run --project ..\template-game\game --platform windows-x64 --configuration debug --target game-editor
scripts\foundation-build.cmd build --project ..\template-game\game --platform windows-x64 --configuration debug --target game
scripts\foundation-build.cmd package --project ..\template-game\game --platform windows-x64 --configuration test --target game-editor
scripts\foundation-build.cmd package --project ..\template-game\game --platform linux-x64 --configuration shipping --target game
```

Runtime arguments can be passed after `--`:

```cmd
scripts\foundation-build.cmd run --project ..\template-game\game --platform windows-x64 --configuration debug --target game -- --some-game-argument
```

Non-shipping runs can bypass the game's default startup scene with a registered BSN scene key, direct asset path, or bracketed ordered stack. Quote bracketed lists when spaces around commas would otherwise be split by the shell:

```cmd
scripts\foundation-build.cmd run --project ..\template-game\game --configuration debug --target game -- --scene template-game/main_menu
scripts\foundation-build.cmd run --project ..\template-game\game --configuration debug --target game -- --scene "[template-game/gameplay_level, scenes/testing_mode.bsn]"
```

`shipping` builds disable Foundation dev tools, so startup scene override arguments are ignored by games that use Foundation's helper.

`game-editor` runs automatically pass `--editor` to the game executable.

Invalid example:

```cmd
scripts\foundation-build.cmd package --project ..\template-game\game --platform windows-x64 --configuration shipping --target game-editor
```

## Package Layout

Packages are written under:

```text
artifacts/packages/<game>/<platform>/<configuration>/<target>/
```

A package contains:

- the game executable named from `package.executable-name`,
- configured game asset roots such as `assets/`,
- engine assets under `assets/engine` when present,
- `foundation.game.toml`,
- generated `foundation.package.toml`,
- a `.tar.gz` archive next to the package directory.

## CI Usage

GitHub workflows call the same local command as developers. Self-hosted runners need:

- Rust toolchain installed,
- target triples installed through `rustup target add`,
- platform linkers and SDKs required by the selected targets,
- `tar` available for archive creation.

The Foundation workflow validates the engine workspace and packages the internal `test-project` fixture (`engine/test-project/`, see `docs/plans/internal-test-project/plan.md`) on pushes and pull requests for `dev` and `main`. Building against an in-repo fixture instead of checking out the external `template-game` repository means engine CI never depends on another repository's branch state. Foundation no longer publishes GitHub Releases because game packages belong to game repositories. It still creates version tags on protected branch pushes after validation and packaging pass. Package artifacts remain available from workflow runs as integration evidence.

Linux runner jobs are currently disabled because no Linux self-hosted runner is available. The workflow can be expanded back to a Windows/Linux matrix when a Linux runner is online.
