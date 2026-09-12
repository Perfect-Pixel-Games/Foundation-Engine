//! Foundation debug console command registry and runtime state.
//!
//! The console command registry is assembled from crates linked into the running
//! game binary. Foundation and the selected game can contribute commands; game
//! crates that are not compiled into the current binary cannot contribute
//! command descriptors.

use std::{
    collections::BTreeMap,
    fmt, fs,
    path::{Path, PathBuf},
};

use crate::{
    bsn_assets::FoundationBsnSceneRegistry,
    scene_stack::{
        OpenSceneOptions, SceneAdded, SceneCommand, SceneId, SceneKey, SceneLoadRequested,
        SceneOwner, ScenePresentation, SceneRemoved, SceneSource, SceneTarget,
    },
};
use bevy::{
    prelude::*,
    text::{EditableText, TextCursorStyle, TextEdit, TextLayout},
};
use bevy_enhanced_input::prelude::*;
use bevy_feathers::FeathersPlugins;
use bevy_input_focus::{
    tab_navigation::{TabGroup, TabIndex},
    AutoFocus, FocusCause, InputFocus,
};
use linkme::distributed_slice;
use serde::{Deserialize, Serialize};

#[doc(hidden)]
pub mod __private {
    pub use linkme;
}

/// Directory used for persistent console files relative to the game process.
pub const FOUNDATION_CONSOLE_SAVE_DIRECTORY: &str = "saved/console";

/// File name used for persisted command history.
pub const FOUNDATION_CONSOLE_HISTORY_FILE_NAME: &str = "history.json";

/// Scene-stack key used by the debug console overlay scene.
pub const FOUNDATION_CONSOLE_SCENE_KEY: &str = "foundation/debug-console";

/// Built-in console command that opens one or more BSN scenes by key or asset path.
pub const FOUNDATION_OPEN_SCENE_COMMAND_NAME: &str = "open";

/// All console commands linked into the current game binary.
#[distributed_slice]
pub static FOUNDATION_CONSOLE_COMMANDS: [ConsoleCommandDescriptor] = [..];

/// Plugin that installs Foundation debug console resources and systems.
#[derive(Default)]
pub struct FoundationConsolePlugin;

impl Plugin for FoundationConsolePlugin {
    fn build(&self, app: &mut App) {
        if app.world().contains_resource::<AssetServer>()
            && !app.is_plugin_added::<bevy_feathers::FeathersCorePlugin>()
        {
            app.add_plugins(FeathersPlugins);
        }

        // Enhanced input must exist before `add_input_context` runs, and this
        // plugin is also used on its own in tests that skip `FoundationPlugin`.
        crate::add_enhanced_input_plugin_if_missing(app);

        app.add_input_context::<FoundationConsoleControlsInput>()
            .add_systems(Startup, spawn_foundation_console_controls_input_context)
            .init_resource::<FoundationConsoleState>()
            .insert_resource(FoundationConsoleHistory::load_from_disk())
            .init_resource::<FoundationConsoleRegistry>()
            .init_resource::<FoundationConsoleUiState>()
            .register_type::<FoundationConsoleRoot>()
            .register_type::<FoundationConsoleInput>()
            .register_type::<FoundationConsoleOutputViewport>()
            .register_type::<FoundationConsoleOutput>()
            .register_type::<FoundationConsoleSuggestion>()
            .register_type::<FoundationConsoleSuggestionOption>()
            .register_type::<FoundationConsoleHistoryItem>()
            .register_type::<FoundationConsoleOutputLine>()
            .add_systems(
                Update,
                (
                    toggle_console_scene,
                    spawn_console_scene_from_stack_request,
                    track_console_scene_added,
                    track_console_scene_removed,
                    update_console_input_state,
                    handle_console_keyboard_actions,
                    refresh_console_text_nodes,
                    handle_clickable_console_reuse,
                    scroll_console_output,
                )
                    .chain(),
            );
    }
}

/// Reusable input context for Foundation debug-console control keys and
/// scrolling.
///
/// Named `*ControlsInput` (not `*Input`) to avoid colliding with
/// [`FoundationConsoleInput`], the pre-existing marker component for the
/// console's editable text-input entity.
///
/// Spawned once by [`spawn_foundation_console_controls_input_context`]; games
/// never need to spawn or configure this themselves.
#[derive(Component, Default)]
pub struct FoundationConsoleControlsInput;

/// Opens/closes the debug console overlay. Bound to the backquote/backtick key.
#[derive(InputAction)]
#[action_output(bool)]
pub struct FoundationConsoleToggle;

/// Closes the debug console overlay from within [`handle_console_keyboard_actions`].
///
/// Bound to the same physical `Escape` key as [`crate::menu::FoundationMenuBack`]
/// and [`crate::splash_screen::FoundationSplashScreenSkip`]; `consume_input`
/// defaults to `false` in `bevy_enhanced_input`, so all three keep triggering
/// independently from a single Escape press.
#[derive(InputAction)]
#[action_output(bool)]
pub struct FoundationConsoleClose;

/// Autocompletes the current console input. Bound to Tab.
#[derive(InputAction)]
#[action_output(bool)]
pub struct FoundationConsoleAutocomplete;

/// Recalls the previous console history entry. Bound to the up arrow.
#[derive(InputAction)]
#[action_output(bool)]
pub struct FoundationConsoleHistoryPrevious;

/// Recalls the next console history entry. Bound to the down arrow.
#[derive(InputAction)]
#[action_output(bool)]
pub struct FoundationConsoleHistoryNext;

/// Submits the current console input as a command. Bound to Enter.
#[derive(InputAction)]
#[action_output(bool)]
pub struct FoundationConsoleSubmit;

/// Scrolls the console output/prediction popup. Bound to the mouse wheel.
///
/// `bevy_enhanced_input` normalizes pixel-unit wheel deltas into line-unit
/// equivalents itself (see `Binding::MouseWheel` in the crate source), using a
/// different constant than this module's previous hand-rolled
/// `MouseScrollUnit::Pixel` multiplier. Scroll sensitivity for pixel-based
/// input devices may feel slightly different as a result; line-unit devices
/// (the common case) are unaffected.
#[derive(InputAction)]
#[action_output(Vec2)]
pub struct FoundationConsoleScroll;

fn spawn_foundation_console_controls_input_context(mut commands: Commands) {
    commands.spawn((
        FoundationConsoleControlsInput,
        actions!(FoundationConsoleControlsInput[
            (
                Action::<FoundationConsoleToggle>::new(),
                bindings![KeyCode::Backquote],
            ),
            (
                Action::<FoundationConsoleClose>::new(),
                bindings![KeyCode::Escape],
            ),
            (
                Action::<FoundationConsoleAutocomplete>::new(),
                bindings![KeyCode::Tab],
            ),
            (
                Action::<FoundationConsoleHistoryPrevious>::new(),
                bindings![KeyCode::ArrowUp],
            ),
            (
                Action::<FoundationConsoleHistoryNext>::new(),
                bindings![KeyCode::ArrowDown],
            ),
            (
                Action::<FoundationConsoleSubmit>::new(),
                bindings![KeyCode::Enter],
            ),
            (
                Action::<FoundationConsoleScroll>::new(),
                bindings![Binding::mouse_wheel()],
            ),
        ]),
    ));
}

/// Runtime open/closed state for the Foundation debug console.
#[derive(Clone, Debug, Default, Resource)]
pub struct FoundationConsoleState {
    /// Whether the debug console scene is currently open.
    pub is_open: bool,
    /// Current scene-stack entry that owns the console UI, when open.
    pub scene_id: Option<SceneId>,
}

impl FoundationConsoleState {
    fn mark_open(&mut self, scene_id: SceneId) {
        self.is_open = true;
        self.scene_id = Some(scene_id);
    }

    fn mark_closed_if_scene_matches(&mut self, removed_scene_id: SceneId) -> bool {
        if self.scene_id != Some(removed_scene_id) {
            return false;
        }

        self.is_open = false;
        self.scene_id = None;
        true
    }
}

/// Runtime UI state for the Foundation debug console.
#[derive(Clone, Debug, Default, Resource)]
pub struct FoundationConsoleUiState {
    /// Current editable command line.
    pub input: String,
    /// Console output and status lines displayed above the input.
    pub output_lines: Vec<String>,
    /// Current history cursor used by Up/Down navigation.
    pub history_cursor: Option<usize>,
    /// Number of rendered output lines to scroll back from the newest output.
    pub output_scroll_lines: usize,
}

/// Root component for the generated Foundation debug console UI.
#[derive(Clone, Copy, Debug, Default, Component, Reflect)]
#[reflect(Component)]
pub struct FoundationConsoleRoot;

