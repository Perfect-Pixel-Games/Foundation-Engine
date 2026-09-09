//! Foundation performance-stat overlay, toggled by the `stat.perf` console command.
//!
//! Modeled on Unreal Engine 5's `stat unit` command in spirit, not in exact
//! content: Bevy doesn't expose a Game/Draw/GPU thread split the way UE5
//! does, so this builds the closest honest equivalent from what Bevy
//! actually measures, plus one custom diagnostic of our own:
//! - **CPU Process**: our own diagnostic (there is no Bevy equivalent) --
//!   wall-clock time of the main app's own schedules (`First` through
//!   `Last`, i.e. all gameplay/UI logic), which runs on a separate thread
//!   from rendering under Bevy's pipelined renderer. This is the closest
//!   honest analogue to UE5's "Game" thread row.
//! - **CPU Render** / **GPU Render**: summed from every
//!   `render/<pass>/elapsed_cpu` / `elapsed_gpu` diagnostic
//!   `RenderDiagnosticsPlugin` writes per render pass.
//! - **FPS**, **Frame Time**, **Memory**, **Entities**: straight from
//!   Bevy's own `FrameTimeDiagnosticsPlugin`,
//!   `SystemInformationDiagnosticsPlugin`, and `EntityCountDiagnosticsPlugin`.
//!
//! Displayed as a grid: stat name, current ("Now") value, and a running
//! average recomputed once every real-time second (a tumbling 1-second
//! window, not Bevy's own frame-count-based `Diagnostic::average()`, so it
//! stays meaningful regardless of frame rate).
//!
//! Below the grid, a frame-time history strip renders the last
//! [`FRAME_TIME_HISTORY_SAMPLE_COUNT`] raw (unsmoothed) per-frame samples as
//! bars, so short stutters/spikes stay visible even when the 1-second
//! average looks fine. The `stat.perf.dump` console command writes a
//! one-shot snapshot of the grid's stats to the log, independent of the
//! overlay's visibility, for capturing perf evidence without a screenshot.

use bevy::{
    diagnostic::{
        Diagnostic, DiagnosticPath, Diagnostics, DiagnosticsStore, EntityCountDiagnosticsPlugin,
        FrameTimeDiagnosticsPlugin, RegisterDiagnostic, SystemInformationDiagnosticsPlugin,
    },
    platform::time::Instant,
    prelude::*,
    render::diagnostic::RenderDiagnosticsPlugin,
    text::FontSize,
    time::Real,
    ui::{GridAutoFlow, RepeatedGridTrack},
};
use std::collections::VecDeque;

/// Toggled by the `stat.perf` console command to show/hide the performance overlay.
#[derive(Clone, Copy, Debug, Default, Resource, Reflect)]
#[reflect(Resource)]
pub struct FoundationPerfOverlayState {
    /// Whether the overlay is currently shown.
    pub visible: bool,
}

/// Wall-clock timestamp recorded at the start of the main app's `First`
/// schedule, read back at `Last` to measure this frame's total main-thread
/// (gameplay/UI logic) duration. Bevy has no built-in diagnostic for this --
/// it only instruments individual render passes -- so this is a small
/// custom one, registered and fed the same way `FrameTimeDiagnosticsPlugin`
/// feeds its own diagnostics.
#[derive(Resource, Default)]
struct FoundationCpuProcessFrameTiming {
    frame_start: Option<Instant>,
}

/// Diagnostic path for [`FoundationCpuProcessFrameTiming`]'s measurement.
const CPU_PROCESS_FRAME_TIME: DiagnosticPath =
    DiagnosticPath::const_new("foundation/cpu_process_frame_time");

/// One row of the performance-stat grid.
#[derive(Clone, Copy, Debug, Component, PartialEq, Eq)]
enum FoundationPerfOverlayLine {
    Fps,
    FrameTime,
    CpuProcess,
    CpuRender,
    GpuRender,
    Memory,
    Entities,
}

const PERF_OVERLAY_LINE_COUNT: usize = 7;

impl FoundationPerfOverlayLine {
    const ALL: [Self; PERF_OVERLAY_LINE_COUNT] = [
        Self::Fps,
        Self::FrameTime,
        Self::CpuProcess,
        Self::CpuRender,
        Self::GpuRender,
        Self::Memory,
        Self::Entities,
    ];

    /// Index into the fixed-size accumulator arrays in
    /// [`FoundationPerfOverlayRunningAverages`]. Safe because this enum is
    /// fieldless and `ALL` is declared in the same order as the variants.
    fn index(self) -> usize {
        self as usize
    }

    fn label(self) -> &'static str {
        match self {
            Self::Fps => "FPS",
            Self::FrameTime => "Frame Time (ms)",
            Self::CpuProcess => "CPU Process (ms)",
            Self::CpuRender => "CPU Render (ms)",
            Self::GpuRender => "GPU Render (ms)",
            Self::Memory => "Memory (MB)",
            Self::Entities => "Entities",
        }
    }

    /// Formats a resolved value for this row -- controls decimal precision
    /// per stat (whole numbers for entity count/memory, 1 decimal for FPS,
    /// 2 for everything else) and shows `--` when there's no data yet.
    fn format(self, value: Option<f64>) -> String {
        match value {
            None => "--".to_string(),
            Some(value) => match self {
                Self::Fps => format!("{value:.1}"),
                Self::Memory | Self::Entities => format!("{value:.0}"),
                Self::FrameTime | Self::CpuProcess | Self::CpuRender | Self::GpuRender => {
                    format!("{value:.2}")
                }
            },
        }
    }
}

