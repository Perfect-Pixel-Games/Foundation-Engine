//! Reusable free-fly debug camera for Foundation games.
//!
//! Any Foundation game can spawn a camera controllable with WASD + up/down
//! movement and mouse-look by adding [`FoundationFreeFlyCameraPlugin`] and
//! including [`foundation_free_fly_camera_bundle`] in the camera's spawn
//! bundle. This is not added automatically by [`crate::FoundationPlugin`],
//! since not every game wants a free-fly camera available.

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use bevy_enhanced_input::prelude::*;

/// Keeps the camera from pitching perfectly vertical, which would make yaw
/// direction ambiguous (gimbal lock at the poles).
const FOUNDATION_FREE_FLY_CAMERA_PITCH_LIMIT_MARGIN: f32 = 0.01;

/// Installs the free-fly debug camera's input context and movement system.
///
/// Opt-in: add this plugin only to games or scenes that want a free-fly
/// camera available, then spawn a camera entity using
/// [`foundation_free_fly_camera_bundle`].
#[derive(Default)]
pub struct FoundationFreeFlyCameraPlugin;

impl Plugin for FoundationFreeFlyCameraPlugin {
    fn build(&self, app: &mut App) {
        // Enhanced input must exist before `add_input_context` runs, and this
        // plugin is also used on its own in tests that skip `FoundationPlugin`.
        crate::add_enhanced_input_plugin_if_missing(app);

        app.add_input_context::<FoundationFreeFlyCameraInput>()
            .add_systems(Update, move_foundation_free_fly_cameras);
    }
}

/// Reusable input context for Foundation's free-fly debug camera.
///
/// Spawned together with a camera's other components via
/// [`foundation_free_fly_camera_bundle`], not added to an existing entity
/// after the fact, since its actions/bindings must be spawned in the same
/// command as the context component.
#[derive(Component)]
#[require(FoundationFreeFlyCameraSettings, FoundationFreeFlyCameraOrientation)]
pub struct FoundationFreeFlyCameraInput;

/// Tunable speed/sensitivity for a free-fly camera entity.
#[derive(Component, Clone, Copy, Debug)]
pub struct FoundationFreeFlyCameraSettings {
    /// Movement speed in world units per second.
    pub move_speed: f32,
    /// Radians of rotation applied per pixel of mouse motion.
    pub look_sensitivity: f32,
}

impl Default for FoundationFreeFlyCameraSettings {
    fn default() -> Self {
        Self {
            move_speed: 10.0,
            look_sensitivity: 0.002,
        }
    }
}

/// Accumulated yaw/pitch for a free-fly camera entity.
///
/// `Transform.rotation` alone can't be incrementally nudged by mouse deltas
/// without re-deriving yaw/pitch from a quaternion each frame, so this
/// component tracks them directly instead.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct FoundationFreeFlyCameraOrientation {
    /// Rotation around the world Y axis, in radians.
    pub yaw: f32,
    /// Rotation around the camera's local X axis, in radians.
    pub pitch: f32,
}

/// Horizontal movement action (WASD). Output `x` is strafe, `y` is forward/back.
#[derive(InputAction)]
#[action_output(Vec2)]
pub struct FoundationFreeFlyCameraMove;

/// Vertical movement action. Space is `+1`, either Shift key is `-1`.
#[derive(InputAction)]
#[action_output(f32)]
pub struct FoundationFreeFlyCameraVertical;

/// Mouse-look action, bound to raw mouse motion.
#[derive(InputAction)]
#[action_output(Vec2)]
pub struct FoundationFreeFlyCameraLook;

/// Returns the bundle a game spawns to make an entity a free-fly camera.
///
/// Combine with [`Camera3d`] and any other camera components:
///
/// ```ignore
/// commands.spawn((
///     Camera3d::default(),
///     foundation_free_fly_camera_bundle(),
/// ));
/// ```
pub fn foundation_free_fly_camera_bundle() -> impl Bundle {
    (
        FoundationFreeFlyCameraInput,
        actions!(FoundationFreeFlyCameraInput[
            (
                Action::<FoundationFreeFlyCameraMove>::new(),
                DeadZone::default(),
                Bindings::spawn(Cardinal::wasd_keys()),
            ),
            (
                Action::<FoundationFreeFlyCameraVertical>::new(),
                bindings![
                    KeyCode::Space,
                    (KeyCode::ShiftLeft, Negate::all()),
                    (KeyCode::ShiftRight, Negate::all()),
                ],
            ),
            (
                Action::<FoundationFreeFlyCameraLook>::new(),
                bindings![Binding::mouse_motion()],
            ),
        ]),
    )
}

type FoundationFreeFlyCameraQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        &'static mut Transform,
        &'static mut FoundationFreeFlyCameraOrientation,
        &'static FoundationFreeFlyCameraSettings,
        &'static Actions<FoundationFreeFlyCameraInput>,
    ),
>;

