# Foundation Scene System Guide

## Purpose

The Foundation scene system is an ECS-first scene stack for Bevy games. Foundation owns stack state, lifecycle messages, scene ownership, and reusable scene behaviors. Games own concrete scene catalogs.

TemplateGame currently defines scenes in Rust with Bevy 0.19 BSN (`bsn!`). Bevy does not currently ship a first-party `.bsn` asset loader, so BSN scene definitions are code-authored for now. The TemplateGame scene catalog lives under `games/template-game/src/scenes/`, with each splash/menu scene in its own Rust module.

## Architecture

```text
Foundation engine (`cargo run -p foundation -- --game template-game`)
        |
        v
Game extension manifest (`games/template-game/foundation.game.toml`)
        |
        v
FoundationRuntimeLibrary
  - SceneStack
  - SceneCommand messages
  - SceneLoadRequested messages
  - SceneOwner cleanup
        |
        v
TemplateGame scene catalog
  - BSN scene functions
  - scene-key routing
  - game-specific plugin glue
```

`foundation-editor-library` remains as a Bevy-only editor-time extension point. Launching with `--editor` enables that shell:

```cmd
cargo run -p foundation -- --game template-game --editor
```

## Scene Stack

The scene stack is an ordered list of active scenes. The bottom scene is older; the top scene is newest.

```text
Top    [ Pause Menu Overlay ]  visible, focused, blocks gameplay input/update
       [ Gameplay Level     ]  visible, paused by overlay
Bottom [ Main Menu          ]  removed before gameplay in the current flow
```

Systems mutate the stack by writing `SceneCommand` messages. Foundation processes commands, emits lifecycle/load messages, and removes entities tagged with `SceneOwner` when their scene leaves the stack.

## Scene Sources

`SceneSource::bsn_scene("template-game/main_menu")` identifies a BSN scene key. Foundation can now resolve these keys through the temporary `.bsn` asset bridge installed by `FoundationBsnAssetPlugin`.

Games register stable scene keys with `FoundationBsnSceneRegistry`:

```rust
registry.register_scene("last-beacon/main_menu", "scenes/main_menu.bsn");
```

If a key is not registered, Foundation treats the key as a direct asset path relative to the active assets directory. This makes `SceneSource::bsn_scene("levels/intro.bsn")` useful for simple level and prefab loading without a separate catalog.

Non-shipping builds can also use the startup scene override argument. A single value opens one BSN scene instead of the game's default startup flow:

```cmd
--scene last-beacon/main_menu
--scene scenes/main_menu.bsn
```

A bracketed list opens scenes in order as a startup stack. Foundation trims whitespace around commas, so all of these list separators are valid:

```cmd
--scene "[last-beacon/gameplay_level,scenes/testing_mode.bsn]"
--scene "[last-beacon/gameplay_level, scenes/testing_mode.bsn]"
--scene "[last-beacon/gameplay_level , scenes/testing_mode.bsn]"
```

The first override scene clears the stack; later entries are opened above it. If no `--scene` argument is present, games should emit their normal default startup scene commands.

Non-shipping builds can also use the debug console to reopen scenes while the game is already running:

```text
open last-beacon/main_menu
open last-beacon/gameplay_level last-beacon/pause_menu
open scenes/main_menu.bsn
```

The `open` command clears the current scene stack, opens the first scene fresh, and then opens each later scene above it in order. Console predictions for `open` arguments list registered scene keys from `FoundationBsnSceneRegistry`, such as `last-beacon/main_menu`. These predictions update while the user types and can appear before `open` is fully typed, so `op` can already preview full commands such as `open last-beacon/main_menu`. Scene-key prediction searches within registered keys, so `open map` can match a registered key such as `last-beacon/mapmap`. Direct asset-relative `.bsn` paths remain valid when typed explicitly, but they are not predicted unless registered as scene keys.

