//! FoundationRuntimeLibrary provides reusable Bevy building blocks for Foundation games.
//!
//! The crate is intentionally small at this baseline stage. Foundation supplies shared runtime systems on top of Bevy. Games provide
//! concrete BSN scene catalogs and game-specific plugins.
//!
//! Game crates should add [`FoundationPlugin`] before their game-specific plugin
//! so shared resources, systems, and reflected types are available first.

extern crate self as foundation_runtime_library;

use bevy::prelude::*;
#[cfg(feature = "dev-tools")]
pub use foundation_console_macros::{console_command, ConsoleCommandInput};

pub mod bsn_assets;
#[cfg(feature = "dev-tools")]
pub mod console;
pub mod credits;
mod dynamic_bsn;
mod dynamic_bsn_lexer;
lalrpop_util::lalrpop_mod!(
    #[allow(clippy::vec_init_then_push)]
    dynamic_bsn_grammar
);
pub mod game_settings;
pub mod logging;
pub mod menu;
pub mod scene_stack;
pub mod splash_screen;
pub mod startup_scene;

/// Shared baseline plugin for Foundation games.
///
/// Add this plugin to both standalone game binaries and Foundation engine launches. Future reusable components, resources, and systems should be
/// registered here when they are intended to be available across games.
#[derive(Default)]
pub struct FoundationPlugin;

impl Plugin for FoundationPlugin {
    fn build(&self, app: &mut App) {
        // Register shared gameplay systems before exposing baseline reflected types.
        app.add_plugins((
            scene_stack::FoundationSceneStackPlugin,
            splash_screen::FoundationSplashScreenPlugin,
            menu::FoundationMenuPlugin,
            credits::FoundationCreditsPlugin,
        ))
        // Keep common settings and actors visible to the editor and reflection tests.
        .register_type::<game_settings::FoundationGameSettings>()
        .register_type::<FoundationSettings>()
        .register_type::<FoundationActor>()
        .init_resource::<game_settings::FoundationGameSettings>()
        .init_resource::<FoundationSettings>();

        // The temporary BSN asset bridge needs Bevy's asset infrastructure,
        // which is installed by DefaultPlugins or AssetPlugin before Foundation.
        if app.world().contains_resource::<AssetServer>() {
            app.add_plugins(bsn_assets::FoundationBsnAssetPlugin);
        }

        if cfg!(feature = "dev-tools") {
            self.add_dev_tool_plugins(app);
        }
    }
}

impl FoundationPlugin {
    #[cfg(feature = "dev-tools")]
    fn add_dev_tool_plugins(&self, app: &mut App) {
        // Debug console tooling is intentionally absent from shipping builds.
        app.add_plugins(console::FoundationConsolePlugin);
    }

    #[cfg(not(feature = "dev-tools"))]
    fn add_dev_tool_plugins(&self, _app: &mut App) {
        // Shipping builds compile without Foundation dev tooling.
    }
}

/// Shared baseline settings resource for Foundation games.
///
/// This currently records the library's display name and gives the baseline
/// plugin a concrete, testable effect. Future shared configuration can grow from
/// this resource or move into more focused resources as needed.
#[derive(Clone, Debug, Reflect, Resource)]
#[reflect(Resource)]
pub struct FoundationSettings {
    /// Human-readable library name for diagnostics and editor display.
    pub display_name: String,
}

impl Default for FoundationSettings {
    fn default() -> Self {
        Self {
            display_name: "FoundationRuntimeLibrary".to_string(),
        }
    }
}

/// Baseline shared component for Foundation-authored actors.
///
/// This component demonstrates the pattern reusable FoundationRuntimeLibrary components
/// should follow to be available to both games and Foundation engine launches:
/// derive [`Component`] and [`Reflect`], add Foundation editor metadata, and
/// register the type from [`FoundationPlugin`].
#[derive(Clone, Debug, Default, Component, Reflect)]
#[reflect(Component)]
pub struct FoundationActor {
    /// Optional human-readable actor label for diagnostics and editor display.
    pub label: String,
}

