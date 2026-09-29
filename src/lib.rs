//! Dark Messenger 2026 — a port of the 2003 DarkBASIC Pro prototype
//! "Dark Messenger" (first-person lightning mage vs. archers) to Bevy.
//!
//! Each original source file maps to a plugin:
//!
//! | 2003 (DarkBASIC Pro)          | 2026 (Bevy)                              |
//! |-------------------------------|------------------------------------------|
//! | `Dark Messenger.dba` (setup)  | `world.rs` (level, sky, lights, statue)  |
//! | `input.dba`                   | `player.rs` (mouse look, movement, jump) |
//! | `main.dba` (weapon states)    | `spell.rs` (chant + lightning strike)    |
//! | `npc.dba`                     | `npc.rs` (archer state machine)          |
//! | `arrow.dba`                   | `arrow.rs` (archer projectiles)          |
//! | `timer.dba`                   | Bevy's `Time`                            |
//! | `intersect object`            | `collision.rs` (ray vs level triangles)  |
//! | particles / `print` debugging | `particles.rs`, `hud.rs`                 |
//! | (new) touch / gamepad         | `touch.rs`, `player.rs`                  |
//! | demo build: hands + dagger    | `viewmodel.rs`                           |
//!
//! Positions from the original source are left-handed (DirectX); `db()`
//! converts them to Bevy's right-handed space by negating Z, matching the
//! model conversion in `tools/xconv`.

mod arrow;
mod collision;
mod hud;
mod npc;
mod particles;
mod player;
mod spell;
mod touch;
mod viewmodel;
mod world;

use bevy::prelude::*;
use bevy_embedded_assets::{EmbeddedAssetPlugin, PluginMode};

/// Android entry point. cargo-apk builds the cdylib and NativeActivity
/// calls into this via the #[bevy_main] generated android_main.
#[bevy_main]
fn main() {
    run_game();
}

/// Top-level flow. The original only ever ran its `play` state; the title
/// overlay exists so the browser has a click to capture the mouse with.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GameState {
    /// Level and collision mesh are streaming in.
    #[default]
    Loading,
    /// Title / pause overlay; click to (re)capture the mouse.
    Paused,
    Playing,
    /// Health reached zero; click to rise again.
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
            collision::CollisionPlugin,
            world::WorldPlugin,
            player::PlayerPlugin,
            spell::SpellPlugin,
            npc::NpcPlugin,
            arrow::ArrowPlugin,
            particles::ParticlesPlugin,
            hud::HudPlugin,
            touch::TouchPlugin,
            viewmodel::ViewModelPlugin,
        ))
        .run();
}