`SceneSource::runtime(SceneKey::new("debug-overlay"))` remains available for system-authored runtime scenes.

## Scene Presentation

| Presentation | Lower visible? | Lower input? | Lower updates? | Typical use |
| --- | --- | --- | --- | --- |
| `FULLSCREEN` | No | No | No | Splash, main menu, gameplay |
| `PAUSE_OVERLAY` | Yes | No | No | Pause menu |
| `INPUT_BLOCKING_OVERLAY` | Yes | No | Yes | Options/modal menu |
| `NON_BLOCKING_OVERLAY` | Yes | Yes | Yes | Debug overlay |

## Readiness Gating (Scene Visibility)

A scene-owned entity can carry `SceneContentLoading` while its authored content is still loading or applying. `sync_scene_entity_visibility` only reveals a scene-owned root once **both** are true:

- the scene stack marks the scene visible (`ScenePresentation`/covering), and
- no entity that scene owns — the root or any descendant — still carries `SceneContentLoading`.

Foundation's BSN bridge (`bsn_assets.rs`) spawns every scene-owned root `Visibility::Hidden` with `SceneContentLoading` attached, and removes the marker once `ScenePatch::apply` succeeds **or fails** — a broken load must degrade to visible-but-broken content, never hide a scene forever. Games that load their own nested content under a scene root (for example Last Beacon's reusable `.bsn` widgets) should follow the same pattern: attach `SceneContentLoading` to a scene-owned entity while that nested load is pending, and remove it on both success and failure.

There is no generic "readiness token" registration API. A scene's readiness is always just "no `SceneContentLoading` anywhere in its owned subtree" — a plain recursive query, not a separately tracked state machine. Keep it that way; a more general mechanism has been tried before on this project and was the direct cause of a startup livelock (see `docs/plans/async-scene-loading/plan.md`'s Codebase Research section for the full account).

## Scene Load Modes

`OpenSceneOptions::load_mode` (`SceneLoadMode`) controls when a scene open becomes visible:

- **`Streaming`** (default): the scene is pushed onto the stack immediately. Content still obeys the readiness gate above, so it pops in as it loads — this is today's baseline behavior for every existing scene transition unless a call site opts into `Blocking`.
- **`Blocking`**: the stack mutation itself is held until the target's content is ready. Foundation starts loading the target off-stack (tagged with a reserved `SceneOwner { scene_id }`, but not yet part of `SceneStack::entries`) the moment the command is processed, and `advance_pending_scene_transitions` activates it — pushes the stack entry, assigns focus — only once nothing owned by that `scene_id` still carries `SceneContentLoading`. The currently active stack keeps rendering, updating, and accepting input for the entire wait; only the transition is deferred, never the frame loop. A failed load still activates (a failure clears `SceneContentLoading` too), so a broken `Blocking` target degrades to visible-but-broken content instead of hanging the transition forever.

`clear_stack`/`close_current` on a `Blocking` open are applied at activation time, not when the command is queued — otherwise the current scene would go blank while the replacement is still loading.

`FoundationSplashScreen::load_mode` propagates into the `SceneCommand` a splash screen emits on completion, so a splash driver can request `Blocking` for its next-scene handoff.

## Scene Preload Declarations

`ScenePreloadRegistry::register_preloads(owner, targets)` lets a scene declare other scene sources that should start loading when `owner` is added to the stack or refocused (`ScenePreloadTarget::background(...)` / `::blocking(...)`). Registering a target only warms its underlying `.bsn` asset (`AssetServer::load`, kept alive for the session in `ScenePreloadHandles`) — it does **not** spawn scene content or interact with the scene stack. A later `SceneCommand::Open` for the same source still constructs its own fresh entity tree as normal; preloading only removes asset I/O/parse latency from that later transition, not the `ScenePatch::apply` cost itself.

There is no automatic refill after a preloaded target is actually opened — deliberately simple, by design, to avoid the cache-lifecycle bugs a more elaborate "prepared scene cache" produced on this project previously.

