//! `npc.dba`: the archers. Each one idles until the player comes within
//! 700 units, walks toward them while it can see them, stops to shoot
//! arrows inside 500 units, and — when a lightning bolt lands on it, or
//! the thrown dagger finds it twice — is flung into the air and plays its
//! death animation (archer2.x).

use crate::collision::{Body, LevelCollision};
use crate::player::{Player, PlayerSet};
use crate::spell::LightningStrike;
use crate::{GameState, db};
use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;

pub struct NpcPlugin;

impl Plugin for NpcPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<DaggerHit>().add_systems(Startup, spawn_archers).add_systems(
            Update,
            (think, take_hits, finish_dying)
                .chain()
                .in_set(NpcSet)
                .after(PlayerSet)
                .run_if(in_state(GameState::Playing)),
        );
    }
}

/// Archer AI; arrows are loosed after it decides who is attacking.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct NpcSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArcherState {
    Idle,
    Walking,
    Attacking,
    Dying,
    Dead,
}

/// The thrown dagger struck this archer.
#[derive(Event)]
pub struct DaggerHit(pub Entity);

#[derive(Component)]
pub struct Archer {
    pub state: ArcherState,
    /// Dagger wounds taken; the second one is fatal.
    wounds: u8,
    idle_model: Entity,
    death_model: Entity,
    /// Entity holding the death scene's AnimationPlayer, once spawned.
    death_player: Option<Entity>,
}

impl Archer {
    pub fn alive(&self) -> bool {
        !matches!(self.state, ArcherState::Dying | ArcherState::Dead)
    }

    /// World-space hit box for an archer standing at `pos`.
    pub fn hit_box(pos: Vec3) -> (Vec3, Vec3) {
        let feet = pos.y - PIVOT_HEIGHT;
        (
            Vec3::new(pos.x - HALF_WIDTH, feet, pos.z - HALF_WIDTH),
            Vec3::new(pos.x + HALF_WIDTH, feet + HEIGHT, pos.z + HALF_WIDTH),
        )
    }
}

/// Marks the death-animation scene and points back at its archer.
#[derive(Component)]
struct DeathModel(Entity);

#[derive(Resource)]
struct DeathAnimation {
    graph: Handle<AnimationGraph>,
    node: AnimationNodeIndex,
}

/// `scale object ..., 250, 250, 250`
const MODEL_SCALE: f32 = 2.5;
/// `moveX# / mag# * 150.0 * avgCycleTime`
const WALK_SPEED: f32 = 150.0;
const WAKE_DISTANCE: f32 = 700.0;
const ATTACK_DISTANCE: f32 = 500.0;
/// Archers stop closing in at this distance.
const MIN_DISTANCE: f32 = 200.0;
/// Horizontal half-size of an archer's hit box.
const HALF_WIDTH: f32 = 30.0;
/// The converted archer model stands with its feet at the pivot.
const PIVOT_HEIGHT: f32 = 1.0;
const HEIGHT: f32 = 100.0;
/// Height of the archer's eyes above its pivot, for line-of-sight checks.
pub const EYE_OFFSET: f32 = 75.0;

/// Starting positions and view targets of `theCharList(1)` and `(2)`.
const ARCHERS: [(Vec3, Vec3); 2] = [
    (db(2574.0, 150.0, 0.0), db(0.0, 150.0, 1.0)),
    (db(2500.0, 150.0, -90.0), db(10.0, 150.0, 29.0)),
];

fn spawn_archers(mut commands: Commands, asset_server: Res<AssetServer>, mut graphs: ResMut<Assets<AnimationGraph>>) {
    let clip = asset_server.load(GltfAssetLabel::Animation(0).from_asset("models/archer_death.glb"));
    let (graph, node) = AnimationGraph::from_clip(clip);
    commands.insert_resource(DeathAnimation { graph: graphs.add(graph), node });

    let idle_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/archer.glb"));
    let death_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/archer_death.glb"));
    for (i, (pos, view)) in ARCHERS.into_iter().enumerate() {
        let scale = Transform::from_scale(Vec3::splat(MODEL_SCALE));
        let idle_model = commands.spawn((SceneRoot(idle_scene.clone()), scale)).id();
        let death_model = commands.spawn((SceneRoot(death_scene.clone()), scale, Visibility::Hidden)).id();
        let archer = commands
            .spawn((
                Name::new(format!("Archer {}", i + 1)),
                Archer { state: ArcherState::Idle, wounds: 0, idle_model, death_model, death_player: None },
                Body {
                    vel_y: 0.0,
                    grounded: false,
                    stand_height: PIVOT_HEIGHT,
                    step: 65.0,
                    top: HEIGHT,
                    radius: 20.0,
                },
                Transform::from_translation(pos).looking_to(flat(view - pos), Vec3::Y),
                Visibility::default(),
            ))
            .add_children(&[idle_model, death_model])
            .id();
        commands.entity(death_model).insert(DeathModel(archer)).observe(attach_death_animation);
    }
}

/// Hooks the archer's death animation graph onto its scene's player.
fn attach_death_animation(
    trigger: Trigger<SceneInstanceReady>,
    mut commands: Commands,
    animation: Res<DeathAnimation>,
    death_models: Query<&DeathModel>,
    children: Query<&Children>,
    players: Query<(), With<AnimationPlayer>>,
    mut archers: Query<&mut Archer>,
) {
    let Ok(DeathModel(owner)) = death_models.get(trigger.entity()) else {
        return;
    };
    let Some(player) = children.iter_descendants(trigger.entity()).find(|e| players.contains(*e)) else {
        return;
    };
    commands.entity(player).insert(AnimationGraphHandle(animation.graph.clone()));
    if let Ok(mut archer) = archers.get_mut(*owner) {
        archer.death_player = Some(player);
    }
}

