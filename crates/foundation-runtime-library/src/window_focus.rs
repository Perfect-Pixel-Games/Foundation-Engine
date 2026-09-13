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
//! module just calls it once, as soon as the windowing backend has actually
//! created the OS window.

use bevy::prelude::*;
use bevy::winit::WinitWindows;

/// Forces the primary window to take OS focus the moment it exists.
///
/// Added automatically by [`crate::FoundationPlugin`]; games shouldn't need
/// to add this directly.
#[derive(Default)]
pub struct FoundationWindowFocusPlugin;

impl Plugin for FoundationWindowFocusPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, force_window_focus_once_created);
    }
}

/// Calls `winit`'s window-focus-stealing trick once, the first frame the
/// windowing backend's OS window becomes available.
///
/// `WinitWindows` is a `NonSend` resource that only exists once
/// `bevy_winit`'s plugin has installed a real event loop, so this is
/// `Option`-wrapped to stay a no-op in headless test apps that only add
/// `MinimalPlugins`.
fn force_window_focus_once_created(
    mut already_focused: Local<bool>,
    windows: Query<Entity, With<Window>>,
    winit_windows: Option<NonSend<WinitWindows>>,
) {
    if *already_focused {
        return;
    }
    let Some(winit_windows) = winit_windows else {
        return;
    };
    let Some(window_entity) = windows.iter().next() else {
        return;
    };
    let Some(winit_window) = winit_windows.get_window(window_entity) else {
        return;
    };

    winit_window.focus_window();
    *already_focused = true;
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
