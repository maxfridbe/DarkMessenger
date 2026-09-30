//! `weapon.dba` + the chant half of `player.dba`. Four weapons, picked with
//! 1–4 while idle; clicking starts the weapon's chant:
//!
//! | key | weapon          | chant                               | recharge |
//! |-----|-----------------|-------------------------------------|----------|
//! | 1   | dagger          | 0.5 s spin, then thrown at 800 u/s; flies home at 1000 u/s | until it returns |
//! | 2   | spread lightning| 2.75 s, world darkens and turns blue; thin bolt 0.75 s | 3.25 s |
//! | 3   | power lightning | same chant; thick bolt 1.85 s       | 4.25 s   |
//! | 4   | bone spike      | none: a spike erupts under the crosshair for 2 s | 6 s |
//!
//! NPCs test themselves against `Weapon::hit_box` (see `npc.rs`), as the
//! original NpcFsm did with `object collision`.

use crate::GameState;
use crate::anim::{self, FRAME, FrameAnim};
use crate::assets::{GameAssets, play};
use crate::collision::{Aabb3, LevelCollision, aabb};
use crate::player::{Player, PlayerInput, PlayerSet};
use crate::world::Ambience;
use bevy::math::Affine2;
use bevy::prelude::*;

pub struct WeaponsPlugin;