/// Marks which of the two numeric columns (current value vs. running
/// average) a perf overlay `Text` cell displays. Name cells carry neither --
/// they're static labels, spawned once and never updated.
#[derive(Clone, Copy, Debug, Component, PartialEq, Eq)]
enum FoundationPerfOverlayColumn {
    Value,
    Average,
}

/// Accumulates one stat's samples over the current 1-second window.
#[derive(Clone, Copy, Debug, Default)]
struct FoundationPerfOverlayAccumulator {
    sum: f64,
    count: u32,
}

impl FoundationPerfOverlayAccumulator {
    fn record(&mut self, value: Option<f64>) {
        if let Some(value) = value {
            self.sum += value;
            self.count += 1;
        }
    }

    fn average(&self) -> Option<f64> {
        (self.count > 0).then_some(self.sum / f64::from(self.count))
    }

    fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Tracks a tumbling (reset every real-time second) running average per stat.
///
/// Deliberately not Bevy's own `Diagnostic::average()`, which averages over a
/// fixed *frame count* history window -- at high frame rates that window can
/// cover well under a second, and at low frame rates well over one, so it
/// doesn't mean "the last second" consistently. This resource instead
/// accumulates samples against a real wall-clock timer and only publishes an
/// average once a full second has actually elapsed.
#[derive(Resource)]
struct FoundationPerfOverlayRunningAverages {
    window_elapsed_secs: f32,
    accumulators: [FoundationPerfOverlayAccumulator; PERF_OVERLAY_LINE_COUNT],
    last_averages: [Option<f64>; PERF_OVERLAY_LINE_COUNT],
}

impl Default for FoundationPerfOverlayRunningAverages {
    fn default() -> Self {
        Self {
            window_elapsed_secs: 0.0,
            accumulators: [FoundationPerfOverlayAccumulator::default(); PERF_OVERLAY_LINE_COUNT],
            last_averages: [None; PERF_OVERLAY_LINE_COUNT],
        }
    }
}

const PERF_OVERLAY_AVERAGE_WINDOW_SECS: f32 = 1.0;

/// Number of frame-time bars shown in the history strip. A frame count
/// rather than a fixed time window, so it stays a small, fixed number of UI
/// entities regardless of frame rate (unlike the running-average window
/// above, this graph is about spotting individual spikes, not summarizing a
/// span of wall-clock time).
const FRAME_TIME_HISTORY_SAMPLE_COUNT: usize = 120;

/// Frame time (ms) that maps to the tallest bar; anything slower is clamped
/// to full height rather than growing the graph unboundedly. 50ms is 20fps,
/// comfortably below the 40fps this overlay was built to help diagnose, so a
/// genuinely bad frame still reads as "near the top" rather than "off the
/// chart".
const FRAME_TIME_HISTORY_BAR_CEILING_MS: f32 = 50.0;

/// Pixel height of the tallest possible bar in the history strip.
const FRAME_TIME_HISTORY_BAR_MAX_HEIGHT_PX: f32 = 40.0;

/// Ring buffer of the most recent raw (unsmoothed) per-frame frame-time
/// samples, oldest first. Deliberately reads `Diagnostic::value()` rather
/// than the `smoothed()` value the "Now" column uses elsewhere in this file
/// -- smoothing is exactly what would hide the short stutters this graph
/// exists to surface.
#[derive(Resource, Default)]
struct FoundationPerfOverlayFrameTimeHistory {
    samples: VecDeque<f32>,
}

/// Converts a raw frame-time sample into the bar height that represents it,
/// clamped at [`FRAME_TIME_HISTORY_BAR_CEILING_MS`].
fn frame_time_history_bar_height_px(frame_time_ms: f32) -> f32 {
    let clamped_frame_time_ms = frame_time_ms.min(FRAME_TIME_HISTORY_BAR_CEILING_MS);
    (clamped_frame_time_ms / FRAME_TIME_HISTORY_BAR_CEILING_MS)
        * FRAME_TIME_HISTORY_BAR_MAX_HEIGHT_PX
}

/// Plugin that installs Foundation's performance-stat overlay and its
/// backing diagnostics plugins.
///
/// None of the diagnostics plugins added here are part of Bevy's
/// `DefaultPlugins`, so this plugin adds each one itself, guarded by
/// `is_plugin_added` in case a game already added one directly.
pub struct FoundationPerfOverlayPlugin;

impl Plugin for FoundationPerfOverlayPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        }
        if !app.is_plugin_added::<EntityCountDiagnosticsPlugin>() {
            app.add_plugins(EntityCountDiagnosticsPlugin::default());
        }
        if !app.is_plugin_added::<SystemInformationDiagnosticsPlugin>() {
            app.add_plugins(SystemInformationDiagnosticsPlugin);
        }
        if !app.is_plugin_added::<RenderDiagnosticsPlugin>() {
            app.add_plugins(RenderDiagnosticsPlugin);
        }

        app.register_diagnostic(
            Diagnostic::new(CPU_PROCESS_FRAME_TIME)
                .with_suffix("ms")
                .with_max_history_length(120)
                .with_smoothing_factor(2.0 / 121.0),
        )
        .init_resource::<FoundationCpuProcessFrameTiming>()
        .add_systems(First, mark_cpu_process_frame_start)
        .add_systems(Last, record_cpu_process_frame_time)
        .register_type::<FoundationPerfOverlayState>()
        .init_resource::<FoundationPerfOverlayState>()
        .init_resource::<FoundationPerfOverlayRunningAverages>()
        .init_resource::<FoundationPerfOverlayFrameTimeHistory>()
        .init_resource::<FoundationPerfOverlayHistoryBarEntities>()
        .add_systems(Startup, spawn_perf_overlay)
        .add_systems(
            Update,
            (
                accumulate_perf_overlay_running_averages,
                record_perf_overlay_frame_time_history,
                sync_perf_overlay_visibility,
                refresh_perf_overlay_text,
                refresh_perf_overlay_frame_time_graph,
            ),
        );
    }
}