/// Marker component for the editable console input entity.
#[derive(Clone, Copy, Debug, Default, Component, Reflect)]
#[reflect(Component)]
pub struct FoundationConsoleInput;

/// Marker component for the scrollable console history/output viewport.
#[derive(Clone, Copy, Debug, Default, Component, Reflect)]
#[reflect(Component)]
pub struct FoundationConsoleOutputViewport;

/// Marker component for the console history and output text entity.
#[derive(Clone, Copy, Debug, Default, Component, Reflect)]
#[reflect(Component)]
pub struct FoundationConsoleOutput;

/// Marker component for the console autocomplete suggestion list entity.
#[derive(Clone, Copy, Debug, Default, Component, Reflect)]
#[reflect(Component)]
pub struct FoundationConsoleSuggestion;

/// Clickable autocomplete suggestion that can replace the current console input.
#[derive(Clone, Debug, Component, Reflect)]
#[reflect(Component)]
pub struct FoundationConsoleSuggestionOption {
    /// Text inserted into the console input when clicked.
    pub replacement: String,
}

/// Clickable command history item that can replace the current console input.
#[derive(Clone, Debug, Component, Reflect)]
#[reflect(Component)]
pub struct FoundationConsoleHistoryItem {
    /// Command line inserted into the console input when clicked.
    pub command_line: String,
}

/// Marker for generated console output rows rebuilt from command output history.
#[derive(Clone, Copy, Debug, Default, Component, Reflect)]
#[reflect(Component)]
pub struct FoundationConsoleOutputLine;

/// Persisted command history for the Foundation debug console.
#[derive(Clone, Debug, Default, Resource, Serialize, Deserialize)]
pub struct FoundationConsoleHistory {
    /// Commands executed by the user, oldest first.
    pub commands: Vec<String>,
}

impl FoundationConsoleHistory {
    /// Returns the path used for persisted command history.
    pub fn history_file_path() -> PathBuf {
        history_file_path_for_executable(std::env::current_exe().ok().as_deref())
    }

    /// Loads persisted command history from the default history path.
    pub fn load_from_disk() -> Self {
        match Self::load_from_path(Self::history_file_path()) {
            Ok(console_history) => console_history,
            Err(load_error) => {
                warn!("Failed to load Foundation console history: {load_error}");
                Self::default()
            }
        }
    }

    /// Saves command history to the default history path.
    pub fn save_to_disk(&self) -> Result<(), String> {
        self.save_to_path(Self::history_file_path())
    }

    /// Adds a non-empty command line to history.
    pub fn push_command(&mut self, command_line: impl Into<String>) {
        let command_line = command_line.into();
        if !command_line.trim().is_empty() {
            self.commands.push(command_line);
        }
    }

    fn load_from_path(history_file_path: impl AsRef<Path>) -> Result<Self, String> {
        let history_file_path = history_file_path.as_ref();
        if !history_file_path.exists() {
            return Ok(Self::default());
        }

        let history_document = fs::read_to_string(history_file_path).map_err(|io_error| {
            format!("could not read {}: {io_error}", history_file_path.display())
        })?;
        serde_json::from_str(&history_document).map_err(|parse_error| {
            format!(
                "could not parse {}: {parse_error}",
                history_file_path.display()
            )
        })
    }

    fn save_to_path(&self, history_file_path: impl AsRef<Path>) -> Result<(), String> {
        let history_file_path = history_file_path.as_ref();
        if let Some(history_directory_path) = history_file_path.parent() {
            fs::create_dir_all(history_directory_path).map_err(|io_error| {
                format!(
                    "could not create {}: {io_error}",
                    history_directory_path.display()
                )
            })?;
        }

        let history_document = serde_json::to_string_pretty(self)
            .map_err(|serialize_error| format!("could not serialize history: {serialize_error}"))?;
        fs::write(history_file_path, history_document).map_err(|io_error| {
            format!(
                "could not write {}: {io_error}",
                history_file_path.display()
            )
        })
    }
}

fn history_file_path_for_executable(executable_path: Option<&Path>) -> PathBuf {
    let history_root_path = executable_path
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    history_root_path
        .join(FOUNDATION_CONSOLE_SAVE_DIRECTORY)
        .join(FOUNDATION_CONSOLE_HISTORY_FILE_NAME)
}

fn toggle_console_scene(
    console_toggle_action: Single<&ActionEvents, With<Action<FoundationConsoleToggle>>>,
    console_state: Res<FoundationConsoleState>,
    mut scene_commands: MessageWriter<SceneCommand>,
) {
    if !console_toggle_action.contains(ActionEvents::START) {
        return;
    }

    let console_scene_key = SceneKey::new(FOUNDATION_CONSOLE_SCENE_KEY);
    if console_state.is_open {
        scene_commands.write(SceneCommand::Close(SceneTarget::Key(console_scene_key)));
    } else {
        let console_scene_source = SceneSource::runtime(console_scene_key.clone());
        let console_scene_options = OpenSceneOptions::default()
            .with_key(console_scene_key)
            .with_presentation(ScenePresentation::INPUT_BLOCKING_OVERLAY);
        scene_commands.write(SceneCommand::open_with_options(
            console_scene_source,
            console_scene_options,
        ));
    }
}

fn spawn_console_scene_from_stack_request(
    mut commands: Commands,
    mut scene_load_requests: MessageReader<SceneLoadRequested>,
    console_history: Res<FoundationConsoleHistory>,
    console_ui_state: Res<FoundationConsoleUiState>,
    mut input_focus: ResMut<InputFocus>,
) {
    for scene_load_request in scene_load_requests.read() {
        if !is_console_scene_source(&scene_load_request.source) {
            continue;
        }

        let input_entity = spawn_console_overlay(
            &mut commands,
            scene_load_request.scene_id,
            &console_history,
            &console_ui_state,
        );
        input_focus.set(input_entity, FocusCause::Navigated);
    }
}

fn track_console_scene_added(
    mut scene_added_messages: MessageReader<SceneAdded>,
    scene_stack: Res<crate::scene_stack::SceneStack>,
    mut console_state: ResMut<FoundationConsoleState>,
) {
    for scene_added_message in scene_added_messages.read() {
        let Some(scene_entry) = scene_stack.get(scene_added_message.scene_id) else {
            continue;
        };
        if is_console_scene_source(&scene_entry.source) {
            console_state.mark_open(scene_added_message.scene_id);
        }
    }
}

fn track_console_scene_removed(
    mut scene_removed_messages: MessageReader<SceneRemoved>,
    mut console_state: ResMut<FoundationConsoleState>,
    mut input_focus: ResMut<InputFocus>,
) {
    for scene_removed_message in scene_removed_messages.read() {
        if console_state.mark_closed_if_scene_matches(scene_removed_message.scene_id) {
            input_focus.clear();
        }
    }
}

fn is_console_scene_source(scene_source: &SceneSource) -> bool {
    matches!(
        scene_source,
        SceneSource::Runtime { key } if key.0 == FOUNDATION_CONSOLE_SCENE_KEY
    )
}

