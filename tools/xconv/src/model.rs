//! Interprets the generic `.x` object tree as frames, meshes, materials,
//! skins and animations. Everything stays in DirectX conventions here
//! (left-handed, row-vector matrices); `gltf.rs` converts on output.

use crate::xfile::{Error, Item, XObj};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub type Mat4 = [f32; 16];

pub const IDENTITY: Mat4 = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.];

#[derive(Debug, Clone)]
pub struct Material {
    pub name: String,
    pub color: [f32; 4],
    pub texture: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct SkinWeights {
    pub bone: String,
    pub indices: Vec<u32>,
    pub weights: Vec<f32>,
    pub offset: Mat4,
}

#[derive(Debug, Clone, Default)]
pub struct Mesh {
    pub name: String,
    pub positions: Vec<[f32; 3]>,
    pub faces: Vec<Vec<u32>>,
    pub normals: Vec<[f32; 3]>,
    /// Per-face normal indices, parallel to `faces` (empty when the mesh has no normals).
    pub normal_faces: Vec<Vec<u32>>,
    pub uvs: Vec<[f32; 2]>,
    pub face_materials: Vec<usize>,
    pub materials: Vec<Material>,
    pub skin: Vec<SkinWeights>,
}

#[derive(Debug, Clone, Default)]
pub struct Frame {
    pub name: String,
    pub matrix: Option<Mat4>,
    pub meshes: Vec<Mesh>,
    pub children: Vec<Frame>,
}

#[derive(Debug, Clone, Default)]
pub struct Animation {
    pub target: String,
    /// Quaternions as stored in the file: (w, x, y, z).
    pub rotation: Vec<(f32, [f32; 4])>,
    pub scale: Vec<(f32, [f32; 3])>,
    pub translation: Vec<(f32, [f32; 3])>,
    pub matrix: Vec<(f32, Mat4)>,
}

#[derive(Debug, Default)]
pub struct Scene {
    pub root: Frame,
    pub animations: Vec<Animation>,
}

pub fn load(path: &Path) -> Result<Scene, Error> {
    let bytes = std::fs::read(path).map_err(|e| Error(format!("{}: {e}", path.display())))?;
    let text = String::from_utf8_lossy(&bytes);
    let objs = crate::xfile::parse(&text).map_err(|e| Error(format!("{}: {e}", path.display())))?;
    let dir = path.parent().unwrap_or(Path::new("."));

    let mut ctx = Ctx { dir, materials: HashMap::new() };
    let mut scene = Scene { root: Frame { name: "root".into(), ..Default::default() }, ..Default::default() };
    for obj in &objs {
        match obj.kind.as_str() {
            "Material" => {
                let m = ctx.material(obj)?;
                ctx.materials.insert(m.name.clone(), m);
            }
            "Frame" => scene.root.children.push(ctx.frame(obj)?),
            "Mesh" => scene.root.meshes.push(ctx.mesh(obj)?),
            "AnimationSet" => {
                for a in obj.children().filter(|c| c.kind == "Animation") {
                    scene.animations.push(animation(a)?);
                }
            }
            "Animation" => scene.animations.push(animation(obj)?),
            _ => {}
        }
    }
    Ok(scene)
}

struct Ctx<'a> {
    dir: &'a Path,
    materials: HashMap<String, Material>,
}

impl Ctx<'_> {
    fn material(&self, obj: &XObj) -> Result<Material, Error> {
        let mut r = obj.reader();
        let c = r.floats(4)?;
        let texture = obj.child("TextureFilename").map(|t| t.reader().string()).transpose()?;
        Ok(Material {
            name: obj.name.clone().unwrap_or_default(),
            color: [c[0], c[1], c[2], c[3]],
            texture: texture.map(|t| self.dir.join(t.replace('\\', "/"))),
        })
    }

    fn frame(&self, obj: &XObj) -> Result<Frame, Error> {
        let mut f = Frame { name: obj.name.clone().unwrap_or_default(), ..Default::default() };
        for c in obj.children() {
            match c.kind.as_str() {
                "FrameTransformMatrix" => f.matrix = Some(mat4(&mut c.reader())?),
                "Mesh" => f.meshes.push(self.mesh(c)?),
                "Frame" => f.children.push(self.frame(c)?),
                _ => {}
            }
        }
        Ok(f)
    }

    fn mesh(&self, obj: &XObj) -> Result<Mesh, Error> {
        let mut r = obj.reader();
        let mut m = Mesh { name: obj.name.clone().unwrap_or_default(), ..Default::default() };
        let nv = r.usize()?;
        m.positions = (0..nv).map(|_| vec3(&mut r)).collect::<Result<_, _>>()?;
        m.faces = faces(&mut r)?;

        for c in obj.children() {
            match c.kind.as_str() {
                "MeshNormals" => {
                    let mut r = c.reader();
                    let n = r.usize()?;
                    m.normals = (0..n).map(|_| vec3(&mut r)).collect::<Result<_, _>>()?;
                    m.normal_faces = faces(&mut r)?;
                }
                "MeshTextureCoords" => {
                    let mut r = c.reader();
                    let n = r.usize()?;
                    m.uvs = (0..n).map(|_| Ok([r.f32()?, r.f32()?])).collect::<Result<_, Error>>()?;
                }
                "MeshMaterialList" => {
                    let mut r = c.reader();
                    let _n_mat = r.usize()?;
                    let n_idx = r.usize()?;
                    m.face_materials = (0..n_idx).map(|_| r.usize()).collect::<Result<_, _>>()?;
                    for item in &c.items {
                        match item {
                            Item::Obj(o) if o.kind == "Material" => m.materials.push(self.material(o)?),
                            Item::Ref(name) => m.materials.push(
                                self.materials
                                    .get(name)
                                    .cloned()
                                    .ok_or_else(|| Error(format!("unknown material reference '{name}'")))?,
                            ),
                            _ => {}
                        }
                    }
                }
                "SkinWeights" => {
                    let mut r = c.reader();
                    let bone = r.string()?;
                    let n = r.usize()?;
                    let indices = (0..n).map(|_| Ok(r.usize()? as u32)).collect::<Result<_, Error>>()?;
                    let weights = r.floats(n)?;
                    let offset = mat4(&mut r)?;
                    m.skin.push(SkinWeights { bone, indices, weights, offset });
                }
                _ => {}
            }
        }

        // A short material list repeats its last entry for the remaining faces.
        if m.materials.is_empty() {
            m.materials.push(Material { name: "default".into(), color: [0.8, 0.8, 0.8, 1.0], texture: None });
        }
        let last = m.face_materials.last().copied().unwrap_or(0);
        m.face_materials.resize(m.faces.len(), last);
        for idx in &mut m.face_materials {
            *idx = (*idx).min(m.materials.len() - 1);
        }
        if !m.normal_faces.is_empty() && m.normal_faces.len() != m.faces.len() {
            eprintln!("  warning: mesh '{}' normal face count mismatch, recomputing normals", m.name);
            m.normals.clear();
            m.normal_faces.clear();
        }
        Ok(m)
    }
}

