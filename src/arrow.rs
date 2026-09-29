//! `arrow.dba`: every archer owns one arrow. While the archer is attacking
//! and its arrow is idle, the arrow is loosed from chest height along the
//! archer's facing and flies 650 units/s for up to 1000 units.
//!
//! The original arrows passed through everything; here they stop at walls
//! and wound the player (the unused `thePlayer.health` of the original).

use crate::GameState;
use crate::collision::LevelCollision;
use crate::npc::{Archer, ArcherState, EYE_OFFSET, NpcSet};
use crate::player::{EYE_HEIGHT, Player};
use bevy::audio::Volume;
use bevy::prelude::*;

pub struct ArrowPlugin;

impl Plugin for ArrowPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<PlayerHit>()
            .add_systems(Startup, load_sound)
            .add_systems(Update, give_archers_arrows)
            .add_systems(Update, (loose, fly).chain().after(NpcSet).run_if(in_state(GameState::Playing)));
    }
}

/// The player was struck by an arrow.
#[derive(Event)]
pub struct PlayerHit;

#[derive(Component)]
struct Arrow {
    owner: Entity,
    flying: bool,
    direction: Vec3,
    travelled: f32,
}

const SPEED: f32 = 650.0;
const RANGE: f32 = 1000.0;
const DAMAGE: i32 = 10;
/// How close (horizontally) an arrow must pass to hit the player.
const HIT_RADIUS: f32 = 24.0;
/// `scale object ..., 750, 750, 750`
const MODEL_SCALE: f32 = 7.5;

#[derive(Resource)]
struct ArrowSound(Handle<AudioSource>);

fn load_sound(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(ArrowSound(asset_server.load("sounds/arrow.wav")));
}

fn give_archers_arrows(mut commands: Commands, asset_server: Res<AssetServer>, archers: Query<Entity, Added<Archer>>) {
    for owner in &archers {
        let model = commands
            .spawn((
                SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/arrow.glb"))),
                Transform::from_scale(Vec3::splat(MODEL_SCALE)),
            ))
            .id();
        commands
            .spawn((
                Name::new("Arrow"),
                Arrow { owner, flying: false, direction: Vec3::NEG_Z, travelled: 0.0 },
                Transform::default(),
                Visibility::Hidden,
            ))
            .add_child(model);
    }
}

fn loose(
    mut commands: Commands,
    sound: Res<ArrowSound>,
    archers: Query<(&Archer, &Transform), Without<Arrow>>,
    mut arrows: Query<(&mut Arrow, &mut Transform, &mut Visibility)>,
) {
    for (mut arrow, mut transform, mut visibility) in &mut arrows {
        let Ok((archer, archer_tf)) = archers.get(arrow.owner) else {
            continue;
        };
        if arrow.flying || archer.state != ArcherState::Attacking {
            continue;
        }
        arrow.flying = true;
        arrow.travelled = 0.0;
        arrow.direction = *archer_tf.forward();
        *transform = Transform::from_translation(archer_tf.translation + Vec3::Y * EYE_OFFSET)
            .looking_to(arrow.direction, Vec3::Y);
        *visibility = Visibility::Inherited;
        commands.spawn((AudioPlayer::new(sound.0.clone()), PlaybackSettings::DESPAWN.with_volume(Volume::new(0.7))));
    }
}

fn fly(
    time: Res<Time>,
    level: Res<LevelCollision>,
    mut hits: EventWriter<PlayerHit>,
    mut player: Query<(&mut Player, &Transform), Without<Arrow>>,
    mut arrows: Query<(&mut Arrow, &mut Transform, &mut Visibility)>,
) {
    let Ok((mut player, player_tf)) = player.get_single_mut() else {
        return;
    };
    let step = SPEED * time.delta_secs().min(0.05);
    for (mut arrow, mut transform, mut visibility) in &mut arrows {
        if !arrow.flying {
            continue;
        }
        let from = transform.translation;
        let to = from + arrow.direction * step;
        arrow.travelled += step;

        // Closest approach to the player's vertical span during this step.
        let eye = player_tf.translation;
        let t = ((eye - from).dot(arrow.direction) / step).clamp(0.0, 1.0);
        let closest = from + arrow.direction * step * t;
        let horizontal = Vec2::new(closest.x - eye.x, closest.z - eye.z).length();
        let struck = horizontal < HIT_RADIUS && closest.y > eye.y - EYE_HEIGHT && closest.y < eye.y + 15.0;

        let blocked = level.ray(from, arrow.direction, step).is_some();
        if struck && player.health > 0 {
            player.health = (player.health - DAMAGE).max(0);
            hits.send(PlayerHit);
        }
        if struck || blocked || arrow.travelled > RANGE {
            arrow.flying = false;
            *visibility = Visibility::Hidden;
        } else {
            transform.translation = to;
        }
    }
}