fn update_console_input_state(
    mut console_ui_state: ResMut<FoundationConsoleUiState>,
    console_inputs: Query<&EditableText, (With<FoundationConsoleInput>, Changed<EditableText>)>,
) {
    for editable_text in &console_inputs {
        let changed_input = editable_text_value(editable_text);
        if changed_input != console_ui_state.input {
            console_ui_state.input = changed_input;
            console_ui_state.history_cursor = None;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_console_keyboard_actions(
    close_action: Single<&ActionEvents, With<Action<FoundationConsoleClose>>>,
    autocomplete_action: Single<&ActionEvents, With<Action<FoundationConsoleAutocomplete>>>,
    history_previous_action: Single<&ActionEvents, With<Action<FoundationConsoleHistoryPrevious>>>,
    history_next_action: Single<&ActionEvents, With<Action<FoundationConsoleHistoryNext>>>,
    submit_action: Single<&ActionEvents, With<Action<FoundationConsoleSubmit>>>,
    input_focus: Res<InputFocus>,
    mut commands: Commands,
    mut console_ui_state: ResMut<FoundationConsoleUiState>,
    console_history: Res<FoundationConsoleHistory>,
    console_registry: Res<FoundationConsoleRegistry>,
    bsn_scene_registry: Option<Res<FoundationBsnSceneRegistry>>,
    mut console_inputs: Query<(Entity, &mut EditableText), With<FoundationConsoleInput>>,
    mut scene_commands: MessageWriter<SceneCommand>,
) {
    let Some((input_entity, mut editable_text)) = console_inputs.iter_mut().next() else {
        return;
    };
    if input_focus.get() != Some(input_entity) {
        return;
    }

    if close_action.contains(ActionEvents::START) {
        let console_scene_key = SceneKey::new(FOUNDATION_CONSOLE_SCENE_KEY);
        scene_commands.write(SceneCommand::Close(SceneTarget::Key(console_scene_key)));
        return;
    }

    if autocomplete_action.contains(ActionEvents::START) {
        let bsn_scene_registry = bsn_scene_registry.as_deref();
        if let Some(completed_input) = autocomplete_console_input(
            &console_ui_state.input,
            &console_registry,
            bsn_scene_registry,
        ) {
            replace_console_input(&mut editable_text, &completed_input);
            console_ui_state.input = completed_input;
        }
        return;
    }

    if history_previous_action.contains(ActionEvents::START) {
        if let Some(history_input) = previous_history_input(&mut console_ui_state, &console_history)
        {
            replace_console_input(&mut editable_text, &history_input);
            console_ui_state.input = history_input;
        }
        return;
    }

    if history_next_action.contains(ActionEvents::START) {
        let history_input = next_history_input(&mut console_ui_state, &console_history);
        replace_console_input(&mut editable_text, &history_input);
        console_ui_state.input = history_input;
        return;
    }

    if submit_action.contains(ActionEvents::START) {
        let submitted_command_line = console_ui_state.input.trim().to_string();
        if submitted_command_line.is_empty() {
            return;
        }

        replace_console_input(&mut editable_text, "");
        console_ui_state.input.clear();
        console_ui_state.history_cursor = None;
        console_ui_state.output_scroll_lines = 0;

        commands.queue(move |world: &mut World| {
            execute_console_command_from_ui(world, submitted_command_line);
        });
    }
}

const FOUNDATION_CONSOLE_SCROLL_LINES_PER_WHEEL_STEP: usize = 3;
const FOUNDATION_CONSOLE_VISIBLE_OUTPUT_LINES: usize = 16;

#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn refresh_console_text_nodes(
    mut commands: Commands,
    console_ui_state: Res<FoundationConsoleUiState>,
    console_history: Res<FoundationConsoleHistory>,
    console_registry: Res<FoundationConsoleRegistry>,
    bsn_scene_registry: Option<Res<FoundationBsnSceneRegistry>>,
    console_outputs: Query<Entity, With<FoundationConsoleOutput>>,
    output_lines: Query<Entity, With<FoundationConsoleOutputLine>>,
    mut console_suggestions: Query<
        (&mut Visibility, &mut Node, &mut ScrollPosition),
        With<FoundationConsoleSuggestion>,
    >,
    suggestion_lists: Query<Entity, With<FoundationConsoleSuggestion>>,
    suggestion_options: Query<Entity, With<FoundationConsoleSuggestionOption>>,
) {
    if console_ui_state.is_changed() || console_history.is_changed() {
        rebuild_console_output_lines(
            &mut commands,
            &console_ui_state,
            &console_outputs,
            &output_lines,
        );
    }

    let suggestions_changed = console_ui_state.is_changed()
        || console_registry.is_changed()
        || bsn_scene_registry
            .as_ref()
            .is_some_and(|registry| registry.is_changed());
    if suggestions_changed {
        let bsn_scene_registry = bsn_scene_registry.as_deref();
        let suggestions = autocomplete_console_candidates(
            &console_ui_state.input,
            &console_registry,
            bsn_scene_registry,
        );
        for (mut visibility, mut node, mut scroll_position) in &mut console_suggestions {
            scroll_position.y = 0.0;
            if suggestions.is_empty() {
                *visibility = Visibility::Hidden;
                node.display = Display::None;
            } else {
                *visibility = Visibility::Visible;
                node.display = Display::Flex;
            }
        }
        rebuild_console_suggestion_items(
            &mut commands,
            &suggestions,
            &suggestion_lists,
            &suggestion_options,
        );
    }
}

fn rebuild_console_suggestion_items(
    commands: &mut Commands,
    suggestions: &[ConsoleAutocompleteCandidate],
    suggestion_lists: &Query<Entity, With<FoundationConsoleSuggestion>>,
    suggestion_options: &Query<Entity, With<FoundationConsoleSuggestionOption>>,
) {
    for suggestion_option_entity in suggestion_options {
        commands.entity(suggestion_option_entity).despawn();
    }

    for suggestion_list_entity in suggestion_lists {
        for suggestion in suggestions {
            let suggestion_z_index = GlobalZIndex(10_004);
            let suggestion_option_entity = spawn_console_reuse_button(
                commands,
                &suggestion.display,
                suggestion_z_index,
                FoundationConsoleSuggestionOption {
                    replacement: suggestion.replacement.clone(),
                },
            );
            commands
                .entity(suggestion_list_entity)
                .add_child(suggestion_option_entity);
        }
    }
}

fn rebuild_console_output_lines(
    commands: &mut Commands,
    console_ui_state: &FoundationConsoleUiState,
    console_outputs: &Query<Entity, With<FoundationConsoleOutput>>,
    output_lines: &Query<Entity, With<FoundationConsoleOutputLine>>,
) {
    for output_line_entity in output_lines {
        commands.entity(output_line_entity).despawn();
    }

    let output_entries = console_output_entries(console_ui_state);
    for console_output_entity in console_outputs {
        for output_entry in &output_entries {
            for (output_line_position, output_line) in output_entry.lines().enumerate() {
                let output_line_entity = if output_line_position == 0 {
                    spawn_console_command_output_button(commands, output_line)
                } else {
                    spawn_console_output_text(commands, output_line)
                };
                commands
                    .entity(console_output_entity)
                    .add_child(output_line_entity);
            }
        }
    }
}

fn spawn_console_command_output_button(commands: &mut Commands, output_line: &str) -> Entity {
    let Some(command_line) = output_line.strip_prefix("> ") else {
        return spawn_console_output_text(commands, output_line);
    };

    let history_z_index = GlobalZIndex(10_001);
    spawn_console_reuse_button(
        commands,
        output_line,
        history_z_index,
        (
            FoundationConsoleHistoryItem {
                command_line: command_line.to_string(),
            },
            FoundationConsoleOutputLine,
        ),
    )
}

fn spawn_console_output_text(commands: &mut Commands, output_line: &str) -> Entity {
    let output_text_color = TextColor(Color::srgba(0.82, 0.88, 0.78, 1.0));
    commands
        .spawn((
            Text::new(output_line.to_string()),
            TextFont {
                font_size: 14.0.into(),
                ..default()
            },
            output_text_color,
            FoundationConsoleOutputLine,
        ))
        .id()
}

fn spawn_console_reuse_button<T: Bundle>(
    commands: &mut Commands,
    label: &str,
    z_index: GlobalZIndex,
    marker: T,
) -> Entity {
    let button_background = BackgroundColor(Color::srgba(0.07, 0.07, 0.085, 0.96));
    let button_text_color = TextColor(Color::srgba(0.78, 0.86, 1.0, 1.0));

    commands
        .spawn((
            Button,
            Node {
                width: Val::Percent(100.0),
                min_height: Val::Px(20.0),
                padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
                ..default()
            },
            button_background,
            z_index,
            marker,
        ))
        .with_child((
            Text::new(label.to_string()),
            TextFont {
                font_size: 12.0.into(),
                ..default()
            },
            button_text_color,
        ))
        .id()
}

#[allow(clippy::type_complexity)]
fn handle_clickable_console_reuse(
    mut input_focus: ResMut<InputFocus>,
    mut console_ui_state: ResMut<FoundationConsoleUiState>,
    mut console_inputs: Query<(Entity, &mut EditableText), With<FoundationConsoleInput>>,
    suggestion_options: Query<
        (&Interaction, &FoundationConsoleSuggestionOption),
        (Changed<Interaction>, With<Button>),
    >,
    history_items: Query<
        (&Interaction, &FoundationConsoleHistoryItem),
        (Changed<Interaction>, With<Button>),
    >,
    suggestion_visibilities: Query<&Visibility, With<FoundationConsoleSuggestion>>,
) {
    let Some((input_entity, mut editable_text)) = console_inputs.iter_mut().next() else {
        return;
    };

    let clicked_suggestion =
        suggestion_options
            .iter()
            .find_map(|(interaction, suggestion_option)| {
                (*interaction == Interaction::Pressed)
                    .then(|| suggestion_option.replacement.clone())
            });
    let is_preview_open = console_preview_is_open(&suggestion_visibilities);
    let clicked_history_item = (!is_preview_open).then(|| {
        history_items
            .iter()
            .find_map(|(interaction, history_item)| {
                (*interaction == Interaction::Pressed).then(|| history_item.command_line.clone())
            })
    });
    let clicked_replacement = clicked_suggestion.or_else(|| clicked_history_item.flatten());

    if let Some(clicked_replacement) = clicked_replacement {
        replace_console_input(&mut editable_text, &clicked_replacement);
        console_ui_state.input = clicked_replacement;
        console_ui_state.history_cursor = None;
        input_focus.set(input_entity, FocusCause::Navigated);
    }
}

fn console_preview_is_open(
    suggestion_visibilities: &Query<&Visibility, With<FoundationConsoleSuggestion>>,
) -> bool {
    suggestion_visibilities
        .iter()
        .any(|visibility| *visibility == Visibility::Visible)
}

fn scroll_console_output(
    console_scroll_action: Single<&Action<FoundationConsoleScroll>>,
    console_state: Res<FoundationConsoleState>,
    mut console_ui_state: ResMut<FoundationConsoleUiState>,
    suggestion_visibilities: Query<&Visibility, With<FoundationConsoleSuggestion>>,
    mut suggestion_scroll_positions: Query<&mut ScrollPosition, With<FoundationConsoleSuggestion>>,
) {
    if !console_state.is_open {
        return;
    }

    // `FoundationConsoleScroll`'s value is already this frame's accumulated,
    // unit-normalized wheel delta (see the action's doc comment), so no
    // per-message unit handling is needed here unlike the pre-migration code.
    let scroll_steps = console_scroll_action.y.round() as i32;

    if scroll_steps == 0 {
        return;
    }

    let scroll_line_delta =
        scroll_steps.unsigned_abs() as usize * FOUNDATION_CONSOLE_SCROLL_LINES_PER_WHEEL_STEP;

    if console_preview_is_open(&suggestion_visibilities) {
        scroll_console_prediction_popup(
            scroll_steps,
            scroll_line_delta,
            &mut suggestion_scroll_positions,
        );
        return;
    }

    let output_line_count = console_output_lines(&console_ui_state).len();
    let max_scroll_lines =
        output_line_count.saturating_sub(FOUNDATION_CONSOLE_VISIBLE_OUTPUT_LINES);

    if scroll_steps > 0 {
        console_ui_state.output_scroll_lines =
            (console_ui_state.output_scroll_lines + scroll_line_delta).min(max_scroll_lines);
    } else {
        console_ui_state.output_scroll_lines = console_ui_state
            .output_scroll_lines
            .saturating_sub(scroll_line_delta);
    }
}

fn scroll_console_prediction_popup(
    scroll_steps: i32,
    scroll_line_delta: usize,
    suggestion_scroll_positions: &mut Query<&mut ScrollPosition, With<FoundationConsoleSuggestion>>,
) {
    let scroll_pixel_delta = scroll_line_delta as f32 * 8.0;
    for mut scroll_position in suggestion_scroll_positions {
        if scroll_steps > 0 {
            scroll_position.y = (scroll_position.y + scroll_pixel_delta).max(0.0);
        } else {
            scroll_position.y = (scroll_position.y - scroll_pixel_delta).max(0.0);
        }
    }
}

fn execute_console_command_from_ui(world: &mut World, submitted_command_line: String) {
    let execution_result = {
        let console_registry = world.resource::<FoundationConsoleRegistry>().clone();
        console_registry.execute_command_line(world, &submitted_command_line)
    };

    let mut output_line = format!("> {submitted_command_line}");
    match execution_result {
        Ok(()) => {
            output_line.push_str("\nCommand completed.");
        }
        Err(command_error) => {
            output_line.push_str(&format!("\nError: {command_error}"));
        }
    }

    let history_save_result = {
        let mut console_history = world.resource_mut::<FoundationConsoleHistory>();
        console_history.push_command(submitted_command_line);
        console_history.save_to_disk()
    };
    if let Err(history_save_error) = history_save_result {
        output_line.push_str(&format!("\nHistory save error: {history_save_error}"));
    }

    let mut console_ui_state = world.resource_mut::<FoundationConsoleUiState>();
    console_ui_state.output_lines.push(output_line);
    console_ui_state.history_cursor = None;
    console_ui_state.output_scroll_lines = 0;
}

fn editable_text_value(editable_text: &EditableText) -> String {
    let mut value = String::new();
    value.reserve(editable_text.value().into_iter().map(str::len).sum());
    for text_part in editable_text.value() {
        value.push_str(text_part);
    }
    value
}

fn replace_console_input(editable_text: &mut EditableText, replacement: &str) {
    editable_text.clear();
    editable_text.editor.set_text(replacement);
    editable_text.queue_edit(TextEdit::TextEnd(false));
}

fn autocomplete_console_input(
    console_input: &str,
    console_registry: &FoundationConsoleRegistry,
    bsn_scene_registry: Option<&FoundationBsnSceneRegistry>,
) -> Option<String> {
    let candidate =
        autocomplete_console_candidates(console_input, console_registry, bsn_scene_registry)
            .into_iter()
            .next()?;

    Some(candidate.replacement)
}

fn autocomplete_console_candidates(
    console_input: &str,
    console_registry: &FoundationConsoleRegistry,
    bsn_scene_registry: Option<&FoundationBsnSceneRegistry>,
) -> Vec<ConsoleAutocompleteCandidate> {
    if console_input.trim().is_empty() {
        return Vec::new();
    }

    let open_scene_argument_context = open_scene_argument_context(console_input);
    let trimmed_start_input = console_input.trim_start();
    if trimmed_start_input == FOUNDATION_OPEN_SCENE_COMMAND_NAME
        || trimmed_start_input.starts_with("open ")
    {
        if let Some(open_scene_argument_context) = open_scene_argument_context {
            return autocomplete_open_scene_arguments(
                open_scene_argument_context,
                bsn_scene_registry,
            );
        }
    }

    let mut autocomplete_candidates = Vec::new();

    let command_search_text = console_input
        .split_whitespace()
        .next()
        .unwrap_or(console_input);
    autocomplete_candidates.extend(
        console_registry
            .autocomplete_command_names(command_search_text)
            .into_iter()
            .map(|candidate| {
                let replacement = command_placeholder(&candidate.replacement, console_registry);
                ConsoleAutocompleteCandidate {
                    display: replacement.clone(),
                    replacement,
                }
            }),
    );

    if let Some(open_scene_argument_context) = open_scene_argument_context {
        autocomplete_candidates.extend(autocomplete_open_scene_arguments(
            open_scene_argument_context,
            bsn_scene_registry,
        ));
    }

    autocomplete_candidates
}

#[cfg(test)]
fn console_suggestion_text(
    console_input: &str,
    console_registry: &FoundationConsoleRegistry,
    bsn_scene_registry: Option<&FoundationBsnSceneRegistry>,
) -> String {
    let suggestions =
        autocomplete_console_candidates(console_input, console_registry, bsn_scene_registry);
    if suggestions.is_empty() {
        return String::new();
    }

    suggestions
        .into_iter()
        .map(|suggestion| suggestion.display)
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OpenSceneArgumentContext {
    line_prefix_before_current_scene: String,
    current_scene_prefix: String,
}

fn open_scene_argument_context(console_input: &str) -> Option<OpenSceneArgumentContext> {
    let trimmed_start_input = console_input.trim_start();
    let open_command_offset = console_input.len() - trimmed_start_input.len();
    if FOUNDATION_OPEN_SCENE_COMMAND_NAME.starts_with(trimmed_start_input) {
        return Some(OpenSceneArgumentContext {
            line_prefix_before_current_scene: format!(
                "{}open ",
                &console_input[..open_command_offset]
            ),
            current_scene_prefix: String::new(),
        });
    }
    if trimmed_start_input != FOUNDATION_OPEN_SCENE_COMMAND_NAME
        && !trimmed_start_input.starts_with("open ")
    {
        return None;
    }

    let open_command_end = open_command_offset + FOUNDATION_OPEN_SCENE_COMMAND_NAME.len();
    let input_after_open_command = &console_input[open_command_end..];
    if input_after_open_command.is_empty() {
        return Some(OpenSceneArgumentContext {
            line_prefix_before_current_scene: format!(
                "{}open ",
                &console_input[..open_command_offset]
            ),
            current_scene_prefix: String::new(),
        });
    }

    let scene_arguments_start = open_command_end + 1;
    let scene_arguments = &console_input[scene_arguments_start..];
    let current_scene_prefix_start = scene_arguments
        .rfind(char::is_whitespace)
        .map(|relative_whitespace_position| {
            scene_arguments_start + relative_whitespace_position + 1
        })
        .unwrap_or(scene_arguments_start);
    let line_prefix_before_current_scene = console_input[..current_scene_prefix_start].to_string();
    let current_scene_prefix = console_input[current_scene_prefix_start..].to_string();

    Some(OpenSceneArgumentContext {
        line_prefix_before_current_scene,
        current_scene_prefix,
    })
}

fn autocomplete_open_scene_arguments(
    open_scene_argument_context: OpenSceneArgumentContext,
    bsn_scene_registry: Option<&FoundationBsnSceneRegistry>,
) -> Vec<ConsoleAutocompleteCandidate> {
    let Some(bsn_scene_registry) = bsn_scene_registry else {
        return Vec::new();
    };

    bsn_scene_registry
        .registered_scene_keys_containing(&open_scene_argument_context.current_scene_prefix)
        .into_iter()
        .map(|registered_scene_key| {
            let replacement = format!(
                "{}{}",
                open_scene_argument_context.line_prefix_before_current_scene, registered_scene_key,
            );
            ConsoleAutocompleteCandidate {
                display: replacement.clone(),
                replacement,
            }
        })
        .collect()
}

fn command_placeholder(command_name: &str, console_registry: &FoundationConsoleRegistry) -> String {
    let Some(command) = console_registry.find_command(command_name) else {
        return command_name.to_string();
    };

    let parameter_placeholders = (command.parameters)()
        .iter()
        .map(|parameter| format!("{}=<{}>", parameter.name, parameter.type_name))
        .collect::<Vec<_>>();

    if parameter_placeholders.is_empty() {
        command_name.to_string()
    } else {
        format!("{} {}", command_name, parameter_placeholders.join(" "))
    }
}

fn previous_history_input(
    console_ui_state: &mut FoundationConsoleUiState,
    console_history: &FoundationConsoleHistory,
) -> Option<String> {
    if console_history.commands.is_empty() {
        return None;
    }

    let history_entry_index = console_ui_state
        .history_cursor
        .map(|history_cursor| history_cursor.saturating_sub(1))
        .unwrap_or(console_history.commands.len() - 1);
    console_ui_state.history_cursor = Some(history_entry_index);
    console_history.commands.get(history_entry_index).cloned()
}

fn next_history_input(
    console_ui_state: &mut FoundationConsoleUiState,
    console_history: &FoundationConsoleHistory,
) -> String {
    let Some(history_cursor) = console_ui_state.history_cursor else {
        return String::new();
    };

    let next_history_index = history_cursor + 1;
    if next_history_index >= console_history.commands.len() {
        console_ui_state.history_cursor = None;
        String::new()
    } else {
        console_ui_state.history_cursor = Some(next_history_index);
        console_history.commands[next_history_index].clone()
    }
}

fn spawn_console_overlay(
    commands: &mut Commands,
    scene_id: crate::scene_stack::SceneId,
    _console_history: &FoundationConsoleHistory,
    console_ui_state: &FoundationConsoleUiState,
) -> Entity {
    let console_root_height = Val::Px(380.0);
    let console_padding = UiRect::all(Val::Px(10.0));
    let console_gap = Val::Px(8.0);
    let console_background = BackgroundColor(Color::srgba(0.02, 0.02, 0.025, 0.92));
    let console_border = BorderColor::all(Color::srgba(0.25, 0.25, 0.30, 1.0));
    let console_text_color = TextColor(Color::srgba(0.82, 0.88, 0.78, 1.0));
    let input_background = BackgroundColor(Color::srgba(0.05, 0.05, 0.06, 1.0));
    let root_entity = commands
        .spawn((
            Name::new("Foundation Debug Console"),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                bottom: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: console_root_height,
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexEnd,
                row_gap: console_gap,
                padding: console_padding,
                border: UiRect::top(Val::Px(1.0)),
                ..default()
            },
            console_background,
            console_border,
            GlobalZIndex(10_000),
            TabGroup::new(0),
            SceneOwner { scene_id },
            FoundationConsoleRoot,
        ))
        .id();

    let output_viewport_entity = commands
        .spawn((
            Name::new("Foundation Debug Console Output Viewport"),
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(290.0),
                flex_grow: 1.0,
                flex_shrink: 1.0,
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexEnd,
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition::default(),
            SceneOwner { scene_id },
            FoundationConsoleOutputViewport,
        ))
        .id();

    let output_entity = commands
        .spawn((
            Name::new("Foundation Debug Console Output"),
            Node {
                width: Val::Percent(100.0),
                flex_shrink: 0.0,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                ..default()
            },
            SceneOwner { scene_id },
            FoundationConsoleOutput,
        ))
        .id();

    for output_entry in console_output_entries(console_ui_state) {
        for (output_line_position, output_line) in output_entry.lines().enumerate() {
            let output_line_entity = if output_line_position == 0 {
                spawn_console_command_output_button(commands, output_line)
            } else {
                spawn_console_output_text(commands, output_line)
            };
            commands.entity(output_entity).add_child(output_line_entity);
        }
    }

    let suggestion_entity = commands
        .spawn((
            Name::new("Foundation Debug Console Suggestions"),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                right: Val::Px(12.0),
                // Keep the popup anchored above the input row and bounded inside the console.
                bottom: Val::Px(62.0),
                top: Val::Px(10.0),
                max_height: Val::Px(280.0),
                display: Display::None,
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(8.0)),
                border: UiRect::all(Val::Px(1.0)),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition::default(),
            BackgroundColor(Color::srgba(0.04, 0.04, 0.05, 0.95)),
            BorderColor::all(Color::srgba(0.30, 0.34, 0.42, 1.0)),
            GlobalZIndex(10_003),
            Visibility::Hidden,
            SceneOwner { scene_id },
            FoundationConsoleSuggestion,
        ))
        .id();

    let input_container_entity = commands
        .spawn((
            Name::new("Foundation Debug Console Input Container"),
            Node {
                width: Val::Percent(100.0),
                min_height: Val::Px(34.0),
                padding: UiRect::all(Val::Px(6.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            input_background,
            BorderColor::all(Color::srgba(0.35, 0.35, 0.40, 1.0)),
            SceneOwner { scene_id },
        ))
        .id();

    let input_entity = commands
        .spawn((
            Name::new("Foundation Debug Console Input"),
            Node {
                width: Val::Percent(100.0),
                ..default()
            },
            FoundationConsoleInput,
            EditableText::new(console_ui_state.input.clone()),
            TextLayout::no_wrap(),
            TextFont {
                font_size: 14.0.into(),
                ..default()
            },
            console_text_color,
            TextCursorStyle::default(),
            TabIndex(0),
            AutoFocus,
            SceneOwner { scene_id },
        ))
        .id();

    commands
        .entity(output_viewport_entity)
        .add_child(output_entity);
    commands
        .entity(input_container_entity)
        .add_child(input_entity);
    commands.entity(root_entity).add_children(&[
        output_viewport_entity,
        suggestion_entity,
        input_container_entity,
    ]);

    input_entity
}

#[cfg(test)]
fn console_output_text(
    _console_history: &FoundationConsoleHistory,
    console_ui_state: &FoundationConsoleUiState,
) -> String {
    console_output_entries(console_ui_state).join("\n")
}

fn console_output_entries(console_ui_state: &FoundationConsoleUiState) -> Vec<String> {
    let output_lines = console_output_lines(console_ui_state);
    if output_lines.is_empty() {
        return vec!["Foundation debug console ready.".to_string()];
    }

    let newest_visible_line_end = output_lines
        .len()
        .saturating_sub(console_ui_state.output_scroll_lines);
    let visible_line_end = newest_visible_line_end.max(1);
    let visible_line_start =
        visible_line_end.saturating_sub(FOUNDATION_CONSOLE_VISIBLE_OUTPUT_LINES);
    output_lines[visible_line_start..visible_line_end].to_vec()
}

fn console_output_lines(console_ui_state: &FoundationConsoleUiState) -> Vec<String> {
    console_ui_state
        .output_lines
        .iter()
        .flat_map(|output_entry| output_entry.lines().map(str::to_string))
        .collect()
}

/// Snapshot of console command descriptors linked into this binary.
#[derive(Clone, Debug, Resource)]
pub struct FoundationConsoleRegistry {
    commands: Vec<&'static ConsoleCommandDescriptor>,
}

impl Default for FoundationConsoleRegistry {
    fn default() -> Self {
        let mut commands = FOUNDATION_CONSOLE_COMMANDS.iter().collect::<Vec<_>>();
        commands.sort_by(|left_command, right_command| left_command.name.cmp(right_command.name));
        Self { commands }
    }
}

impl FoundationConsoleRegistry {
    /// Returns registered commands sorted by command name.
    pub fn commands(&self) -> &[&'static ConsoleCommandDescriptor] {
        &self.commands
    }

    /// Finds a command by exact name.
    pub fn find_command(&self, command_name: &str) -> Option<&'static ConsoleCommandDescriptor> {
        self.commands
            .iter()
            .copied()
            .find(|command| command.name == command_name)
    }

    /// Returns deterministic autocomplete candidates whose command names contain search text.
    pub fn autocomplete_command_names(
        &self,
        command_search_text: &str,
    ) -> Vec<ConsoleAutocompleteCandidate> {
        let mut autocomplete_candidates = self
            .commands
            .iter()
            .filter(|command| command.name.contains(command_search_text))
            .map(|command| ConsoleAutocompleteCandidate {
                replacement: command.name.to_string(),
                display: command.name.to_string(),
            })
            .collect::<Vec<_>>();

        if FOUNDATION_OPEN_SCENE_COMMAND_NAME.contains(command_search_text) {
            autocomplete_candidates.push(ConsoleAutocompleteCandidate {
                replacement: FOUNDATION_OPEN_SCENE_COMMAND_NAME.to_string(),
                display: FOUNDATION_OPEN_SCENE_COMMAND_NAME.to_string(),
            });
        }
        autocomplete_candidates.sort_by(|left_candidate, right_candidate| {
            left_candidate.display.cmp(&right_candidate.display)
        });
        autocomplete_candidates
    }

    /// Executes a parsed console command against the provided Bevy world.
    pub fn execute_command_line(
        &self,
        world: &mut World,
        command_line: &str,
    ) -> ConsoleCommandResult<()> {
        if is_open_scene_command_line(command_line) {
            return execute_open_scene_command(world, command_line);
        }

        let parsed_command_line = ParsedConsoleCommandLine::parse(command_line)?;
        let Some(command) = self.find_command(&parsed_command_line.command_name) else {
            return Err(ConsoleCommandError::UnknownCommand {
                command_name: parsed_command_line.command_name,
            });
        };

        (command.execute)(world, parsed_command_line.arguments)
    }
}

fn is_open_scene_command_line(command_line: &str) -> bool {
    let trimmed_command_line = command_line.trim_start();
    trimmed_command_line == "open" || trimmed_command_line.starts_with("open ")
}

fn execute_open_scene_command(world: &mut World, command_line: &str) -> ConsoleCommandResult<()> {
    let scene_keys = parse_open_scene_command_scene_keys(command_line)?;
    let scene_commands = runtime_open_scene_commands(scene_keys);
    let Some(mut scene_command_messages) = world.get_resource_mut::<Messages<SceneCommand>>()
    else {
        return Err(ConsoleCommandError::SceneCommandsUnavailable);
    };

    for scene_command in scene_commands {
        scene_command_messages.write(scene_command);
    }

    Ok(())
}

fn parse_open_scene_command_scene_keys(command_line: &str) -> ConsoleCommandResult<Vec<String>> {
    let scene_keys = command_line
        .split_whitespace()
        .skip(1)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if scene_keys.is_empty() {
        return Err(ConsoleCommandError::MissingOpenSceneArgument);
    }

    Ok(scene_keys)
}

fn runtime_open_scene_commands(scene_keys: Vec<String>) -> Vec<SceneCommand> {
    scene_keys
        .into_iter()
        .enumerate()
        .map(|(scene_key_position, scene_key)| {
            let scene_source = SceneSource::bsn_scene(scene_key);
            if scene_key_position == 0 {
                let clear_stack_options = OpenSceneOptions::default().clear_stack();
                SceneCommand::open_with_options(scene_source, clear_stack_options)
            } else {
                SceneCommand::open(scene_source)
            }
        })
        .collect()
}

/// Metadata and executor for one Foundation console command.
#[derive(Clone, Copy)]
pub struct ConsoleCommandDescriptor {
    /// Name typed by users to invoke this command.
    pub name: &'static str,
    /// Function returning user-provided command parameter metadata.
    pub parameters: fn() -> &'static [ConsoleCommandParameter],
    /// Function that parses user inputs and executes the generated command system.
    pub execute: ConsoleCommandExecutor,
}

impl fmt::Debug for ConsoleCommandDescriptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConsoleCommandDescriptor")
            .field("name", &self.name)
            .field("parameters", &(self.parameters)())
            .finish_non_exhaustive()
    }
}

/// Function pointer used by generated console command adapters.
pub type ConsoleCommandExecutor =
    fn(&mut World, ConsoleCommandArguments) -> ConsoleCommandResult<()>;

/// Metadata for one named user-provided console command parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConsoleCommandParameter {
    /// Name typed by users for this parameter.
    pub name: &'static str,
    /// Rust type name used for placeholder text and diagnostics.
    pub type_name: &'static str,
    /// Whether this parameter is required by the command input struct.
    pub required: bool,
}

