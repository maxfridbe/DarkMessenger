//! On-screen touch controls: a floating move stick on the left, drag
//! anywhere else to look, and buttons for chant, dagger, jump, bolt swap
//! and pause.
//!
//! They show on Android by default and on any platform as soon as the
//! screen is touched, and hide again when a keyboard key is pressed or a
//! gamepad is connected.

use crate::GameState;
use crate::player::{InputSet, PlayerInput, PlayerSet, read_input};
use crate::spell::Spell;
use bevy::prelude::*;
use bevy::utils::HashMap;
use bevy::window::PrimaryWindow;

pub struct TouchPlugin;

impl Plugin for TouchPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(TouchControls { visible: cfg!(any(target_os = "android", target_os = "ios")) })
            .add_systems(Startup, spawn_ui)
            .add_systems(Update, detect_input_device)
            .add_systems(
                Update,
                touch_input.after(read_input).in_set(InputSet).run_if(in_state(GameState::Playing)),
            )
            .add_systems(Update, update_ui.after(PlayerSet));
    }
}

/// Whether the on-screen controls are in use.
#[derive(Resource)]
pub struct TouchControls {
    pub visible: bool,
}

/// Radius of the stick's travel, in logical pixels.
const STICK_RADIUS: f32 = 64.0;
const STICK_KNOB: f32 = 56.0;
/// Where the stick rests when nobody is touching it (from bottom-left).
const STICK_HOME: Vec2 = Vec2::new(40.0 + STICK_RADIUS, 40.0 + STICK_RADIUS);
/// Touches starting left of this fraction of the width drive the stick.
const STICK_ZONE: f32 = 0.45;
const LOOK_SENSITIVITY: f32 = 0.005;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Button {
    Cast,
    Throw,
    Jump,
    Swap,
    Pause,
}

impl Button {
    const ALL: [Button; 5] = [Button::Cast, Button::Throw, Button::Jump, Button::Swap, Button::Pause];

    /// Diameter and centre offset: x from the right edge, y from the bottom
    /// (or from the top for Pause).
    fn layout(self) -> (f32, Vec2) {
        match self {
            Button::Cast => (112.0, Vec2::new(36.0 + 56.0, 40.0 + 56.0)),
            Button::Throw => (84.0, Vec2::new(190.0, 180.0)),
            Button::Jump => (80.0, Vec2::new(170.0 + 40.0, 28.0 + 40.0)),
            Button::Swap => (64.0, Vec2::new(44.0 + 32.0, 176.0 + 32.0)),
            Button::Pause => (52.0, Vec2::new(20.0 + 26.0, 20.0 + 26.0)),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Button::Cast => "CAST",
            Button::Throw => "DAGGER",
            Button::Jump => "JUMP",
            Button::Swap => "BOLT",
            Button::Pause => "II",
        }
    }

    /// Centre in window coordinates (origin top-left).
    fn center(self, window: Vec2) -> Vec2 {
        let (_, offset) = self.layout();
        match self {
            Button::Pause => Vec2::new(window.x - offset.x, offset.y),
            _ => Vec2::new(window.x - offset.x, window.y - offset.y),
        }
    }

    fn hit(self, window: Vec2, point: Vec2) -> bool {
        // A little larger than drawn, fingers are imprecise.
        point.distance(self.center(window)) < self.layout().0 * 0.6
    }
}

#[derive(Clone, Copy, Debug)]
enum Role {
    /// Stick with its origin where the finger landed.
    Stick(Vec2),
    Look,
    Button(Button),
}

#[derive(Component)]
struct TouchRoot;

#[derive(Component)]
struct StickBase;

#[derive(Component)]
struct StickKnob;

#[derive(Component)]
struct ButtonNode(Button);

#[derive(Component)]
struct SwapLabel;

fn circle(size: f32) -> Node {
    Node {
        position_type: PositionType::Absolute,
        width: Val::Px(size),
        height: Val::Px(size),
        border: UiRect::all(Val::Px(2.0)),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    }
}

fn spawn_ui(mut commands: Commands) {
    let ring = BorderColor(Color::srgba(0.85, 0.75, 1.0, 0.45));
    let fill = BackgroundColor(Color::srgba(0.35, 0.2, 0.55, 0.25));
    commands
        .spawn((
            TouchRoot,
            Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
            Visibility::Hidden,
        ))
        .with_children(|root| {
            let d = STICK_RADIUS * 2.0;
            root.spawn((
                StickBase,
                Node { left: Val::Px(STICK_HOME.x - STICK_RADIUS), bottom: Val::Px(STICK_HOME.y - STICK_RADIUS), ..circle(d) },
                ring,
                fill,
                BorderRadius::MAX,
            ))
            .with_child((
                StickKnob,
                Node { left: Val::Px(STICK_RADIUS - STICK_KNOB / 2.0 - 2.0), top: Val::Px(STICK_RADIUS - STICK_KNOB / 2.0 - 2.0), ..circle(STICK_KNOB) },
                BorderColor(Color::srgba(0.9, 0.85, 1.0, 0.7)),
                BackgroundColor(Color::srgba(0.6, 0.4, 0.9, 0.45)),
                BorderRadius::MAX,
            ));

            for button in Button::ALL {
                let (size, offset) = button.layout();
                let mut node = Node { right: Val::Px(offset.x - size / 2.0), ..circle(size) };
                if button == Button::Pause {
                    node.top = Val::Px(offset.y - size / 2.0);
                } else {
                    node.bottom = Val::Px(offset.y - size / 2.0);
                }
                let mut entity = root.spawn((ButtonNode(button), node, ring, fill, BorderRadius::MAX));
                entity.with_children(|b| {
                    let mut label = b.spawn((
                        Text::new(button.label()),
                        TextFont { font_size: if button == Button::Cast { 20.0 } else { 15.0 }, ..default() },
                        TextColor(Color::srgba(0.95, 0.9, 1.0, 0.85)),
                        TextLayout::new_with_justify(JustifyText::Center),
                    ));
                    if button == Button::Swap {
                        label.insert(SwapLabel);
                    }
                });
            }
        });
}

