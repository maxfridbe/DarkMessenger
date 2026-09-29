# Dark Messenger 2026

A Bevy (Rust) port of **Dark Messenger**, a first-person DarkBASIC Pro prototype from 2003: you are a mage in a lightmapped castle, archers guard the far hall, and your only weapon is a lightning bolt you have to chant for. It is built on [GameBase](../GameBase), so it ships to **Linux, Windows, macOS, Android and the browser** from one crate.

## Playing

```bash
./run_linux.sh                 # native
./build_web.sh && python3 -m http.server -d target/web_dist 8080   # browser
```

| Input | Action |
|-------|--------|
| WASD / arrows | move |
| Mouse | look (click the window to capture the mouse, Esc to release) |
| Space | jump |
| Hold left click | chant: the world darkens for 2.75 s, then lightning strikes the spot under the crosshair |
| 1 / 2 / Tab (Ctrl = power, as in the original) | spread bolt (0.75 s, 1.25 s recharge) / power bolt (1.25 s, 3 s recharge) |
| F3 | debug readout (the original `print`ed this every frame) |
| Gamepad (desktop / web) | left stick move, right stick look, A jump, RT chant, X / Y swap bolt, Start pause |
| Touch | on-screen stick (left) to walk, drag elsewhere to look, hold CAST, JUMP, BOLT (swap), II (pause) |

The touch controls (`src/touch.rs`) show by default on Android and whenever the screen is touched, and hide when a keyboard key is pressed or a gamepad is connected. The launcher / macOS icon is drawn by `tools/make_icon.py`.

Archers wake up when you come within 700 units, walk toward you while they can see you, and fire arrows inside 500. A bolt landing within 100 units throws an archer into the air and plays its death animation.

## Project layout

```
src/lib.rs          app setup, GameState (Loading / Paused / Playing / Dead), db() coordinate helper
src/world.rs        level + lightmaps, sky sphere, purple sun, Darius statue, wind loop, light level
src/collision.rs    ray casts against the level triangles (DarkBASIC's `intersect object`), walking bodies
src/player.rs       first-person camera, movement, jump, gravity, cursor capture, touch input
src/spell.rs        chant / strike state machine, bolt visuals, aim marker
src/npc.rs          archer state machine and death animation
src/arrow.rs        archer arrows, damage to the player
src/particles.rs    torch and spray emitters
src/hud.rs          crosshair, health / spell bars, overlays, debug text
tools/xconv/        converter: DirectX text .x -> .glb (standalone crate)
tools/convert_assets.sh   regenerates assets/ from the original project
assets/             converted, embedded game assets (committed)
```

The original sources live next to this folder (`../Dark MessengerFrozen/Dark Messenger/*.dba`, the later of the two snapshots). Each `.dba` file maps to one module; the header of `src/lib.rs` has the table.

## Asset pipeline

Bevy cannot read DirectX `.x` models, so `tools/xconv` converts them to binary glTF:

- frames become nodes, meshes are split per material, n-gons are fan-triangulated;
- left-handed DirectX space is mirrored to Bevy's right-handed space (Z negated, winding reversed); the game uses `db(x, y, z)` for positions taken from the original source;
- skin weights and keyframe animations (rotation / scale / position / matrix keys) become glTF skins and animations; skinned meshes are placed outside their skeleton because Bevy 0.15's loader hangs otherwise;
- the level (`one.x`) is merged with its lightmap export (`one_lm.x`): lightmap UVs become `TEXCOORD_1` and each material is named `material@lightmap`, which `world.rs` turns into a Bevy `Lightmap`;
- `--merge` collapses the level's 150 meshes into one (39 draw calls instead of 1041).

`tools/convert_assets.sh` runs xconv on every model, converts the lightmaps and lightning frames from BMP to PNG, the 5.6 MB wind track to Ogg Vorbis and the sound effects to 16-bit WAV (needs `ffmpeg`). Only rerun it when the conversion changes; its output is already in `assets/`.

A few textures referenced by `archer1.x` were never shipped with the original project; those parts render with their material colour.