/// Parsed user-provided named arguments for a console command.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConsoleCommandArguments {
    values: BTreeMap<String, String>,
}

impl ConsoleCommandArguments {
    /// Creates arguments from key/value pairs.
    pub fn from_pairs(
        pairs: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        let values = pairs
            .into_iter()
            .map(|(parameter_name, parameter_value)| {
                (parameter_name.into(), parameter_value.into())
            })
            .collect();
        Self { values }
    }

    /// Returns true when no named arguments were provided.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Returns a required parameter value or a structured command error.
    pub fn required(&self, parameter_name: &'static str) -> ConsoleCommandResult<&str> {
        self.values
            .get(parameter_name)
            .map(String::as_str)
            .ok_or(ConsoleCommandError::MissingParameter { parameter_name })
    }

    /// Returns a named parameter value when it was provided.
    pub fn get(&self, parameter_name: &str) -> Option<&str> {
        self.values.get(parameter_name).map(String::as_str)
    }
}

/// Wrapper around user-provided input structs in console command signatures.
#[derive(Clone, Debug)]
pub struct ConsoleInputs<T> {
    values: T,
}

impl<T> ConsoleInputs<T> {
    /// Creates a console input wrapper from parsed values.
    pub fn new(values: T) -> Self {
        Self { values }
    }

