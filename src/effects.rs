//! Level props from `Dark Messenger.dba` / `main.dba`:
//! - "flamers": four fire braziers and the fountain, animated textures
//!   (fire01–50, water 01–50) on boxes that turn to face the player;
//! - the book in the library: E opens it (frames 1–24) and, once open, the
//!   vortex appears; walking into the vortex loads level two;
//! - level two's "Game Over" graffiti.

use crate::GameState;
use crate::anim::{self, FRAME, FrameAnim};
use crate::assets::GameAssets;
use crate::collision::{aabb, aabb_overlap};
use crate::player::{Player, PlayerInput, PlayerSet};
use crate::world::{LevelEntity, LevelId, LoadLevel};
use crate::db;
use bevy::prelude::*;

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (cycle_frames, face_camera))
            .add_systems(Update, (use_book, animate_book, enter_vortex).chain().after(PlayerSet).run_if(in_state(GameState::Playing)));
    }
}

/// Cycles a mesh through a list of materials (`texture object n, fireTex`).
#[derive(Component)]
struct FrameCycler {
    frames: Vec<Handle<StandardMaterial>>,
    period: f32,
    time: f32,
    index: usize,
}

/// Turns about Y to face the camera (`point object n, player x, y, player z`).
#[derive(Component)]
struct FaceCamera;

