//! The lightning spell from `main.dba`. Clicking starts a chant: the world
//! darkens for 2.75 s (`lightLevel#` drains 35/s) while thunder builds, then
//! a bolt falls from the sky onto the spot under the crosshair and flings
//! any archer within 100 units into the air.
//!
//! Two alignments exist, as in the original: *spread* (thin bolt, 0.75 s,
//! +1.25 s cast delay) and *power* (thick bolt, 1.25 s, +3 s cast delay).

use crate::GameState;
use crate::collision::LevelCollision;
use crate::player::{Player, PlayerInput, PlayerSet};
use crate::world::Ambience;
use bevy::audio::Volume;
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::prelude::*;

pub struct SpellPlugin;

impl Plugin for SpellPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Spell>()
            .add_event::<LightningStrike>()
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (select_alignment, aim, cast, animate_bolt)
                    .chain()
                    .after(PlayerSet)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(OnExit(GameState::Playing), restore_light);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    Spread,
    Power,
}

impl Alignment {
    /// Bolt width, height, strike duration and added cast delay.
    fn stats(self) -> (f32, f32, f32, f32) {
        match self {
            Alignment::Spread => (30.0, 700.0, 0.75, 1.25),
            Alignment::Power => (30.0, 900.0, 1.25, 3.0),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Alignment::Spread => "Spread",
            Alignment::Power => "Power",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpellState {
    Idle,
    Chanting { time: f32, thunder: bool },
    Striking { time: f32 },
}

#[derive(Resource)]
pub struct Spell {
    pub state: SpellState,
    pub alignment: Alignment,
    pub cast_delay: f32,
    /// Ground point the bolt will hit (`theWeapon.loc`).
    pub strike_point: Vec3,
    /// Where the crosshair ray meets the level (`object 2000`).
    pub aim_point: Option<Vec3>,
}

impl Default for Spell {
    fn default() -> Self {
        Self {
            state: SpellState::Idle,
            alignment: Alignment::Spread,
            cast_delay: 0.0,
            strike_point: Vec3::ZERO,
            aim_point: None,
        }
    }
}

/// Sent every frame a bolt is on the ground: archers inside the 200-unit
/// box around `point` are hit (`object collision (543, npc)`).
#[derive(Event)]
pub struct LightningStrike {
    pub point: Vec3,
    pub half_extent: f32,
}

const CHANT_TIME: f32 = 2.75;
/// `if timeChanting# > 1.75 ... play sound 1`
const THUNDER_TIME: f32 = 1.75;
/// `lightLevel# = lightLevel# - 35 * cycleTime`
const DARKEN_PER_SECOND: f32 = 0.35;
/// `make object box 543, 200, 200, 200`
const STRIKE_BOX_HALF: f32 = 100.0;
/// Texture scroll: `if timeTot# > .05 then curIndex = curIndex + 1`
const FRAME_TIME: f32 = 0.05;

#[derive(Resource)]
struct SpellAssets {
    chant: Handle<AudioSource>,
    thunder: Handle<AudioSource>,
    frames: Vec<Handle<StandardMaterial>>,
}

#[derive(Component)]
struct Bolt;

#[derive(Component)]
struct BoltLight;

#[derive(Component)]
struct AimMarker;

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // l1.bmp .. l3.bmp, additive (`ghost object on`), texture repeated 5x
    // along the bolt (`scale object texture 1000, 1, 5`).
    let repeat = |s: &mut ImageLoaderSettings| {
        s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            ..ImageSamplerDescriptor::linear()
        });
    };
    let frames = (1..=3)
        .map(|i| {
            materials.add(StandardMaterial {
                base_color_texture: Some(asset_server.load_with_settings(format!("textures/lightning{i}.png"), repeat)),
                uv_transform: Affine2::from_scale(Vec2::new(1.0, 5.0)),
                alpha_mode: AlphaMode::Add,
                unlit: true,
                cull_mode: None,
                fog_enabled: false,
                ..default()
            })
        })
        .collect::<Vec<_>>();

