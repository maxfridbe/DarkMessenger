//! `set object frame obj, frame`: the original never played animations, it
//! set the frame number every loop and moved it along itself (e.g. the
//! knight's walk is frames 31–50, `frame + 150 * cycleTime * 30`). Models
//! keep that timeline (150 ticks per frame, 4800 ticks per second in the
//! converted glTF), and `FrameAnim` pins an AnimationPlayer to a tick.

use bevy::prelude::*;
use bevy::render::view::NoFrustumCulling;
use bevy::scene::SceneInstanceReady;

pub struct AnimPlugin;

impl Plugin for AnimPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, apply_frames.before(bevy::app::Animation));
    }
}

/// Converted animation timelines run at this many ticks per second.
pub const TICKS_PER_SECOND: f32 = 4800.0;
/// One DarkBASIC animation frame.
pub const FRAME: f32 = 150.0;

#[derive(Clone)]
pub struct AnimSource {
    pub graph: Handle<AnimationGraph>,
    pub node: AnimationNodeIndex,
}

/// Put on a `SceneRoot` entity (spawned with `.observe(anim::hook)`).
#[derive(Component)]
pub struct FrameAnim {
    /// Current position in DarkBASIC ticks (frame * 150).
    pub ticks: f32,
    source: AnimSource,
    player: Option<Entity>,
}

impl FrameAnim {
    pub fn new(source: &AnimSource, ticks: f32) -> Self {
        Self { ticks, source: source.clone(), player: None }
    }
}

/// Finds the scene's AnimationPlayer and holds it paused on `ticks`.
pub fn hook(
    trigger: Trigger<SceneInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    mut anims: Query<&mut FrameAnim>,
    mut players: Query<&mut AnimationPlayer>,
    meshes: Query<(), With<Mesh3d>>,
) {
    let Ok(mut anim) = anims.get_mut(trigger.entity()) else {
        return;
    };
    for entity in children.iter_descendants(trigger.entity()) {
        if let Ok(mut player) = players.get_mut(entity) {
            player.play(anim.source.node).pause();
            commands.entity(entity).insert(AnimationGraphHandle(anim.source.graph.clone()));
            anim.player = Some(entity);
        }
        // Skinned meshes are culled by their bind-pose bounds, which the
        // animated pose leaves behind; always draw them.
        if meshes.contains(entity) {
            commands.entity(entity).insert(NoFrustumCulling);
        }
    }
}

fn apply_frames(anims: Query<&FrameAnim>, mut players: Query<&mut AnimationPlayer>) {
    for anim in &anims {
        let Some(mut player) = anim.player.and_then(|e| players.get_mut(e).ok()) else {
            continue;
        };
        if let Some(active) = player.animation_mut(anim.source.node) {
            active.seek_to(anim.ticks.max(0.0) / TICKS_PER_SECOND);
        }
    }
}
