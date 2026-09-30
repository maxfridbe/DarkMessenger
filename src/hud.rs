//! On-screen UI from `Dark Messenger.dba` / `updateHealthMana()`: the red
//! bar on the left (mana.bmp inside healthbar.png: the recharge, or while
//! the dagger is out, how far away it is), the blue bar on the right
//! (health.bmp inside cast.png), and the cross.bmp crosshair. Added for the
//! port: the current weapon, a hint at the book, pause/death overlays, a red
//! flash when hurt, and an F3 readout standing in for the `print`s.

use crate::GameState;
use crate::arrow::PlayerHit;
use crate::assets::GameAssets;
use crate::collision::Body;
use crate::effects::at_book;
use crate::npc::Npc;
use crate::player::{MAX_HEALTH, Player};
use crate::weapons::{Weapon, WeaponKind};
use crate::world::CurrentLevel;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin)
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (
                    update_bars,
                    update_labels,
                    update_overlay,
                    flash_on_hit,
                    toggle_debug,
                    update_debug,
                    check_death.run_if(in_state(GameState::Playing)),
                    click_to_continue.run_if(in_state(GameState::Paused).or(in_state(GameState::Dead))),
                ),
            );
    }
}

const TEXT: Color = Color::srgb(0.86, 0.8, 0.95);
const ACCENT: Color = Color::srgb(0.72, 0.42, 1.0);

#[derive(Component)]
struct HudRoot;

#[derive(Component)]
struct Overlay;

#[derive(Component)]
struct OverlayTitle;

#[derive(Component)]
struct OverlayBody;

#[derive(Component)]
struct HealthFill;

#[derive(Component)]
struct ManaFill;

#[derive(Component)]
struct WeaponLabel;

#[derive(Component)]
struct HintLabel;

#[derive(Component)]
struct HitFlash;

#[derive(Component)]
struct DebugText;

fn text(value: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(value), TextFont { font_size: size, ..default() }, TextColor(color))
}

fn full_screen() -> Node {
    Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }
}

/// A 30-px bar from 20 px below the top to the bottom edge (748 of 768 in
/// the original 1024×768 mode): a fill strip under a frame image.
fn bar(parent: &mut ChildBuilder, left: bool, fill: Handle<Image>, frame: Handle<Image>, marker: impl Component) {
    let mut node = Node {
        position_type: PositionType::Absolute,
        top: Val::Px(20.0),
        bottom: Val::Px(0.0),
        width: Val::Px(30.0),
        ..default()
    };
    if left {
        node.left = Val::Px(20.0);
    } else {
        node.right = Val::Px(20.0);
    }
    parent.spawn(node).with_children(|b| {
        b.spawn((
            ImageNode::new(fill),
            Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(0.0), ..default() },
            marker,
        ));
        b.spawn((ImageNode::new(frame), Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }));
    });
}

fn setup(mut commands: Commands, assets: Res<GameAssets>) {
    commands.spawn((HitFlash, full_screen(), BackgroundColor(Color::NONE)));

    commands
        .spawn((HudRoot, full_screen(), Visibility::Hidden))
        .with_children(|hud| {
            // `sprite 235, screen width () / 2 - 8, ... : size sprite 235, 16, 16`
            hud.spawn(Node { justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..full_screen() })
                .with_child((ImageNode::new(assets.ui_crosshair.clone()), Node { width: Val::Px(16.0), height: Val::Px(16.0), ..default() }));
            bar(hud, true, assets.ui_mana_fill.clone(), assets.ui_mana_frame.clone(), ManaFill);
            bar(hud, false, assets.ui_health_fill.clone(), assets.ui_health_frame.clone(), HealthFill);

            hud.spawn(Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                bottom: Val::Px(14.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(4.0),
                ..default()
            })
            .with_children(|b| {
                b.spawn((text("", 18.0, TEXT), HintLabel));
                b.spawn((text("", 14.0, Color::srgba(0.86, 0.8, 0.95, 0.75)), WeaponLabel));
            });
        });

    commands.spawn((
        DebugText,
        text("", 13.0, Color::srgb(0.6, 1.0, 0.6)),
        Node { position_type: PositionType::Absolute, left: Val::Px(64.0), top: Val::Px(24.0), ..default() },
        Visibility::Hidden,
    ));

    commands
        .spawn((
            Overlay,
            Node {
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(18.0),
                padding: UiRect::all(Val::Px(16.0)),
                ..full_screen()
            },
            BackgroundColor(Color::srgba(0.02, 0.0, 0.05, 0.78)),
            Visibility::Hidden,
        ))
        .with_children(|p| {
            p.spawn((text("DARK MESSENGER", 54.0, ACCENT), OverlayTitle, TextLayout::new_with_justify(JustifyText::Center)));
            p.spawn((text("", 18.0, TEXT), OverlayBody, TextLayout::new_with_justify(JustifyText::Center)));
        });
}

