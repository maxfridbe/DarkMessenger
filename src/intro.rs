//! The start of `Dark Messenger.dba`:
//!
//! 1. Intro: the D2 model turns slowly in the dark while `darkness.wav`
//!    plays; it ends with the sound, or on Space (click / tap / A here).
//! 2. Menu: `menueFinal.bmp` full screen. Clicking "REVENGE" (the middle
//!    third, a quarter to half-way down) starts; "Cower" (bottom quarter)
//!    quits.
//! 3. `load.jpg` while level one loads, then play.

use crate::assets::GameAssets;
use crate::player::Player;
use crate::world::{LevelId, LoadLevel};
use crate::{GameState, db};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

pub struct IntroPlugin;

impl Plugin for IntroPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::Intro), start_intro)
            .add_systems(Update, run_intro.run_if(in_state(GameState::Intro)))
            .add_systems(OnExit(GameState::Intro), end_intro)
            .add_systems(OnEnter(GameState::Menu), show_menu)
            .add_systems(Update, run_menu.run_if(in_state(GameState::Menu)))
            .add_systems(OnExit(GameState::Menu), hide_menu);
    }
}

#[derive(Component)]
struct IntroEntity;

#[derive(Component)]
struct IntroModel;

#[derive(Component)]
struct IntroSong;

#[derive(Component)]
struct MenuScreen;

/// `darkness.wav` runs 31.4 s; the intro ends with it.
const INTRO_LENGTH: f32 = 31.5;
/// `turn object right 232, 1.25 * tinterval#`: 1.25 degrees a second.
const INTRO_TURN: f32 = 1.25;

fn start_intro(mut commands: Commands, assets: Res<GameAssets>, mut camera: Query<&mut Transform, With<Player>>) {
    commands.spawn((
        IntroEntity,
        IntroModel,
        Name::new("Intro model"),
        SceneRoot(assets.darius.scene.clone()),
        Transform::default(),
    ));
    commands.spawn((
        IntroEntity,
        DirectionalLight { illuminance: 2.5, shadows_enabled: false, ..default() },
        Transform::default().looking_to(Vec3::new(-0.3, -0.5, -1.0), Vec3::Y),
    ));
    commands.spawn((IntroEntity, IntroSong, AudioPlayer::new(assets.sounds.darkness.clone()), PlaybackSettings::DESPAWN));
    // `position camera 10, 0, 10 : point camera 0, 10, 0`
    if let Ok(mut t) = camera.get_single_mut() {
        *t = Transform::from_translation(db(10.0, 0.0, 10.0)).looking_at(db(0.0, 10.0, 0.0), Vec3::Y);
    }
}

fn run_intro(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    gamepads: Query<&Gamepad>,
    mut model: Query<&mut Transform, With<IntroModel>>,
    mut elapsed: Local<f32>,
    mut next: ResMut<NextState<GameState>>,
) {
    *elapsed += time.delta_secs();
    for mut t in &mut model {
        // DarkBASIC "right" is clockwise seen from above in its left-handed space.
        t.rotate_y((INTRO_TURN * time.delta_secs()).to_radians());
    }
    let skip = keys.any_just_pressed([KeyCode::Space, KeyCode::Enter, KeyCode::Escape])
        || buttons.just_pressed(MouseButton::Left)
        || touches.iter_just_pressed().next().is_some()
        || gamepads.iter().any(|p| p.any_just_pressed([GamepadButton::South, GamepadButton::Start]));
    if skip || *elapsed > INTRO_LENGTH {
        *elapsed = 0.0;
        next.set(GameState::Menu);
    }
}

fn end_intro(mut commands: Commands, intro: Query<Entity, With<IntroEntity>>) {
    for e in &intro {
        commands.entity(e).despawn_recursive();
    }
}

fn show_menu(mut commands: Commands, assets: Res<GameAssets>) {
    commands.spawn((
        MenuScreen,
        ImageNode::new(assets.ui_menu.clone()),
        Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
        GlobalZIndex(10),
    ));
}

/// Hit regions from the menu loop (fractions of the screen).
fn menu_choice(p: Vec2, size: Vec2) -> Option<bool> {
    let (x, y) = (p.x / size.x, p.y / size.y);
    let middle = (1.0 / 3.0..2.0 / 3.0).contains(&x);
    if middle && (0.25..0.5).contains(&y) {
        Some(true)
    } else if middle && y >= 0.75 {
        Some(false)
    } else {
        None
    }
}

fn run_menu(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    gamepads: Query<&Gamepad>,
    window: Query<&Window, With<PrimaryWindow>>,
    assets: Res<GameAssets>,
    mut screen: Query<&mut ImageNode, With<MenuScreen>>,
    mut load: EventWriter<LoadLevel>,
    mut exit: EventWriter<AppExit>,
    mut loading: Local<u8>,
    mut next: ResMut<NextState<GameState>>,
) {
    // Show load.jpg for a couple of frames, then start (the original synced
    // one frame of it while level one loaded).
    if *loading > 0 {
        *loading += 1;
        if *loading > 3 {
            *loading = 0;
            load.send(LoadLevel(LevelId::One));
            next.set(GameState::Playing);
        }
        return;
    }
    let Ok(window) = window.get_single() else {
        return;
    };
    let size = Vec2::new(window.width(), window.height());
    let mut choice = None;
    if buttons.just_pressed(MouseButton::Left) {
        choice = window.cursor_position().and_then(|p| menu_choice(p, size));
    }
    for t in touches.iter_just_pressed() {
        choice = choice.or(menu_choice(t.position(), size));
    }
    if keys.just_pressed(KeyCode::Enter) || gamepads.iter().any(|p| p.just_pressed(GamepadButton::South)) {
        choice = Some(true);
    }
    match choice {
        Some(true) => {
            if let Ok(mut image) = screen.get_single_mut() {
                image.image = assets.ui_loading.clone();
            }
            *loading = 1;
        }
        Some(false) if cfg!(not(target_arch = "wasm32")) => {
            exit.send(AppExit::Success);
        }
        _ => {}
    }
}

fn hide_menu(mut commands: Commands, menu: Query<Entity, With<MenuScreen>>) {
    for e in &menu {
        commands.entity(e).despawn_recursive();
    }
}
