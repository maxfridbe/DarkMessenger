//! `input.dba` + the player half of `main.dba`: first-person mouse look,
//! WASD movement, jumping and gravity. The camera *is* the player's eye.
//! Gamepads work too; touch input is layered on top by `touch.rs`.

use crate::collision::{Body, LevelCollision};
use crate::{GameState, db};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::render::camera::Exposure;
use bevy::window::{CursorGrabMode, PrimaryWindow};

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerInput>()
            .add_systems(Startup, spawn_player)
            .add_systems(OnEnter(GameState::Playing), (grab_cursor, |mut input: ResMut<PlayerInput>| input.armed = false))
            .add_systems(OnExit(GameState::Playing), release_cursor)
            .configure_sets(Update, InputSet.before(look).in_set(PlayerSet))
            .add_systems(Update, read_input.in_set(InputSet).run_if(in_state(GameState::Playing)))
            .add_systems(Update, (look, walk).chain().in_set(PlayerSet).run_if(in_state(GameState::Playing)))
            .add_systems(Update, (pause_on_escape, regrab_on_click).run_if(in_state(GameState::Playing)));
    }
}

/// Runs before anything that reads the player's position this frame.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlayerSet;

/// Fills `PlayerInput` (keyboard/mouse/gamepad, then touch) before the
/// player looks and moves.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct InputSet;

pub const SPAWN: Vec3 = db(0.0, 100.0, 0.0);
pub const MAX_HEALTH: i32 = 100;
/// `speed# = 250.0`
const MOVE_SPEED: f32 = 250.0;
/// `thePlayer.gravity# = 300` on jump.
const JUMP_SPEED: f32 = 300.0;
/// The original kept the eye ~80–87 units above the floor.
pub const EYE_HEIGHT: f32 = 85.0;
/// `theCamera.phi#` was clamped to ±80 degrees.
const PITCH_LIMIT: f32 = 80.0_f32.to_radians();
const MOUSE_SENSITIVITY: f32 = 0.0025;
/// Radians per second at full right-stick deflection.
const GAMEPAD_LOOK_SPEED: f32 = 2.6;
const STICK_DEADZONE: f32 = 0.15;

#[derive(Component)]
pub struct Player {
    pub health: i32,
    /// Radians; 0 looks down +X like the original `theCamera.theta# = 0`.
    pub yaw: f32,
    pub pitch: f32,
}

impl Player {
    pub fn new() -> Self {
        Self { health: MAX_HEALTH, yaw: 0.0, pitch: 0.0 }
    }

    pub fn body() -> Body {
        Body { vel_y: 0.0, grounded: false, stand_height: EYE_HEIGHT, step: 35.0, top: 95.0, radius: 16.0 }
    }
}

/// This frame's intent, merged from keyboard/mouse, gamepad and touch.
#[derive(Resource, Default)]
pub struct PlayerInput {
    /// x = strafe right, y = forward.
    pub movement: Vec2,
    pub look: Vec2,
    pub jump: bool,
    /// Held: keep chanting whenever the cast delay allows (`mouseclick() = 1`).
    pub cast: bool,
    /// False until the click/tap/button that started or resumed play has
    /// been released, so it doesn't also start a chant.
    pub armed: bool,
}

fn spawn_player(mut commands: Commands) {
    commands.spawn((
        Name::new("Player"),
        Player::new(),
        Player::body(),
        Camera3d::default(),
        // `set camera fov 75 : set camera range 1, 10000`
        Projection::Perspective(PerspectiveProjection {
            fov: 75.0_f32.to_radians(),
            near: 1.0,
            far: 12_000.0,
            ..default()
        }),
        // Unit exposure + no tonemapping: light values behave like the
        // original fixed-function pipeline (1.0 = full brightness).
        Exposure { ev100: -(1.2_f32.log2()) },
        Tonemapping::None,
        Msaa::Sample4,
        Transform::from_translation(SPAWN),
    ));
}