The first-person hands and floating dagger (`model\hand2\hand2.x`, `model\dagger2\dagger2.x`) come from the later **Dark Messenger Playable Demo** build (Dec 2003), which has no source but whose `Dark Messenger.pck` names every file it loads. `convert_assets.sh` takes that folder as its optional second argument (default `~/Downloads/Dark Messenger Playable Demo`). `src/viewmodel.rs` rides them on the camera: the hands play their gesture while you chant, and the dagger hovers point-up above the palms, spinning faster as the chant builds. The placement is a reconstruction; the demo's code isn't available.

## Differences from the 2003 build

The port keeps the original numbers (speeds, distances, timings, sizes, positions) but fixes or finishes a few things:

- **Gravity and timers are time-based.** The original subtracted `9.8` from the vertical speed once per frame; the port uses 588 units/s² (the same at 60 fps).
- **Archers wake below 500 units too.** The original only woke them between 500 and 700 units.
- **Arrows hurt.** `thePlayer.health` and the `death` game state existed but were unused; arrows now deal 10 damage, stop at walls, and dying shows a "click to rise again" screen.
- **The bolt lands on the floor under the crosshair.** The original cast a ray down from y = 10000, which only worked because its ray test ignored back faces; the port's ray test is double-sided, so it would have hit the level's sealed ceiling.
- **The crosshair ray is fixed.** `input.dba` built its end point from `pos.x` for all three axes.
- **Walls slide.** Movement blocked by a wall slides along it instead of stopping dead.
- A title/pause overlay was added (browsers need a click before capturing the mouse), plus HUD bars and a win message.

---

## How the multi-platform methodology works

One Rust crate produces every platform:

```
src/lib.rs   -> ALL game code lives here (run_game()).
                crate-type = ["staticlib", "cdylib", "rlib"]
                #[bevy_main] fn main() is the Android entry point.
src/main.rs  -> tiny desktop wrapper: fn main() { dark_messenger::run_game() }
```

| Platform | Mechanism | Entry artifact |
|----------|-----------|----------------|
| Linux | native `cargo run` (x11 feature) | `target/release/dark_messenger` |
| Windows | cross-compile from Linux with MinGW (`x86_64-pc-windows-gnu`) | `target/windows_dist/` (exe + assets, zip and ship) |
| macOS (arm64) | cross-compile from Linux in a podman container: osxcross + Apple SDK → `.app` bundle → `.dmg` | `target/macos_dist/` (`<Game>.app` + `<game>-macos-arm64-v<version>.dmg`) |
| Android A (**primary**) | `cargo apk` builds the **cdylib** into an APK per ABI; NativeActivity, no Java code at all | `target/{debug,release}/apk/*.apk` |
| Android B (alternative) | `cargo ndk` drops the cdylib into `app/src/main/jniLibs/`, then Gradle wraps it with a Java `GameActivity` into one universal APK | `app/build/outputs/apk/debug/app-debug.apk` |
| Browser (WASM) | compile the bin to `wasm32-unknown-unknown` (webgl2 feature), `wasm-bindgen` generates the JS glue, `web/index.html` hosts the canvas | `target/web_dist/` (static site — serve anywhere) |

Key load-bearing details (easy to lose, hard to rediscover):

