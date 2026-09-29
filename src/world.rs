//! The static world from `Dark Messenger.dba`: lightmapped level, sky
//! sphere, purple directional light, the animated Darius statue and the
//! ambient wind loop. Also owns `Ambience`, the global light level that the
//! lightning chant drains (`set ambient light lightLevel#`).

use crate::collision::LevelCollision;
use crate::{GameState, db};
use bevy::gltf::{Gltf, GltfMaterialName};
use bevy::pbr::Lightmap;
use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Ambience>()
            .insert_resource(AmbientLight { color: Color::WHITE, brightness: AMBIENT })
            .add_systems(Startup, setup)
            .add_systems(Update, finish_loading.run_if(in_state(GameState::Loading)))
            .add_systems(Update, apply_ambience)
            .add_systems(PostUpdate, follow_camera.before(TransformSystem::TransformPropagate));
    }
}

/// Brightness of the lightmaps at full light level.
const LIGHTMAP_EXPOSURE: f32 = 1.0;
/// Ambient term for unlightmapped models (archers, statue, arrows).
const AMBIENT: f32 = 0.55;
/// `color light 4, 111, 0, 111`
const SUN_COLOR: Color = Color::srgb(111.0 / 255.0, 0.0, 111.0 / 255.0);
const SUN_ILLUMINANCE: f32 = 2.2;
/// `make object sphere 9999, 9000` (DarkBASIC sizes are diameters).
const SKY_RADIUS: f32 = 4500.0;

#[derive(Resource)]
pub struct LevelAssets {
    pub gltf: Handle<Gltf>,
    level_ready: bool,
    materials: Vec<Handle<StandardMaterial>>,
}

/// Scene-wide light level. `level` is the original `lightLevel# / 100`;
/// `flash` brightens the lightmaps while a bolt is striking.
#[derive(Resource)]
pub struct Ambience {
    pub level: f32,
    pub flash: f32,
    applied: (f32, f32),
}

impl Default for Ambience {
    fn default() -> Self {
        Self { level: 1.0, flash: 0.0, applied: (-1.0, -1.0) }
    }
}

#[derive(Component)]
struct Sky;

#[derive(Component)]
struct Sun;

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    // Level (one.x + one_lm.x lightmap overlay, merged by tools/xconv).
    commands.insert_resource(LevelAssets {
        gltf: asset_server.load("models/level.glb"),
        level_ready: false,
        materials: Vec::new(),
    });
    commands
        .spawn((
            Name::new("Level"),
            SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/level.glb"))),
        ))
        .observe(prepare_level);

    // Sky sphere, textured on the inside and kept centred on the camera.
    commands.spawn((
        Sky,
        Name::new("Sky"),
        Mesh3d(meshes.add(Sphere::new(SKY_RADIUS).mesh().uv(48, 24))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: Some(asset_server.load("textures/sky.jpg")),
            unlit: true,
            cull_mode: None,
            fog_enabled: false,
            ..default()
        })),
        Transform::default(),
    ));

    // `hide light 0 : make light 4 : set directional light 4, 0, -10, -3`
    commands.spawn((
        Sun,
        DirectionalLight { color: SUN_COLOR, illuminance: SUN_ILLUMINANCE, shadows_enabled: false, ..default() },
        Transform::default().looking_to(db(0.0, -10.0, -3.0), Vec3::Y),
    ));

    // Darius statue: dariusanim.x, looping forever (`set object frame 4, frame`).
    let clip = asset_server.load(GltfAssetLabel::Animation(0).from_asset("models/darius.glb"));
    let (graph, node) = AnimationGraph::from_clip(clip);
    let graph = graphs.add(graph);
    commands
        .spawn((
            Name::new("Darius"),
            SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/darius.glb"))),
            Transform::from_translation(db(200.0, 200.0, 400.0)).with_scale(Vec3::splat(4.0)),
        ))
        .observe(
            move |trigger: Trigger<SceneInstanceReady>,
                  mut commands: Commands,
                  children: Query<&Children>,
                  mut players: Query<&mut AnimationPlayer>| {
                for entity in children.iter_descendants(trigger.entity()) {
                    if let Ok(mut player) = players.get_mut(entity) {
                        player.play(node).repeat();
                        commands.entity(entity).insert(AnimationGraphHandle(graph.clone()));
                    }
                }
            },
        );

    // `load music "Blowing wind.wav", 1 : loop music 1`
    commands.spawn((
        AudioPlayer::new(asset_server.load("sounds/wind.ogg")),
        PlaybackSettings::LOOP.with_volume(bevy::audio::Volume::new(0.6)),
    ));
}

/// Attaches each level material's lightmap (encoded by xconv in the material
/// name as `material_3@one12`) and hides the invisible "caulk" brushes,
/// which stay in the collision mesh.
fn prepare_level(
    trigger: Trigger<SceneInstanceReady>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut level: ResMut<LevelAssets>,
    children: Query<&Children>,
    parts: Query<(&GltfMaterialName, &MeshMaterial3d<StandardMaterial>)>,
) {
    for entity in children.iter_descendants(trigger.entity()) {
        let Ok((name, material)) = parts.get(entity) else {
            continue;
        };
        if name.0 == "caulk" {
            commands.entity(entity).insert(Visibility::Hidden);
        } else if let Some((_, lightmap)) = name.0.split_once('@') {
            commands.entity(entity).insert(Lightmap {
                image: asset_server.load(format!("textures/lightmaps/{lightmap}.png")),
                uv_rect: Rect::new(0.0, 0.0, 1.0, 1.0),
            });
            level.materials.push(material.0.clone());
        }
    }
    level.level_ready = true;
}

fn finish_loading(
    level: Res<LevelAssets>,
    collision: Option<Res<LevelCollision>>,
    mut next: ResMut<NextState<GameState>>,
) {
    if level.level_ready && collision.is_some() {
        next.set(GameState::Paused);
    }
}

/// Pushes the light level into the ambient light, the sun and the level's
/// lightmap exposure (only when it changes, to avoid re-uploading materials).
fn apply_ambience(
    mut ambience: ResMut<Ambience>,
    level: Res<LevelAssets>,
    mut ambient: ResMut<AmbientLight>,
    mut sun: Query<&mut DirectionalLight, With<Sun>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let state = (ambience.level, ambience.flash);
    if state == ambience.applied || level.materials.is_empty() {
        return;
    }
    ambience.applied = state;
    let (light, flash) = state;
    ambient.brightness = AMBIENT * (light + flash);
    for mut sun in &mut sun {
        sun.illuminance = SUN_ILLUMINANCE * (light + flash * 2.0);
    }
    for handle in &level.materials {
        if let Some(m) = materials.get_mut(handle) {
            m.lightmap_exposure = LIGHTMAP_EXPOSURE * (light + flash);
        }
    }
}

/// `position object 9999, thePlayer.pos.x#, ...`: the sky rides with the camera.
fn follow_camera(camera: Query<&Transform, (With<Camera3d>, Without<Sky>)>, mut sky: Query<&mut Transform, With<Sky>>) {
    let (Ok(camera), Ok(mut sky)) = (camera.get_single(), sky.get_single_mut()) else {
        return;
    };
    sky.translation = camera.translation;
}