pub fn read_input(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    touches: Res<Touches>,
    gamepads: Query<&Gamepad>,
    mut input: ResMut<PlayerInput>,
) {
    let mut movement = Vec2::ZERO;
    if keys.any_pressed([KeyCode::KeyW, KeyCode::ArrowUp]) {
        movement.y += 1.0;
    }
    if keys.any_pressed([KeyCode::KeyS, KeyCode::ArrowDown]) {
        movement.y -= 1.0;
    }
    if keys.any_pressed([KeyCode::KeyA, KeyCode::ArrowLeft]) {
        movement.x -= 1.0;
    }
    if keys.any_pressed([KeyCode::KeyD, KeyCode::ArrowRight]) {
        movement.x += 1.0;
    }
    input.look = motion.delta * MOUSE_SENSITIVITY;
    input.jump = keys.pressed(KeyCode::Space);
    let mut cast = buttons.pressed(MouseButton::Left);

    // Left stick moves, right stick looks, A / cross jumps, right trigger chants.
    let deadzone = |v: Vec2| if v.length() < STICK_DEADZONE { Vec2::ZERO } else { v };
    let mut all_released = !cast;
    for pad in &gamepads {
        movement += deadzone(pad.left_stick());
        let look = deadzone(pad.right_stick());
        input.look += Vec2::new(look.x, -look.y) * GAMEPAD_LOOK_SPEED * time.delta_secs();
        input.jump |= pad.pressed(GamepadButton::South);
        let pad_cast = pad.pressed(GamepadButton::RightTrigger2) || pad.pressed(GamepadButton::RightTrigger);
        cast |= pad_cast;
        all_released &= !pad_cast && !pad.pressed(GamepadButton::South) && !pad.pressed(GamepadButton::Start);
    }
    input.cast = input.armed && cast;
    input.armed |= all_released && touches.iter().next().is_none();
    input.movement = movement.clamp_length_max(1.0);
}

/// `rotateOnLeftRight` / `rotateOnUpDown`.
fn look(input: Res<PlayerInput>, mut player: Query<(&mut Player, &mut Transform)>) {
    let Ok((mut player, mut transform)) = player.get_single_mut() else {
        return;
    };
    player.yaw -= input.look.x;
    player.pitch = (player.pitch - input.look.y).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    // Bevy cameras look down -Z; yaw 0 should face +X.
    transform.rotation = Quat::from_euler(EulerRot::YXZ, player.yaw - std::f32::consts::FRAC_PI_2, player.pitch, 0.0);
}

fn walk(
    time: Res<Time>,
    input: Res<PlayerInput>,
    level: Res<LevelCollision>,
    mut player: Query<(&Player, &mut Body, &mut Transform)>,
) {
    let Ok((player, mut body, mut transform)) = player.get_single_mut() else {
        return;
    };
    let dt = time.delta_secs().min(0.05);
    let forward = Vec3::new(player.yaw.cos(), 0.0, -player.yaw.sin());
    let right = Vec3::new(-forward.z, 0.0, forward.x);
    let horizontal = (forward * input.movement.y + right * input.movement.x) * MOVE_SPEED * dt;

    if input.jump && body.grounded {
        body.vel_y = JUMP_SPEED;
    }
    let mut pos = transform.translation;
    level.move_body(&mut pos, &mut body, horizontal, dt);
    // Fell out of the level: start over at the spawn point.
    if pos.y < -3000.0 {
        pos = SPAWN;
        body.vel_y = 0.0;
    }
    transform.translation = pos;
}

fn set_grab(window: &mut Window, grab: bool) {
    window.cursor_options.grab_mode = if grab { CursorGrabMode::Locked } else { CursorGrabMode::None };
    window.cursor_options.visible = !grab;
}

fn grab_cursor(mut window: Query<&mut Window, With<PrimaryWindow>>) {
    if let Ok(mut window) = window.get_single_mut() {
        set_grab(&mut window, true);
    }
}

fn release_cursor(mut window: Query<&mut Window, With<PrimaryWindow>>) {
    if let Ok(mut window) = window.get_single_mut() {
        set_grab(&mut window, false);
    }
}

fn pause_on_escape(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut next: ResMut<NextState<GameState>>,
) {
    // Browsers report focus unreliably for the canvas; rely on Esc/click there.
    let unfocused = cfg!(not(target_arch = "wasm32")) && window.get_single().is_ok_and(|w| !w.focused);
    let start = gamepads.iter().any(|p| p.just_pressed(GamepadButton::Start));
    if keys.just_pressed(KeyCode::Escape) || unfocused || start {
        next.set(GameState::Paused);
    }
}

/// Browsers drop pointer lock on Esc without telling the game; clicking
/// again re-requests it.
fn regrab_on_click(buttons: Res<ButtonInput<MouseButton>>, mut window: Query<&mut Window, With<PrimaryWindow>>) {
    if buttons.just_pressed(MouseButton::Left)
        && let Ok(mut window) = window.get_single_mut() {
            set_grab(&mut window, true);
        }
}
