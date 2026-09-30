//! `npc.dba`: knights (knight2.x) and archers (archer3.x). Every state
//! plays a frame range of the model's single timeline, advanced by hand
//! exactly as NpcFsm did:
//!
//! | state        | knight frames        | archer frames      |
//! |--------------|----------------------|--------------------|
//! | idle         | 11–30                | 21–30              |
//! | walking      | 31–50                | 11–20              |
//! | attacking    | 50–75 (sword swings) | 63–76 (shooting)   |
//! | getting hit  | 76–79                | 31–35              |
//! | dying        | 76–100               | 31–62              |
//!
//! Knights wake within 700 units, walk up and swing (15 damage a swing).
//! Archers stand their ground and shoot from 1000 units while they can see
//! the player. The dagger wounds (20–60 damage archers, 20–40 knights),
//! lightning and the bone spike kill outright. Corpses vanish after 10 s.

use crate::GameState;
use crate::anim::{self, FRAME, FrameAnim};
use crate::assets::{GameAssets, play};
use crate::collision::{Aabb3, Body, LevelCollision, aabb, aabb_overlap, ray_box};
use crate::player::{Player, PlayerInput, PlayerSet};
use crate::weapons::{HitKind, Weapon, WeaponSet};
use crate::world::LevelEntity;
use bevy::prelude::*;

pub struct NpcPlugin;

impl Plugin for NpcPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (think, move_npcs).chain().in_set(NpcSet).after(PlayerSet).after(WeaponSet).run_if(in_state(GameState::Playing)),
        );
    }
}

/// NPC AI; arrows are loosed after it decides who is shooting.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct NpcSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NpcKind {
    Knight,
    Archer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NpcState {
    Idle,
    Walking,
    Attacking,
    GettingHit,
    Dying,
    Dead,
    Modeling,
}

#[derive(Component)]
pub struct Npc {
    pub kind: NpcKind,
    pub state: NpcState,
    pub health: f32,
    /// Animation position in DarkBASIC ticks.
    frame: f32,
    /// Knight swing direction (`theCharList().flag`).
    flag: u8,
    /// Seconds until a corpse is removed (`timeToRot#`).
    rot: f32,
    /// Point the NPC faces (`theCharList().view`).
    pub view: Vec3,
    /// Horizontal step for this frame (walking only).
    step: Vec3,
    /// An archer with a clear shot this frame.
    pub shooting: bool,
    model: Entity,
}

impl Npc {
    pub fn alive(&self) -> bool {
        !matches!(self.state, NpcState::Dying | NpcState::Dead)
    }

    /// `make object box boundingBox, 50, 100, 50` at pos + 50.
    pub fn hit_box(pos: Vec3) -> Aabb3 {
        aabb(pos + Vec3::Y * 50.0, Vec3::new(50.0, 100.0, 50.0))
    }

    fn last_frame(&self) -> f32 {
        match self.kind {
            NpcKind::Knight => 100.0 * FRAME,
            NpcKind::Archer => 76.0 * FRAME,
        }
    }
}

/// `if frame < a*150 or frame > b*150 then frame = a*150`
fn clamp_range(frame: &mut f32, a: f32, b: f32) {
    if *frame < a * FRAME || *frame > b * FRAME {
        *frame = a * FRAME;
    }
}