fn move_foundation_free_fly_cameras(
    time: Res<Time>,
    mut free_fly_cameras: FoundationFreeFlyCameraQuery,
    move_actions: Query<&Action<FoundationFreeFlyCameraMove>>,
    vertical_actions: Query<&Action<FoundationFreeFlyCameraVertical>>,
    look_actions: Query<&Action<FoundationFreeFlyCameraLook>>,
) {
    let elapsed_seconds = time.delta_secs();
    for (mut transform, mut orientation, settings, camera_actions) in &mut free_fly_cameras {
        // `Action<A>` lives on a separate entity related to the context entity
        // via the `Actions<C>`/`ActionOf<C>` relationship, not as a component
        // on the context entity itself, hence the `iter_many` lookups below.
        let Some(move_action) = move_actions.iter_many(camera_actions).next() else {
            continue;
        };
        let Some(vertical_action) = vertical_actions.iter_many(camera_actions).next() else {
            continue;
        };
        let Some(look_action) = look_actions.iter_many(camera_actions).next() else {
            continue;
        };

        // Mouse motion is in screen pixels; negate so moving the mouse right
        // yaws right and moving it up pitches up.
        orientation.yaw -= look_action.x * settings.look_sensitivity;
        orientation.pitch = (orientation.pitch - look_action.y * settings.look_sensitivity).clamp(
            -FRAC_PI_2 + FOUNDATION_FREE_FLY_CAMERA_PITCH_LIMIT_MARGIN,
            FRAC_PI_2 - FOUNDATION_FREE_FLY_CAMERA_PITCH_LIMIT_MARGIN,
        );
        transform.rotation =
            Quat::from_euler(EulerRot::YXZ, orientation.yaw, orientation.pitch, 0.0);

        // WASD's "forward" (+Y from the Cardinal preset) maps to -Z in Bevy's
        // right-handed space, per `bevy_enhanced_input`'s own preset docs.
        let local_move_direction = Vec3::new(move_action.x, 0.0, -move_action.y);
        let mut world_move_direction = transform.rotation * local_move_direction;
        // Vertical movement is handled separately in world space below, so
        // strip any vertical component that came from pitching the camera.
        world_move_direction.y = 0.0;
        if world_move_direction.length_squared() > 0.0 {
            world_move_direction = world_move_direction.normalize();
        }

        let vertical_move_amount: f32 = **vertical_action;
        let world_movement = world_move_direction + Vec3::Y * vertical_move_amount;
        transform.translation += world_movement * settings.move_speed * elapsed_seconds;
    }
}

#[cfg(test)]
mod tests {
    use bevy::input::mouse::AccumulatedMouseMotion;

    use super::*;

    fn test_app_with_free_fly_camera() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<AccumulatedMouseMotion>();
        app.add_plugins(FoundationFreeFlyCameraPlugin);
        // `bevy_enhanced_input` finishes context setup in `Plugin::finish`, which
        // only runs automatically through `App::run()`. Tests that drive the app
        // with bare `update()` calls must invoke it manually first.
        app.finish();
        app.cleanup();

        let free_fly_camera_entity = app
            .world_mut()
            .spawn((Transform::default(), foundation_free_fly_camera_bundle()))
            .id();
        // Let the freshly spawned context/action/binding entities settle
        // before the test starts asserting on action values.
        app.update();

        (app, free_fly_camera_entity)
    }

    #[test]
    fn pressing_w_moves_camera_forward() {
        let (mut app, free_fly_camera_entity) = test_app_with_free_fly_camera();

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        app.update();

        // `TimePlugin` (part of `MinimalPlugins`) recomputes `Time`'s delta from
        // the real clock every frame, so this test can't force an exact elapsed
        // duration -- it instead checks the movement formula against whatever
        // real (small, non-deterministic) delta actually elapsed.
        let elapsed_seconds = app.world().resource::<Time>().delta_secs();
        let camera_transform = app
            .world()
            .get::<Transform>(free_fly_camera_entity)
            .expect("free-fly camera should have a Transform");
        let default_move_speed = FoundationFreeFlyCameraSettings::default().move_speed;
        let expected_z = -default_move_speed * elapsed_seconds;
        // Facing the default -Z direction, "forward" (W) should decrease Z by
        // move_speed * elapsed_seconds.
        assert!(
            (camera_transform.translation.z - expected_z).abs() < 0.0001,
            "expected the camera to move to z={expected_z}, got {:?}",
            camera_transform.translation
        );
        assert!(camera_transform.translation.x.abs() < 0.0001);
        assert!(camera_transform.translation.y.abs() < 0.01);
    }

    #[test]
    fn mouse_motion_rotates_camera_yaw() {
        let (mut app, free_fly_camera_entity) = test_app_with_free_fly_camera();

        app.world_mut()
            .resource_mut::<AccumulatedMouseMotion>()
            .delta = Vec2::new(100.0, 0.0);
        app.update();

        let orientation = app
            .world()
            .get::<FoundationFreeFlyCameraOrientation>(free_fly_camera_entity)
            .expect("free-fly camera should have an orientation");
        // Moving the mouse right (+X) should yaw the camera right, which this
        // module implements as a negative yaw delta.
        assert!(
            orientation.yaw < 0.0,
            "expected negative yaw after rightward mouse motion, got {}",
            orientation.yaw
        );
    }
}