    commands.spawn((
        Bolt,
        Name::new("Lightning bolt"),
        Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
        MeshMaterial3d(frames[0].clone()),
        Transform::default(),
        Visibility::Hidden,
    ));
    commands.spawn((
        BoltLight,
        PointLight { color: Color::srgb(0.75, 0.8, 1.0), intensity: 0.0, range: 1500.0, ..default() },
        Transform::default(),
    ));
    // `make object box 2000, 5, 5, 5`: the aim point marker.
    commands.spawn((
        AimMarker,
        Name::new("Aim marker"),
        Mesh3d(meshes.add(Cuboid::new(6.0, 6.0, 6.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.8, 0.3, 1.0),
            unlit: true,
            ..default()
        })),
        Transform::default(),
        Visibility::Hidden,
    ));

    commands.insert_resource(SpellAssets {
        chant: asset_server.load("sounds/chant.wav"),
        thunder: asset_server.load("sounds/lightning.wav"),
        frames,
    });
}

impl Spell {
    pub fn toggle_alignment(&mut self) {
        self.alignment = match self.alignment {
            Alignment::Spread => Alignment::Power,
            Alignment::Power => Alignment::Spread,
        };
    }
}

/// 1 / 2 pick spread / power. Ctrl selects power, as the original did.
/// Tab, Q or a gamepad's X / Y (square / triangle) swap them.
fn select_alignment(keys: Res<ButtonInput<KeyCode>>, gamepads: Query<&Gamepad>, mut spell: ResMut<Spell>) {
    if keys.just_pressed(KeyCode::Digit1) {
        spell.alignment = Alignment::Spread;
    }
    if keys.any_just_pressed([KeyCode::Digit2, KeyCode::ControlLeft, KeyCode::ControlRight]) {
        spell.alignment = Alignment::Power;
    }
    let pad_swap = gamepads.iter().any(|p| p.just_pressed(GamepadButton::West) || p.just_pressed(GamepadButton::North));
    if keys.any_just_pressed([KeyCode::Tab, KeyCode::KeyQ]) || pad_swap {
        spell.toggle_alignment();
    }
}

/// Casts the view ray into the level and parks the marker where it lands.
fn aim(
    level: Res<LevelCollision>,
    mut spell: ResMut<Spell>,
    camera: Query<&Transform, (With<Player>, Without<AimMarker>)>,
    mut marker: Query<(&mut Transform, &mut Visibility), With<AimMarker>>,
) {
    let (Ok(camera), Ok((mut marker, mut visibility))) = (camera.get_single(), marker.get_single_mut()) else {
        return;
    };
    let dir = camera.forward();
    spell.aim_point = level.ray(camera.translation, *dir, 20_000.0).map(|d| camera.translation + dir * d);
    match spell.aim_point {
        Some(p) => {
            marker.translation = p;
            *visibility = Visibility::Inherited;
        }
        None => *visibility = Visibility::Hidden,
    }
}