#[derive(Component)]
pub struct Book {
    state: BookState,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BookState {
    Closed,
    Opening,
    Open,
    Closing,
}

#[derive(Component)]
struct Vortex {
    open: bool,
}

/// Fire texture period: `if fireTime# > .03`.
const FIRE_PERIOD: f32 = 0.03;
/// The vortex advanced a frame every loop; ~30 fps on 2003 hardware.
const VORTEX_PERIOD: f32 = 1.0 / 30.0;
/// `bookFrame + 150 * interval# * 15`
const BOOK_SPEED: f32 = FRAME * 15.0;
const BOOK_LAST_FRAME: f32 = 24.0 * FRAME;
/// Where the player must stand to use the book (original coordinates:
/// x 2110..2200, y 0..200, z -1207..-1040).
pub fn at_book(pos: Vec3) -> bool {
    (2110.0..2200.0).contains(&pos.x) && (0.0..200.0).contains(&pos.y) && (1040.0..1207.0).contains(&pos.z)
}

fn additive(images: &[Handle<Image>], materials: &mut Assets<StandardMaterial>) -> Vec<Handle<StandardMaterial>> {
    images
        .iter()
        .map(|image| {
            materials.add(StandardMaterial {
                base_color_texture: Some(image.clone()),
                alpha_mode: AlphaMode::Add,
                unlit: true,
                cull_mode: None,
                fog_enabled: false,
                ..default()
            })
        })
        .collect()
}

pub fn spawn_level_one(
    commands: &mut Commands,
    assets: &GameAssets,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let fire = additive(&assets.fire, materials);
    let water = additive(&assets.water, materials);
    let flame = meshes.add(Rectangle::new(20.0, 70.0));
    for pos in [
        db(-115.0, 225.0, -622.0),
        db(180.0, 225.0, -622.0),
        db(1100.0, 466.0, 1135.0),
        db(1114.0, 837.0, 1135.0),
    ] {
        commands.spawn((
            LevelEntity,
            Name::new("Fire"),
            FaceCamera,
            FrameCycler { frames: fire.clone(), period: FIRE_PERIOD, time: 0.0, index: 0 },
            Mesh3d(flame.clone()),
            MeshMaterial3d(fire[0].clone()),
            Transform::from_translation(pos),
        ));
    }
    // Object 1104: the flamer box scaled 400/600/400 with the water frames.
    commands.spawn((
        LevelEntity,
        Name::new("Fountain"),
        FaceCamera,
        FrameCycler { frames: water.clone(), period: FIRE_PERIOD, time: 0.0, index: 0 },
        Mesh3d(meshes.add(Rectangle::new(80.0, 420.0))),
        MeshMaterial3d(water[0].clone()),
        Transform::from_translation(db(2574.0, -10.0, -302.0)),
    ));

    // `load object "model/book/book.x", 2002 : position ... : turn object left 90`
    if let Some(source) = &assets.book.anim {
        commands
            .spawn((
                LevelEntity,
                Name::new("Book"),
                Book { state: BookState::Closed },
                SceneRoot(assets.book.scene.clone()),
                FrameAnim::new(source, FRAME),
                Transform::from_translation(db(2102.8, 86.65, -1121.2))
                    .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
            ))
            .observe(anim::hook);
    }

    // `make object box 1111, 128, 128, 128 : position object 1111, -837, 704, -360`
    let vortex = additive(&assets.vortex, materials);
    commands.spawn((
        LevelEntity,
        Name::new("Vortex"),
        Vortex { open: false },
        FaceCamera,
        FrameCycler { frames: vortex.clone(), period: VORTEX_PERIOD, time: 0.0, index: 0 },
        Mesh3d(meshes.add(Rectangle::new(128.0, 128.0))),
        MeshMaterial3d(vortex[0].clone()),
        Transform::from_translation(db(-837.0, 704.0, -360.0)),
        Visibility::Hidden,
    ));
}

pub fn spawn_level_two(
    commands: &mut Commands,
    assets: &GameAssets,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    // `make object box 512, 512, 192, 10 : position object 512, 0, 0, -400`
    commands.spawn((
        LevelEntity,
        Name::new("Graffiti"),
        Mesh3d(meshes.add(Rectangle::new(512.0, 192.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: Some(assets.graffiti.clone()),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            ..default()
        })),
        Transform::from_translation(db(0.0, 0.0, -400.0)),
    ));
}

fn cycle_frames(time: Res<Time>, mut cyclers: Query<(&mut FrameCycler, &mut MeshMaterial3d<StandardMaterial>)>) {
    for (mut c, mut material) in &mut cyclers {
        c.time += time.delta_secs();
        if c.time < c.period {
            continue;
        }
        c.time %= c.period;
        c.index = (c.index + 1) % c.frames.len();
        material.0 = c.frames[c.index].clone();
    }
}

fn face_camera(camera: Query<&Transform, (With<Camera3d>, Without<FaceCamera>)>, mut faces: Query<&mut Transform, With<FaceCamera>>) {
    let Ok(camera) = camera.get_single() else {
        return;
    };
    for mut t in &mut faces {
        let d = camera.translation - t.translation;
        t.rotation = Quat::from_rotation_y(d.x.atan2(d.z));
    }
}

/// `keystate(18)` (E) near the book opens it, or closes it again.
fn use_book(
    input: Res<PlayerInput>,
    player: Query<&Transform, With<Player>>,
    mut book: Query<(&mut Book, &mut FrameAnim)>,
) {
    let (Ok(player), Ok((mut book, mut anim))) = (player.get_single(), book.get_single_mut()) else {
        return;
    };
    let p = player.translation;
    if !input.use_book || !at_book(Vec3::new(p.x, p.y, p.z)) {
        return;
    }
    match book.state {
        BookState::Closed => {
            anim.ticks = FRAME;
            book.state = BookState::Opening;
        }
        BookState::Open => {
            anim.ticks = BOOK_LAST_FRAME;
            book.state = BookState::Closing;
        }
        _ => {}
    }
}

fn animate_book(
    time: Res<Time>,
    mut book: Query<(&mut Book, &mut FrameAnim)>,
    mut vortex: Query<(&mut Vortex, &mut Visibility)>,
) {
    let (Ok((mut book, mut anim)), Ok((mut vortex, mut visibility))) = (book.get_single_mut(), vortex.get_single_mut())
    else {
        return;
    };
    let dt = time.delta_secs();
    match book.state {
        BookState::Opening => {
            anim.ticks += BOOK_SPEED * dt;
            if anim.ticks >= BOOK_LAST_FRAME {
                anim.ticks = BOOK_LAST_FRAME;
                book.state = BookState::Open;
                vortex.open = true;
                *visibility = Visibility::Inherited;
            }
        }
        BookState::Closing => {
            anim.ticks -= BOOK_SPEED * dt;
            if anim.ticks <= 0.0 {
                anim.ticks = FRAME;
                book.state = BookState::Closed;
                vortex.open = false;
                *visibility = Visibility::Hidden;
            }
        }
        _ => {}
    }
}

/// `if object collision(vortex, 10002) = 1 and vortexOpen = 1 : goto Load2`
fn enter_vortex(
    player: Query<&Transform, With<Player>>,
    vortex: Query<(&Vortex, &Transform)>,
    mut load: EventWriter<LoadLevel>,
) {
    let (Ok(player), Ok((vortex, t))) = (player.get_single(), vortex.get_single()) else {
        return;
    };
    let player_box = aabb(player.translation, Vec3::new(50.0, 100.0, 50.0));
    if vortex.open && aabb_overlap(player_box, aabb(t.translation, Vec3::splat(128.0))) {
        load.send(LoadLevel(LevelId::Two));
    }
}
