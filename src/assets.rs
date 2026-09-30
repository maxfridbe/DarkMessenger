//! Every asset the game uses, loaded once at startup (the original's
//! `load object` / `load image` / `load sound` block). Loading finishes when
//! both levels' collision meshes are built.

use crate::GameState;
use crate::anim::AnimSource;
use crate::collision::LevelLibrary;
use bevy::gltf::Gltf;
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;

pub struct AssetsPlugin;

impl Plugin for AssetsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, load)
            .add_systems(Update, finish_loading.run_if(in_state(GameState::Loading)));
    }
}

#[derive(Clone)]
pub struct Model {
    pub scene: Handle<Scene>,
    /// Present when the model has keyframes (driven by frame number).
    pub anim: Option<AnimSource>,
}

/// A sound effect and the volume it plays at. The 2003 effects were
/// mastered far louder than the music (the chant and yell near -10 LUFS
/// against the wind's -30), so each is trimmed to sit in the mix.
pub struct Sfx {
    pub handle: Handle<AudioSource>,
    pub volume: f32,
}

pub struct Sounds {
    pub wind: Handle<AudioSource>,
    /// The intro speech (normalised louder in convert_assets.sh).
    pub darkness: Handle<AudioSource>,
    pub lightning: Sfx,
    pub chant: Sfx,
    pub arrow: Sfx,
    pub knife: Sfx,
    pub throw: Sfx,
    pub scream: Sfx,
    pub bone: Sfx,
    pub grunt: Sfx,
    pub sword: Sfx,
    pub yell: Sfx,
}

#[derive(Resource)]
pub struct GameAssets {
    pub level1: Handle<Gltf>,
    pub level2: Handle<Gltf>,
    pub level1_scene: Handle<Scene>,
    pub level2_scene: Handle<Scene>,
    pub knight: Model,
    pub archer: Model,
    pub arrow: Model,
    pub hands: Model,
    pub dagger: Model,
    pub spike: Model,
    pub book: Model,
    pub darius: Model,
    pub sky: Handle<Image>,
    pub lightning: Vec<Handle<Image>>,
    pub fire: Vec<Handle<Image>>,
    pub water: Vec<Handle<Image>>,
    pub vortex: Vec<Handle<Image>>,
    pub graffiti: Handle<Image>,
    pub ui_menu: Handle<Image>,
    pub ui_loading: Handle<Image>,
    pub ui_crosshair: Handle<Image>,
    pub ui_health_fill: Handle<Image>,
    pub ui_health_frame: Handle<Image>,
    pub ui_mana_fill: Handle<Image>,
    pub ui_mana_frame: Handle<Image>,
    pub sounds: Sounds,
}

impl GameAssets {
    pub fn lightmap(&self, asset_server: &AssetServer, name: &str) -> Handle<Image> {
        asset_server.load(format!("textures/lightmaps/{name}.png"))
    }
}

fn load(mut commands: Commands, asset_server: Res<AssetServer>, mut graphs: ResMut<Assets<AnimationGraph>>) {
    let mut model = |path: &str, animated: bool| Model {
        scene: asset_server.load(GltfAssetLabel::Scene(0).from_asset(path.to_string())),
        anim: animated.then(|| {
            let clip = asset_server.load(GltfAssetLabel::Animation(0).from_asset(path.to_string()));
            let (graph, node) = AnimationGraph::from_clip(clip);
            AnimSource { graph: graphs.add(graph), node }
        }),
    };
    let knight = model("models/knight.glb", true);
    let archer = model("models/archer.glb", true);
    let arrow = model("models/arrow.glb", false);
    let hands = model("models/hands.glb", true);
    let dagger = model("models/dagger.glb", false);
    let spike = model("models/spike.glb", true);
    let book = model("models/book.glb", true);
    let darius = model("models/darius.glb", false);

    let repeat = |s: &mut ImageLoaderSettings| {
        s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            ..ImageSamplerDescriptor::linear()
        });
    };
    let frames = |dir: &str, range: std::ops::RangeInclusive<u32>| -> Vec<Handle<Image>> {
        range.map(|i| asset_server.load(format!("textures/{dir}/{i:02}.png"))).collect()
    };
    let sound = |name: &str| asset_server.load(format!("sounds/{name}"));
    let sfx = |name: &str, volume: f32| Sfx { handle: asset_server.load(format!("sounds/{name}")), volume };

    commands.insert_resource(GameAssets {
        level1: asset_server.load("models/level1.glb"),
        level2: asset_server.load("models/level2.glb"),
        level1_scene: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/level1.glb")),
        level2_scene: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/level2.glb")),
        knight,
        archer,
        arrow,
        hands,
        dagger,
        spike,
        book,
        darius,
        sky: asset_server.load("textures/sky.jpg"),
        lightning: (1..=3)
            .map(|i| asset_server.load_with_settings(format!("textures/lightning{i}.png"), repeat))
            .collect(),
        fire: frames("fire", 1..=50),
        water: frames("water", 1..=50),
        vortex: frames("vortex", 0..=20),
        graffiti: asset_server.load("textures/graffiti.png"),
        ui_menu: asset_server.load("ui/menu.png"),
        ui_loading: asset_server.load("ui/loading.jpg"),
        ui_crosshair: asset_server.load("ui/crosshair.png"),
        ui_health_fill: asset_server.load("ui/health_fill.png"),
        ui_health_frame: asset_server.load("ui/health_frame.png"),
        ui_mana_fill: asset_server.load("ui/mana_fill.png"),
        ui_mana_frame: asset_server.load("ui/mana_frame.png"),
        sounds: Sounds {
            wind: sound("wind.ogg"),
            darkness: sound("darkness.ogg"),
            // Volumes from measured loudness (LUFS in brackets), aiming for
            // roughly -24 LUFS so they sit just above the wind.
            lightning: sfx("lightning.wav", 0.35), // -13
            chant: sfx("chant.wav", 0.3),          // -10
            arrow: sfx("arrow.wav", 0.8),          // -24
            knife: sfx("knife.wav", 0.3),          // -14
            throw: sfx("throw.wav", 0.35),         // short, peaks at 0 dBFS
            scream: sfx("scream.wav", 0.3),        // -12
            bone: sfx("bone.wav", 0.7),            // -25
            grunt: sfx("grunt.wav", 0.35),         // short, peaks at 0 dBFS
            sword: sfx("sword.wav", 0.45),         // -18
            yell: sfx("yell.wav", 0.25),           // -9
        },
    });
}

fn finish_loading(library: Option<Res<LevelLibrary>>, mut next: ResMut<NextState<GameState>>) {
    if library.is_some() {
        next.set(GameState::Intro);
    }
}

/// Plays a one-shot sound effect (`play sound n`).
pub fn play(commands: &mut Commands, sound: &Sfx) {
    commands.spawn((
        AudioPlayer::new(sound.handle.clone()),
        PlaybackSettings::DESPAWN.with_volume(bevy::audio::Volume::new(sound.volume)),
    ));
}
