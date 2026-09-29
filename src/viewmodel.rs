//! First-person view model from the Dec 2003 playable demo: Darius's hands
//! (model\hand2, a skinned rig with a 1.5 s spell gesture) and the dagger
//! that floats in front of him (model\dagger2). Both ride on the camera.
//! The hands play their gesture while chanting; the dagger bobs and turns,
//! spinning faster as the chant builds and hardest while a bolt strikes.

use crate::player::Player;
use crate::spell::{Spell, SpellState};
use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;

pub struct ViewModelPlugin;

impl Plugin for ViewModelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn.after(crate::player::spawn_player))
            .add_systems(Update, (animate_hands, float_dagger));
    }
}

// Placement relative to the camera (camera looks down -Z, +Y up, +X right).
const HANDS_OFFSET: Vec3 = Vec3::new(0.0, -9.0, -14.0);
const HANDS_SCALE: f32 = 0.35;
const HANDS_ROTATION_Y: f32 = 0.0;
/// The dagger hovers point-up above the open palms.
const DAGGER_OFFSET: Vec3 = Vec3::new(0.0, -7.5, -18.0);
const DAGGER_SCALE: f32 = 0.7;

#[derive(Resource)]
struct Gesture {
    graph: Handle<AnimationGraph>,
    node: AnimationNodeIndex,
    player: Option<Entity>,
}

#[derive(Component)]
struct Hands;

#[derive(Component)]
struct Dagger;

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
    let dagger = commands
        .spawn((
            Dagger,
            Name::new("Floating dagger"),
            SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/dagger.glb"))),
            Transform::from_translation(DAGGER_OFFSET).with_scale(Vec3::splat(DAGGER_SCALE)),
        ))
        .id();
    commands.entity(camera).add_children(&[hands, dagger]);
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

/// Slow hover and spin; spins fast and rises while chanting.
fn float_dagger(time: Res<Time>, spell: Res<Spell>, mut dagger: Query<&mut Transform, With<Dagger>>, mut spin: Local<f32>) {
    let Ok(mut transform) = dagger.get_single_mut() else {
        return;
    };
    let t = time.elapsed_secs();
    let (rate, lift) = match spell.state {
        SpellState::Idle => (0.8, 0.0),
        SpellState::Chanting { time, .. } => (0.8 + time * 4.0, time.min(1.0) * 2.0),
        SpellState::Striking { .. } => (14.0, 2.0),
    };
    *spin += rate * time.delta_secs();
    transform.translation = DAGGER_OFFSET + Vec3::Y * ((t * 1.7).sin() * 0.6 + lift);
    // The blade runs along +Z in the model: stand it point-up and turn it
    // about the vertical axis.
    transform.rotation = Quat::from_rotation_y(*spin) * Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
}