/// Keyboard or gamepad in use hides the touch UI; touching shows it.
fn detect_input_device(
    keys: Res<ButtonInput<KeyCode>>,
    touches: Res<Touches>,
    gamepads: Query<(), With<Gamepad>>,
    mut controls: ResMut<TouchControls>,
) {
    let has_other = keys.get_just_pressed().next().is_some() || !gamepads.is_empty();
    let touched = touches.iter_just_pressed().next().is_some();
    let visible = if touched && gamepads.is_empty() {
        true
    } else if has_other {
        false
    } else {
        controls.visible
    };
    if controls.visible != visible {
        controls.visible = visible;
    }
}

fn touch_input(
    touches: Res<Touches>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut input: ResMut<PlayerInput>,
    mut spell: ResMut<Spell>,
    mut next: ResMut<NextState<GameState>>,
    mut roles: Local<HashMap<u64, Role>>,
) {
    let Ok(window) = window.get_single() else {
        return;
    };
    let size = Vec2::new(window.width(), window.height());

    for touch in touches.iter_just_pressed() {
        let p = touch.position();
        let role = match Button::ALL.into_iter().find(|b| b.hit(size, p)) {
            Some(b) => Role::Button(b),
            None if p.x < size.x * STICK_ZONE && !roles.values().any(|r| matches!(r, Role::Stick(_))) => Role::Stick(p),
            None => Role::Look,
        };
        match role {
            Role::Button(Button::Jump) => input.jump = true,
            Role::Button(Button::Throw) => input.throw = true,
            Role::Button(Button::Swap) => spell.toggle_alignment(),
            Role::Button(Button::Pause) => next.set(GameState::Paused),
            _ => {}
        }
        roles.insert(touch.id(), role);
    }
    roles.retain(|id, _| touches.get_pressed(*id).is_some());

    for touch in touches.iter() {
        match roles.get(&touch.id()) {
            Some(Role::Stick(origin)) => {
                let v = ((touch.position() - *origin) / STICK_RADIUS).clamp_length_max(1.0);
                input.movement = (input.movement + Vec2::new(v.x, -v.y)).clamp_length_max(1.0);
            }
            Some(Role::Look) => input.look += touch.delta() * LOOK_SENSITIVITY,
            Some(Role::Button(Button::Cast)) => input.cast |= input.armed,
            _ => {}
        }
    }
}

/// Shows/hides the overlay and moves the stick under the thumb.
fn update_ui(
    controls: Res<TouchControls>,
    state: Res<State<GameState>>,
    touches: Res<Touches>,
    spell: Res<Spell>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut root: Query<&mut Visibility, With<TouchRoot>>,
    mut base: Query<&mut Node, (With<StickBase>, Without<StickKnob>)>,
    mut knob: Query<&mut Node, (With<StickKnob>, Without<StickBase>)>,
    mut buttons: Query<(&ButtonNode, &mut BackgroundColor)>,
    mut swap_label: Query<&mut Text, With<SwapLabel>>,
) {
    let show = controls.visible && *state.get() == GameState::Playing;
    if let Ok(mut v) = root.get_single_mut() {
        let want = if show { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
    if !show {
        return;
    }
    let Ok(window) = window.get_single() else {
        return;
    };
    let size = Vec2::new(window.width(), window.height());

    // The stick follows the first touch that started in its zone.
    let stick = touches
        .iter()
        .filter(|t| t.start_position().x < size.x * STICK_ZONE && !Button::ALL.iter().any(|b| b.hit(size, t.start_position())))
        .min_by_key(|t| t.id());
    let (origin, offset) = match stick {
        Some(t) => (t.start_position(), (t.position() - t.start_position()).clamp_length_max(STICK_RADIUS)),
        None => (Vec2::new(STICK_HOME.x, size.y - STICK_HOME.y), Vec2::ZERO),
    };
    if let Ok(mut node) = base.get_single_mut() {
        node.left = Val::Px(origin.x - STICK_RADIUS);
        node.bottom = Val::Px(size.y - origin.y - STICK_RADIUS);
    }
    if let Ok(mut node) = knob.get_single_mut() {
        let corner = STICK_RADIUS - STICK_KNOB / 2.0 - 2.0;
        node.left = Val::Px(corner + offset.x);
        node.top = Val::Px(corner + offset.y);
    }

    for (button, mut bg) in &mut buttons {
        let held = touches.iter().any(|t| button.0.hit(size, t.start_position()));
        let alpha = if held { 0.55 } else { 0.25 };
        bg.0 = Color::srgba(0.35, 0.2, 0.55, alpha);
    }
    if let Ok(mut label) = swap_label.get_single_mut() {
        let text = format!("BOLT\n{}", &spell.alignment.name()[..1]);
        if label.0 != text {
            label.0 = text;
        }
    }
}