    /// Returns the parsed command input values.
    pub fn values(&self) -> &T {
        &self.values
    }

    /// Consumes the wrapper and returns parsed command input values.
    pub fn into_values(self) -> T {
        self.values
    }
}

impl<T> std::ops::Deref for ConsoleInputs<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

/// Trait implemented by command input structs.
pub trait ConsoleCommandInput: Sized {
    /// Returns named parameter metadata used by placeholder text and autocomplete.
    fn parameters() -> &'static [ConsoleCommandParameter];

    /// Parses command-line arguments into strongly typed input values.
    fn parse(console_command_arguments: &ConsoleCommandArguments) -> ConsoleCommandResult<Self>;
}

impl ConsoleCommandInput for () {
    fn parameters() -> &'static [ConsoleCommandParameter] {
        &[]
    }

    fn parse(console_command_arguments: &ConsoleCommandArguments) -> ConsoleCommandResult<Self> {
        if console_command_arguments.is_empty() {
            Ok(())
        } else {
            Err(ConsoleCommandError::UnexpectedParameters)
        }
    }
}

/// Trait used by generated command adapters to normalize command return values.
pub trait IntoConsoleCommandResult {
    /// Converts a command return value into the standard console command result.
    fn into_console_command_result(self) -> ConsoleCommandResult<()>;
}

