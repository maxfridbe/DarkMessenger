//! First-person view model from the Dec 2003 playable demo: Darius's hands
//! (model\hand2, a skinned rig with a 1.5 s spell gesture) and the dagger
//! that floats in front of him (model\dagger2). Both ride on the camera.
//! The hands play their gesture while chanting; the dagger bobs and turns,
//! spinning faster as the chant builds and hardest while a bolt strikes.
//!
//! The dagger is also a weapon: thrown, it flies at the crosshair, wounds
//! the first archer it meets (`npc::DaggerHit`) or glances off a wall, then
//! floats back to the hands.

use crate::GameState;
use crate::collision::LevelCollision;
use crate::npc::{Archer, DaggerHit};
use crate::player::{Player, PlayerInput, PlayerSet};
use crate::spell::{Spell, SpellState};
use bevy::audio::Volume;
use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;

pub struct ViewModelPlugin;

impl Plugin for ViewModelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn.after(crate::player::spawn_player))
            .add_systems(Update, animate_hands)
            .add_systems(Update, throw_dagger.after(PlayerSet).run_if(in_state(GameState::Playing)))
            .add_systems(Update, move_dagger.after(throw_dagger));
    }
}

// Placement relative to the camera (camera looks down -Z, +Y up, +X right).
const HANDS_OFFSET: Vec3 = Vec3::new(0.0, -9.0, -14.0);
const HANDS_SCALE: f32 = 0.35;
const HANDS_ROTATION_Y: f32 = 0.0;
/// The dagger hovers above the open palms, point away from the viewer.
const DAGGER_OFFSET: Vec3 = Vec3::new(0.0, -7.5, -18.0);
const DAGGER_SCALE: f32 = 0.7;
/// Thrown, the dagger grows to a readable size in the world.
const DAGGER_FLIGHT_SCALE: f32 = 3.5;
const THROW_SPEED: f32 = 1600.0;
const RETURN_SPEED: f32 = 1100.0;
const THROW_RANGE: f32 = 1500.0;

#[derive(Clone, Copy, PartialEq)]
enum DaggerState {
    Floating,
    Flying { direction: Vec3, travelled: f32 },
    Returning,
}

#[derive(Resource)]
struct Gesture {
    graph: Handle<AnimationGraph>,
    node: AnimationNodeIndex,
    player: Option<Entity>,
}

#[derive(Component)]
struct Hands;

#[derive(Component)]
struct Dagger {
    state: DaggerState,
    spin: f32,
}

#[derive(Resource)]
struct DaggerSounds {
    throw: Handle<AudioSource>,
    hit: Handle<AudioSource>,
}

pub(crate) fn spawn(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    camera: Query<Entity, With<Player>>,
) {
    let Ok(camera) = camera.get_single() else {
        return;
    };
    let clip = asset_server.load(GltfAssetLabel::Animation(0).from_asset("models/hands.glb"));
    let (graph, node) = AnimationGraph::from_clip(clip);
    commands.insert_resource(Gesture { graph: graphs.add(graph), node, player: None });

    let hands = commands
        .spawn((
            Hands,
            Name::new("Hands"),
            SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/hands.glb"))),
            Transform::from_translation(HANDS_OFFSET)
                .with_rotation(Quat::from_rotation_y(HANDS_ROTATION_Y))
                .with_scale(Vec3::splat(HANDS_SCALE)),
        ))
        .observe(hook_gesture)
        .id();
    commands.entity(camera).add_child(hands);
    // The dagger lives in world space so it can leave the hands; while
    // floating it is re-placed relative to the camera every frame.
    commands.spawn((
        Dagger { state: DaggerState::Floating, spin: 0.0 },
        Name::new("Floating dagger"),
        SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/dagger.glb"))),
        Transform::from_scale(Vec3::splat(DAGGER_SCALE)),
    ));
    commands.insert_resource(DaggerSounds {
        throw: asset_server.load("sounds/dagger_throw.wav"),
        hit: asset_server.load("sounds/dagger_hit.wav"),
    });
}

/// Parks the gesture on its first frame until a chant starts.
fn hook_gesture(
    trigger: Trigger<SceneInstanceReady>,
    mut commands: Commands,
    mut gesture: ResMut<Gesture>,
    children: Query<&Children>,
    mut players: Query<&mut AnimationPlayer>,
) {
    for entity in children.iter_descendants(trigger.entity()) {
        if let Ok(mut player) = players.get_mut(entity) {
            player.start(gesture.node).pause();
            commands.entity(entity).insert(AnimationGraphHandle(gesture.graph.clone()));
            gesture.player = Some(entity);
        }
    }
}

fn animate_hands(spell: Res<Spell>, gesture: Res<Gesture>, mut players: Query<&mut AnimationPlayer>, mut was_chanting: Local<bool>) {
    let Some(mut player) = gesture.player.and_then(|e| players.get_mut(e).ok()) else {
        return;
    };
    let chanting = matches!(spell.state, SpellState::Chanting { .. });
    if chanting && !*was_chanting {
        // `start` rewinds but keeps the paused flag from the idle pose.
        player.start(gesture.node).resume();
    } else if !chanting && *was_chanting {
        player.start(gesture.node).pause();
    }
    *was_chanting = chanting;
}

fn play(commands: &mut Commands, sound: &Handle<AudioSource>) {
    commands.spawn((AudioPlayer::new(sound.clone()), PlaybackSettings::DESPAWN.with_volume(Volume::new(0.8))));
}

