//! Levels from `Dark Messenger.dba`: level one (the castle: eight guards,
//! the book and its vortex, fire braziers, the fountain) and level two (the
//! "Game Over" room where the models pose). Also the sky dome, the light
//! that turns blue while chanting, the ambient light the chant drains
//! (`set ambient light lightLevel#`) and the wind loop.

use crate::assets::GameAssets;
use crate::collision::{LevelCollision, LevelLibrary};
use crate::npc::{NpcKind, spawn_npc};
use crate::collision::Body;
use crate::player::Player;
use crate::{GameState, db, effects};
use bevy::audio::Volume;
use bevy::gltf::GltfMaterialName;
use bevy::pbr::Lightmap;
use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Ambience>()
            .init_resource::<LevelMaterials>()
            .add_event::<LoadLevel>()
            .insert_resource(AmbientLight { color: Color::WHITE, brightness: AMBIENT })
            .add_systems(Startup, start_music)
            .add_systems(Update, (load_level.run_if(resource_exists::<LevelLibrary>), apply_ambience))
            .add_systems(OnEnter(GameState::Intro), unload_level)
            .add_systems(PostUpdate, follow_camera.before(TransformSystem::TransformPropagate));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelId {
    One,
    Two,
}

/// Loads a level, replacing whatever is loaded (`goto Load2`).
#[derive(Event)]
pub struct LoadLevel(pub LevelId);

#[derive(Resource)]
pub struct CurrentLevel(pub LevelId);

/// Everything that belongs to the loaded level; despawned on level change.
#[derive(Component)]
pub struct LevelEntity;

/// Ambient term for models (NPCs, hands); the level itself is lit by its
/// lightmaps (`set object ambient 1, 0` + the ghosted lightmap overlay).
const AMBIENT: f32 = 0.6;
const LIGHTMAP_EXPOSURE: f32 = 1.0;
/// `color light 4, 10, 10, 10`, and `0, 0, 157` while chanting.
const SUN: Color = Color::srgb(10.0 / 255.0, 10.0 / 255.0, 10.0 / 255.0);
const SUN_CHANT: Color = Color::srgb(0.0, 0.0, 157.0 / 255.0);
const SUN_ILLUMINANCE: f32 = 3.0;
/// `make object sphere 9999, 9000, 4, 4` (diameter 9000, 4x4 segments).
const SKY_RADIUS: f32 = 4500.0;

/// Scene-wide light: `level` is `lightLevel# / 100`, `flash` brightens the
/// lightmaps while a spread bolt strikes (`ghost object on 2, 2`), `chant`
/// turns the sun blue.
#[derive(Resource)]
pub struct Ambience {
    pub level: f32,
    pub flash: f32,
    pub chant: bool,
    applied: Option<(f32, f32, bool)>,
}

impl Default for Ambience {
    fn default() -> Self {
        Self { level: 1.0, flash: 0.0, chant: false, applied: None }
    }
}

#[derive(Resource, Default)]
struct LevelMaterials(Vec<Handle<StandardMaterial>>);

#[derive(Component)]
struct Sky;

#[derive(Component)]
struct Sun;

fn start_music(mut commands: Commands, assets: Res<GameAssets>) {
    // `load music "Blowing wind.wav", 1 : loop music 1`
    commands.spawn((
        AudioPlayer::new(assets.sounds.wind.clone()),
        PlaybackSettings::LOOP.with_volume(Volume::new(0.6)),
    ));
}

fn unload_level(
    mut commands: Commands,
    level: Query<Entity, With<LevelEntity>>,
    mut materials: ResMut<LevelMaterials>,
    mut ambience: ResMut<Ambience>,
) {
    for entity in &level {
        commands.entity(entity).despawn_recursive();
    }
    materials.0.clear();
    *ambience = Ambience::default();
    commands.remove_resource::<CurrentLevel>();
}

