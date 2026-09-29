//! On-screen UI: crosshair, health and spell status, the title/pause and
//! death overlays, a red flash when an arrow lands, and an F3 debug readout
//! standing in for the original's `print` statements.

use crate::GameState;
use crate::arrow::PlayerHit;
use crate::collision::Body;
use crate::npc::{Archer, ArcherState};
use crate::player::{MAX_HEALTH, Player, SPAWN};
use crate::spell::{Spell, SpellState};
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
                    update_status,
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
const DIM: Color = Color::srgba(0.86, 0.8, 0.95, 0.6);
const ACCENT: Color = Color::srgb(0.72, 0.42, 1.0);

#[derive(Component)]
struct Overlay;

#[derive(Component)]
struct OverlayTitle;

#[derive(Component)]
struct OverlayBody;

#[derive(Component)]
struct HealthFill;

#[derive(Component)]
struct CastFill;

#[derive(Component)]
struct SpellText;

#[derive(Component)]
struct Banner;

#[derive(Component)]
struct HitFlash;

#[derive(Component)]
struct DebugText;

fn text(value: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(value), TextFont { font_size: size, ..default() }, TextColor(color))
}

fn bar(parent: &mut ChildBuilder, fill_color: Color, marker: impl Component) {
    parent
        .spawn((
            Node { width: Val::Px(180.0), height: Val::Px(8.0), ..default() },
            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.12)),
        ))
        .with_child((Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(fill_color), marker));
}

fn full_screen() -> Node {
    Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }
}

fn setup(mut commands: Commands) {
    // Red wash when an arrow hits.
    commands.spawn((HitFlash, full_screen(), BackgroundColor(Color::NONE)));

    // Crosshair.
    commands
        .spawn(Node { justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..full_screen() })
        .with_child(text("+", 26.0, Color::srgba(1.0, 1.0, 1.0, 0.7)));

    // Status, top left (the bottom corners belong to the touch controls).
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            left: Val::Px(16.0),
            top: Val::Px(16.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            ..default()
        })
        .with_children(|p| {
            p.spawn(text("HEALTH", 12.0, DIM));
            bar(p, Color::srgb(0.75, 0.12, 0.2), HealthFill);
            p.spawn(text("LIGHTNING", 12.0, DIM));
            bar(p, ACCENT, CastFill);
            p.spawn((text("", 14.0, TEXT), SpellText));
        });

    // Centre-top message (objectives).
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            top: Val::Px(18.0),
            justify_content: JustifyContent::Center,
            ..default()
        })
        .with_child((text("", 18.0, TEXT), Banner));

    // Debug readout (F3).
    commands.spawn((
        DebugText,
        text("", 13.0, Color::srgb(0.6, 1.0, 0.6)),
        Node { position_type: PositionType::Absolute, right: Val::Px(12.0), top: Val::Px(84.0), ..default() },
        Visibility::Hidden,
    ));

    // Title / pause / death overlay.
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
        ))
        .with_children(|p| {
            p.spawn((text("DARK MESSENGER", 54.0, ACCENT), OverlayTitle, TextLayout::new_with_justify(JustifyText::Center)));
            p.spawn((text("Loading...", 18.0, TEXT), OverlayBody, TextLayout::new_with_justify(JustifyText::Center)));
        });
}

const CONTROLS: &str = "WASD - move    Mouse - look    Space - jump\n\
Hold left click - chant lightning at the crosshair\n\
1 / 2 (or Tab) - spread / power bolt    Esc - pause    F3 - debug\n\n\
Gamepad: sticks move / look, A jump, RT chant, X / Y swap bolt, Start pause\n\
Touch: left stick walks, drag elsewhere to look, hold CAST to chant";

fn update_overlay(
    state: Res<State<GameState>>,
    mut overlay: Query<&mut Visibility, With<Overlay>>,
    mut title: Query<&mut Text, (With<OverlayTitle>, Without<OverlayBody>)>,
    mut body: Query<&mut Text, (With<OverlayBody>, Without<OverlayTitle>)>,
) {
    if !state.is_changed() {
        return;
    }
    let (Ok(mut visibility), Ok(mut title), Ok(mut body)) =
        (overlay.get_single_mut(), title.get_single_mut(), body.get_single_mut())
    else {
        return;
    };
    let (t, b) = match state.get() {
        GameState::Loading => ("DARK MESSENGER", "Loading...".to_string()),
        GameState::Paused => ("DARK MESSENGER", format!("Click or tap to play\n\n{CONTROLS}")),
        GameState::Dead => ("YOU HAVE FALLEN", "Click or tap to rise again".to_string()),
        GameState::Playing => ("", String::new()),
    };
    *visibility = if *state.get() == GameState::Playing { Visibility::Hidden } else { Visibility::Inherited };
    title.0 = t.to_string();
    body.0 = b;
}