fn mark_cpu_process_frame_start(mut timing: ResMut<FoundationCpuProcessFrameTiming>) {
    timing.frame_start = Some(Instant::now());
}

fn record_cpu_process_frame_time(
    mut diagnostics: Diagnostics,
    timing: Res<FoundationCpuProcessFrameTiming>,
) {
    let Some(frame_start) = timing.frame_start else {
        return;
    };
    let elapsed_ms = frame_start.elapsed().as_secs_f64() * 1000.0;
    diagnostics.add_measurement(&CPU_PROCESS_FRAME_TIME, || elapsed_ms);
}

/// Marks the perf overlay's full-screen positioning wrapper, used to toggle
/// its `Display`. The wrapper (not the visible panel) is what centers the
/// panel vertically and pins it to the right edge -- see `spawn_perf_overlay`.
#[derive(Clone, Copy, Debug, Component)]
struct FoundationPerfOverlayRoot;

/// Marks one bar entity in the frame-time history strip.
#[derive(Clone, Copy, Debug, Component)]
struct FoundationPerfOverlayHistoryBar;

/// The history strip's bar entities in left-to-right (oldest-to-newest slot)
/// spawn order, so [`refresh_perf_overlay_frame_time_graph`] can walk them
/// alongside [`FoundationPerfOverlayFrameTimeHistory`]'s samples without a
/// query-ordering dependency.
#[derive(Resource, Default)]
struct FoundationPerfOverlayHistoryBarEntities {
    bars_oldest_to_newest: Vec<Entity>,
}