`ScenePreloadMode::Blocking` exists in the type today for a future readiness integration but is **not yet wired to anything** — both modes currently behave identically (asset warming only). Revisit only once a concrete scene genuinely needs to block its own readiness on a dependency's load, not just keep it warm.

## Current TemplateGame Flow

```text
Startup
  -> open template-game/splash_pixel_perfect

Pixel Perfect splash completes
  -> template-game/splash_bevy

Bevy splash completes
  -> clear stack and open template-game/main_menu

Main menu
  -> New Game: clear stack and open template-game/gameplay_level
  -> Options: open template-game/options_menu overlay
  -> Exit: request AppExit

Gameplay
  -> Escape: open template-game/pause_menu as pause overlay

Pause menu
  -> Resume: close pause overlay and unpause
  -> Options: open options overlay
  -> Main Menu: clear stack and open main menu
```

## Ownership Rules

Every root entity spawned for a stack scene should receive:

```rust
SceneOwner { scene_id }
```

Foundation cleanup removes owned entities when a scene leaves the stack. Generated UI or gameplay entities should inherit the same owner so they do not leak across scene transitions.

## BSN Asset Authoring Rules

Foundation includes a temporary `.bsn` asset bridge for Bevy 0.19. Bevy currently ships the `bsn!` macro, but not the official file-backed `.bsn` asset loader. Foundation's bridge is intentionally isolated in `foundation-runtime-library` so it can be removed when Bevy provides first-party support.

Use `.bsn` assets for ECS-authored levels and prefabs:

```text
game/assets/scenes/main_menu.bsn
game/assets/scenes/gameplay_level.bsn
game/assets/prefabs/loot_crate.bsn
```

Rules:

- Keep concrete game scene and prefab files in the game asset directory.
- Use `.bsn` for static entity/component hierarchy where practical.
- Keep Rust glue for runtime behavior that cannot live in static assets, such as systems, resources, scene transition drivers, and strongly typed callbacks.
- Register stable scene keys with `FoundationBsnSceneRegistry` when user-facing keys should not expose asset paths.
- Keep reusable loader, spawn, and hot-reload behavior in `foundation-runtime-library`.
- Do not use `.bsn` as a general data format for arbitrary non-ECS content unless a later plan explicitly expands the scope.

### Hot reload behavior

When Bevy reports that a loaded `.bsn` `ScenePatch` changed, Foundation's bridge performs whole-instance replacement:

1. Find live `FoundationBsnInstance` roots using that asset.
2. Despawn each old root recursively, removing authored children with it.
3. Spawn a fresh root from the reloaded `.bsn` asset.
4. Reapply Foundation scene ownership and parent attachment context that belongs to the instance.

Foundation does not attempt in-place diffing or gameplay-state preservation. Entity references into a reloaded prefab or level may become stale after replacement. This is an accepted development-time tradeoff for simple, deterministic hot reload.

Only a genuine on-disk edit to an already-loaded asset triggers this replacement. `AssetEvent::LoadedWithDependencies` never does — it fires on every normal first-time load completion, not just edits, and `apply_pending_bsn_instances` already applies that same load onto the existing entity via its own polling loop. `AssetEvent::Modified` events that fall inside `FoundationBsnSelfResolveSuppression`'s short grace window (500ms) after Foundation's own resolve step are also ignored, since caching the resolved form back onto the asset (`Assets::get_mut`) fires that same event as an unavoidable Bevy side effect, indistinguishable at the event level from a real edit. Without this suppression, every instance would despawn and re-resolve itself shortly after applying, forever.

## Build Modes Direction

Foundation is intended to support:

1. **Static bundled games** for distributed single-executable builds.
2. **Loose game modules** for future development/multi-game engine installs.

The current launcher discovers game manifests and forwards to the selected game package, so the engine does not depend on concrete game crates. Static bundled builds remain a future distribution mode.
