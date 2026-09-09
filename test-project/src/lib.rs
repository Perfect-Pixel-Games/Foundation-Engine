//! Foundation engine internal test fixture.
//!
//! This crate is not a real game. Foundation engine CI builds, runs, and
//! packages it through `foundation-build --project test-project` so engine
//! changes are validated end-to-end without checking out or pinning to the
//! external `template-game` repository's branch state. See
//! `docs/plans/internal-test-project/plan.md` for the decision behind this.

use std::path::PathBuf;

use bevy::{
    asset::AssetPlugin,
    prelude::*,
    render::{
        settings::{Backends, InstanceFlags, RenderCreation, WgpuSettings},
        RenderPlugin,
    },
};
#[cfg(feature = "editor")]
use foundation_editor_library::prelude::*;
use foundation_runtime_library::prelude::*;

/// Foundation game name used by the engine `--game`/`--project` argument.
pub const GAME_NAME: &str = "test-project";

/// Scene key for the fixture's only registered scene.
pub const SMOKE_TEST_SCENE: &str = "smoke_test";

/// Returns the test fixture's asset root.
///
/// Mirrors `template-game`'s asset-root resolution so packaging validates the
/// same environment-variable and packaged-executable lookup order real games use.
pub fn asset_root() -> PathBuf {
    if let Ok(explicit_asset_root) = std::env::var("FOUNDATION_ASSET_ROOT") {
        return PathBuf::from(explicit_asset_root);
    }

    if let Some(packaged_asset_root) = packaged_asset_root() {
        return packaged_asset_root;
    }

    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets")
}

fn packaged_asset_root() -> Option<PathBuf> {
    let executable_directory = std::env::current_exe()
        .ok()
        .and_then(|executable_path| executable_path.parent().map(std::path::Path::to_path_buf))?;
    let packaged_asset_root = executable_directory.join("assets");
    packaged_asset_root.is_dir().then_some(packaged_asset_root)
}

/// Runs the test fixture with Foundation runtime systems installed.
pub fn run() -> AppExit {
    let asset_root = asset_root().to_string_lossy().to_string();
    let editor_enabled =
        cfg!(feature = "editor") && std::env::args().any(|argument| argument == "--editor");

    let mut app = App::new();
    app.insert_resource(ClearColor(Color::BLACK))
        .set_error_handler(bevy::ecs::error::error)
        .add_plugins(test_project_default_plugins(asset_root))
        .add_plugins(FoundationPlugin)
        .add_plugins(TestProjectPlugin)
        .add_systems(Startup, spawn_default_camera);

    add_editor_plugins(&mut app, editor_enabled);

    app.run()
}

fn test_project_default_plugins(asset_root: String) -> impl PluginGroup {
    DefaultPlugins
        .build()
        .set(foundation_log_plugin())
        .disable::<GilrsPlugin>()
        .set(AssetPlugin {
            file_path: asset_root,
            ..default()
        })
        .set(RenderPlugin {
            render_creation: RenderCreation::Automatic(Box::new(WgpuSettings {
                backends: platform_render_backends(),
                instance_flags: InstanceFlags::empty().with_env(),
                ..default()
            })),
            ..default()
        })
}

fn platform_render_backends() -> Option<Backends> {
    #[cfg(target_os = "windows")]
    {
        Some(Backends::from_env().unwrap_or(Backends::VULKAN))
    }

    #[cfg(not(target_os = "windows"))]
    {
        Some(Backends::from_env().unwrap_or(Backends::PRIMARY))
    }
}

fn spawn_default_camera(mut commands: Commands) {
    let camera_order = 100;
    commands.spawn((
        Camera2d,
        Camera {
            order: camera_order,
            ..default()
        },
    ));
}

/// The test fixture's Bevy plugin.
///
/// Registers the fixture's single `.bsn` scene and opens it at startup so
/// engine CI exercises the same scene-registration and asset-loading path
/// real games use, without carrying a full reference game's content.
#[derive(Default)]
pub struct TestProjectPlugin;

impl Plugin for TestProjectPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (register_smoke_test_scene, open_smoke_test_scene).chain(),
        )
        .add_systems(Update, exit_game_on_foundation_exit_request);
    }
}

fn register_smoke_test_scene(mut registry: ResMut<FoundationBsnSceneRegistry>) {
    register_smoke_test_scene_path(&mut registry);
}

fn register_smoke_test_scene_path(registry: &mut FoundationBsnSceneRegistry) {
    registry.register_scene(SMOKE_TEST_SCENE, "scenes/smoke_test.bsn");
}

fn open_smoke_test_scene(mut scene_commands: MessageWriter<SceneCommand>) {
    let smoke_test_scene_source = SceneSource::bsn_scene(SMOKE_TEST_SCENE);
    scene_commands.write(SceneCommand::Clear);
    scene_commands.write(SceneCommand::open(smoke_test_scene_source));
}

fn exit_game_on_foundation_exit_request(
    mut exit_requests: MessageReader<FoundationExitRequested>,
    mut app_exit: MessageWriter<AppExit>,
) {
    for _exit_request in exit_requests.read() {
        app_exit.write(AppExit::Success);
    }
}

#[cfg(feature = "editor")]
fn add_editor_plugins(app: &mut App, editor_enabled: bool) {
    if editor_enabled {
        app.add_plugins(FoundationEditorPlugin);
        app.insert_resource(FoundationEditorMode { enabled: true });
        debug!("Foundation editor mode enabled for TestProject.");
    }
}

#[cfg(not(feature = "editor"))]
fn add_editor_plugins(_app: &mut App, editor_enabled: bool) {
    if editor_enabled {
        warn!("TestProject was built without editor support; ignoring `--editor`.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_name_matches_foundation_launch_argument() {
        assert_eq!(GAME_NAME, "test-project");
    }

    #[test]
    fn smoke_test_scene_key_resolves_to_its_bsn_asset_path() {
        // Exercise the registration system's mapping directly rather than
        // through a full App: driving `FoundationPlugin` with an AssetServer
        // present (needed for the BSN scene registry) also satisfies
        // `FoundationConsolePlugin`'s AssetServer gate for `bevy_feathers`
        // (see `console/mod.rs`), which then expects a full render stack
        // that a MinimalPlugins-only test does not provide.
        let mut registry = FoundationBsnSceneRegistry::default();
        register_smoke_test_scene_path(&mut registry);

        assert_eq!(
            registry.resolve_scene_path(SMOKE_TEST_SCENE),
            "scenes/smoke_test.bsn"
        );
    }
}