impl Plugin for WeaponsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Weapon>()
            .add_systems(Startup, spawn_visuals)
            .add_systems(
                Update,
                (select, aim, trigger, update, show_bolt, show_spike, show_dagger)
                    .chain()
                    .in_set(WeaponSet)
                    .after(PlayerSet)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(OnEnter(GameState::Intro), |mut weapon: ResMut<Weapon>| *weapon = Weapon::default())
            .add_systems(OnEnter(GameState::Dead), calm_ambience)
            .add_systems(Update, hide_outside_play.run_if(not(in_state(GameState::Playing).or(in_state(GameState::Paused)))));
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct WeaponSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeaponKind {
    Dagger,
    Spread,
    Power,
    Bone,
}

impl WeaponKind {
    pub const ALL: [WeaponKind; 4] = [WeaponKind::Dagger, WeaponKind::Spread, WeaponKind::Power, WeaponKind::Bone];

    pub fn name(self) -> &'static str {
        match self {
            WeaponKind::Dagger => "Dagger",
            WeaponKind::Spread => "Spread Lightning",
            WeaponKind::Power => "Power Lightning",
            WeaponKind::Bone => "Bone Spike",
        }
    }

    fn is_lightning(self) -> bool {
        matches!(self, WeaponKind::Spread | WeaponKind::Power)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WeaponState {
    Idle,
    /// The player's chant (`thePlayer.currentState = theState.chanting`).
    Chanting { time: f32, thunder: bool },
    /// Bolt / spike active, or the dagger flying out.
    Falling { time: f32 },
    /// The dagger flying home.
    Returning,
}

/// What an NPC is being hit by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitKind {
    Dagger,
    Bone,
    Lightning,
}

#[derive(Resource)]
pub struct Weapon {
    pub kind: WeaponKind,
    pub state: WeaponState,
    /// `thePlayer.castDelay#`; weapons can't be used until it reaches 0.
    pub cast_delay: f32,
    pub has_dagger: bool,
    /// Strike point (lightning, spike) or the dagger's position.
    pub loc: Vec3,
    dagger_dir: Vec3,
    /// `theWeapon.yScale#`: bob angle of the floating dagger, degrees.
    bob: f32,
    /// Where the crosshair ray meets the level (`object 2000`).
    pub aim_point: Option<Vec3>,
    spike_ticks: f32,
    /// Hand animation frame the view model should jump to.
    pub hand_cue: Option<f32>,
}

impl Default for Weapon {
    fn default() -> Self {
        Self {
            kind: WeaponKind::Spread,
            state: WeaponState::Idle,
            cast_delay: 0.0,
            has_dagger: true,
            loc: Vec3::ZERO,
            dagger_dir: Vec3::NEG_Z,
            bob: 0.0,
            aim_point: None,
            spike_ticks: FRAME,
            hand_cue: None,
        }
    }
}

impl Weapon {
    pub fn chanting(&self) -> bool {
        matches!(self.state, WeaponState::Chanting { .. })
    }

    /// The box NPCs test against this frame, if the weapon is live.
    pub fn hit_box(&self) -> Option<(HitKind, Aabb3)> {
        match (self.kind, self.state) {
            (WeaponKind::Dagger, WeaponState::Falling { .. } | WeaponState::Returning) => {
                Some((HitKind::Dagger, aabb(self.loc, Vec3::splat(24.0))))
            }
            (WeaponKind::Bone, WeaponState::Falling { .. }) => {
                Some((HitKind::Bone, aabb(self.loc + Vec3::Y * 30.0, Vec3::splat(60.0))))
            }
            // `make object box 543, 200, 200, 200` at the strike point.
            (WeaponKind::Spread | WeaponKind::Power, WeaponState::Falling { .. }) => {
                Some((HitKind::Lightning, aabb(self.loc, Vec3::splat(200.0))))
            }
            _ => None,
        }
    }

    /// Fraction for the left (mana) bar: the recharge, or while the dagger
    /// is out, how far away it is.
    pub fn mana_bar(&self, eye: Vec3) -> f32 {
        if self.kind == WeaponKind::Dagger && matches!(self.state, WeaponState::Falling { .. } | WeaponState::Returning) {
            (self.loc.distance(eye) * 10.0 / 748.0).min(1.0)
        } else {
            (self.cast_delay / 6.0).clamp(0.0, 1.0)
        }
    }
}

const CHANT_TIME: f32 = 2.75;
const THUNDER_TIME: f32 = 1.75;
const DARKEN_PER_SECOND: f32 = 0.35;
const DAGGER_CHANT: f32 = 0.5;
const DAGGER_SPEED: f32 = 800.0;
const DAGGER_RETURN_SPEED: f32 = 1000.0;
const DAGGER_SCALE: f32 = 2.0;
const SPIKE_SCALE: f32 = 3.5;
const SPIKE_TIME: f32 = 2.0;
const SPIKE_LAST_FRAME: f32 = 10.0 * FRAME;
/// Texture scroll: `if timeTot# > .05 then curIndex = curIndex + 1`
const BOLT_FRAME_TIME: f32 = 0.05;

#[derive(Component)]
struct Bolt;

#[derive(Component)]
struct BoltLight;

#[derive(Component)]
struct AimMarker;

#[derive(Component)]
struct Spike;

#[derive(Component)]
struct DaggerModel;

#[derive(Resource)]
struct BoltFrames(Vec<Handle<StandardMaterial>>);

fn spawn_visuals(
    mut commands: Commands,
    assets: Res<GameAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // l1–l3.bmp, additive, repeated 5x along the bolt (`scale object texture 1000, 1, 5`).
    let frames: Vec<_> = assets
        .lightning
        .iter()
        .map(|image| {
            materials.add(StandardMaterial {
                base_color_texture: Some(image.clone()),
                uv_transform: Affine2::from_scale(Vec2::new(1.0, 5.0)),
                alpha_mode: AlphaMode::Add,
                unlit: true,
                cull_mode: None,
                fog_enabled: false,
                ..default()
            })
        })
        .collect();
    commands.spawn((
        Bolt,
        Name::new("Lightning bolt"),
        Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
        MeshMaterial3d(frames[0].clone()),
        Transform::default(),
        Visibility::Hidden,
    ));
    commands.insert_resource(BoltFrames(frames));
    commands.spawn((
        BoltLight,
        PointLight { color: Color::srgb(0.75, 0.8, 1.0), intensity: 0.0, range: 1500.0, ..default() },
        Transform::default(),
    ));
    // `make object box 2000, 5, 5, 5 : texture object 2000, 2000 (cross.bmp) : ghost`
    commands.spawn((
        AimMarker,
        Name::new("Aim marker"),
        Mesh3d(meshes.add(Cuboid::new(5.0, 5.0, 5.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: Some(assets.ui_crosshair.clone()),
            alpha_mode: AlphaMode::Add,
            unlit: true,
            ..default()
        })),
        Transform::default(),
        Visibility::Hidden,
    ));
    if let Some(source) = &assets.spike.anim {
        commands
            .spawn((
                Spike,
                Name::new("Bone spike"),
                SceneRoot(assets.spike.scene.clone()),
                FrameAnim::new(source, FRAME),
                Transform::from_scale(Vec3::splat(SPIKE_SCALE)),
                Visibility::Hidden,
            ))
            .observe(anim::hook);
    }
    commands.spawn((
        DaggerModel,
        Name::new("Dagger"),
        SceneRoot(assets.dagger.scene.clone()),
        Transform::from_scale(Vec3::splat(DAGGER_SCALE)),
        Visibility::Hidden,
    ));
}

/// Keys 1–4 (or cycling) while nothing is in progress.
fn select(input: Res<PlayerInput>, mut weapon: ResMut<Weapon>) {
    if weapon.state != WeaponState::Idle {
        return;
    }
    let mut choice = input.select;
    if input.cycle != 0 {
        let i = WeaponKind::ALL.iter().position(|k| *k == weapon.kind).unwrap_or(0) as i32;
        let mut n = i;
        loop {
            n = (n + input.cycle).rem_euclid(4);
            let k = WeaponKind::ALL[n as usize];
            if k != WeaponKind::Dagger || weapon.has_dagger || n == i {
                choice = Some(k);
                break;
            }
        }
    }
    let Some(kind) = choice else {
        return;
    };
    if kind == WeaponKind::Dagger && !weapon.has_dagger {
        return;
    }
    if kind == WeaponKind::Dagger && weapon.kind != WeaponKind::Dagger {
        // `if handFrame < 31 * 150 then handFrame = 31 * 150`: draw the dagger.
        weapon.hand_cue = Some(31.0 * FRAME);
    }
    weapon.kind = kind;
}

/// The crosshair ray into the level; parks the marker where it lands.
fn aim(
    level: Res<LevelCollision>,
    mut weapon: ResMut<Weapon>,
    camera: Query<&Transform, (With<Player>, Without<AimMarker>)>,
    mut marker: Query<(&mut Transform, &mut Visibility), With<AimMarker>>,
) {
    let (Ok(camera), Ok((mut marker, mut visibility))) = (camera.get_single(), marker.get_single_mut()) else {
        return;
    };
    let dir = camera.forward();
    if let Some(d) = level.ray(camera.translation, *dir, 20_000.0) {
        weapon.aim_point = Some(camera.translation + dir * d);
        marker.translation = camera.translation + dir * d;
        *visibility = Visibility::Inherited;
    } else {
        *visibility = Visibility::Hidden;
    }
}

/// `if mouseclick() = 1 and castDelay# <= 0`: start the weapon's chant.
fn trigger(input: Res<PlayerInput>, mut weapon: ResMut<Weapon>) {
    if !input.cast || weapon.cast_delay > 0.0 || weapon.state != WeaponState::Idle {
        return;
    }
    match weapon.kind {
        WeaponKind::Spread => {
            weapon.cast_delay += 3.25;
            weapon.hand_cue = Some(24.0 * FRAME);
        }
        WeaponKind::Power => {
            weapon.cast_delay += 4.25;
            weapon.hand_cue = Some(24.0 * FRAME);
        }
        WeaponKind::Bone => {
            weapon.cast_delay += 6.0;
            weapon.spike_ticks = FRAME;
        }
        WeaponKind::Dagger => weapon.hand_cue = Some(45.0 * FRAME),
    }
    weapon.bob = 0.0;
    weapon.state = WeaponState::Chanting { time: 0.0, thunder: false };
}

/// Floor under the crosshair (the original dropped a ray from y = ±10000;
/// its test skipped back faces, so the sealed ceiling never got in the way).
fn ground_under_aim(level: &LevelCollision, weapon: &Weapon, eye: Vec3) -> Vec3 {
    let aim = weapon.aim_point.unwrap_or(eye);
    level.floor_below(aim + Vec3::Y, 20_000.0).map_or(aim, |y| Vec3::new(aim.x, y, aim.z))
}

fn update(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<GameAssets>,
    level: Res<LevelCollision>,
    mut weapon: ResMut<Weapon>,
    mut ambience: ResMut<Ambience>,
    player: Query<&Transform, With<Player>>,
) {
    let Ok(player) = player.get_single() else {
        return;
    };
    let eye = player.translation;
    let dt = time.delta_secs().min(0.05);
    // `if thePlayer.currentState = theState.idle : castDelay# - interval#`
    if !weapon.chanting() {
        weapon.cast_delay = (weapon.cast_delay - dt).max(0.0);
    }

    match weapon.state {
        WeaponState::Idle => {}
        WeaponState::Chanting { time, thunder } => {
            let time = time + dt;
            match weapon.kind {
                WeaponKind::Dagger => {
                    weapon.bob += 360.0 * dt;
                    weapon.state = if time > DAGGER_CHANT {
                        play(&mut commands, &assets.sounds.throw);
                        weapon.cast_delay = 100.0;
                        weapon.has_dagger = false;
                        weapon.loc = eye;
                        weapon.dagger_dir = *player.forward();
                        WeaponState::Falling { time: 0.0 }
                    } else {
                        WeaponState::Chanting { time, thunder }
                    };
                }
                WeaponKind::Bone => {
                    weapon.loc = ground_under_aim(&level, &weapon, eye);
                    play(&mut commands, &assets.sounds.bone);
                    weapon.state = WeaponState::Falling { time: 0.0 };
                }
                WeaponKind::Spread | WeaponKind::Power => {
                    if !ambience.chant {
                        ambience.chant = true;
                        play(&mut commands, &assets.sounds.chant);
                        weapon.loc = ground_under_aim(&level, &weapon, eye);
                    }
                    ambience.level = (ambience.level - DARKEN_PER_SECOND * dt).max(0.0);
                    let thunder_now = !thunder && time > THUNDER_TIME;
                    if thunder_now {
                        play(&mut commands, &assets.sounds.lightning);
                    }
                    weapon.state = if time > CHANT_TIME {
                        ambience.chant = false;
                        ambience.level = 1.0;
                        WeaponState::Falling { time: 0.0 }
                    } else {
                        WeaponState::Chanting { time, thunder: thunder || thunder_now }
                    };
                }
            }
        }
        WeaponState::Falling { time } => {
            let time = time + dt;
            match weapon.kind {
                WeaponKind::Bone => {
                    if weapon.spike_ticks < SPIKE_LAST_FRAME {
                        weapon.spike_ticks = (weapon.spike_ticks + FRAME * 30.0 * dt).min(SPIKE_LAST_FRAME);
                    }
                    weapon.state = if time > SPIKE_TIME {
                        weapon.spike_ticks = FRAME;
                        WeaponState::Idle
                    } else {
                        WeaponState::Falling { time }
                    };
                }
                WeaponKind::Spread | WeaponKind::Power => {
                    let duration = if weapon.kind == WeaponKind::Spread { 0.75 } else { 1.85 };
                    if weapon.kind == WeaponKind::Spread {
                        // `ghost object on 2, 2`: the lightmaps flare.
                        ambience.flash = 0.6 + 0.6 * (time * 41.0).sin().abs();
                    }
                    weapon.state = if time > duration {
                        ambience.flash = 0.0;
                        WeaponState::Idle
                    } else {
                        WeaponState::Falling { time }
                    };
                }
                WeaponKind::Dagger => {
                    let step = weapon.dagger_dir * DAGGER_SPEED * dt;
                    weapon.loc += step;
                    let wall = level.ray(weapon.loc, weapon.dagger_dir, 50.0);
                    weapon.state = if wall.is_some() || weapon.loc.distance(eye) > 8000.0 {
                        WeaponState::Returning
                    } else {
                        WeaponState::Falling { time }
                    };
                }
            }
        }
        WeaponState::Returning => {
            let to_eye = eye - weapon.loc;
            if to_eye.length() < 30.0 {
                weapon.has_dagger = true;
                weapon.cast_delay = 0.0;
                weapon.state = WeaponState::Idle;
            } else {
                weapon.loc += to_eye.normalize() * (DAGGER_RETURN_SPEED * dt).min(to_eye.length());
            }
        }
    }
}

/// The bolt while it strikes: 50 ms texture frames, turned to the player.
fn show_bolt(
    time: Res<Time>,
    weapon: Res<Weapon>,
    frames: Res<BoltFrames>,
    player: Query<&Transform, (With<Player>, Without<Bolt>, Without<BoltLight>)>,
    mut bolt: Query<(&mut Transform, &mut Visibility, &mut MeshMaterial3d<StandardMaterial>), With<Bolt>>,
    mut light: Query<(&mut Transform, &mut PointLight), (With<BoltLight>, Without<Bolt>)>,
) {
    let (Ok(player), Ok((mut t, mut vis, mut material)), Ok((mut light_t, mut light))) =
        (player.get_single(), bolt.get_single_mut(), light.get_single_mut())
    else {
        return;
    };
    let WeaponState::Falling { time: elapsed } = weapon.state else {
        *vis = Visibility::Hidden;
        light.intensity = 0.0;
        return;
    };
    if !weapon.kind.is_lightning() {
        *vis = Visibility::Hidden;
        light.intensity = 0.0;
        return;
    }
    // `scale object 1000, 30, 700, 30` (spread) / `100, 700, 100` (power)
    let width = if weapon.kind == WeaponKind::Spread { 30.0 } else { 100.0 };
    t.translation = weapon.loc + Vec3::Y * 350.0;
    t.scale = Vec3::new(width, 700.0, width);
    let d = player.translation - t.translation;
    t.rotation = Quat::from_rotation_y(d.x.atan2(d.z));
    *vis = Visibility::Inherited;
    let frame = ((elapsed / BOLT_FRAME_TIME) as usize) % frames.0.len();
    if material.0 != frames.0[frame] {
        material.0 = frames.0[frame].clone();
    }
    light_t.translation = weapon.loc + Vec3::Y * 60.0;
    light.intensity = 4.0e6 * (0.6 + 0.4 * (time.elapsed_secs() * 53.0).sin().abs());
}

fn show_spike(weapon: Res<Weapon>, mut spike: Query<(&mut Transform, &mut Visibility, &mut FrameAnim), With<Spike>>) {
    let Ok((mut t, mut vis, mut anim)) = spike.get_single_mut() else {
        return;
    };
    let active = weapon.kind == WeaponKind::Bone && matches!(weapon.state, WeaponState::Falling { .. });
    *vis = if active { Visibility::Inherited } else { Visibility::Hidden };
    t.translation = weapon.loc;
    anim.ticks = weapon.spike_ticks;
}

/// The dagger floats in front of the player while it's the chosen weapon,
/// bobs harder while chanting, and flies point first.
fn show_dagger(
    time: Res<Time>,
    mut weapon: ResMut<Weapon>,
    player: Query<(&Player, &Transform), Without<DaggerModel>>,
    mut dagger: Query<(&mut Transform, &mut Visibility), With<DaggerModel>>,
) {
    let (Ok((player, eye_t)), Ok((mut t, mut vis))) = (player.get_single(), dagger.get_single_mut()) else {
        return;
    };
    let eye = eye_t.translation;
    let forward = Vec3::new(player.yaw.cos(), 0.0, -player.yaw.sin());
    let reach = 12.0 * player.pitch.cos();
    match weapon.state {
        WeaponState::Idle if weapon.kind == WeaponKind::Dagger => {
            // `yScale# + 180 * cycleTime#`, `pos.y - 10 + cos(yScale#) * 3`
            weapon.bob += 180.0 * time.delta_secs();
            t.translation = eye + forward * reach + Vec3::Y * (-10.0 + weapon.bob.to_radians().cos() * 3.0);
            t.look_to(-forward, Vec3::Y);
            *vis = Visibility::Inherited;
        }
        WeaponState::Chanting { .. } if weapon.kind == WeaponKind::Dagger => {
            t.translation = eye + forward * reach + Vec3::Y * (-20.0 + weapon.bob.to_radians().cos() * 10.0);
            t.look_to(-forward, Vec3::Y);
            *vis = Visibility::Inherited;
        }
        WeaponState::Falling { .. } if weapon.kind == WeaponKind::Dagger => {
            t.translation = weapon.loc;
            t.look_to(-weapon.dagger_dir, Vec3::Y);
            *vis = Visibility::Inherited;
        }
        WeaponState::Returning => {
            t.translation = weapon.loc;
            let home = eye - weapon.loc;
            if home.length_squared() > 1.0 {
                t.look_to(home, Vec3::Y);
            }
            *vis = Visibility::Inherited;
        }
        _ => *vis = Visibility::Hidden,
    }
}

/// Leaving play mid-chant must not leave the world dark.
fn calm_ambience(mut ambience: ResMut<Ambience>) {
    ambience.level = 1.0;
    ambience.flash = 0.0;
    ambience.chant = false;
}

fn hide_outside_play(
    mut q: Query<&mut Visibility, Or<(With<Bolt>, With<AimMarker>, With<Spike>, With<DaggerModel>)>>,
    mut light: Query<&mut PointLight, With<BoltLight>>,
) {
    for mut v in &mut q {
        *v = Visibility::Hidden;
    }
    for mut l in &mut light {
        l.intensity = 0.0;
    }
}