fn load_level(
    mut commands: Commands,
    mut events: EventReader<LoadLevel>,
    assets: Res<GameAssets>,
    library: Res<LevelLibrary>,
    level_entities: Query<Entity, With<LevelEntity>>,
    mut level_materials: ResMut<LevelMaterials>,
    mut ambience: ResMut<Ambience>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut player: Query<(&mut Player, &mut Body, &mut Transform)>,
) {
    let Some(LoadLevel(id)) = events.read().last() else {
        return;
    };
    for entity in &level_entities {
        commands.entity(entity).despawn_recursive();
    }
    level_materials.0.clear();
    *ambience = Ambience::default();

    let (scene, collision) = match id {
        LevelId::One => (&assets.level1_scene, &library.levels[0]),
        LevelId::Two => (&assets.level2_scene, &library.levels[1]),
    };
    commands.insert_resource::<LevelCollision>(collision.clone());
    commands.insert_resource(CurrentLevel(*id));
    commands.spawn((LevelEntity, Name::new("Level"), SceneRoot(scene.clone()))).observe(prepare_level);

    // Sky dome riding on the camera, rotated onto its side like the original.
    commands.spawn((
        LevelEntity,
        Sky,
        Name::new("Sky"),
        Mesh3d(meshes.add(Sphere::new(SKY_RADIUS).mesh().uv(4, 4))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: Some(assets.sky.clone()),
            unlit: true,
            cull_mode: None,
            fog_enabled: false,
            ..default()
        })),
        Transform::from_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
    ));
    // `hide light 0 : make light 4 : set directional light 4, 0, -10, -3`
    commands.spawn((
        LevelEntity,
        Sun,
        DirectionalLight { color: SUN, illuminance: SUN_ILLUMINANCE, shadows_enabled: false, ..default() },
        Transform::default().looking_to(db(0.0, -10.0, -3.0), Vec3::Y),
    ));

    match id {
        LevelId::One => {
            use NpcKind::{Archer, Knight};
            // (kind, position, view point) from the prepareNpc calls.
            let npcs = [
                // king's chambers
                (Knight, db(-402.0, 115.0, -870.0), db(0.0, 115.0, 1.0)),
                (Knight, db(-765.0, 115.0, -772.0), db(0.0, 115.0, 1.0)),
                // on top
                (Archer, db(835.0, 350.0, -640.0), db(0.0, 350.0, 1.0)),
                (Archer, db(1083.0, 350.0, -640.0), db(0.0, 350.0, 1.0)),
                // hall
                (Knight, db(1138.0, 65.0, 338.0), db(0.0, 65.0, 1.0)),
                // courtyard
                (Knight, db(2358.0, 25.0, -218.0), db(0.0, 25.0, 1.0)),
                // balconies
                (Archer, db(1990.0, 708.0, 300.0), db(0.0, 708.0, 1.0)),
                (Archer, db(-60.0, 720.0, 223.0), db(0.0, 720.0, 1.0)),
            ];
            for (kind, pos, view) in npcs {
                spawn_npc(&mut commands, &assets, kind, pos, view, false);
            }
            effects::spawn_level_one(&mut commands, &assets, &mut meshes, &mut materials);
        }
        LevelId::Two => {
            use NpcKind::{Archer, Knight};
            let npcs = [
                (Archer, db(112.0, 150.0, -274.0)),
                (Knight, db(614.0, 150.0, -274.0)),
                (Knight, db(360.0, 150.0, -274.0)),
            ];
            for (kind, pos) in npcs {
                // Every NPC here is in the "modeling" state: posing through
                // its whole animation for the player.
                spawn_npc(&mut commands, &assets, kind, pos, db(1.0, 150.0, 0.0), true);
            }
            effects::spawn_level_two(&mut commands, &assets, &mut meshes, &mut materials);
        }
    }

    // `thePlayer.pos = 0, 100, 0` (health carries over between levels).
    if let Ok((mut p, mut body, mut transform)) = player.get_single_mut() {
        transform.translation = crate::player::SPAWN;
        *body = Player::body();
        p.yaw = 0.0;
        p.pitch = 0.0;
    }
}

/// Attaches each level material's lightmap (encoded by xconv in the material
/// name as `material_3@one12`) and hides the invisible "caulk" brushes,
/// which stay in the collision mesh.
fn prepare_level(
    trigger: Trigger<SceneInstanceReady>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    assets: Res<GameAssets>,
    mut level: ResMut<LevelMaterials>,
    mut ambience: ResMut<Ambience>,
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
                image: assets.lightmap(&asset_server, lightmap),
                uv_rect: Rect::new(0.0, 0.0, 1.0, 1.0),
            });
            level.0.push(material.0.clone());
        }
    }
    ambience.applied = None;
}

/// Pushes the light level into the ambient light, the sun and the level's
/// lightmap exposure (only when it changes, to avoid re-uploading materials).
fn apply_ambience(
    mut ambience: ResMut<Ambience>,
    level: Res<LevelMaterials>,
    mut ambient: ResMut<AmbientLight>,
    mut sun: Query<&mut DirectionalLight, With<Sun>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let state = (ambience.level, ambience.flash, ambience.chant);
    if ambience.applied == Some(state) || level.0.is_empty() {
        return;
    }
    ambience.applied = Some(state);
    let (light, flash, chant) = state;
    ambient.brightness = AMBIENT * (light + flash);
    for mut sun in &mut sun {
        sun.color = if chant { SUN_CHANT } else { SUN };
    }
    for handle in &level.0 {
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
