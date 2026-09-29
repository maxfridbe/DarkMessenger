//! The three particle emitters from `Dark Messenger.dba`: two orange torch
//! flames by the entrance and a blue spray at the far end of the level.
//! Particles are additive camera-facing quads with a generated glow sprite.

use crate::db;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

pub struct ParticlesPlugin;

impl Plugin for ParticlesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup).add_systems(Update, (emit, simulate).chain());
    }
}

#[derive(Component)]
struct Emitter {
    per_second: f32,
    pending: f32,
    /// Base launch velocity and random spread added on top.
    velocity: Vec3,
    spread: f32,
    /// Downward acceleration (negative rises, like heat).
    gravity: f32,
    life: f32,
    size: f32,
    material: Handle<StandardMaterial>,
    seed: u32,
}

#[derive(Component)]
struct Particle {
    velocity: Vec3,
    age: f32,
    life: f32,
    size: f32,
    gravity: f32,
    floor: f32,
}

#[derive(Resource)]
struct Quad(Handle<Mesh>);

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let glow = images.add(glow_sprite(32));
    commands.insert_resource(Quad(meshes.add(Rectangle::new(1.0, 1.0))));
    let mut material = |r: u8, g: u8, b: u8| {
        materials.add(StandardMaterial {
            base_color: Color::srgb_u8(r, g, b),
            base_color_texture: Some(glow.clone()),
            alpha_mode: AlphaMode::Add,
            unlit: true,
            fog_enabled: false,
            ..default()
        })
    };
    let torch = material(235, 107, 59);
    let spray = material(0, 150, 255);

    // `make particles 6/7, 1, 10, 30`: torches.
    for (i, pos) in [db(-115.0, 200.0, -632.0), db(180.0, 200.0, -632.0)].into_iter().enumerate() {
        commands.spawn((
            Name::new("Torch particles"),
            Emitter {
                per_second: 45.0,
                pending: 0.0,
                velocity: Vec3::Y * 45.0,
                spread: 18.0,
                gravity: -30.0,
                life: 0.7,
                size: 16.0,
                material: torch.clone(),
                seed: 17 + i as u32 * 7919,
            },
            Transform::from_translation(pos),
        ));
    }
    // `make particles 8, 1, 100, 30` + `ghost particles on 8, 5`: blue spray.
    commands.spawn((
        Name::new("Spray particles"),
        Emitter {
            per_second: 70.0,
            pending: 0.0,
            velocity: Vec3::Y * 170.0,
            spread: 55.0,
            gravity: 260.0,
            life: 1.8,
            size: 12.0,
            material: spray,
            seed: 4242,
        },
        Transform::from_translation(db(2574.0, -184.0, -302.0)),
    ));
}

/// Soft round sprite standing in for the original's default particle image.
fn glow_sprite(size: u32) -> Image {
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    let c = (size as f32 - 1.0) / 2.0;
    for y in 0..size {
        for x in 0..size {
            let d = Vec2::new(x as f32 - c, y as f32 - c).length() / c;
            let a = (1.0 - d).clamp(0.0, 1.0).powf(1.8);
            let v = (a * 255.0) as u8;
            data.extend([v, v, v, v]);
        }
    }
    Image::new(
        Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// Tiny xorshift so emitters don't need a rand dependency.
fn next(seed: &mut u32) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    (*seed as f32 / u32::MAX as f32) * 2.0 - 1.0
}

fn emit(mut commands: Commands, time: Res<Time>, quad: Res<Quad>, mut emitters: Query<(&mut Emitter, &Transform)>) {
    for (mut e, transform) in &mut emitters {
        e.pending += e.per_second * time.delta_secs().min(0.1);
        while e.pending >= 1.0 {
            e.pending -= 1.0;
            let mut seed = e.seed;
            let jitter = Vec3::new(next(&mut seed), next(&mut seed) * 0.5, next(&mut seed)) * e.spread;
            let offset = Vec3::new(next(&mut seed), 0.0, next(&mut seed)) * e.size * 0.3;
            let life = e.life * (0.75 + 0.25 * next(&mut seed).abs());
            e.seed = seed;
            commands.spawn((
                Particle {
                    velocity: e.velocity + jitter,
                    age: 0.0,
                    life,
                    size: e.size,
                    gravity: e.gravity,
                    floor: transform.translation.y,
                },
                Mesh3d(quad.0.clone()),
                MeshMaterial3d(e.material.clone()),
                Transform::from_translation(transform.translation + offset),
            ));
        }
    }
}

fn simulate(
    mut commands: Commands,
    time: Res<Time>,
    camera: Query<&Transform, (With<Camera3d>, Without<Particle>)>,
    mut particles: Query<(Entity, &mut Particle, &mut Transform)>,
) {
    let Ok(camera) = camera.get_single() else {
        return;
    };
    let dt = time.delta_secs().min(0.1);
    for (entity, mut p, mut transform) in &mut particles {
        p.age += dt;
        if p.age >= p.life {
            commands.entity(entity).despawn();
            continue;
        }
        p.velocity.y -= p.gravity * dt;
        transform.translation += p.velocity * dt;
        // `set particle floor`: nothing falls below its emitter.
        transform.translation.y = transform.translation.y.max(p.floor);
        transform.rotation = camera.rotation;
        transform.scale = Vec3::splat(p.size * (1.0 - p.age / p.life).sqrt());
    }
}