pub const CONTROLS: &str = "WASD - move    Mouse - look    Space - jump    E - open the book\n\
1 dagger   2 spread lightning   3 power lightning   4 bone spike\n\
Hold left click - chant    Esc - pause    F3 - debug    H - heal\n\n\
Gamepad: sticks move / look, A jump, RT chant, B use, bumpers weapon, Start pause\n\
Touch: left stick walks, drag elsewhere to look, CAST, WEAPON, JUMP, USE";

/// The YY.MMDD.## version, carried as semver build metadata (see
/// increment_version.sh).
pub fn version() -> &'static str {
    let full = env!("CARGO_PKG_VERSION");
    full.split_once('+').map_or(full, |(_, v)| v)
}

fn update_overlay(
    state: Res<State<GameState>>,
    mut overlay: Query<&mut Visibility, (With<Overlay>, Without<HudRoot>)>,
    mut hud: Query<&mut Visibility, (With<HudRoot>, Without<Overlay>)>,
    mut title: Query<&mut Text, (With<OverlayTitle>, Without<OverlayBody>)>,
    mut body: Query<&mut Text, (With<OverlayBody>, Without<OverlayTitle>)>,
) {
    if !state.is_changed() {
        return;
    }
    let (Ok(mut visibility), Ok(mut hud), Ok(mut title), Ok(mut body)) =
        (overlay.get_single_mut(), hud.get_single_mut(), title.get_single_mut(), body.get_single_mut())
    else {
        return;
    };
    let (show, t, b) = match state.get() {
        GameState::Loading => (true, "DARK MESSENGER", "Loading...".to_string()),
        GameState::Paused => (true, "PAUSED", format!("Click or tap to continue\n\n{CONTROLS}\n\nv{}", version())),
        GameState::Dead => (true, "YOU HAVE FALLEN", "Click or tap".to_string()),
        GameState::Playing | GameState::Intro | GameState::Menu => (false, "", String::new()),
    };
    *visibility = if show { Visibility::Inherited } else { Visibility::Hidden };
    let in_game = matches!(state.get(), GameState::Playing | GameState::Paused | GameState::Dead);
    *hud = if in_game { Visibility::Inherited } else { Visibility::Hidden };
    title.0 = t.to_string();
    body.0 = b;
}

/// `stretch sprite 1, 100, castDelay# / 6 * 74800` etc.
fn update_bars(
    weapon: Res<Weapon>,
    player: Query<(&Player, &Transform)>,
    mut health: Query<&mut Node, (With<HealthFill>, Without<ManaFill>)>,
    mut mana: Query<&mut Node, (With<ManaFill>, Without<HealthFill>)>,
) {
    let Ok((player, t)) = player.get_single() else {
        return;
    };
    if let Ok(mut node) = health.get_single_mut() {
        node.height = Val::Percent(100.0 * (player.health / MAX_HEALTH).clamp(0.0, 1.0));
    }
    if let Ok(mut node) = mana.get_single_mut() {
        node.height = Val::Percent(100.0 * weapon.mana_bar(t.translation));
    }
}

