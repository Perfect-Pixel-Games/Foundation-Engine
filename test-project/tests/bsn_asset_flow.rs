use bevy::{prelude::*, scene::ScenePatch};
use foundation_runtime_library::prelude::*;
use test_project::asset_root;

#[test]
fn smoke_test_scene_loads_as_a_scene_patch() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(bevy::asset::AssetPlugin {
        file_path: asset_root().to_string_lossy().to_string(),
        ..default()
    });
    app.add_plugins(bevy::scene::ScenePlugin);
    app.add_message::<SceneLoadRequested>();
    app.add_plugins(FoundationBsnAssetPlugin);
    register_bsn_test_types(&mut app);

    let asset_server = app.world().resource::<AssetServer>().clone();
    let scene_handle = asset_server.load::<ScenePatch>("scenes/smoke_test.bsn");

    for _frame_number in 0..60 {
        app.update();
    }

    let scene_assets = app.world().resource::<Assets<ScenePatch>>();
    assert!(
        scene_assets.get(&scene_handle).is_some(),
        "the converted smoke_test .bsn asset should load as a ScenePatch"
    );
}

#[test]
fn smoke_test_scene_spawns_authored_text_through_foundation_bridge() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(bevy::asset::AssetPlugin {
        file_path: asset_root().to_string_lossy().to_string(),
        ..default()
    });
    app.add_plugins(bevy::scene::ScenePlugin);
    app.add_message::<SceneLoadRequested>();
    app.add_plugins(FoundationBsnAssetPlugin);
    register_bsn_test_types(&mut app);

    let scene_key = "smoke_test";
    app.world_mut()
        .resource_mut::<FoundationBsnSceneRegistry>()
        .register_scene(scene_key, "scenes/smoke_test.bsn");
    app.world_mut().write_message(SceneLoadRequested {
        scene_id: SceneId(1),
        source: SceneSource::bsn_scene(scene_key),
    });

    for _frame_number in 0..60 {
        app.update();
    }

    let mut text_query = app.world_mut().query::<(&Text, Option<&SceneOwner>)>();
    let texts = text_query
        .iter(app.world())
        .map(|(text, scene_owner)| (text.0.clone(), scene_owner.copied()))
        .collect::<Vec<_>>();

    assert!(
        texts.iter().any(|(text, scene_owner)| {
            text == "Foundation Test Project"
                && *scene_owner == Some(SceneOwner { scene_id: SceneId(1) })
        }),
        "the Foundation BSN bridge should spawn the authored smoke-test text with scene ownership; found {texts:?}",
    );
}

fn register_bsn_test_types(app: &mut App) {
    app.register_type::<Node>()
        .register_type::<Val>()
        .register_type::<AlignItems>()
        .register_type::<JustifyContent>()
        .register_type::<Text>()
        .register_type::<TextFont>()
        .register_type::<TextColor>()
        .register_type::<bevy::text::FontSize>()
        .register_type::<Color>()
        .register_type::<Srgba>();
}
