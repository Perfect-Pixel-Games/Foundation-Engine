//! Installs Avian3D physics simulation for Foundation games.

use avian3d::prelude::*;
use bevy::prelude::*;

/// Installs Avian3D's physics simulation.
///
/// Centralized here so every Foundation game gets the same physics backend
/// without each game crate needing to know or choose which physics engine is
/// in use.
#[derive(Default)]
pub struct FoundationPhysicsPlugin;

impl Plugin for FoundationPhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(PhysicsPlugins::default());
    }
}