/// Spawns the overlay as a grid panel inside a full-screen, invisible
/// wrapper rather than positioning the panel itself absolutely: pinning to
/// the right edge while centering vertically needs the wrapper's flexbox
/// alignment (`align_items: End` for the cross axis, `justify_content:
/// Center` for the main axis) since Bevy UI has no percentage-based
/// self-centering transform for an absolutely-positioned, auto-sized node.
fn spawn_perf_overlay(
    mut commands: Commands,
    mut history_bar_entities: ResMut<FoundationPerfOverlayHistoryBarEntities>,
) {
    let panel_background = BackgroundColor(Color::srgba(0.02, 0.02, 0.025, 0.85));
    let panel_border = BorderColor::all(Color::srgba(0.25, 0.25, 0.30, 1.0));
    let header_text_color = TextColor(Color::srgba(0.6, 0.65, 0.6, 1.0));
    let text_color = TextColor(Color::srgba(0.85, 0.9, 0.8, 1.0));

    let wrapper_entity = commands
        .spawn((
            Name::new("Foundation Perf Overlay Wrapper"),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                display: Display::None,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::End,
                justify_content: JustifyContent::Center,
                padding: UiRect::right(Val::Px(8.0)),
                ..default()
            },
            GlobalZIndex(9_000),
            FoundationPerfOverlayRoot,
        ))
        .id();

    let panel_entity = commands
        .spawn((
            Name::new("Foundation Perf Overlay Panel"),
            Node {
                display: Display::Grid,
                grid_auto_flow: GridAutoFlow::Row,
                grid_template_columns: vec![
                    RepeatedGridTrack::fr(1, 2.0),
                    RepeatedGridTrack::fr(1, 1.0),
                    RepeatedGridTrack::fr(1, 1.0),
                ],
                column_gap: Val::Px(14.0),
                row_gap: Val::Px(2.0),
                padding: UiRect::all(Val::Px(8.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            panel_background,
            panel_border,
        ))
        .id();
    commands.entity(wrapper_entity).add_child(panel_entity);

    let header_cells = [
        spawn_perf_overlay_label(&mut commands, "Stat", header_text_color, JustifySelf::Start),
        spawn_perf_overlay_label(&mut commands, "Now", header_text_color, JustifySelf::End),
        spawn_perf_overlay_label(&mut commands, "Avg 1s", header_text_color, JustifySelf::End),
    ];
    for header_cell in header_cells {
        commands.entity(panel_entity).add_child(header_cell);
    }

    for line in FoundationPerfOverlayLine::ALL {
        let name_cell =
            spawn_perf_overlay_label(&mut commands, line.label(), text_color, JustifySelf::Start);
        let value_cell = spawn_perf_overlay_data_cell(
            &mut commands,
            line,
            FoundationPerfOverlayColumn::Value,
            text_color,
        );
        let average_cell = spawn_perf_overlay_data_cell(
            &mut commands,
            line,
            FoundationPerfOverlayColumn::Average,
            text_color,
        );
        for cell in [name_cell, value_cell, average_cell] {
            commands.entity(panel_entity).add_child(cell);
        }
    }

    let history_panel_entity = commands
        .spawn((
            Name::new("Foundation Perf Overlay Frame Time History"),
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::FlexEnd,
                column_gap: Val::Px(1.0),
                height: Val::Px(FRAME_TIME_HISTORY_BAR_MAX_HEIGHT_PX + 16.0),
                padding: UiRect::all(Val::Px(8.0)),
                margin: UiRect::top(Val::Px(4.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            panel_background,
            panel_border,
        ))
        .id();
    commands
        .entity(wrapper_entity)
        .add_child(history_panel_entity);

    let history_bar_color = BackgroundColor(Color::srgba(0.5, 0.85, 0.5, 1.0));
    for _ in 0..FRAME_TIME_HISTORY_SAMPLE_COUNT {
        let history_bar_entity = commands
            .spawn((
                Node {
                    width: Val::Px(2.0),
                    // Bars start at zero height and are filled in as samples
                    // arrive -- see refresh_perf_overlay_frame_time_graph.
                    height: Val::Px(0.0),
                    ..default()
                },
                history_bar_color,
                FoundationPerfOverlayHistoryBar,
            ))
            .id();
        commands
            .entity(history_panel_entity)
            .add_child(history_bar_entity);
        history_bar_entities
            .bars_oldest_to_newest
            .push(history_bar_entity);
    }
}

fn spawn_perf_overlay_label(
    commands: &mut Commands,
    text: &str,
    text_color: TextColor,
    justify_self: JustifySelf,
) -> Entity {
    commands
        .spawn((
            Text::new(text),
            TextFont {
                font_size: FontSize::Px(13.0),
                ..default()
            },
            text_color,
            Node {
                justify_self,
                ..default()
            },
        ))
        .id()
}

fn spawn_perf_overlay_data_cell(
    commands: &mut Commands,
    line: FoundationPerfOverlayLine,
    column: FoundationPerfOverlayColumn,
    text_color: TextColor,
) -> Entity {
    commands
        .spawn((
            Text::new("--"),
            TextFont {
                font_size: FontSize::Px(13.0),
                ..default()
            },
            text_color,
            Node {
                justify_self: JustifySelf::End,
                ..default()
            },
            line,
            column,
        ))
        .id()
}

/// Toggles the overlay's `Display`, only writing when the value actually
/// differs -- writing unconditionally would mark `Node` "changed" every
/// frame and force Bevy's UI layout engine to redo work for no reason, the
/// exact performance bug this overlay exists to help catch.
fn sync_perf_overlay_visibility(
    state: Res<FoundationPerfOverlayState>,
    mut roots: Query<&mut Node, With<FoundationPerfOverlayRoot>>,
) {
    let desired_display = if state.visible {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut roots {
        if node.display != desired_display {
            node.display = desired_display;
        }
    }
}

/// Feeds this frame's value for every stat into its running-average
/// accumulator, and publishes+resets once a full real-time second has
/// elapsed. Runs regardless of overlay visibility so the average is already
/// warmed up and accurate the moment the overlay is toggled on, instead of
/// restarting the window from zero.
fn accumulate_perf_overlay_running_averages(
    mut averages: ResMut<FoundationPerfOverlayRunningAverages>,
    diagnostics: Res<DiagnosticsStore>,
    time: Res<Time<Real>>,
) {
    for line in FoundationPerfOverlayLine::ALL {
        let value = current_value(line, &diagnostics);
        averages.accumulators[line.index()].record(value);
    }

    averages.window_elapsed_secs += time.delta_secs();
    if averages.window_elapsed_secs >= PERF_OVERLAY_AVERAGE_WINDOW_SECS {
        averages.window_elapsed_secs = 0.0;
        for line in FoundationPerfOverlayLine::ALL {
            let index = line.index();
            averages.last_averages[index] = averages.accumulators[index].average();
            averages.accumulators[index].reset();
        }
    }
}

/// Appends this frame's raw frame-time sample to the history ring buffer,
/// dropping the oldest sample once the buffer is full. Runs regardless of
/// overlay visibility for the same reason as the running averages above --
/// the graph should already be full of real data the moment it's toggled on.
fn record_perf_overlay_frame_time_history(
    mut history: ResMut<FoundationPerfOverlayFrameTimeHistory>,
    diagnostics: Res<DiagnosticsStore>,
) {
    let Some(latest_frame_time_ms) = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(Diagnostic::value)
    else {
        return;
    };

    history.samples.push_back(latest_frame_time_ms as f32);
    if history.samples.len() > FRAME_TIME_HISTORY_SAMPLE_COUNT {
        history.samples.pop_front();
    }
}

/// Refreshes the overlay's data cells from `DiagnosticsStore` and the
/// running-average resource. Skips all work when the overlay is hidden, and
/// only writes a cell's `Text` when its rendered content actually changed
/// (same reasoning as the visibility sync above).
fn refresh_perf_overlay_text(
    state: Res<FoundationPerfOverlayState>,
    diagnostics: Res<DiagnosticsStore>,
    averages: Res<FoundationPerfOverlayRunningAverages>,
    mut cells: Query<(
        &FoundationPerfOverlayLine,
        &FoundationPerfOverlayColumn,
        &mut Text,
    )>,
) {
    if !state.visible {
        return;
    }

    for (line, column, mut text) in &mut cells {
        let value = match column {
            FoundationPerfOverlayColumn::Value => current_value(*line, &diagnostics),
            FoundationPerfOverlayColumn::Average => averages.last_averages[line.index()],
        };
        let rendered = line.format(value);
        if text.0 != rendered {
            text.0 = rendered;
        }
    }
}

/// Refreshes the frame-time history strip's bar heights from the ring
/// buffer. Skips all work when the overlay is hidden (same reasoning as
/// `refresh_perf_overlay_text`), and only writes a bar's `Node::height` when
/// it actually changed to avoid marking every bar "changed" every frame.
///
/// Bars are laid out oldest-to-newest left-to-right, matching
/// [`FoundationPerfOverlayHistoryBarEntities`]'s spawn order; while the
/// buffer hasn't filled up yet (e.g. just after startup), the oldest bar
/// slots that don't have a sample yet are shown at zero height.
fn refresh_perf_overlay_frame_time_graph(
    state: Res<FoundationPerfOverlayState>,
    history: Res<FoundationPerfOverlayFrameTimeHistory>,
    history_bar_entities: Res<FoundationPerfOverlayHistoryBarEntities>,
    mut bars: Query<&mut Node, With<FoundationPerfOverlayHistoryBar>>,
) {
    if !state.visible {
        return;
    }

    let bar_slot_count = history_bar_entities.bars_oldest_to_newest.len();
    let filled_sample_count = history.samples.len();
    let empty_slot_count = bar_slot_count.saturating_sub(filled_sample_count);

    for (bar_slot_index, &bar_entity) in history_bar_entities
        .bars_oldest_to_newest
        .iter()
        .enumerate()
    {
        let Ok(mut bar_node) = bars.get_mut(bar_entity) else {
            continue;
        };
        let target_height_px = if bar_slot_index < empty_slot_count {
            0.0
        } else {
            let sample_index = bar_slot_index - empty_slot_count;
            frame_time_history_bar_height_px(history.samples[sample_index])
        };
        if bar_node.height != Val::Px(target_height_px) {
            bar_node.height = Val::Px(target_height_px);
        }
    }
}

/// Resolves one stat's current raw value from `DiagnosticsStore`, with no
/// formatting -- shared by the "Now" column and the running-average
/// accumulator so both read from exactly the same source of truth.
fn current_value(line: FoundationPerfOverlayLine, diagnostics: &DiagnosticsStore) -> Option<f64> {
    match line {
        FoundationPerfOverlayLine::Fps => diagnostics
            .get(&FrameTimeDiagnosticsPlugin::FPS)
            .and_then(Diagnostic::smoothed),
        FoundationPerfOverlayLine::FrameTime => diagnostics
            .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
            .and_then(Diagnostic::smoothed),
        FoundationPerfOverlayLine::CpuProcess => diagnostics
            .get(&CPU_PROCESS_FRAME_TIME)
            .and_then(Diagnostic::smoothed),
        FoundationPerfOverlayLine::CpuRender => {
            let (_, _, render_cpu_total_ms, render_cpu_sample_count) =
                sum_render_pass_diagnostics(diagnostics);
            (render_cpu_sample_count > 0).then_some(render_cpu_total_ms)
        }
        FoundationPerfOverlayLine::GpuRender => {
            let (gpu_total_ms, gpu_sample_count, _, _) = sum_render_pass_diagnostics(diagnostics);
            (gpu_sample_count > 0).then_some(gpu_total_ms)
        }
        FoundationPerfOverlayLine::Memory => diagnostics
            .get(&SystemInformationDiagnosticsPlugin::PROCESS_MEM_USAGE)
            .and_then(Diagnostic::value)
            .map(|mem_gib| mem_gib * 1024.0),
        FoundationPerfOverlayLine::Entities => diagnostics
            .get(&EntityCountDiagnosticsPlugin::ENTITY_COUNT)
            .and_then(Diagnostic::value),
    }
}

/// Returns `(gpu_total_ms, gpu_sample_count, render_cpu_total_ms, render_cpu_sample_count)`,
/// summed across every `render/<pass>/elapsed_cpu`/`elapsed_gpu` diagnostic.
/// GPU timestamps are only real on Vulkan/DX12 (see `RenderDiagnosticsPlugin`'s
/// docs) -- other backends never populate `elapsed_gpu` paths at all, which
/// callers use to distinguish "no GPU timing on this backend" from "CPU
/// Render" having no data either.
fn sum_render_pass_diagnostics(diagnostics: &DiagnosticsStore) -> (f64, usize, f64, usize) {
    let mut gpu_total_ms = 0.0;
    let mut gpu_sample_count = 0;
    let mut render_cpu_total_ms = 0.0;
    let mut render_cpu_sample_count = 0;

    for diagnostic in diagnostics.iter() {
        let path = diagnostic.path().as_str();
        if !path.starts_with("render/") {
            continue;
        }
        let Some(value) = diagnostic.value() else {
            continue;
        };
        if path.ends_with("/elapsed_gpu") {
            gpu_total_ms += value;
            gpu_sample_count += 1;
        } else if path.ends_with("/elapsed_cpu") {
            render_cpu_total_ms += value;
            render_cpu_sample_count += 1;
        }
    }

    (
        gpu_total_ms,
        gpu_sample_count,
        render_cpu_total_ms,
        render_cpu_sample_count,
    )
}

/// Toggles the performance-stat overlay on/off.
#[crate::console_command(name = "stat.perf")]
pub fn stat_perf(mut state: ResMut<FoundationPerfOverlayState>) {
    state.visible = !state.visible;
}

/// Writes a one-shot snapshot of every performance stat's current and
/// 1-second-average value to the log, whether or not the overlay is
/// currently visible -- useful for capturing perf evidence (for example, in
/// a bug report) without needing a screenshot.
#[crate::console_command(name = "stat.perf.dump")]
fn stat_perf_dump(
    diagnostics: Res<DiagnosticsStore>,
    averages: Res<FoundationPerfOverlayRunningAverages>,
) {
    info!("Foundation perf snapshot:");
    for line in FoundationPerfOverlayLine::ALL {
        let now_value = current_value(line, &diagnostics);
        let average_value = averages.last_averages[line.index()];
        info!(
            "  {}: now={} avg1s={}",
            line.label(),
            line.format(now_value),
            line.format(average_value)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostic_with_value(path: &'static str, value: f64) -> Diagnostic {
        let mut diagnostic = Diagnostic::new(DiagnosticPath::const_new(path));
        diagnostic.add_measurement(bevy::diagnostic::DiagnosticMeasurement {
            time: Instant::now(),
            value,
        });
        diagnostic
    }

    #[test]
    fn stat_perf_command_toggles_overlay_visibility() {
        let mut app = App::new();
        app.init_resource::<FoundationPerfOverlayState>();

        assert!(!app.world().resource::<FoundationPerfOverlayState>().visible);

        let mut state = app.world_mut().resource_mut::<FoundationPerfOverlayState>();
        state.visible = !state.visible;
        assert!(app.world().resource::<FoundationPerfOverlayState>().visible);
    }

    #[test]
    fn sync_visibility_shows_and_hides_the_overlay_root() {
        let mut app = App::new();
        app.insert_resource(FoundationPerfOverlayState { visible: false });
        app.add_systems(Update, sync_perf_overlay_visibility);

        let root_entity = app
            .world_mut()
            .spawn((Node::default(), FoundationPerfOverlayRoot))
            .id();

        app.update();
        assert_eq!(
            app.world().get::<Node>(root_entity).unwrap().display,
            Display::None
        );

        app.world_mut()
            .resource_mut::<FoundationPerfOverlayState>()
            .visible = true;
        app.update();
        assert_eq!(
            app.world().get::<Node>(root_entity).unwrap().display,
            Display::Flex
        );
    }

    #[test]
    fn sync_visibility_does_not_rewrite_node_when_already_correct() {
        // Regression test for the exact class of bug this overlay exists to
        // help catch: writing `Node` unconditionally every frame marks it
        // "changed" every frame regardless of whether the value differs,
        // forcing Bevy's UI layout engine to redo work for nothing.
        let mut app = App::new();
        app.insert_resource(FoundationPerfOverlayState { visible: true });
        app.add_systems(Update, sync_perf_overlay_visibility);

        let root_entity = app
            .world_mut()
            .spawn((Node::default(), FoundationPerfOverlayRoot))
            .id();

        app.update();
        let tick_after_first_write = app
            .world()
            .entity(root_entity)
            .get_change_ticks::<Node>()
            .unwrap()
            .changed;

        app.update();
        let tick_after_second_frame = app
            .world()
            .entity(root_entity)
            .get_change_ticks::<Node>()
            .unwrap()
            .changed;

        assert_eq!(
            tick_after_first_write, tick_after_second_frame,
            "Node must not be marked changed on a frame where visibility already matched the state"
        );
    }

    #[test]
    fn current_value_reads_fps_and_frame_time() {
        let mut diagnostics = DiagnosticsStore::default();
        diagnostics.add(diagnostic_with_value("fps", 60.0));
        diagnostics.add(diagnostic_with_value("frame_time", 16.6));

        assert_eq!(
            current_value(FoundationPerfOverlayLine::Fps, &diagnostics),
            Some(60.0)
        );
        assert_eq!(
            current_value(FoundationPerfOverlayLine::FrameTime, &diagnostics),
            Some(16.6)
        );
    }

    #[test]
    fn current_value_reads_the_custom_cpu_process_diagnostic() {
        let mut diagnostics = DiagnosticsStore::default();
        diagnostics.add(diagnostic_with_value(
            "foundation/cpu_process_frame_time",
            4.2,
        ));

        assert_eq!(
            current_value(FoundationPerfOverlayLine::CpuProcess, &diagnostics),
            Some(4.2)
        );
    }

    #[test]
    fn current_value_converts_memory_from_gib_to_mb() {
        let mut diagnostics = DiagnosticsStore::default();
        diagnostics.add(diagnostic_with_value("process/mem_usage", 0.5));

        assert_eq!(
            current_value(FoundationPerfOverlayLine::Memory, &diagnostics),
            Some(512.0)
        );
    }

    #[test]
    fn sum_render_pass_diagnostics_only_counts_render_elapsed_paths() {
        let mut diagnostics = DiagnosticsStore::default();
        diagnostics.add(diagnostic_with_value("render/main_pass/elapsed_cpu", 1.0));
        diagnostics.add(diagnostic_with_value("render/main_pass/elapsed_gpu", 2.5));
        diagnostics.add(diagnostic_with_value("render/ui_pass/elapsed_cpu", 0.5));
        // Unrelated diagnostics must not be swept into the render totals.
        diagnostics.add(diagnostic_with_value("fps", 60.0));
        diagnostics.add(diagnostic_with_value("entity_count", 100.0));

        let (gpu_total, gpu_count, cpu_total, cpu_count) =
            sum_render_pass_diagnostics(&diagnostics);

        assert_eq!(gpu_count, 1);
        assert!((gpu_total - 2.5).abs() < f64::EPSILON);
        assert_eq!(cpu_count, 2);
        assert!((cpu_total - 1.5).abs() < f64::EPSILON);
    }

    #[test]
    fn current_value_is_none_for_gpu_render_when_no_gpu_timestamps_exist() {
        let mut diagnostics = DiagnosticsStore::default();
        diagnostics.add(diagnostic_with_value("render/main_pass/elapsed_cpu", 1.25));

        assert_eq!(
            current_value(FoundationPerfOverlayLine::CpuRender, &diagnostics),
            Some(1.25)
        );
        assert_eq!(
            current_value(FoundationPerfOverlayLine::GpuRender, &diagnostics),
            None
        );
    }

    #[test]
    fn format_shows_placeholder_for_missing_values() {
        assert_eq!(FoundationPerfOverlayLine::Fps.format(None), "--");
    }

    #[test]
    fn format_uses_expected_precision_per_stat() {
        assert_eq!(FoundationPerfOverlayLine::Fps.format(Some(60.04)), "60.0");
        assert_eq!(
            FoundationPerfOverlayLine::Entities.format(Some(596.0)),
            "596"
        );
        assert_eq!(FoundationPerfOverlayLine::Memory.format(Some(523.7)), "524");
        assert_eq!(
            FoundationPerfOverlayLine::CpuProcess.format(Some(23.128)),
            "23.13"
        );
    }

    #[test]
    fn accumulator_averages_recorded_values_and_ignores_missing_samples() {
        let mut accumulator = FoundationPerfOverlayAccumulator::default();
        assert_eq!(accumulator.average(), None);

        accumulator.record(Some(10.0));
        accumulator.record(None);
        accumulator.record(Some(20.0));

        assert_eq!(accumulator.average(), Some(15.0));

        accumulator.reset();
        assert_eq!(accumulator.average(), None);
    }

    #[test]
    fn running_averages_publish_only_after_a_full_second_elapses() {
        let mut app = App::new();
        app.init_resource::<DiagnosticsStore>();
        app.insert_resource(Time::<Real>::default());
        app.init_resource::<FoundationPerfOverlayRunningAverages>();
        app.world_mut()
            .resource_mut::<DiagnosticsStore>()
            .add(diagnostic_with_value("fps", 60.0));
        app.add_systems(Update, accumulate_perf_overlay_running_averages);

        // Advance by less than a second: still accumulating, nothing published yet.
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(std::time::Duration::from_millis(400));
        app.update();
        assert_eq!(
            app.world()
                .resource::<FoundationPerfOverlayRunningAverages>()
                .last_averages[FoundationPerfOverlayLine::Fps.index()],
            None
        );

        // Cross the 1-second mark: the window publishes and resets.
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(std::time::Duration::from_millis(700));
        app.update();
        assert_eq!(
            app.world()
                .resource::<FoundationPerfOverlayRunningAverages>()
                .last_averages[FoundationPerfOverlayLine::Fps.index()],
            Some(60.0)
        );
    }

    #[test]
    fn frame_time_history_bar_height_clamps_at_the_ceiling() {
        let far_above_ceiling_ms = FRAME_TIME_HISTORY_BAR_CEILING_MS * 2.0;
        assert_eq!(
            frame_time_history_bar_height_px(far_above_ceiling_ms),
            FRAME_TIME_HISTORY_BAR_MAX_HEIGHT_PX
        );
    }

    #[test]
    fn frame_time_history_bar_height_scales_linearly_below_the_ceiling() {
        let half_ceiling_frame_time_ms = FRAME_TIME_HISTORY_BAR_CEILING_MS / 2.0;
        let expected_half_height_px = FRAME_TIME_HISTORY_BAR_MAX_HEIGHT_PX / 2.0;
        let actual_height_px = frame_time_history_bar_height_px(half_ceiling_frame_time_ms);
        assert!((actual_height_px - expected_half_height_px).abs() < f32::EPSILON);
    }

    #[test]
    fn record_frame_time_history_appends_samples_and_caps_at_the_sample_count() {
        let mut app = App::new();
        app.init_resource::<DiagnosticsStore>();
        app.init_resource::<FoundationPerfOverlayFrameTimeHistory>();
        app.world_mut()
            .resource_mut::<DiagnosticsStore>()
            .add(diagnostic_with_value("frame_time", 0.0));
        app.add_systems(Update, record_perf_overlay_frame_time_history);

        let recorded_sample_count = FRAME_TIME_HISTORY_SAMPLE_COUNT + 5;
        for sample_index in 0..recorded_sample_count {
            let frame_time_ms = sample_index as f64;
            app.world_mut()
                .resource_mut::<DiagnosticsStore>()
                .get_mut(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
                .unwrap()
                .add_measurement(bevy::diagnostic::DiagnosticMeasurement {
                    time: Instant::now(),
                    value: frame_time_ms,
                });
            app.update();
        }

        let history = app
            .world()
            .resource::<FoundationPerfOverlayFrameTimeHistory>();
        assert_eq!(history.samples.len(), FRAME_TIME_HISTORY_SAMPLE_COUNT);

        let oldest_surviving_sample_index = recorded_sample_count - FRAME_TIME_HISTORY_SAMPLE_COUNT;
        assert_eq!(
            history.samples.front().copied(),
            Some(oldest_surviving_sample_index as f32)
        );
        assert_eq!(
            history.samples.back().copied(),
            Some((recorded_sample_count - 1) as f32)
        );
    }

    #[test]
    fn refresh_frame_time_graph_sets_bar_height_from_history_when_visible() {
        let mut app = App::new();
        app.insert_resource(FoundationPerfOverlayState { visible: true });
        let mut history = FoundationPerfOverlayFrameTimeHistory::default();
        history
            .samples
            .push_back(FRAME_TIME_HISTORY_BAR_CEILING_MS / 2.0);
        app.insert_resource(history);

        let history_bar_entity = app
            .world_mut()
            .spawn((Node::default(), FoundationPerfOverlayHistoryBar))
            .id();
        app.insert_resource(FoundationPerfOverlayHistoryBarEntities {
            bars_oldest_to_newest: vec![history_bar_entity],
        });
        app.add_systems(Update, refresh_perf_overlay_frame_time_graph);

        app.update();

        let expected_height_px = FRAME_TIME_HISTORY_BAR_MAX_HEIGHT_PX / 2.0;
        assert_eq!(
            app.world().get::<Node>(history_bar_entity).unwrap().height,
            Val::Px(expected_height_px)
        );
    }

    #[test]
    fn refresh_frame_time_graph_leaves_unfilled_slots_at_zero_height() {
        let mut app = App::new();
        app.insert_resource(FoundationPerfOverlayState { visible: true });
        app.insert_resource(FoundationPerfOverlayFrameTimeHistory::default());

        let history_bar_entity = app
            .world_mut()
            .spawn((Node::default(), FoundationPerfOverlayHistoryBar))
            .id();
        app.insert_resource(FoundationPerfOverlayHistoryBarEntities {
            bars_oldest_to_newest: vec![history_bar_entity],
        });
        app.add_systems(Update, refresh_perf_overlay_frame_time_graph);

        app.update();

        assert_eq!(
            app.world().get::<Node>(history_bar_entity).unwrap().height,
            Val::Px(0.0)
        );
    }

    #[test]
    fn refresh_frame_time_graph_skips_work_when_overlay_hidden() {
        let mut app = App::new();
        app.insert_resource(FoundationPerfOverlayState { visible: false });
        let mut history = FoundationPerfOverlayFrameTimeHistory::default();
        history.samples.push_back(FRAME_TIME_HISTORY_BAR_CEILING_MS);
        app.insert_resource(history);

        let unrelated_existing_height_px = 999.0;
        let history_bar_entity = app
            .world_mut()
            .spawn((
                Node {
                    height: Val::Px(unrelated_existing_height_px),
                    ..default()
                },
                FoundationPerfOverlayHistoryBar,
            ))
            .id();
        app.insert_resource(FoundationPerfOverlayHistoryBarEntities {
            bars_oldest_to_newest: vec![history_bar_entity],
        });
        app.add_systems(Update, refresh_perf_overlay_frame_time_graph);

        app.update();

        assert_eq!(
            app.world().get::<Node>(history_bar_entity).unwrap().height,
            Val::Px(unrelated_existing_height_px)
        );
    }
}