impl IntoConsoleCommandResult for () {
    fn into_console_command_result(self) -> ConsoleCommandResult<()> {
        Ok(())
    }
}

impl IntoConsoleCommandResult for ConsoleCommandResult<()> {
    fn into_console_command_result(self) -> ConsoleCommandResult<()> {
        self
    }
}

/// User-facing autocomplete candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsoleAutocompleteCandidate {
    /// Text inserted if the candidate is accepted.
    pub replacement: String,
    /// Text shown in the autocomplete UI.
    pub display: String,
}

/// Result alias used by Foundation console APIs.
pub type ConsoleCommandResult<T> = std::result::Result<T, ConsoleCommandError>;

/// Errors produced while parsing or executing console commands.
#[derive(Debug)]
pub enum ConsoleCommandError {
    /// The command line did not contain a command name.
    EmptyCommand,
    /// A command name did not match the linked command registry.
    UnknownCommand {
        /// Unknown command name typed by the user.
        command_name: String,
    },
    /// A command token could not be parsed as `name=value`.
    InvalidArgumentToken {
        /// Raw token that failed parsing.
        token: String,
    },
    /// A required named parameter was missing.
    MissingParameter {
        /// Missing parameter name.
        parameter_name: &'static str,
    },
    /// A named parameter could not be parsed into its Rust type.
    InvalidParameter {
        /// Parameter name that failed parsing.
        parameter_name: &'static str,
        /// Expected Rust type name.
        expected_type_name: &'static str,
        /// Parse error details.
        reason: String,
    },
    /// A no-input command received one or more parameters.
    UnexpectedParameters,
    /// The built-in `open` command did not receive any scene keys or paths.
    MissingOpenSceneArgument,
    /// The scene-stack message resource was not installed in the current app.
    SceneCommandsUnavailable,
    /// Bevy failed to run a generated command system.
    SystemFailed {
        /// Bevy system failure details.
        reason: String,
    },
}