fn update_status(
    spell: Res<Spell>,
    player: Query<&Player>,
    archers: Query<&Archer>,
    mut health: Query<&mut Node, (With<HealthFill>, Without<CastFill>)>,
    mut cast: Query<(&mut Node, &mut BackgroundColor), (With<CastFill>, Without<HealthFill>)>,
    mut spell_text: Query<&mut Text, (With<SpellText>, Without<Banner>)>,
    mut banner: Query<&mut Text, (With<Banner>, Without<SpellText>)>,
) {
    if let (Ok(player), Ok(mut node)) = (player.get_single(), health.get_single_mut()) {
        node.width = Val::Percent(100.0 * player.health as f32 / MAX_HEALTH as f32);
    }
    if let Ok((mut node, mut color)) = cast.get_single_mut() {
        let (fill, c) = match spell.state {
            SpellState::Chanting { time, .. } => ((time / 2.75).min(1.0), Color::srgb(1.0, 1.0, 1.0)),
            SpellState::Striking { .. } => (1.0, Color::srgb(0.8, 0.9, 1.0)),
            SpellState::Idle if spell.cast_delay > 0.0 => (1.0 - (spell.cast_delay / 3.0).min(1.0), Color::srgb(0.35, 0.2, 0.5)),
            SpellState::Idle => (1.0, ACCENT),
        };
        node.width = Val::Percent(100.0 * fill);
        color.0 = c;
    }
    if let Ok(mut t) = spell_text.get_single_mut() {
        let status = match spell.state {
            SpellState::Chanting { .. } => "chanting...",
            SpellState::Striking { .. } => "STRIKE",
            SpellState::Idle if spell.cast_delay > 0.0 => "gathering power",
            SpellState::Idle => "ready",
        };
        t.0 = format!("{} bolt - {status}", spell.alignment.name());
    }
    if let Ok(mut t) = banner.get_single_mut() {
        let alive = archers.iter().filter(|a| a.alive()).count();
        t.0 = match alive {
            0 => "The archers are silenced.".to_string(),
            n => format!("Archers remaining: {n}"),
        };
    }
}

fn flash_on_hit(
    time: Res<Time>,
    mut hits: EventReader<PlayerHit>,
    mut flash: Query<&mut BackgroundColor, With<HitFlash>>,
    mut strength: Local<f32>,
) {
    if hits.read().count() > 0 {
        *strength = 0.45;
    }
    *strength = (*strength - time.delta_secs() * 1.2).max(0.0);
    if let Ok(mut bg) = flash.get_single_mut() {
        bg.0 = Color::srgba(0.7, 0.0, 0.05, *strength);
    }
}

fn check_death(player: Query<&Player>, mut next: ResMut<NextState<GameState>>) {
    if player.get_single().is_ok_and(|p| p.health <= 0) {
        next.set(GameState::Dead);
    }
}

fn click_to_continue(
    state: Res<State<GameState>>,
    buttons: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    gamepads: Query<&Gamepad>,
    mut player: Query<(&mut Player, &mut Body, &mut Transform)>,
    mut next: ResMut<NextState<GameState>>,
) {
    let pad = gamepads.iter().any(|p| p.just_pressed(GamepadButton::South) || p.just_pressed(GamepadButton::Start));
    if !buttons.just_pressed(MouseButton::Left) && touches.iter_just_pressed().next().is_none() && !pad {
        return;
    }
    if *state.get() == GameState::Dead
        && let Ok((mut p, mut body, mut transform)) = player.get_single_mut() {
            *p = Player::new();
            *body = Player::body();
            transform.translation = SPAWN;
        }
    next.set(GameState::Playing);
}

fn toggle_debug(keys: Res<ButtonInput<KeyCode>>, mut debug: Query<&mut Visibility, With<DebugText>>) {
    if keys.just_pressed(KeyCode::F3)
        && let Ok(mut v) = debug.get_single_mut() {
            *v = if *v == Visibility::Hidden { Visibility::Inherited } else { Visibility::Hidden };
        }
}

fn update_debug(
    diagnostics: Res<DiagnosticsStore>,
    spell: Res<Spell>,
    player: Query<(&Transform, &Body), With<Player>>,
    archers: Query<(&Name, &Archer, &Transform)>,
    mut debug: Query<(&mut Text, &Visibility), With<DebugText>>,
) {
    let Ok((mut text, visibility)) = debug.get_single_mut() else {
        return;
    };
    if *visibility == Visibility::Hidden {
        return;
    }
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    let mut s = format!("FPS: {fps:.0}\n");
    if let Ok((t, body)) = player.get_single() {
        let p = t.translation;
        // Reported in the original's left-handed coordinates.
        s += &format!("Player: {:.0}, {:.0}, {:.0}  vy {:.0}  grounded {}\n", p.x, p.y, -p.z, body.vel_y, body.grounded);
    }
    s += &format!("Spell: {:?}  cast delay {:.2}\n", spell.state, spell.cast_delay);
    if let Some(a) = spell.aim_point {
        s += &format!("Aim: {:.0}, {:.0}, {:.0}\n", a.x, a.y, -a.z);
    }
    for (name, archer, t) in &archers {
        let state = match archer.state {
            ArcherState::Idle => "idle",
            ArcherState::Walking => "walking",
            ArcherState::Attacking => "attacking",
            ArcherState::Dying => "dying",
            ArcherState::Dead => "dead",
        };
        let p = t.translation;
        s += &format!("{name}: {state} at {:.0}, {:.0}, {:.0}\n", p.x, p.y, -p.z);
    }
    text.0 = s;
}