/// Spawns an NPC (`prepareNpc`): knights at 375%, archers at 250%.
pub fn spawn_npc(commands: &mut Commands, assets: &GameAssets, kind: NpcKind, pos: Vec3, view: Vec3, modeling: bool) {
    let (model, scale, name) = match kind {
        NpcKind::Knight => (&assets.knight, 3.75, "Knight"),
        NpcKind::Archer => (&assets.archer, 2.5, "Archer"),
    };
    let Some(source) = &model.anim else {
        return;
    };
    let model_entity = commands
        .spawn((SceneRoot(model.scene.clone()), FrameAnim::new(source, 0.0), Transform::from_scale(Vec3::splat(scale))))
        .observe(anim::hook)
        .id();
    commands
        .spawn((
            LevelEntity,
            Name::new(name),
            Npc {
                kind,
                state: if modeling { NpcState::Modeling } else { NpcState::Idle },
                health: 100.0,
                frame: 0.0,
                flag: 0,
                rot: 0.0,
                view,
                step: Vec3::ZERO,
                shooting: false,
                model: model_entity,
            },
            Body { vel_y: 0.0, grounded: false, stand_height: 0.0, step: 50.0, top: 100.0, radius: 20.0 },
            Transform::from_translation(pos).looking_to(flat(view - pos), Vec3::Y),
            Visibility::default(),
        ))
        .add_child(model_entity);
}

fn flat(v: Vec3) -> Vec3 {
    let v = Vec3::new(v.x, 0.0, v.z);
    if v.length_squared() < 1e-6 { Vec3::NEG_Z } else { v.normalize() }
}

/// Tiny xorshift for `rnd(n)`.
fn rnd(seed: &mut u32, n: f32) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    (*seed as f32 / u32::MAX as f32) * n
}

