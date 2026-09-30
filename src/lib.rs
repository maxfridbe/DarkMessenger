//! Dark Messenger 2026 — a port of the DarkBASIC Pro game "Dark Messenger"
//! (Nov 2003; Mark Tulewicz, Wiktor Kopec, models by Maksim Fridberg) to
//! Bevy. You are Darius, a messenger with a returning dagger, lightning and
//! a bone spike, fighting through a castle of knights and archers to a
//! book that opens the way out.
//!
//! Each original source file maps to modules here:
//!
//! | 2003 (DarkBASIC Pro)            | 2026 (Bevy)                                   |
//! |---------------------------------|-----------------------------------------------|
//! | `Dark Messenger.dba` intro/menu | `intro.rs`                                    |
//! | `Dark Messenger.dba` setup      | `assets.rs`, `world.rs` (levels, sky, light)  |
//! | `input.dba`, `player.dba`       | `player.rs` (look, move, gravity), `viewmodel.rs` (hands) |
//! | `weapon.dba` + chanting         | `weapons.rs` (dagger, lightning, bone spike)  |
//! | `npc.dba`                       | `npc.rs` (knights and archers)                |
//! | `arrow.dba`                     | `arrow.rs`                                    |
//! | `main.dba` (fire, vortex, book) | `effects.rs`                                  |
//! | `timer.dba`                     | Bevy's `Time`                                 |
//! | `set object frame`              | `anim.rs`                                     |
//! | `intersect object`              | `collision.rs` (rays vs level triangles)      |
//! | sprites / `print` debugging     | `hud.rs`                                      |
//! | (new) touch / gamepad           | `touch.rs`, `player.rs`                       |
//!
//! Positions from the original source are left-handed (DirectX); `db()`
//! converts them to Bevy's right-handed space by negating Z, matching the
//! model conversion in `tools/xconv`.

// Bevy systems routinely take many parameters and nested query filters.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod anim;
mod arrow;
mod assets;
mod collision;
mod effects;
mod hud;
mod intro;
mod npc;
mod player;
mod touch;
mod viewmodel;
mod weapons;
mod world;

use bevy::prelude::*;
use bevy_embedded_assets::{EmbeddedAssetPlugin, PluginMode};

/// Android entry point. cargo-apk builds the cdylib and NativeActivity
/// calls into this via the #[bevy_main] generated android_main.
#[bevy_main]
fn main() {
    run_game();
}

/// Top-level flow (the original's `theGame.currentState`).
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GameState {
    /// Assets and collision meshes are streaming in.
    #[default]
    Loading,
    /// The D2 model turning in the dark to `darkness.wav`.
    Intro,
    /// The painted menu: REVENGE or Cower.
    Menu,
    Playing,
    /// Pause overlay; click to (re)capture the mouse.
    Paused,
    /// Health reached zero; click to start over.
    Dead,
}

/// Converts a DarkBASIC (left-handed) position to Bevy space.
pub const fn db(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, -z)
}

pub fn run_game() {
    App::new()
        .insert_resource(ClearColor(Color::BLACK))
        // EmbeddedAssetPlugin must be added BEFORE DefaultPlugins so the
        // embedded asset source is registered before the AssetServer starts.
        // ReplaceDefault makes plain load("...") paths resolve to the
        // embedded copies; the AutoLoad default only serves embedded:// URLs,
        // so on the web assets would be fetched over HTTP and 404.
        .add_plugins(EmbeddedAssetPlugin {
            mode: PluginMode::ReplaceDefault,
        })
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Dark Messenger".into(),
                // Browser build: render into the canvas provided by
                // web/index.html and track its CSS size. Ignored on native.
                canvas: Some("#game-canvas".into()),
                fit_canvas_to_parent: true,
                ..default()
            }),
            ..default()
        }))
        .init_state::<GameState>()
        .add_plugins((
            assets::AssetsPlugin,
            anim::AnimPlugin,
            collision::CollisionPlugin,
            intro::IntroPlugin,
            world::WorldPlugin,
            player::PlayerPlugin,
            weapons::WeaponsPlugin,
            npc::NpcPlugin,
            arrow::ArrowPlugin,
            effects::EffectsPlugin,
            hud::HudPlugin,
            touch::TouchPlugin,
            viewmodel::ViewModelPlugin,
        ))
        .run();
}