- **`bevy_embedded_assets`** embeds `assets/` into the binary. On Android there is no loose filesystem for Bevy's `AssetServer`, so this is what makes assets work in the APK. It must be added **before** `DefaultPlugins`.
- **`cpal` with `oboe-shared-stdcxx`** makes audio work on Android; it links against `libc++_shared.so`, which the Gradle path copies out of the NDK explicitly (Step 2 of `buildanddeploy.sh`).
- Bevy is built with `default-features = false`; the `android-native-activity` feature is what Path A needs, `x11` is what native Linux needs. If you use **Path B (Gradle/GameActivity)**, switch the bevy feature `android-native-activity` → `android-game-activity` so it matches the Java `GameActivity` wrapper (Path A was the proven/primary path in the source project).
- Two APKs are built on purpose in Path A: **x86_64** for the desktop emulator, **arm64-v8a** for real phones. Path B builds one fat APK containing both.
- Emulator GPU emulation matters for wgpu/Vulkan: `swangle_indirect` (run_emulator.sh) is the most stable; `start_new_emulator.sh` is the SwiftShader/CPU fallback for hosts whose GPU driver crashes the emulator.
- **macOS is built in a container, not on a Mac.** `build_macos.sh` → `Containerfile.macos` → osxcross + Apple's SDK. Two details are load-bearing: Apple Silicon **refuses to exec an arm64 binary with no code signature at all**, so the build ad-hoc signs the bundle with `rcodesign` (this is not the same as Developer ID signing); and `bindgen` (pulled in by `coreaudio-sys` → `cpal` → `bevy_audio`) ignores `CC_*` and needs the sysroot passed via `BINDGEN_EXTRA_CLANG_ARGS_*`, which the image bakes in. See [macOS notes](#macos-apple-silicon-notes).
- `game.env` centralizes the game identity + Android SDK paths; every script sources it.
- Browser build details: `bevy_embedded_assets` means the `.wasm` is fully self-contained (no asset fetch issues on static hosts); `wasm-bindgen-cli` must exactly match the `wasm-bindgen` crate version in `Cargo.lock` (`build_web.sh` auto-installs the right one); `getrandom` 0.3 (pulled via ahash/bevy) needs the `wasm_js` feature **and** `RUSTFLAGS=--cfg getrandom_backend="wasm_js"` — both are wired in already; the window is bound to the `#game-canvas` element in `web/index.html`.
- Browser build size: the web build uses the dedicated `[profile.wasm-release]` (opt-level `z`, fat LTO — native platforms keep the fast default release profile) and then `wasm-opt -Oz` from binaryen if installed. Expect roughly half the size of a plain release wasm.

## macOS (Apple Silicon) notes

`./build_macos.sh` builds `target/macos_dist/<Game>.app` and a matching `.dmg` without a Mac anywhere in the loop. Everything happens inside the image defined by `Containerfile.macos`:

| Piece | Source | Why |
|-------|--------|-----|
| [osxcross](https://github.com/tpoechtrager/osxcross) | pinned commit, built from source | clang/cctools/ld64 wrappers that target Apple's SDK |
| macOS SDK 14.5 | [`joseluisq/macosx-sdks`](https://github.com/joseluisq/macosx-sdks/releases) (public mirror), sha256-pinned | headers + framework stubs for AppKit/Metal/CoreAudio |
| [`rcodesign`](https://github.com/indygreg/apple-platform-rs) | prebuilt release, sha256-verified | ad-hoc code signing from Linux |
| `xorriso` | Ubuntu package | writes the `.dmg` disk image |

The first run compiles osxcross and is slow (**~15–20 minutes, several GB of image**). After that the image is cached and a build is just `cargo build` + packaging. Crates are cached in a podman volume (`gamebase-macos-cargo-registry`) across runs.

In CI the `macos` job runs the exact same image. Building it on every push would be wasteful, so the job pushes it to this repo's container registry (`ghcr.io/<owner>/<repo>/macos-builder:<sdk-version>`, private by default) and pulls it on later runs. Measured: the first run took **18m30s** (12m building and pushing the image, 6m for the cargo build); the next run, pulling the cached image, took **6m49s** — the pull step itself dropped to 41s. A fork whose token cannot push to GHCR just rebuilds the image each time and logs a warning; the build still succeeds.

Things worth knowing before you ship the `.dmg`:

- **Ad-hoc signing is mandatory, not cosmetic.** arm64 macOS will not execute a binary with no signature at all, so `build_macos_bundle.sh` always runs `rcodesign sign`. That is *not* Developer ID signing.
- **Gatekeeper will still block it**, because the app is neither Developer-ID-signed nor notarized — those steps need a paid Apple Developer account and Apple's notary service. Your users open it once via **right-click → Open**, or you run `xattr -dr com.apple.quarantine "/Applications/Dark Messenger.app"`. To do it properly, sign and notarize with your own certificate (`rcodesign` can do both from Linux).
- **The `.dmg` is an ISO9660+Rock Ridge image**, which macOS mounts as a read-only disk with the usual drag-to-`/Applications` layout. A compressed UDBZ/HFS+ image needs Apple's `hdiutil` (or a from-source `libdmg-hfsplus`); if you want one, run `hdiutil convert in.dmg -format UDBZ -o out.dmg` on any Mac.
- **Intel Macs are not covered.** The same image can build `x86_64-apple-darwin` — swap the target in `build_macos_bundle.sh` and `lipo` the two binaries into a universal one — but the default output is arm64-only.
- **Licensing:** the macOS SDK is Apple's property and its licence only permits use on Apple-branded hardware. The mirror is publicly reachable; that is not the same as a licence. Decide for yourself whether this route fits your situation, and don't redistribute the built image.
- **App icon:** `png2icns` builds `AppIcon.icns` from `assets/macos-icon.png` if you add one (1024x1024 works best). Without it the build falls back to the 48x48 Android launcher icon, or ships with no icon at all.

## Scripts

| Script | What it does |
|--------|--------------|
| `setup_env.sh --check` | Verify every requirement of every build script (no sudo, no installs) — prints `[ OK ]`/`[MISS]` per item |
| `setup_env.sh` | Check, then install only what's missing: system packages (apt or dnf detected automatically), Rust + cross targets, MinGW-w64, podman, cargo-apk/cargo-ndk, Android SDK/NDK 26, debug keystore |
| `run_linux.sh [debug]` | Build + run natively on Linux |
| `build_windows.sh` | Cross-compile Windows release, package exe + assets into `target/windows_dist/` |
| `build_macos.sh` | Cross-compile the macOS arm64 `.app` + `.dmg` in a podman container into `target/macos_dist/` (`--rebuild` to refresh the toolchain image, `--shell` to poke around inside it) |
| `build_web.sh` | Build the browser version into `target/web_dist/` (compile to wasm, run wasm-bindgen, add `web/index.html`) |
| `build_cargo_apk.sh` / `_debug.sh` | Path A: build emulator (x86_64) + phone (ARM64) APKs |
| `deploy_cargo_apk.sh` | Install Path A APK to running emulator, launch, follow logcat |
| `deploy_phone.sh` | Install Path A ARM64 APK to USB phone, launch, follow logcat |
| `run_emulator.sh` | Start a clean emulator (Swangle GPU — most stable for Bevy) |
| `start_new_emulator.sh` | Fallback emulator (SwiftShader CPU rendering) |
| `buildanddeploy.sh` | Path B: cargo-ndk → jniLibs → Gradle universal APK → install + launch + logs |
| `deploy_apk_to_emulator.sh` | Re-install Path B APK without rebuilding |
| `read_logs.sh` / `read_logs_phone.sh` | Dump last 50 relevant log lines (emulator / phone) |
| `kill_phone_app.sh` | Force-stop the game on the phone |
| `increment_version.sh [major\|minor\|patch]` | Bump `version.txt` (default: patch) and sync the version into `Cargo.toml` and `app/build.gradle` |

## Versioning + CI releases

`version.txt` at the repo root is the single source of truth for the game version.

1. `./increment_version.sh` (or `minor` / `major`) bumps it and syncs `Cargo.toml` + Gradle `versionName`/`versionCode`.
2. Commit and push to `main`.
3. `.github/workflows/release.yml` builds every platform on GitHub Actions and publishes a **GitHub Release tagged `v<version>`** with:
   - `<game>-linux-x86_64-v<version>.tar.gz` (binary + assets)
   - `<game>-windows-x86_64-v<version>.zip` (exe + assets, MinGW cross-compiled)
   - `<game>-macos-arm64-v<version>.dmg` (Apple Silicon `.app` in a disk image)
   - `<game>-android-arm64-v<version>.apk` (phones) and `<game>-android-x86_64-v<version>.apk` (emulator)
   - `<game>-web-v<version>.zip` (static site) — the same build is also deployed to **GitHub Pages** as the live demo (first run: if the `pages` job fails, enable Pages once under repo Settings → Pages → Source: GitHub Actions)

Pushing again without bumping the version updates the existing release for that tag rather than creating a new one. CI signs APKs with a freshly generated debug keystore — replace that step with a real keystore (repo secret) before shipping to a store.

## Quick start

```bash
./setup_env.sh --check  # see what your machine is missing
./setup_env.sh          # once per machine; installs only the missing pieces (apt or dnf)
./run_linux.sh          # play on Linux
./build_windows.sh      # produce target/windows_dist/ for Windows
./build_macos.sh        # produce target/macos_dist/ for Apple Silicon (podman; slow first run)
./build_web.sh          # produce target/web_dist/ for the browser
python3 -m http.server -d target/web_dist 8080   # ...then play at localhost:8080
./run_emulator.sh       # boot the Android emulator...
./build_cargo_apk_debug.sh && ./deploy_cargo_apk.sh   # ...and play in it
./build_cargo_apk.sh && ./deploy_phone.sh             # play on a USB phone
```

---

## Making it yours (renaming checklist)

This copy is already renamed to `dark_messenger` / "Dark Messenger" (package `com.darkmessenger.game`). GameBase's renaming checklist is kept below for reference; the one-shot commands assume the original `game_base` names.

**1. `Cargo.toml`** — the source of truth:

```toml
[package]  name = "my_game"            # crate name
[lib]      name = "my_game"            # native library name -> libmy_game.so
[[bin]]    name = "my_game"            # exe name -> my_game / my_game.exe

[package.metadata.android]
package = "com.yourstudio.mygame"      # Android application id (Path A)
label = "My Game"                      # app name shown under the icon
```

Also update `src/main.rs` (`my_game::run_game();`) and the window title in `src/lib.rs`.

> The `[package.metadata.android.signing.release]` block points at a **debug keystore** — fine for sideloading; generate a real keystore (`keytool -genkey ...`) before any store release.

**2. `game.env`** — the scripts read the same names from here:

```bash
GAME_NAME="my_game"                    # = Cargo.toml lib/bin name
GAME_LABEL="My Game"                   # = [package.metadata.android] label; macOS .app / .dmg name
MACOS_BUNDLE_ID="com.yourstudio.mygame"   # macOS bundle identifier
ANDROID_PACKAGE="com.yourstudio.mygame"   # = [package.metadata.android] package
GRADLE_PACKAGE="org.yourstudio.my_game"   # = app/build.gradle applicationId
```

**3. App icon** — replace `assets/android-res/mipmap-mdpi/ic_launcher.png` (used by both Android paths). Optionally add `assets/macos-icon.png` at 1024x1024 for the macOS `.app` icon.

**3b. Demo link** — the browser demo deploys to `https://<your-user>.github.io/<your-repo>/`; update the link at the top of this README. (`web/index.html` needs no changes — `build_web.sh` fills in the game name.)

**4. Only if you use Path B (Gradle):**

- `app/build.gradle` — `namespace` and `applicationId`
- `app/src/main/AndroidManifest.xml` — `android:label` (app name) and the `android.app.lib_name` meta-data value (= `GAME_NAME`)
- `app/src/main/java/.../MainActivity.java` — the `package` line and `System.loadLibrary("my_game")`; move the file to a directory matching the new package (`app/src/main/java/com/yourstudio/mygame/`)
- `settings.gradle` — `rootProject.name`
- `Cargo.toml` — swap bevy feature `android-native-activity` → `android-game-activity`

**One-shot rename** (Path A pieces, from the repo root — review the diff after):

```bash
NEW=my_game; NEWPKG=com.yourstudio.mygame; NEWLABEL="My Game"
sed -i "s/game_base/$NEW/g" Cargo.toml src/main.rs game.env
sed -i "s/com\.gamebase\.game/$NEWPKG/g" Cargo.toml game.env
sed -i "s/Game Base/$NEWLABEL/g" Cargo.toml src/lib.rs game.env
```

**5. Grow the game** — game code lives in `src/`, one plugin per module. The pattern that scales (used by the parent project): split features into modules, each exposing a Bevy `Plugin`, and `add_plugins(...)` them in `run_game()`. Put new assets in `assets/` — they are embedded automatically on every platform.

## Version pins that matter

| Thing | Version | Why pinned |
|-------|---------|-----------|
| bevy | 0.15.x | input/render APIs used here |
| bevy_embedded_assets | 0.12 | matches bevy 0.15 |
| Android NDK | 26.1.10909125 | referenced by every script + setup |
| AGP / Gradle | 8.4.0 / 8.6 | Path B only |
| games-activity | 4.4.0 | must stay compatible with bevy's `android-activity` crate |
| macOS SDK | 14.5 | pinned by version **and** sha256 in `Containerfile.macos`; bump both together |
| osxcross | pinned commit | its build script and wrapper naming change over time |
| rcodesign (apple-codesign) | 0.29.0 | prebuilt binary, sha256-verified |
| wasm-bindgen-cli | = `wasm-bindgen` in Cargo.lock | hard requirement; `build_web.sh`/CI resolve it automatically |
| minSdk / target/compileSdk | 30 / 33 / 34 | Path B only; cargo-apk defaults handle Path A |