impl ConsoleCommandError {
    /// Creates an invalid-parameter error.
    pub fn invalid_parameter(
        parameter_name: &'static str,
        expected_type_name: &'static str,
        reason: String,
    ) -> Self {
        Self::InvalidParameter {
            parameter_name,
            expected_type_name,
            reason,
        }
    }

    /// Converts a Bevy one-shot system failure into a console command error.
    pub fn from_run_system_error(error: bevy::ecs::system::RunSystemError) -> Self {
        Self::SystemFailed {
            reason: error.to_string(),
        }
    }
}

impl fmt::Display for ConsoleCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyCommand => write!(formatter, "Expected a console command."),
            Self::UnknownCommand { command_name } => {
                write!(formatter, "Unknown console command `{command_name}`.")
            }
            Self::InvalidArgumentToken { token } => {
                write!(formatter, "Expected console argument `{token}` to use name=value syntax.")
            }
            Self::MissingParameter { parameter_name } => {
                write!(formatter, "Missing required console parameter `{parameter_name}`.")
            }
            Self::InvalidParameter {
                parameter_name,
                expected_type_name,
                reason,
            } => write!(
                formatter,
                "Failed to parse console parameter `{parameter_name}` as {expected_type_name}: {reason}"
            ),
            Self::UnexpectedParameters => write!(formatter, "This console command does not accept parameters."),
            Self::MissingOpenSceneArgument => write!(formatter, "Expected `open` to include at least one BSN scene key or asset-relative `.bsn` path."),
            Self::SceneCommandsUnavailable => write!(formatter, "The Foundation scene stack is not available, so `open` cannot change scenes."),
            Self::SystemFailed { reason } => write!(formatter, "Console command system failed: {reason}"),
        }
    }
}

impl std::error::Error for ConsoleCommandError {}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ParsedConsoleCommandLine {
    command_name: String,
    arguments: ConsoleCommandArguments,
}

impl ParsedConsoleCommandLine {
    fn parse(command_line: &str) -> ConsoleCommandResult<Self> {
        let mut command_tokens = command_line.split_whitespace();
        let Some(command_name) = command_tokens.next() else {
            return Err(ConsoleCommandError::EmptyCommand);
        };

        let mut argument_values = BTreeMap::new();
        for command_token in command_tokens {
            let Some((parameter_name, parameter_value)) = command_token.split_once('=') else {
                return Err(ConsoleCommandError::InvalidArgumentToken {
                    token: command_token.to_string(),
                });
            };
            argument_values.insert(parameter_name.to_string(), parameter_value.to_string());
        }

        Ok(Self {
            command_name: command_name.to_string(),
            arguments: ConsoleCommandArguments {
                values: argument_values,
            },
        })
    }
}

/// Inputs for the built-in history size command.
#[derive(Clone, Debug, crate::ConsoleCommandInput)]
pub struct FoundationConsoleHistorySizeInputs {
    /// Maximum number of entries that should remain in history.
    pub max_entries: usize,
}