/// Common imports for games using FoundationRuntimeLibrary.
pub mod prelude {
    pub use crate::bsn_assets::{
        propagate_loaded_bsn_scene_owners, FoundationBsnAssetPlugin, FoundationBsnCommandsExt,
        FoundationBsnInstance, FoundationBsnSceneRegistry,
    };
    #[cfg(feature = "dev-tools")]
    pub use crate::console::{
        ConsoleAutocompleteCandidate, ConsoleCommandArguments, ConsoleCommandDescriptor,
        ConsoleCommandError, ConsoleCommandInput, ConsoleCommandParameter, ConsoleCommandResult,
        ConsoleInputs, FoundationConsoleHistory, FoundationConsoleHistorySizeInputs,
        FoundationConsolePlugin, FoundationConsoleRegistry, FoundationConsoleState,
        FOUNDATION_CONSOLE_HISTORY_FILE_NAME, FOUNDATION_CONSOLE_SAVE_DIRECTORY,
        FOUNDATION_CONSOLE_SCENE_KEY, FOUNDATION_OPEN_SCENE_COMMAND_NAME,
    };
    pub use crate::credits::{
        flatten_credits_document, header_font_size_for_depth, CreditPerson, CreditsDisplayRow,
        CreditsDocument, CreditsGroup, FoundationCreditsAssetRoots, FoundationCreditsPlugin,
        FoundationCreditsRoll, FoundationCreditsRuntime, FoundationCreditsRuntimeSettings,
        FoundationGeneratedCreditsUi,
    };
    pub use crate::game_settings::{
        FoundationGameSettings, FoundationGameSettingsIoError, FOUNDATION_GAME_SETTINGS_FILE_NAME,
    };
    pub use crate::logging::{
        foundation_file_logging_enabled, foundation_latest_log_file_path,
        foundation_log_directory_from_executable, foundation_log_plugin,
        foundation_log_window_requested, foundation_log_window_requested_from_environment,
        foundation_should_show_log_window, foundation_unique_crash_log_file_path,
        FoundationLoggingPaths, FOUNDATION_CRASH_LOG_FILE_PREFIX, FOUNDATION_LATEST_LOG_FILE_NAME,
        FOUNDATION_LOG_ARGUMENT, FOUNDATION_LOG_DIRECTORY,
    };
    pub use crate::menu::{
        foundation_is_not_paused, foundation_is_paused, FoundationCloseOnEscape,
        FoundationExitRequested, FoundationGeneratedMenuUi, FoundationMenuButton,
        FoundationMenuPlugin, FoundationMenuRuntimeSettings, FoundationOptionsMenu,
        FoundationPauseOpener, FoundationPauseState, FoundationPlaceholderMenu,
        FoundationResumeOnEscape, FoundationSimpleGameplayLevel, FoundationSpin,
    };
    pub use crate::scene_stack::{
        FoundationSceneStackPlugin, OpenSceneOptions, SceneAdded, SceneCommand, SceneCommandsExt,
        SceneContentLoading, SceneFocused, SceneId, SceneKey, SceneLoadRequested, SceneOwner,
        ScenePresentation, SceneRemoved, SceneRuntimeFlags, SceneSource, SceneStack,
        SceneStackEntry, SceneTarget, SceneUnfocused,
    };
    pub use crate::splash_screen::{
        FoundationSplashRuntimeSettings, FoundationSplashScreen, FoundationSplashScreenPlugin,
        FoundationSplashText, FoundationSplashTimings, FoundationSplashUiParent,
        FoundationSplashUiRoot, FoundationSplashUiTargetCamera,
    };
    pub use crate::startup_scene::{
        startup_scene_commands_or_default, FoundationStartupSceneOverrideError,
        FOUNDATION_STARTUP_SCENE_ARGUMENT,
    };
    #[cfg(feature = "dev-tools")]
    pub use crate::{console_command, ConsoleCommandInput};
    pub use crate::{FoundationActor, FoundationPlugin, FoundationSettings};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foundation_plugin_registers_settings_resource_and_actor_type() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(FoundationPlugin);

        let settings = app.world().resource::<FoundationSettings>();
        assert_eq!(settings.display_name, "FoundationRuntimeLibrary");

        let registry = app
            .world()
            .resource::<bevy::ecs::reflect::AppTypeRegistry>()
            .read();
        assert!(registry.contains(std::any::TypeId::of::<FoundationActor>()));
        assert!(registry.contains(std::any::TypeId::of::<game_settings::FoundationGameSettings>()));
    }

    #[test]
    fn foundation_bsn_authored_components_reflect_default() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(FoundationPlugin);

        let registry = app
            .world()
            .resource::<bevy::ecs::reflect::AppTypeRegistry>()
            .read();

        assert_reflects_default::<splash_screen::FoundationSplashScreen>(&registry);
        assert_reflects_default::<splash_screen::FoundationSplashUiRoot>(&registry);
        assert_reflects_default::<splash_screen::FoundationSplashText>(&registry);
        assert_reflects_default::<menu::FoundationMenuButton>(&registry);
        assert_reflects_default::<menu::FoundationOptionsMenu>(&registry);
        assert_reflects_default::<menu::FoundationPlaceholderMenu>(&registry);
        assert_reflects_default::<menu::FoundationCloseOnEscape>(&registry);
        assert_reflects_default::<menu::FoundationResumeOnEscape>(&registry);
        assert_reflects_default::<menu::FoundationPauseOpener>(&registry);
        assert_reflects_default::<menu::FoundationSimpleGameplayLevel>(&registry);
        assert_reflects_default::<menu::FoundationSpin>(&registry);
        assert_reflects_default::<credits::FoundationCreditsRoll>(&registry);
    }

    fn assert_reflects_default<T: 'static>(registry: &bevy::reflect::TypeRegistry) {
        let type_registration = registry
            .get(std::any::TypeId::of::<T>())
            .expect("Foundation BSN-authored component should be registered");
        assert!(
            type_registration
                .data::<bevy::prelude::ReflectDefault>()
                .is_some(),
            "{} should reflect Default for dynamic BSN loading",
            type_registration.type_info().type_path(),
        );
    }
}