fn think(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<GameAssets>,
    level: Res<LevelCollision>,
    weapon: Res<Weapon>,
    input: Res<PlayerInput>,
    mut player: Query<(&mut Player, &Transform), Without<Npc>>,
    mut npcs: Query<(Entity, &mut Npc, &mut Body, &Transform)>,
    mut models: Query<(&mut FrameAnim, &mut Visibility)>,
    mut seed: Local<u32>,
) {
    let Ok((mut player, player_t)) = player.get_single_mut() else {
        return;
    };
    if *seed == 0 {
        *seed = 0x9E37_79B9;
    }
    let eye = player_t.translation;
    let dt = time.delta_secs().min(0.05);
    let weapon_hit = weapon.hit_box();
    // Other NPCs' boxes for the avoidance rays.
    let boxes: Vec<(Entity, Aabb3)> = npcs
        .iter()
        .filter(|(_, n, _, _)| n.state != NpcState::Dead)
        .map(|(e, _, _, t)| (e, Npc::hit_box(t.translation)))
        .collect();

    for (entity, mut npc, mut body, transform) in &mut npcs {
        let npc = &mut *npc;
        npc.shooting = false;
        npc.step = Vec3::ZERO;
        let pos = transform.translation;

        // Debug keys: K knights / L archers in and out of "modeling".
        let toggle = match npc.kind {
            NpcKind::Knight => input.model_knights,
            NpcKind::Archer => input.model_archers,
        };
        if toggle && npc.alive() {
            npc.state = if npc.state == NpcState::Modeling { NpcState::Idle } else { NpcState::Modeling };
        }

        let fwd = flat(npc.view - pos);
        let left = Vec3::new(fwd.z, 0.0, -fwd.x);
        let distance = pos.distance(eye);
        let chest = pos + Vec3::Y * 75.0;

        // Steering away from walls and other NPCs within 50 units.
        let mut side = Vec3::ZERO;
        let mut back = Vec3::ZERO;
        if npc.state == NpcState::Walking {
            let near = |d: Option<f32>| d.is_some_and(|d| d > 0.0 && d < 50.0);
            let waist = pos + Vec3::Y * 50.0;
            for (other, b) in &boxes {
                if *other == entity {
                    continue;
                }
                if near(ray_box(waist, left, 50.0, *b)) {
                    side = -left;
                }
                if near(ray_box(waist, -left, 50.0, *b)) {
                    side = left;
                }
                if near(ray_box(waist, fwd, 50.0, *b)) {
                    back = -fwd * 2.0;
                }
            }
            if near(level.ray(chest, left, 50.0)) {
                side = -left;
            }
            if near(level.ray(chest, -left, 50.0)) {
                side = left;
            }
            if near(level.ray(chest, fwd, 50.0)) {
                back = -fwd * 2.0;
            }
        }
        let avoid = (side + back) * 150.0 * dt;

        // Weapon hits (`object collision (knife | spike | 543, npc)`).
        let vulnerable = !matches!(
            npc.state,
            NpcState::GettingHit | NpcState::Dying | NpcState::Dead | NpcState::Modeling
        );
        if let Some((kind, hit)) = weapon_hit
            && vulnerable
            && aabb_overlap(hit, Npc::hit_box(pos))
        {
            match kind {
                HitKind::Dagger => {
                    npc.frame = match npc.kind {
                        NpcKind::Archer => 31.0 * FRAME,
                        NpcKind::Knight => 76.0 * FRAME,
                    };
                    npc.state = NpcState::GettingHit;
                    play(&mut commands, &assets.sounds.knife);
                    let damage = match npc.kind {
                        NpcKind::Archer => rnd(&mut seed, 40.0) + 20.0,
                        NpcKind::Knight => rnd(&mut seed, 20.0) + 20.0,
                    };
                    npc.health -= damage;
                    if npc.health <= 0.0 {
                        play(&mut commands, &assets.sounds.scream);
                        npc.state = NpcState::Dying;
                    }
                }
                HitKind::Bone => {
                    play(&mut commands, &assets.sounds.knife);
                    play(&mut commands, &assets.sounds.yell);
                    npc.health = 0.0;
                    npc.state = NpcState::Dying;
                }
                HitKind::Lightning => {
                    // `gravity# = rnd(100) + 400`: thrown into the air.
                    body.vel_y = rnd(&mut seed, 100.0) + 400.0;
                    body.grounded = false;
                    play(&mut commands, &assets.sounds.scream);
                    npc.health = 0.0;
                    npc.state = NpcState::Dying;
                }
            }
        }

        let walk_toward_player = |npc: &mut Npc| {
            npc.view = Vec3::new(eye.x, npc.view.y, eye.z);
            npc.step = flat(eye - pos) * 150.0 * dt + avoid;
        };

        match (npc.kind, npc.state) {
            (NpcKind::Knight, NpcState::Idle) => {
                clamp_range(&mut npc.frame, 11.0, 30.0);
                npc.frame += 37.5 * 30.0 * dt;
                if distance < 700.0 {
                    npc.state = NpcState::Walking;
                }
            }
            (NpcKind::Archer, NpcState::Idle) => {
                clamp_range(&mut npc.frame, 21.0, 30.0);
                npc.frame += 37.5 * 30.0 * dt;
                if distance < 1000.0 {
                    npc.state = NpcState::Attacking;
                }
            }
            (NpcKind::Knight, NpcState::Walking) => {
                clamp_range(&mut npc.frame, 31.0, 50.0);
                npc.frame += 150.0 * 30.0 * dt;
                walk_toward_player(npc);
                if distance <= 100.0 {
                    npc.state = NpcState::Attacking;
                }
                if distance > 1000.0 {
                    npc.state = NpcState::Idle;
                }
            }
            (NpcKind::Archer, NpcState::Walking) => {
                clamp_range(&mut npc.frame, 11.0, 20.0);
                npc.frame += 37.5 * 30.0 * dt;
                walk_toward_player(npc);
            }
            (NpcKind::Knight, NpcState::Attacking) => {
                // Swings up to frame 75 and back down to 50, 15 damage at
                // each end (with the original's `flag` quirk, the swing
                // never stops short at frame 53).
                let mut swing = |npc: &mut Npc, frame: f32, flag: u8| {
                    npc.frame = frame * FRAME;
                    npc.flag = flag;
                    play(&mut commands, &assets.sounds.sword);
                    play(&mut commands, &assets.sounds.grunt);
                    player.health -= 15.0;
                };
                if npc.frame <= 50.0 * FRAME {
                    swing(npc, 50.0, 0);
                }
                if npc.frame >= 75.0 * FRAME {
                    swing(npc, 72.0, 1);
                }
                let dir = if npc.flag == 1 { -1.0 } else { 1.0 };
                npc.frame += dir * 150.0 * 30.0 * dt;
                npc.view = Vec3::new(eye.x, npc.view.y, eye.z);
                if distance >= 100.0 {
                    npc.state = NpcState::Walking;
                }
                if distance >= 750.0 {
                    npc.state = NpcState::Idle;
                }
            }
            (NpcKind::Archer, NpcState::Attacking) => {
                npc.view = Vec3::new(eye.x, npc.view.y, eye.z);
                if level.line_of_sight(chest, eye) {
                    clamp_range(&mut npc.frame, 63.0, 76.0);
                    npc.frame += 39.0 * 30.0 * dt;
                    npc.shooting = true;
                }
                if distance > 1200.0 {
                    npc.state = NpcState::Idle;
                }
            }
            (NpcKind::Archer, NpcState::GettingHit) => {
                if npc.frame < 35.0 * FRAME {
                    npc.frame += 65.0 * 30.0 * dt;
                } else {
                    npc.state = NpcState::Attacking;
                }
            }
            (NpcKind::Knight, NpcState::GettingHit) => {
                if npc.frame < 79.0 * FRAME {
                    npc.frame += 65.0 * 30.0 * dt;
                } else {
                    npc.state = NpcState::Walking;
                }
            }
            (NpcKind::Archer, NpcState::Dying) => {
                clamp_range(&mut npc.frame, 31.0, 62.0);
                npc.frame += 65.0 * 30.0 * dt;
                if npc.frame >= 62.0 * FRAME {
                    npc.frame = 62.0 * FRAME;
                    npc.rot = 10.0;
                    npc.state = NpcState::Dead;
                }
            }
            (NpcKind::Knight, NpcState::Dying) => {
                clamp_range(&mut npc.frame, 76.0, 100.0);
                let speed = if npc.frame < 80.0 * FRAME { 15.0 } else { 65.0 };
                npc.frame += speed * 30.0 * dt;
                if npc.frame >= 100.0 * FRAME {
                    npc.frame = 100.0 * FRAME;
                    npc.rot = 10.0;
                    npc.state = NpcState::Dead;
                }
            }
            (_, NpcState::Dead) => {
                npc.rot -= dt;
                if input.resurrect {
                    // `keystate(19)`: the dead rise again.
                    npc.state = NpcState::Idle;
                    npc.health = 100.0;
                    body.vel_y = 100.0;
                    body.grounded = false;
                }
            }
            (_, NpcState::Modeling) => {
                npc.frame += 37.5 * 30.0 * dt;
                if npc.frame > npc.last_frame() {
                    npc.frame = 0.0;
                }
            }
        }

        if let Ok((mut anim, mut vis)) = models.get_mut(npc.model) {
            anim.ticks = npc.frame;
            // swapNpc: a corpse that has rotted away is removed.
            let gone = npc.state == NpcState::Dead && npc.rot <= 0.0;
            let want = if gone { Visibility::Hidden } else { Visibility::Inherited };
            if *vis != want {
                *vis = want;
            }
        }
    }
}

/// NpcCalcGrav / NpcGrav / NpcUpdatePos, plus facing the view point.
fn move_npcs(time: Res<Time>, level: Res<LevelCollision>, mut npcs: Query<(&Npc, &mut Body, &mut Transform)>) {
    let dt = time.delta_secs().min(0.05);
    for (npc, mut body, mut t) in &mut npcs {
        let step = if npc.state == NpcState::Walking { npc.step } else { Vec3::ZERO };
        let mut pos = t.translation;
        level.move_body(&mut pos, &mut body, step, dt);
        t.translation = pos;
        let face = flat(npc.view - pos);
        t.look_to(face, Vec3::Y);
    }
}
