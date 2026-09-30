//! Replacement for DarkBASIC's `intersect object` and `object collision`:
//! ray casts against a level's triangles (including invisible "caulk"
//! faces) and a few box tests. Both levels' triangle sets are built once
//! while loading; `LevelCollision` holds the active one.

use crate::GameState;
use crate::assets::GameAssets;
use bevy::gltf::{Gltf, GltfMesh};
use bevy::prelude::*;
use bevy::render::mesh::{Indices, VertexAttributeValues};
use std::sync::Arc;

pub struct CollisionPlugin;

impl Plugin for CollisionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, build_level_collision.run_if(in_state(GameState::Loading)));
    }
}

/// `gravity# = gravity# - 9.8 * avgCycleTime# * 100` (player.dba, npc.dba).
pub const GRAVITY: f32 = 980.0;

/// Something that walks on the level: the player (origin = eye) or an NPC
/// (origin = feet).
#[derive(Component, Clone)]
pub struct Body {
    pub vel_y: f32,
    pub grounded: bool,
    /// Height of the entity's origin above the floor when standing.
    pub stand_height: f32,
    /// Tallest ledge it can walk up.
    pub step: f32,
    /// Height of the top of the body above the floor.
    pub top: f32,
    pub radius: f32,
}

struct Triangle {
    a: Vec3,
    e1: Vec3,
    e2: Vec3,
}

/// Triangles of the active level (cheap to clone: shared).
#[derive(Resource, Clone)]
pub struct LevelCollision {
    triangles: Arc<Vec<Triangle>>,
}

/// Collision for both levels, built during loading.
#[derive(Resource)]
pub struct LevelLibrary {
    pub levels: [LevelCollision; 2],
}