/// Launches the floating dagger at whatever the crosshair is on.
fn throw_dagger(
    mut commands: Commands,
    input: Res<PlayerInput>,
    spell: Res<Spell>,
    sounds: Res<DaggerSounds>,
    camera: Query<&Transform, (With<Player>, Without<Dagger>)>,
    mut dagger: Query<(&mut Dagger, &Transform)>,
) {
    let (Ok(camera), Ok((mut dagger, transform))) = (camera.get_single(), dagger.get_single_mut()) else {
        return;
    };
    if !input.throw || dagger.state != DaggerState::Floating {
        return;
    }
    let target = spell.aim_point.unwrap_or(camera.translation + *camera.forward() * THROW_RANGE);
    let direction = (target - transform.translation).normalize_or(*camera.forward());
    dagger.state = DaggerState::Flying { direction, travelled: 0.0 };
    play(&mut commands, &sounds.throw);
}

/// Hover in front of the hands, fly, strike, and float home.
fn move_dagger(
    mut commands: Commands,
    time: Res<Time>,
    state: Res<State<GameState>>,
    spell: Res<Spell>,
    sounds: Res<DaggerSounds>,
    level: Option<Res<LevelCollision>>,
    mut hits: EventWriter<DaggerHit>,
    camera: Query<&Transform, (With<Player>, Without<Dagger>)>,
    archers: Query<(Entity, &Archer, &Transform), Without<Dagger>>,
    mut dagger: Query<(&mut Dagger, &mut Transform)>,
) {
    let (Ok(camera), Ok((mut dagger, mut transform))) = (camera.get_single(), dagger.get_single_mut()) else {
        return;
    };
    let dt = time.delta_secs().min(0.05);
    let t = time.elapsed_secs();
    let (rate, lift) = match spell.state {
        SpellState::Idle => (0.8, 0.0),
        SpellState::Chanting { time, .. } => (0.8 + time * 4.0, time.min(1.0) * 2.0),
        SpellState::Striking { .. } => (14.0, 2.0),
    };
    let hover = DAGGER_OFFSET + Vec3::Y * ((t * 1.7).sin() * 0.6 + lift);
    let home = camera.transform_point(hover);
    let playing = *state.get() == GameState::Playing;

    match dagger.state {
        DaggerState::Floating => {
            dagger.spin += rate * dt;
            transform.translation = home;
            transform.scale = Vec3::splat(DAGGER_SCALE);
            // The point is +Z in the model; aim it down the camera's view
            // (-Z) and turn the blade about its length.
            transform.rotation =
                camera.rotation * Quat::from_rotation_y(std::f32::consts::PI) * Quat::from_rotation_z(dagger.spin);
        }
        DaggerState::Flying { direction, travelled } if playing => {
            let step = THROW_SPEED * dt;
            let from = transform.translation;
            let struck = archers.iter().filter(|(_, a, _)| a.alive()).find(|(_, _, a)| {
                let (min, max) = Archer::hit_box(a.translation);
                segment_hits_box(from, direction, step, min, max)
            });
            let wall = level.as_ref().and_then(|l| l.ray(from, direction, step));
            if let Some((entity, _, _)) = struck {
                hits.send(DaggerHit(entity));
                play(&mut commands, &sounds.hit);
                dagger.state = DaggerState::Returning;
            } else if let Some(d) = wall {
                transform.translation = from + direction * d;
                play(&mut commands, &sounds.hit);
                dagger.state = DaggerState::Returning;
            } else {
                transform.translation = from + direction * step;
                dagger.state = if travelled + step > THROW_RANGE {
                    DaggerState::Returning
                } else {
                    DaggerState::Flying { direction, travelled: travelled + step }
                };
            }
            dagger.spin += 22.0 * dt;
            let grow = ((travelled + step) / 150.0).min(1.0);
            transform.scale = Vec3::splat(DAGGER_SCALE + (DAGGER_FLIGHT_SCALE - DAGGER_SCALE) * grow);
            // Point first, spinning about the blade.
            transform.rotation = Quat::from_rotation_arc(Vec3::Z, direction) * Quat::from_rotation_z(dagger.spin);
        }
        DaggerState::Returning if playing => {
            // Drifts home through walls, turning lazily, shrinking back.
            let to_home = home - transform.translation;
            let step = RETURN_SPEED * dt;
            dagger.spin += 3.0 * dt;
            if to_home.length() <= step {
                dagger.state = DaggerState::Floating;
                transform.translation = home;
            } else {
                transform.translation += to_home.normalize() * step;
                let near = (to_home.length() / 900.0).min(1.0);
                transform.scale = Vec3::splat(DAGGER_SCALE + (DAGGER_FLIGHT_SCALE - DAGGER_SCALE) * near);
                transform.rotation = Quat::from_rotation_arc(Vec3::Z, -to_home.normalize()) * Quat::from_rotation_z(dagger.spin);
            }
        }
        _ => {}
    }
}

/// Slab test: does the segment `from + dir * [0, len]` touch the box?
fn segment_hits_box(from: Vec3, dir: Vec3, len: f32, min: Vec3, max: Vec3) -> bool {
    let (mut t0, mut t1) = (0.0f32, len);
    for i in 0..3 {
        if dir[i].abs() < 1e-6 {
            if from[i] < min[i] || from[i] > max[i] {
                return false;
            }
            continue;
        }
        let (a, b) = ((min[i] - from[i]) / dir[i], (max[i] - from[i]) / dir[i]);
        t0 = t0.max(a.min(b));
        t1 = t1.min(a.max(b));
        if t0 > t1 {
            return false;
        }
    }
    true
}