fn update_labels(
    weapon: Res<Weapon>,
    level: Option<Res<CurrentLevel>>,
    player: Query<&Transform, With<Player>>,
    mut labels: ParamSet<(Query<&mut Text, With<WeaponLabel>>, Query<&mut Text, With<HintLabel>>)>,
) {
    let key = WeaponKind::ALL.iter().position(|k| *k == weapon.kind).unwrap_or(0) + 1;
    if let Ok(mut t) = labels.p0().get_single_mut() {
        let text = format!("[{key}] {}", weapon.kind.name());
        if t.0 != text {
            t.0 = text;
        }
    }
    let near_book = level.is_some_and(|l| l.0 == crate::world::LevelId::One)
        && player.get_single().is_ok_and(|p| at_book(p.translation));
    if let Ok(mut t) = labels.p1().get_single_mut() {
        let text = if near_book { "E - the book" } else { "" };
        if t.0 != text {
            t.0 = text.to_string();
        }
    }
}

fn flash_on_hit(
    time: Res<Time>,
    mut hits: EventReader<PlayerHit>,
    player: Query<&Player, Changed<Player>>,
    mut flash: Query<&mut BackgroundColor, With<HitFlash>>,
    mut strength: Local<f32>,
    mut last_health: Local<f32>,
) {
    let mut hurt = hits.read().count() > 0;
    if let Ok(p) = player.get_single() {
        hurt |= p.health < *last_health - 0.5;
        *last_health = p.health;
    }
    if hurt {
        *strength = 0.45;
    }
    *strength = (*strength - time.delta_secs() * 1.2).max(0.0);
    if let Ok(mut bg) = flash.get_single_mut() {
        bg.0 = Color::srgba(0.7, 0.0, 0.05, *strength);
    }
}

/// `if thePlayer.health <= 0 : exit` — the game loop ends.
fn check_death(player: Query<&Player>, mut next: ResMut<NextState<GameState>>) {
    if player.get_single().is_ok_and(|p| p.health <= 0.0) {
        next.set(GameState::Dead);
    }
}

/// Paused: resume. Dead: back to the start (`goto restart`).
fn click_to_continue(
    state: Res<State<GameState>>,
    buttons: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    gamepads: Query<&Gamepad>,
    mut next: ResMut<NextState<GameState>>,
) {
    let pad = gamepads.iter().any(|p| p.just_pressed(GamepadButton::South) || p.just_pressed(GamepadButton::Start));
    if !buttons.just_pressed(MouseButton::Left) && touches.iter_just_pressed().next().is_none() && !pad {
        return;
    }
    next.set(if *state.get() == GameState::Dead { GameState::Intro } else { GameState::Playing });
}

fn toggle_debug(keys: Res<ButtonInput<KeyCode>>, mut debug: Query<&mut Visibility, With<DebugText>>) {
    if keys.just_pressed(KeyCode::F3)
        && let Ok(mut v) = debug.get_single_mut()
    {
        *v = if *v == Visibility::Hidden { Visibility::Inherited } else { Visibility::Hidden };
    }
}

fn update_debug(
    diagnostics: Res<DiagnosticsStore>,
    weapon: Res<Weapon>,
    player: Query<(&Player, &Transform, &Body)>,
    npcs: Query<(&Name, &Npc, &Transform)>,
    mut debug: Query<(&mut Text, &Visibility), With<DebugText>>,
) {
    let Ok((mut text, visibility)) = debug.get_single_mut() else {
        return;
    };
    if *visibility == Visibility::Hidden {
        return;
    }
    let fps = diagnostics.get(&FrameTimeDiagnosticsPlugin::FPS).and_then(|d| d.smoothed()).unwrap_or(0.0);
    let mut s = format!("FPS: {fps:.0}\n");
    if let Ok((p, t, body)) = player.get_single() {
        let pos = t.translation;
        // Reported in the original's left-handed coordinates.
        s += &format!(
            "Player: {:.0}, {:.0}, {:.0}  health {:.0}  vy {:.0}  grounded {}\n",
            pos.x, pos.y, -pos.z, p.health, body.vel_y, body.grounded
        );
    }
    s += &format!("Weapon: {:?} {:?}  cast delay {:.2}\n", weapon.kind, weapon.state, weapon.cast_delay);
    for (name, npc, t) in &npcs {
        let p = t.translation;
        s += &format!("{name}: {:?} {:.0}hp at {:.0}, {:.0}, {:.0}\n", npc.state, npc.health, p.x, p.y, -p.z);
    }
    text.0 = s;
}
