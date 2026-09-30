//! `arrow.dba`: every archer owns one arrow (arrow.x at 300%). While the
//! archer has a clear shot and its arrow is idle, the arrow leaves from
//! 65 units up, aimed at the player's eye height, flies 800 units/s for
//! 1500 units, and takes 10 health (plus a grunt) if it passes through the
//! player's 50×100×50 box.

use crate::GameState;
use crate::assets::{GameAssets, play};
use crate::collision::{aabb, segment_hits_box};
use crate::npc::{Npc, NpcKind, NpcSet};
use crate::player::Player;
use bevy::prelude::*;

pub struct ArrowPlugin;

impl Plugin for ArrowPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<PlayerHit>()
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
    hit: bool,
    direction: Vec3,
    travelled: f32,
}

const SPEED: f32 = 800.0;
const RANGE: f32 = 1500.0;
const DAMAGE: f32 = 10.0;
const MODEL_SCALE: f32 = 3.0;

fn give_archers_arrows(mut commands: Commands, assets: Res<GameAssets>, archers: Query<(Entity, &Npc), Added<Npc>>) {
    for (owner, npc) in &archers {
        if npc.kind != NpcKind::Archer {
            continue;
        }
        let model = commands
            .spawn((SceneRoot(assets.arrow.scene.clone()), Transform::from_scale(Vec3::splat(MODEL_SCALE))))
            .id();
        commands
            .spawn((
                crate::world::LevelEntity,
                Name::new("Arrow"),
                Arrow { owner, flying: false, hit: false, direction: Vec3::NEG_Z, travelled: 0.0 },
                Transform::default(),
                Visibility::Hidden,
            ))
            .add_child(model);
    }
}

fn loose(
    mut commands: Commands,
    assets: Res<GameAssets>,
    player: Query<&Transform, (With<Player>, Without<Arrow>)>,
    archers: Query<(&Npc, &Transform), Without<Arrow>>,
    mut arrows: Query<(&mut Arrow, &mut Transform, &mut Visibility)>,
) {
    let Ok(player) = player.get_single() else {
        return;
    };
    for (mut arrow, mut transform, mut visibility) in &mut arrows {
        let Ok((archer, archer_t)) = archers.get(arrow.owner) else {
            continue;
        };
        if arrow.flying || !archer.shooting {
            continue;
        }
        let from = archer_t.translation + Vec3::Y * 65.0;
        let target = Vec3::new(archer.view.x, player.translation.y, archer.view.z);
        arrow.direction = (target - from).normalize_or(Vec3::NEG_Z);
        arrow.flying = true;
        arrow.hit = false;
        arrow.travelled = 0.0;
        *transform = Transform::from_translation(from).looking_to(arrow.direction, Vec3::Y);
        *visibility = Visibility::Inherited;
        play(&mut commands, &assets.sounds.arrow);
    }
}

fn fly(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<GameAssets>,
    mut hits: EventWriter<PlayerHit>,
    mut player: Query<(&mut Player, &Transform), Without<Arrow>>,
    mut arrows: Query<(&mut Arrow, &mut Transform, &mut Visibility)>,
) {
    let Ok((mut player, player_t)) = player.get_single_mut() else {
        return;
    };
    let step = SPEED * time.delta_secs().min(0.05);
    let body = aabb(player_t.translation, Vec3::new(50.0, 100.0, 50.0));
    for (mut arrow, mut transform, mut visibility) in &mut arrows {
        if !arrow.flying {
            continue;
        }
        if !arrow.hit && segment_hits_box(transform.translation, arrow.direction, step, body) {
            arrow.hit = true;
            player.health -= DAMAGE;
            play(&mut commands, &assets.sounds.grunt);
            hits.send(PlayerHit);
        }
        transform.translation += arrow.direction * step;
        arrow.travelled += step;
        if arrow.travelled > RANGE {
            arrow.flying = false;
            *visibility = Visibility::Hidden;
        }
    }
}