/// Trims persisted console history to a maximum number of entries.
#[allow(unused_mut)]
#[crate::console_command]
pub fn foundation_console_history_size(
    mut console_history: ResMut<FoundationConsoleHistory>,
    inputs: ConsoleInputs<FoundationConsoleHistorySizeInputs>,
) {
    let max_entries = inputs.max_entries;
    if console_history.commands.len() > max_entries {
        let removed_history_entry_count = console_history.commands.len() - max_entries;
        console_history
            .commands
            .drain(0..removed_history_entry_count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, crate::ConsoleCommandInput)]
    struct ExampleInputs {
        amount: usize,
        label: String,
    }

    #[test]
    fn input_metadata_uses_named_struct_fields() {
        let parameters = ExampleInputs::parameters();

        assert_eq!(parameters[0].name, "amount");
        assert_eq!(parameters[0].type_name, "usize");
        assert_eq!(parameters[1].name, "label");
        assert_eq!(parameters[1].type_name, "String");
    }

    #[test]
    fn input_parser_reads_named_arguments() {
        let arguments = ConsoleCommandArguments::from_pairs([("amount", "7"), ("label", "test")]);
        let inputs = ExampleInputs::parse(&arguments).expect("inputs should parse");

        assert_eq!(inputs.amount, 7);
        assert_eq!(inputs.label, "test");
    }

    #[test]
    fn command_line_parser_reads_command_name_and_named_arguments() {
        let parsed_command_line = ParsedConsoleCommandLine::parse("spawn amount=3 label=crate")
            .expect("command line should parse");

        assert_eq!(parsed_command_line.command_name, "spawn");
        assert_eq!(parsed_command_line.arguments.get("amount"), Some("3"));
        assert_eq!(parsed_command_line.arguments.get("label"), Some("crate"));
    }

    #[test]
    fn history_file_path_uses_saved_console_directory_beside_executable() {
        let executable_path = PathBuf::from("target/debug/template-game.exe");

        assert_eq!(
            history_file_path_for_executable(Some(&executable_path)),
            PathBuf::from("target/debug")
                .join("saved/console")
                .join("history.json")
        );
    }

    #[test]
    fn history_file_path_falls_back_to_current_directory_without_executable_path() {
        assert_eq!(
            history_file_path_for_executable(None),
            PathBuf::from(".")
                .join("saved/console")
                .join("history.json")
        );
    }

    #[test]
    fn console_history_round_trips_through_json_file() {
        let history_file_path = unique_test_history_file_path("round_trip");
        let console_history = FoundationConsoleHistory {
            commands: vec![
                "example.say-hello name=Jon".to_string(),
                "hello world".to_string(),
            ],
        };

        console_history
            .save_to_path(&history_file_path)
            .expect("history should save");
        let loaded_history = FoundationConsoleHistory::load_from_path(&history_file_path)
            .expect("history should load");

        assert_eq!(loaded_history.commands, console_history.commands);
        let _ = fs::remove_file(history_file_path);
    }

    #[test]
    fn missing_console_history_file_loads_empty_history() {
        let history_file_path = unique_test_history_file_path("missing");
        let _ = fs::remove_file(&history_file_path);

        let loaded_history = FoundationConsoleHistory::load_from_path(history_file_path)
            .expect("missing history should load as empty");

        assert!(loaded_history.commands.is_empty());
    }

    fn unique_test_history_file_path(test_name: &str) -> PathBuf {
        let process_id = std::process::id();
        let thread_id = format!("{:?}", std::thread::current().id());
        std::env::temp_dir().join(format!(
            "foundation-console-history-{test_name}-{process_id}-{thread_id}.json"
        ))
    }

    #[test]
    fn console_state_tracks_scene_id_across_reopen_cycles() {
        let mut console_state = FoundationConsoleState::default();
        let first_console_scene_id = SceneId(7);
        let second_console_scene_id = SceneId(8);

        console_state.mark_open(first_console_scene_id);
        assert!(console_state.is_open);
        assert_eq!(console_state.scene_id, Some(first_console_scene_id));

        assert!(console_state.mark_closed_if_scene_matches(first_console_scene_id));
        assert!(!console_state.is_open);
        assert_eq!(console_state.scene_id, None);

        console_state.mark_open(second_console_scene_id);
        assert!(console_state.is_open);
        assert_eq!(console_state.scene_id, Some(second_console_scene_id));
    }

    #[test]
    fn console_state_ignores_unrelated_scene_removals() {
        let mut console_state = FoundationConsoleState::default();
        let console_scene_id = SceneId(11);
        let unrelated_scene_id = SceneId(12);

        console_state.mark_open(console_scene_id);

        assert!(!console_state.mark_closed_if_scene_matches(unrelated_scene_id));
        assert!(console_state.is_open);
        assert_eq!(console_state.scene_id, Some(console_scene_id));
    }

    #[test]
    fn console_output_uses_execution_log_without_reappending_navigation_history() {
        let console_history = FoundationConsoleHistory {
            commands: vec!["example.say-hello name=Jon".to_string()],
        };
        let console_ui_state = FoundationConsoleUiState {
            input: String::new(),
            output_lines: vec![
                "> example.say-hello name=Jon\nCommand completed.".to_string(),
                "> hello world\nError: Expected console argument `world` to use name=value syntax."
                    .to_string(),
            ],
            history_cursor: None,
            output_scroll_lines: 0,
        };

        let output_text = console_output_text(&console_history, &console_ui_state);

        assert_eq!(
            output_text,
            "> example.say-hello name=Jon\nCommand completed.\n> hello world\nError: Expected console argument `world` to use name=value syntax."
        );
    }

    #[test]
    fn console_output_scrolls_to_older_lines() {
        let console_history = FoundationConsoleHistory::default();
        let console_ui_state = FoundationConsoleUiState {
            input: String::new(),
            output_lines: (1..=12)
                .map(|line_number| format!("line {line_number}"))
                .collect(),
            history_cursor: None,
            output_scroll_lines: 3,
        };

        let output_text = console_output_text(&console_history, &console_ui_state);

        assert_eq!(
            output_text,
            "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9"
        );
    }

    #[test]
    fn history_navigation_cycles_through_all_commands_until_clear_input() {
        let console_history = FoundationConsoleHistory {
            commands: vec![
                "first".to_string(),
                "second".to_string(),
                "third".to_string(),
            ],
        };
        let mut console_ui_state = FoundationConsoleUiState::default();

        assert_eq!(
            previous_history_input(&mut console_ui_state, &console_history),
            Some("third".to_string())
        );
        assert_eq!(
            previous_history_input(&mut console_ui_state, &console_history),
            Some("second".to_string())
        );
        assert_eq!(
            previous_history_input(&mut console_ui_state, &console_history),
            Some("first".to_string())
        );
        assert_eq!(
            previous_history_input(&mut console_ui_state, &console_history),
            Some("first".to_string())
        );
        assert_eq!(
            next_history_input(&mut console_ui_state, &console_history),
            "second"
        );
        assert_eq!(
            next_history_input(&mut console_ui_state, &console_history),
            "third"
        );
        assert_eq!(
            next_history_input(&mut console_ui_state, &console_history),
            ""
        );
    }

    #[test]
    fn console_output_defaults_to_newest_lines() {
        let console_history = FoundationConsoleHistory::default();
        let console_ui_state = FoundationConsoleUiState {
            input: String::new(),
            output_lines: (1..=12)
                .map(|line_number| format!("line {line_number}"))
                .collect(),
            history_cursor: None,
            output_scroll_lines: 0,
        };

        let output_text = console_output_text(&console_history, &console_ui_state);

        assert_eq!(
            output_text,
            "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9\nline 10\nline 11\nline 12"
        );
    }

    #[test]
    fn registry_contains_builtin_foundation_command() {
        let registry = FoundationConsoleRegistry::default();

        assert!(registry
            .commands()
            .iter()
            .any(|command| command.name == "foundation_console_history_size"));
    }

    #[test]
    fn registry_executes_builtin_command_as_bevy_system() {
        let mut world = World::new();
        world.insert_resource(FoundationConsoleHistory {
            commands: vec![
                "first".to_string(),
                "second".to_string(),
                "third".to_string(),
            ],
        });
        let registry = FoundationConsoleRegistry::default();

        registry
            .execute_command_line(&mut world, "foundation_console_history_size max_entries=2")
            .expect("command should execute");

        let console_history = world.resource::<FoundationConsoleHistory>();
        assert_eq!(console_history.commands, vec!["second", "third"]);
    }

    #[test]
    fn open_scene_command_builds_clear_then_ordered_open_commands() {
        let scene_keys = vec![
            "last-beacon/gameplay_level".to_string(),
            "last-beacon/pause_menu".to_string(),
        ];

        let scene_commands = runtime_open_scene_commands(scene_keys);

        assert_eq!(
            scene_commands,
            vec![
                SceneCommand::open_with_options(
                    SceneSource::bsn_scene("last-beacon/gameplay_level"),
                    OpenSceneOptions::default().clear_stack(),
                ),
                SceneCommand::open(SceneSource::bsn_scene("last-beacon/pause_menu")),
            ]
        );
    }

    #[test]
    fn registry_executes_open_scene_command_against_scene_stack() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::scene_stack::FoundationSceneStackPlugin);
        let registry = FoundationConsoleRegistry::default();

        registry
            .execute_command_line(
                app.world_mut(),
                "open last-beacon/gameplay_level last-beacon/pause_menu",
            )
            .expect("open command should queue scene stack commands");
        app.update();

        let scene_stack = app.world().resource::<crate::scene_stack::SceneStack>();
        assert_eq!(scene_stack.len(), 2);
        assert_eq!(
            scene_stack.entries()[0].source,
            SceneSource::bsn_scene("last-beacon/gameplay_level")
        );
        assert_eq!(
            scene_stack.entries()[1].source,
            SceneSource::bsn_scene("last-beacon/pause_menu")
        );
    }

    #[test]
    fn open_scene_command_rejects_missing_scene_arguments() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::scene_stack::FoundationSceneStackPlugin);
        let registry = FoundationConsoleRegistry::default();

        let command_error = registry
            .execute_command_line(app.world_mut(), "open")
            .expect_err("open command should require at least one scene");

        assert!(matches!(
            command_error,
            ConsoleCommandError::MissingOpenSceneArgument
        ));
    }

    #[test]
    fn open_scene_autocomplete_predicts_registered_scene_keys_only() {
        let registry = FoundationConsoleRegistry::default();
        let mut bsn_scene_registry = FoundationBsnSceneRegistry::default();
        bsn_scene_registry.register_scene("last-beacon/main_menu", "scenes/main_menu.bsn");
        bsn_scene_registry.register_scene("last-beacon/my_map", "scenes/my_map.bsn");
        bsn_scene_registry.register_scene("other-game/main_menu", "scenes/other_menu.bsn");

        let suggestions =
            autocomplete_console_candidates("open las", &registry, Some(&bsn_scene_registry));

        assert_eq!(
            suggestions,
            vec![
                ConsoleAutocompleteCandidate {
                    replacement: "open last-beacon/main_menu".to_string(),
                    display: "open last-beacon/main_menu".to_string(),
                },
                ConsoleAutocompleteCandidate {
                    replacement: "open last-beacon/my_map".to_string(),
                    display: "open last-beacon/my_map".to_string(),
                },
            ]
        );
    }

    #[test]
    fn open_scene_autocomplete_starts_before_open_is_fully_typed() {
        let registry = FoundationConsoleRegistry::default();
        let mut bsn_scene_registry = FoundationBsnSceneRegistry::default();
        bsn_scene_registry.register_scene("last-beacon/mapmap", "scenes/mapmap.bsn");

        let suggestions =
            autocomplete_console_candidates("op", &registry, Some(&bsn_scene_registry));

        assert!(suggestions.contains(&ConsoleAutocompleteCandidate {
            replacement: "open".to_string(),
            display: "open".to_string(),
        }));
        assert!(suggestions.contains(&ConsoleAutocompleteCandidate {
            replacement: "open last-beacon/mapmap".to_string(),
            display: "open last-beacon/mapmap".to_string(),
        }));
    }

    #[test]
    fn open_scene_autocomplete_matches_registered_scene_key_substrings() {
        let registry = FoundationConsoleRegistry::default();
        let mut bsn_scene_registry = FoundationBsnSceneRegistry::default();
        bsn_scene_registry.register_scene("last-beacon/mapmap", "scenes/mapmap.bsn");

        let suggestions =
            autocomplete_console_candidates("open mapmap", &registry, Some(&bsn_scene_registry));

        assert_eq!(
            suggestions,
            vec![ConsoleAutocompleteCandidate {
                replacement: "open last-beacon/mapmap".to_string(),
                display: "open last-beacon/mapmap".to_string(),
            }]
        );
    }

    #[test]
    fn command_autocomplete_lists_multiple_sorted_predictions() {
        let registry = FoundationConsoleRegistry::default();

        let suggestion_text = console_suggestion_text("o", &registry, None);

        assert!(suggestion_text.contains("foundation_console_history_size"));
        assert!(suggestion_text.contains("open"));
    }

    #[test]
    fn command_autocomplete_is_hidden_for_blank_input() {
        let registry = FoundationConsoleRegistry::default();

        assert_eq!(console_suggestion_text("", &registry, None), "");
        assert_eq!(console_suggestion_text("   ", &registry, None), "");
    }

    #[test]
    fn command_autocomplete_matches_text_contained_anywhere() {
        let registry = FoundationConsoleRegistry::default();

        let suggestion_text = console_suggestion_text("op", &registry, None);

        assert!(suggestion_text.contains("open"));
    }
}