fn vec3(r: &mut crate::xfile::Reader) -> Result<[f32; 3], Error> {
    Ok([r.f32()?, r.f32()?, r.f32()?])
}

fn mat4(r: &mut crate::xfile::Reader) -> Result<Mat4, Error> {
    let v = r.floats(16)?;
    Ok(v.try_into().unwrap())
}

fn faces(r: &mut crate::xfile::Reader) -> Result<Vec<Vec<u32>>, Error> {
    let n = r.usize()?;
    (0..n)
        .map(|_| {
            let k = r.usize()?;
            (0..k).map(|_| Ok(r.usize()? as u32)).collect()
        })
        .collect()
}

fn animation(obj: &XObj) -> Result<Animation, Error> {
    let mut a = Animation::default();
    for item in &obj.items {
        match item {
            Item::Ref(name) => a.target = name.clone(),
            // Some exporters inline the target as `Frame name {}`.
            Item::Obj(o) if o.kind == "Frame" => a.target = o.name.clone().unwrap_or_default(),
            Item::Obj(o) if o.kind == "AnimationKey" => {
                let mut r = o.reader();
                let kind = r.usize()?;
                let n = r.usize()?;
                for _ in 0..n {
                    let t = r.f32()?;
                    let count = r.usize()?;
                    let v = r.floats(count)?;
                    match (kind, count) {
                        (0, 4) => a.rotation.push((t, [v[0], v[1], v[2], v[3]])),
                        (1, 3) => a.scale.push((t, [v[0], v[1], v[2]])),
                        (2, 3) => a.translation.push((t, [v[0], v[1], v[2]])),
                        (3 | 4, 16) => a.matrix.push((t, v.try_into().unwrap())),
                        _ => return Err(Error(format!("unsupported AnimationKey type {kind} with {count} values"))),
                    }
                }
            }
            _ => {}
        }
    }
    Ok(a)
}

impl Scene {
    /// Collapses every mesh into one under the root frame. Only valid for
    /// static scenes whose frames all have identity transforms (the
    /// Cartography Shop level export), where it cuts ~1000 draw calls to one
    /// per material.
    pub fn merge_meshes(&mut self) -> Result<(), Error> {
        fn take(f: &mut Frame, out: &mut Vec<Mesh>) -> Result<(), Error> {
            if f.matrix.is_some_and(|m| m != IDENTITY) {
                return Err(Error(format!("--merge: frame '{}' has a non-identity transform", f.name)));
            }
            out.append(&mut f.meshes);
            for c in &mut f.children {
                take(c, out)?;
            }
            Ok(())
        }
        let mut meshes = Vec::new();
        take(&mut self.root, &mut meshes)?;
        let mut merged = Mesh { name: "merged".into(), ..Default::default() };
        for m in meshes {
            let base = merged.positions.len() as u32;
            let nbase = merged.normals.len() as u32;
            let mbase = merged.materials.len();
            let has_normals = !m.normal_faces.is_empty();
            if !m.skin.is_empty() {
                return Err(Error("--merge: skinned meshes cannot be merged".into()));
            }
            merged.faces.extend(m.faces.iter().map(|f| f.iter().map(|i| i + base).collect::<Vec<_>>()));
            if has_normals {
                merged.normal_faces.extend(m.normal_faces.iter().map(|f| f.iter().map(|i| i + nbase).collect::<Vec<_>>()));
            }
            merged.normals.extend(&m.normals);
            let mut uvs = m.uvs.clone();
            uvs.resize(m.positions.len(), [0.0, 0.0]);
            merged.uvs.extend(uvs);
            merged.positions.extend(&m.positions);
            merged.face_materials.extend(m.face_materials.iter().map(|i| i + mbase));
            merged.materials.extend(m.materials);
        }
        if merged.normal_faces.len() != merged.faces.len() {
            merged.normals.clear();
            merged.normal_faces.clear();
        }
        self.root.children.clear();
        self.root.meshes = vec![merged];
        Ok(())
    }
}