fn flat(v: Vec3) -> Vec3 {
    let v = Vec3::new(v.x, 0.0, v.z);
    if v.length_squared() < 1e-6 { Vec3::NEG_Z } else { v.normalize() }
}

/// `NpcFsm` + `NpcUpdatePos` + `NpcCalcGrav`/`NpcGrav`.
fn think(
    time: Res<Time>,
    level: Res<LevelCollision>,
    player: Query<&Transform, (With<Player>, Without<Archer>)>,
    mut archers: Query<(&mut Archer, &mut Body, &mut Transform)>,
) {
    let Ok(player) = player.get_single() else {
        return;
    };
    let dt = time.delta_secs().min(0.05);
    for (mut archer, mut body, mut transform) in &mut archers {
        let pos = transform.translation;
        let distance = pos.distance(player.translation);
        let sees_player = || level.line_of_sight(pos + Vec3::Y * EYE_OFFSET, player.translation);
        let to_player = flat(player.translation - pos);
        let mut step = Vec3::ZERO;

        archer.state = match archer.state {
            // The original only woke between 500 and 700 units, so an archer
            // approached from closer range never noticed the player.
            ArcherState::Idle if distance < WAKE_DISTANCE => ArcherState::Walking,
            ArcherState::Walking if distance > WAKE_DISTANCE => ArcherState::Idle,
            ArcherState::Walking if distance <= ATTACK_DISTANCE && sees_player() => ArcherState::Attacking,
            ArcherState::Walking => {
                if distance > MIN_DISTANCE && sees_player() {
                    step = to_player * WALK_SPEED * dt;
                    transform.look_to(to_player, Vec3::Y);
                }
                ArcherState::Walking
            }
            ArcherState::Attacking if distance > ATTACK_DISTANCE => ArcherState::Walking,
            ArcherState::Attacking => {
                transform.look_to(to_player, Vec3::Y);
                ArcherState::Attacking
            }
            state => state,
        };

        let mut pos = transform.translation;
        level.move_body(&mut pos, &mut body, step, dt);
        transform.translation = pos;
    }
}

/// Kills the archer: it is thrown up (the original's `gravity# = rnd(100)
/// + 400`) and swaps to the death model.
fn kill(
    archer: &mut Archer,
    body: &mut Body,
    launch: f32,
    animation: &DeathAnimation,
    visibility: &mut Query<&mut Visibility>,
    players: &mut Query<&mut AnimationPlayer>,
) {
    archer.state = ArcherState::Dying;
    body.vel_y = launch;
    body.grounded = false;
    if let Ok(mut v) = visibility.get_mut(archer.idle_model) {
        *v = Visibility::Hidden;
    }
    if let Ok(mut v) = visibility.get_mut(archer.death_model) {
        *v = Visibility::Inherited;
    }
    if let Some(mut player) = archer.death_player.and_then(|e| players.get_mut(e).ok()) {
        player.play(animation.node);
    }
}

/// A bolt landing within reach kills outright; the dagger wounds first and
/// kills on the second hit. A wounded archer staggers and gives chase.
#[allow(clippy::too_many_arguments)]
fn take_hits(
    mut commands: Commands,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    mut strikes: EventReader<LightningStrike>,
    mut daggers: EventReader<DaggerHit>,
    animation: Res<DeathAnimation>,
    mut archers: Query<(&mut Archer, &mut Body, &Transform)>,
    mut visibility: Query<&mut Visibility>,
    mut players: Query<&mut AnimationPlayer>,
) {
    let jitter = (time.elapsed_secs() * 7919.0).fract() * 100.0;
    for strike in strikes.read() {
        for (mut archer, mut body, transform) in &mut archers {
            let (min, max) = Archer::hit_box(transform.translation);
            let reach = Vec3::splat(strike.half_extent);
            let (smin, smax) = (strike.point - reach, strike.point + reach);
            let hit = min.cmplt(smax).all() && max.cmpgt(smin).all();
            if archer.alive() && hit {
                kill(&mut archer, &mut body, 400.0 + jitter, &animation, &mut visibility, &mut players);
            }
        }
    }
    for DaggerHit(entity) in daggers.read() {
        let Ok((mut archer, mut body, _)) = archers.get_mut(*entity) else {
            continue;
        };
        if !archer.alive() {
            continue;
        }
        archer.wounds += 1;
        commands.spawn((
            AudioPlayer::new(asset_server.load("sounds/archer_grunt.wav")),
            PlaybackSettings::DESPAWN,
        ));
        if archer.wounds >= 2 {
            kill(&mut archer, &mut body, 250.0 + jitter, &animation, &mut visibility, &mut players);
        } else {
            body.vel_y = 160.0;
            body.grounded = false;
            if archer.state == ArcherState::Idle {
                archer.state = ArcherState::Walking;
            }
        }
    }
}

/// `if frame >= total object frames(...) then currentState = 10`
fn finish_dying(animation: Res<DeathAnimation>, players: Query<&AnimationPlayer>, mut archers: Query<&mut Archer>) {
    for mut archer in &mut archers {
        if archer.state != ArcherState::Dying {
            continue;
        }
        let done = archer
            .death_player
            .and_then(|e| players.get(e).ok())
            .is_none_or(|p| p.animation(animation.node).is_none_or(|a| a.is_finished()));
        if done {
            archer.state = ArcherState::Dead;
        }
    }
}
