//! Forces the primary window to take OS focus as soon as it's created.
//!
//! Foundation games are normally launched as a child process of
//! `foundation-build`'s own child process, some time after however long the
//! build step took -- often long enough that the player has switched focus
//! to another window by the time the game's window actually appears.
//! Windows in particular refuses to grant a newly created window foreground
//! focus once the process that spawned it is no longer the current
//! foreground process, so without this, the game can silently open behind
//! whatever else is on screen. `winit`'s `Window::focus_window` already
//! implements the platform-specific "steal focus" trick (a synthesized Alt
//! keypress on Windows satisfies the OS's foreground-lock heuristic); this
//! module calls it as soon as the OS window is actually showing.

use bevy::prelude::*;
use bevy::winit::WinitWindows;

/// How many times [`force_window_focus_once_visible`] retries
/// `winit_window.focus_window()` after the OS window first reports itself
/// visible, before giving up. Bounded rather than unbounded so a window that
/// somehow never reports itself focused doesn't keep synthesizing Alt
/// keypresses forever and disrupting whatever application the player is
/// actually using.
const MAX_FOCUS_ATTEMPTS: u8 = 10;

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

/// Calls `winit`'s window-focus-stealing trick every frame once the OS
/// window reports itself visible, until Bevy's own `Window::focused`
/// reflects success or [`MAX_FOCUS_ATTEMPTS`] is reached.
///
/// `bevy_winit` creates the OS window invisible and only makes it visible a
/// frame or two later (a UIA/AccessKit requirement -- see
/// `bevy_winit::winit_windows`), so calling `focus_window()` on the very
/// first frame the window entity exists is frequently a silent no-op:
/// `winit`'s Windows implementation only performs the focus-stealing trick
/// when the window is already visible. Waiting for
/// `winit_window.is_visible() == Some(true)` before calling it (and
/// retrying a bounded number of times if the OS still hasn't reported focus
/// back) avoids that race.
///
/// `WinitWindows` is a `NonSend` resource that only exists once
/// `bevy_winit`'s plugin has installed a real event loop, so this is
/// `Option`-wrapped to stay a no-op in headless test apps that only add
/// `MinimalPlugins`.
fn force_window_focus_once_visible(
    mut attempts_remaining: Local<Option<u8>>,
    windows: Query<(Entity, &Window)>,
    winit_windows: Option<NonSend<WinitWindows>>,
) {
    let remaining = attempts_remaining.get_or_insert(MAX_FOCUS_ATTEMPTS);
    if *remaining == 0 {
        return;
    }

    let Some(winit_windows) = winit_windows else {
        return;
    };
    let Some((window_entity, window)) = windows.iter().next() else {
        return;
    };

    if window.focused {
        *remaining = 0;
        return;
    }

    let Some(winit_window) = winit_windows.get_window(window_entity) else {
        return;
    };
    if winit_window.is_visible() != Some(true) {
        // Not showing yet -- don't spend an attempt on a guaranteed no-op.
        return;
    }

    winit_window.focus_window();
    *remaining -= 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn does_nothing_without_a_windowing_backend() {
        // `MinimalPlugins` never inserts `WinitWindows`, matching how
        // Foundation's other headless tests exercise `FoundationPlugin`.
        // This should not panic despite the missing `NonSend` resource.
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.world_mut().spawn(Window::default());
        app.add_plugins(FoundationWindowFocusPlugin);

        app.update();
        app.update();
    }
}