fn cast(
    mut commands: Commands,
    time: Res<Time>,
    input: Res<PlayerInput>,
    level: Res<LevelCollision>,
    assets: Res<SpellAssets>,
    mut spell: ResMut<Spell>,
    mut ambience: ResMut<Ambience>,
    mut strikes: EventWriter<LightningStrike>,
) {
    let dt = time.delta_secs();
    match spell.state {
        SpellState::Idle => {
            spell.cast_delay = (spell.cast_delay - dt).max(0.0);
            if input.cast && spell.cast_delay <= 0.0
                && let Some(aim) = spell.aim_point {
                    // The original dropped a ray from y = 10000 onto the level.
                    // Its test skipped back faces, so the level's sealed
                    // ceiling was invisible to it; here the bolt lands on the
                    // floor under the aim point instead.
                    spell.strike_point = level.floor_below(aim + Vec3::Y, 20_000.0).map_or(aim, |y| Vec3::new(aim.x, y, aim.z));
                    spell.state = SpellState::Chanting { time: 0.0, thunder: false };
                    play(&mut commands, &assets.chant);
                }
        }
        SpellState::Chanting { time, thunder } => {
            let time = time + dt;
            ambience.level = (ambience.level - DARKEN_PER_SECOND * dt).max(0.0);
            let thunder_now = !thunder && time > THUNDER_TIME;
            if thunder_now {
                play(&mut commands, &assets.thunder);
            }
            let thunder = thunder || thunder_now;
            spell.state = if time > CHANT_TIME {
                ambience.level = 1.0;
                spell.cast_delay += spell.alignment.stats().3;
                SpellState::Striking { time: 0.0 }
            } else {
                SpellState::Chanting { time, thunder }
            };
        }
        SpellState::Striking { time } => {
            let time = time + dt;
            let (_, _, duration, _) = spell.alignment.stats();
            strikes.send(LightningStrike { point: spell.strike_point, half_extent: STRIKE_BOX_HALF });
            // `ghost object on 2, 2`: the lightmaps flare while the bolt is down.
            ambience.flash = 0.6 + 0.6 * (time * 41.0).sin().abs();
            spell.state = if time > duration {
                ambience.flash = 0.0;
                SpellState::Idle
            } else {
                SpellState::Striking { time }
            };
        }
    }
}

fn play(commands: &mut Commands, sound: &Handle<AudioSource>) {
    commands.spawn((AudioPlayer::new(sound.clone()), PlaybackSettings::DESPAWN.with_volume(Volume::new(0.9))));
}

/// Shows the bolt while striking: texture frames cycle every 50 ms and the
/// bolt turns to face the player (`point object 1000, player x, y, z`).
fn animate_bolt(
    time: Res<Time>,
    spell: Res<Spell>,
    assets: Res<SpellAssets>,
    player: Query<&Transform, (With<Player>, Without<Bolt>, Without<BoltLight>)>,
    mut bolt: Query<(&mut Transform, &mut Visibility, &mut MeshMaterial3d<StandardMaterial>), With<Bolt>>,
    mut light: Query<(&mut Transform, &mut PointLight), (With<BoltLight>, Without<Bolt>)>,
) {
    let (Ok(player), Ok((mut transform, mut visibility, mut material)), Ok((mut light_tf, mut light))) =
        (player.get_single(), bolt.get_single_mut(), light.get_single_mut())
    else {
        return;
    };
    let SpellState::Striking { time: t } = spell.state else {
        *visibility = Visibility::Hidden;
        light.intensity = 0.0;
        return;
    };
    let (width, height, _, _) = spell.alignment.stats();
    let base = spell.strike_point;
    transform.translation = base + Vec3::Y * height / 2.0;
    transform.scale = Vec3::new(width, height, width);
    let to_player = Vec3::new(player.translation.x, transform.translation.y, player.translation.z);
    if to_player.distance_squared(transform.translation) > 1.0 {
        transform.look_at(to_player, Vec3::Y);
    }
    *visibility = Visibility::Inherited;
    let frame = ((t / FRAME_TIME) as usize) % assets.frames.len();
    if material.0 != assets.frames[frame] {
        material.0 = assets.frames[frame].clone();
    }
    light_tf.translation = base + Vec3::Y * 60.0;
    light.intensity = 4.0e6 * (0.6 + 0.4 * (time.elapsed_secs() * 53.0).sin().abs());
}

/// Leaving play mid-chant must not leave the world dark.
fn restore_light(mut spell: ResMut<Spell>, mut ambience: ResMut<Ambience>) {
    if !matches!(spell.state, SpellState::Idle) {
        spell.state = SpellState::Idle;
    }
    ambience.level = 1.0;
    ambience.flash = 0.0;
}
