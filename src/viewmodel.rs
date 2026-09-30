//! `drawHands()` from `player.dba`: Darius's hands (model\hand2, one
//! 48-frame timeline) ride in front of the camera. The frame range depends
//! on what the player is doing:
//!
//! - idle / bone spike: frames 0.5–24 (looping);
//! - chanting lightning: frames 24–30, slowly;
//! - dagger: draws with frames 31–45, then holds on 45–48.
//!
//! Looking down past 55° ghosts the hands (`ghost object on hand, 1`).

use crate::GameState;
use crate::anim::{self, FRAME, FrameAnim};
use crate::assets::GameAssets;
use crate::player::{Player, PlayerSet};
use crate::weapons::{Weapon, WeaponKind, WeaponSet, WeaponState};
use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;

pub struct ViewModelPlugin;

impl Plugin for ViewModelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn.after(crate::player::spawn_player))
            .add_systems(Update, animate_hands.after(PlayerSet).after(WeaponSet).run_if(in_state(GameState::Playing)))
            .add_systems(Update, show_hands);
    }
}

// Placement relative to the camera (camera looks down -Z, +Y up, +X right):
// low and close, turned to face away, so the sleeves rise out of the
// bottom corners of the screen. (drawHands used 6 ahead / 20 down / 75%
// with DarkBASIC's narrower view; this frames the same way.)
const HANDS_OFFSET: Vec3 = Vec3::new(0.0, -12.0, -12.0);
const HANDS_SCALE: f32 = 0.35;
/// `if theCamera.phi# > 55`
const GHOST_PITCH: f32 = 55.0_f32.to_radians();

#[derive(Component)]
struct Hands {
    materials: Vec<(Handle<StandardMaterial>, AlphaMode)>,
    ghosted: bool,
}

fn spawn(mut commands: Commands, assets: Res<GameAssets>, camera: Query<Entity, With<Player>>) {
    let (Ok(camera), Some(source)) = (camera.get_single(), &assets.hands.anim) else {
        return;
    };
    let hands = commands
        .spawn((
            Hands { materials: Vec::new(), ghosted: false },
            Name::new("Hands"),
            SceneRoot(assets.hands.scene.clone()),
            FrameAnim::new(source, 0.5 * FRAME),
            Transform::from_translation(HANDS_OFFSET)
                .with_rotation(Quat::from_rotation_y(std::f32::consts::PI))
                .with_scale(Vec3::splat(HANDS_SCALE)),
            Visibility::Hidden,
        ))
        .observe(anim::hook)
        .observe(collect_materials)
        .id();
    commands.entity(camera).add_child(hands);
}

fn collect_materials(
    trigger: Trigger<SceneInstanceReady>,
    children: Query<&Children>,
    parts: Query<&MeshMaterial3d<StandardMaterial>>,
    materials: Res<Assets<StandardMaterial>>,
    mut hands: Query<&mut Hands>,
) {
    let Ok(mut hands) = hands.get_mut(trigger.entity()) else {
        return;
    };
    for entity in children.iter_descendants(trigger.entity()) {
        if let Ok(m) = parts.get(entity) {
            let mode = materials.get(&m.0).map_or(AlphaMode::Opaque, |m| m.alpha_mode);
            hands.materials.push((m.0.clone(), mode));
        }
    }
}

fn animate_hands(
    time: Res<Time>,
    mut weapon: ResMut<Weapon>,
    player: Query<&Player>,
    mut hands: Query<(&mut Hands, &mut FrameAnim)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let (Ok(player), Ok((mut hands, mut anim))) = (player.get_single(), hands.get_single_mut()) else {
        return;
    };
    let dt = time.delta_secs().min(0.05);
    if let Some(cue) = weapon.hand_cue.take() {
        anim.ticks = cue;
    }
    let chanting = matches!(weapon.state, WeaponState::Chanting { .. });
    match weapon.kind {
        WeaponKind::Dagger => {
            anim.ticks += 37.5 * 30.0 * dt;
            if anim.ticks > 48.0 * FRAME {
                anim.ticks = 45.0 * FRAME;
            }
        }
        WeaponKind::Spread | WeaponKind::Power if chanting => {
            anim.ticks += 11.5 * 30.0 * dt;
            if anim.ticks > 30.0 * FRAME {
                anim.ticks = 24.0 * FRAME;
            }
        }
        _ => {
            anim.ticks += 37.5 * 30.0 * dt;
            if anim.ticks > 24.0 * FRAME {
                anim.ticks = 0.5 * FRAME;
            }
        }
    }

    let ghost = player.pitch < -GHOST_PITCH;
    if ghost != hands.ghosted {
        hands.ghosted = ghost;
        for (handle, original) in &hands.materials {
            if let Some(m) = materials.get_mut(handle) {
                m.alpha_mode = if ghost { AlphaMode::Add } else { *original };
            }
        }
    }
}

fn show_hands(state: Res<State<GameState>>, mut hands: Query<&mut Visibility, With<Hands>>) {
    let visible = matches!(state.get(), GameState::Playing | GameState::Paused | GameState::Dead);
    for mut v in &mut hands {
        let want = if visible { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
}
