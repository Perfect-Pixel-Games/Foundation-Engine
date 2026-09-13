//! Forces the primary window to take OS focus as soon as it's created.
//!
//! Foundation games are normally launched as a child process (of
//! `foundation-build`'s own child process, or of `cargo run` directly),
//! often after however long the build step took -- long enough that the
//! player has switched focus to another window by the time the game's
//! window actually appears. Windows in particular refuses to grant a newly
//! created window foreground focus once the process that spawned it is no
//! longer the current foreground process, so without this, the game can
//! silently open behind whatever else is on screen. `winit`'s
//! `Window::focus_window` already implements the platform-specific
//! "steal focus" trick (a synthesized Alt keypress on Windows satisfies the
//! OS's foreground-lock heuristic); this module calls it as soon as the OS
//! window is actually showing.

use bevy::{ecs::system::NonSendMarker, prelude::*, winit::WINIT_WINDOWS};

/// How many consecutive frames [`force_window_focus_once_visible`] calls
/// `winit_window.focus_window()` after the OS window first reports itself
/// visible, before giving up. Bounded rather than unbounded so a window
/// that somehow never takes focus doesn't keep synthesizing Alt keypresses
/// forever and disrupting whatever application the player is actually
/// using.
const MAX_FOCUS_ATTEMPTS: u8 = 5;

/// Forces the primary window to take OS focus the moment it exists.
///
/// Added automatically by [`crate::FoundationPlugin`]; games shouldn't need
/// to add this directly.
#[derive(Default)]
pub struct FoundationWindowFocusPlugin;

impl Plugin for FoundationWindowFocusPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, force_window_focus_once_visible);
    }
}

/// Calls `winit`'s window-focus-stealing trick for a few frames once the OS
/// window reports itself visible.
///
/// `bevy_winit` creates the OS window invisible and only makes it visible a
/// frame or two later (a UIA/AccessKit requirement -- see
/// `bevy_winit::winit_windows`), so calling `focus_window()` on the very
/// first frame the window entity exists is frequently a silent no-op:
/// `winit`'s Windows implementation only performs the focus-stealing trick
/// when the window is already visible. This waits for
/// `winit_window.is_visible() == Some(true)` before calling it.
///
/// This deliberately does not stop early once Bevy's `Window::focused` looks
/// true: that field defaults to `true` at spawn (it records the *desired*
/// state, not a confirmed OS event) and never flips if the window opens
/// unfocused and simply stays that way, since no real focus-lost transition
/// ever fires to correct it. Instead this always spends its full attempt
/// budget the first few frames the window is visible, which is cheap and
/// harmless if the window was already focused (`focus_window()` itself is a
/// no-op in that case).
///
/// Unlike older Bevy versions, `bevy_winit` 0.19 does not expose
/// `WinitWindows` as an ECS resource at all -- it's kept in the
/// [`WINIT_WINDOWS`] thread-local instead (see that item's docs), populated
/// only on the thread running the winit event loop. `NonSendMarker` forces
/// this system onto that same thread, matching the pattern
/// `bevy_winit`'s own internal systems (e.g. `changed_windows`) use to
/// access it safely.
fn force_window_focus_once_visible(
    mut attempts_remaining: Local<Option<u8>>,
    windows: Query<Entity, With<Window>>,
    _non_send_marker: NonSendMarker,
) {
    let remaining = attempts_remaining.get_or_insert(MAX_FOCUS_ATTEMPTS);
    if *remaining == 0 {
        return;
    }
    let Some(window_entity) = windows.iter().next() else {
        return;
    };

    let focus_attempted = WINIT_WINDOWS.with_borrow(|winit_windows| {
        let Some(winit_window) = winit_windows.get_window(window_entity) else {
            return false;
        };
        if winit_window.is_visible() != Some(true) {
            // Not showing yet -- don't spend an attempt on a guaranteed no-op.
            return false;
        }

        winit_window.focus_window();
        true
    });

    if focus_attempted {
        *remaining -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn does_nothing_without_a_real_os_window() {
        // `MinimalPlugins` never drives a real winit event loop, so
        // `WINIT_WINDOWS` stays at its empty default on this thread. This
        // should not panic despite there being no OS window to find.
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.world_mut().spawn(Window::default());
        app.add_plugins(FoundationWindowFocusPlugin);

        app.update();
        app.update();
    }
}
