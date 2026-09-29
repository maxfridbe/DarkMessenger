//! xconv: converts Dark Messenger's DirectX text `.x` models to `.glb`.
//!
//! Usage:
//!   xconv <in.x> <out.glb> [--lightmap <lm.x>] [--ticks-per-second N] [--double-sided]
//!         [--search <texture dir>]... [--merge]
//!
//! `--lightmap` pairs a Cartography Shop level with its lightmap export: the
//! second file's UVs become TEXCOORD_1 and each material is renamed
//! `<material>@<lightmap image stem>` so the game can attach the lightmap.

mod gltf;
mod model;
mod xfile;

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xconv: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mut positional = Vec::new();
    let mut merge = false;
    let mut opts = gltf::Options { ticks_per_second: 4800.0, double_sided: false, lightmap: None, search_dirs: Vec::new() };
    while let Some(a) = args.next() {
        match a.as_str() {
            "--lightmap" => {
                let p = PathBuf::from(args.next().ok_or("--lightmap needs a path")?);
                opts.lightmap = Some(model::load(&p).map_err(|e| e.to_string())?);
            }
            "--ticks-per-second" => {
                opts.ticks_per_second =
                    args.next().and_then(|v| v.parse().ok()).ok_or("--ticks-per-second needs a number")?;
            }
            "--double-sided" => opts.double_sided = true,
            "--merge" => merge = true,
            "--search" => opts.search_dirs.push(PathBuf::from(args.next().ok_or("--search needs a directory")?)),
            _ if a.starts_with("--") => return Err(format!("unknown option {a}")),
            _ => positional.push(PathBuf::from(a)),
        }
    }
    let [input, output] = positional.as_slice() else {
        return Err("usage: xconv <in.x> <out.glb> [--lightmap <lm.x>] [--ticks-per-second N] [--double-sided] [--search <dir>] [--merge]".into());
    };

    let mut scene = model::load(input).map_err(|e| e.to_string())?;
    if merge {
        scene.merge_meshes().map_err(|e| e.to_string())?;
        if let Some(lm) = opts.lightmap.as_mut() {
            lm.merge_meshes().map_err(|e| e.to_string())?;
        }
    }
    let warnings = gltf::write(&scene, &opts, output)?;
    for w in &warnings {
        eprintln!("  warning: {w}");
    }
    let size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
    println!("{} -> {} ({} KiB, {} animation tracks)", input.display(), output.display(), size / 1024, scene.animations.len());
    Ok(())
}