impl LevelCollision {
    /// Distance along `dir` (normalized) to the nearest level surface within
    /// `max`, from either side of a triangle.
    pub fn ray(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<f32> {
        let mut best = max;
        let mut hit = false;
        for t in self.triangles.iter() {
            // Möller–Trumbore
            let p = dir.cross(t.e2);
            let det = t.e1.dot(p);
            if det.abs() < 1e-6 {
                continue;
            }
            let inv = 1.0 / det;
            let s = origin - t.a;
            let u = s.dot(p) * inv;
            if !(0.0..=1.0).contains(&u) {
                continue;
            }
            let q = s.cross(t.e1);
            let v = dir.dot(q) * inv;
            if v < 0.0 || u + v > 1.0 {
                continue;
            }
            let d = t.e2.dot(q) * inv;
            if d > 0.0 && d < best {
                best = d;
                hit = true;
            }
        }
        hit.then_some(best)
    }

    /// Height of the floor below `pos` (searching up to `max` down), if any.
    pub fn floor_below(&self, pos: Vec3, max: f32) -> Option<f32> {
        self.ray(pos, Vec3::NEG_Y, max).map(|d| pos.y - d)
    }

    /// Moves a body: horizontal motion blocked by walls (sliding along the
    /// free axis), step-up onto low ledges, gravity and ceilings. This merges
    /// the original's separate terrain-follow, gravity and wall-ray passes.
    pub fn move_body(&self, pos: &mut Vec3, body: &mut Body, horizontal: Vec3, dt: f32) {
        let floor = pos.y - body.stand_height;
        for axis in [Vec3::X, Vec3::Z] {
            let d = horizontal.dot(axis);
            if d == 0.0 {
                continue;
            }
            let dir = axis * d.signum();
            let blocked = [floor + body.step + 1.0, floor + body.top].iter().any(|&y| {
                self.ray(Vec3::new(pos.x, y, pos.z), dir, d.abs() + body.radius).is_some()
            });
            if !blocked {
                *pos += axis * d;
            }
        }

        let probe = Vec3::new(pos.x, pos.y - body.stand_height + body.step, pos.z);
        let ground = self.floor_below(probe, 50_000.0).map(|f| f + body.stand_height);
        if let Some(g) = ground {
            // Stick to the floor (including walking down steps) while not rising.
            let snap = if body.grounded { body.step } else { 0.5 };
            if body.vel_y <= 0.0 && pos.y - g <= snap {
                pos.y = g;
                body.vel_y = 0.0;
                body.grounded = true;
                return;
            }
        }
        body.grounded = false;
        body.vel_y -= GRAVITY * dt;
        if body.vel_y > 0.0 {
            let head = body.top - body.stand_height;
            if self.ray(*pos, Vec3::Y, head + body.vel_y * dt).is_some() {
                body.vel_y = 0.0;
            }
        }
        pos.y += body.vel_y * dt;
        if let Some(g) = ground
            && pos.y < g
        {
            pos.y = g;
            body.vel_y = 0.0;
            body.grounded = true;
        }
    }

    /// True when nothing in the level blocks the segment `from` -> `to`.
    pub fn line_of_sight(&self, from: Vec3, to: Vec3) -> bool {
        let delta = to - from;
        let len = delta.length();
        len < 1e-3 || self.ray(from, delta / len, len).is_none()
    }
}

/// Axis-aligned box as (min, max).
pub type Aabb3 = (Vec3, Vec3);

/// Box of `size` centred on `center`.
pub fn aabb(center: Vec3, size: Vec3) -> Aabb3 {
    (center - size / 2.0, center + size / 2.0)
}

pub fn aabb_overlap(a: Aabb3, b: Aabb3) -> bool {
    a.0.cmplt(b.1).all() && a.1.cmpgt(b.0).all()
}

/// Slab test: does the segment `from + dir * [0, len]` touch the box?
pub fn segment_hits_box(from: Vec3, dir: Vec3, len: f32, (min, max): Aabb3) -> bool {
    let (mut t0, mut t1) = (0.0f32, len);
    for i in 0..3 {
        if dir[i].abs() < 1e-6 {
            if from[i] < min[i] || from[i] > max[i] {
                return false;
            }
            continue;
        }
        let (a, b) = ((min[i] - from[i]) / dir[i], (max[i] - from[i]) / dir[i]);
        t0 = t0.max(a.min(b));
        t1 = t1.min(a.max(b));
        if t0 > t1 {
            return false;
        }
    }
    true
}

/// Distance along the ray to the box, if it is hit (for NPC avoidance rays).
pub fn ray_box(from: Vec3, dir: Vec3, max: f32, (min, max_b): Aabb3) -> Option<f32> {
    let (mut t0, mut t1) = (0.0f32, max);
    for i in 0..3 {
        if dir[i].abs() < 1e-6 {
            if from[i] < min[i] || from[i] > max_b[i] {
                return None;
            }
            continue;
        }
        let (a, b) = ((min[i] - from[i]) / dir[i], (max_b[i] - from[i]) / dir[i]);
        t0 = t0.max(a.min(b));
        t1 = t1.min(a.max(b));
        if t0 > t1 {
            return None;
        }
    }
    Some(t0)
}

fn triangles(gltf: &Gltf, gltf_meshes: &Assets<GltfMesh>, meshes: &Assets<Mesh>) -> Option<Vec<Triangle>> {
    let mut triangles = Vec::new();
    for handle in &gltf.meshes {
        for primitive in &gltf_meshes.get(handle)?.primitives {
            let mesh = meshes.get(&primitive.mesh)?;
            let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
                continue;
            };
            let indices: Vec<usize> = match mesh.indices() {
                Some(Indices::U32(i)) => i.iter().map(|&i| i as usize).collect(),
                Some(Indices::U16(i)) => i.iter().map(|&i| i as usize).collect(),
                None => (0..positions.len()).collect(),
            };
            for tri in indices.as_chunks::<3>().0 {
                let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(positions[tri[k]]));
                triangles.push(Triangle { a, e1: b - a, e2: c - a });
            }
        }
    }
    Some(triangles)
}

fn build_level_collision(
    mut commands: Commands,
    assets: Option<Res<GameAssets>>,
    existing: Option<Res<LevelLibrary>>,
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    meshes: Res<Assets<Mesh>>,
) {
    let (Some(assets), None) = (assets, existing) else {
        return;
    };
    let mut built = Vec::new();
    for handle in [&assets.level1, &assets.level2] {
        let Some(tris) = gltfs.get(handle).and_then(|g| triangles(g, &gltf_meshes, &meshes)) else {
            return;
        };
        info!("level collision: {} triangles", tris.len());
        built.push(LevelCollision { triangles: Arc::new(tris) });
    }
    let [one, two]: [LevelCollision; 2] = built.try_into().ok().unwrap();
    commands.insert_resource(one.clone());
    commands.insert_resource(LevelLibrary { levels: [one, two] });
}
