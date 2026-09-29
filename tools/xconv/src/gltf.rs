//! Writes a parsed `.x` scene as a binary glTF (.glb).
//!
//! Conversion rules (DirectX left-handed -> glTF right-handed): Z is negated
//! on positions, normals, translations and matrices, triangle winding is
//! reversed, and quaternions become (w, -x, -y, z). DirectX row-vector
//! matrices read in file order are already glTF's column-major layout.

use crate::model::{Frame, IDENTITY, Mat4, Material, Mesh, Scene};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct Options {
    /// Ticks per second for animation key times.
    pub ticks_per_second: f32,
    /// Render both faces (the original set `set object cull obj, 0`).
    pub double_sided: bool,
    /// Lightmap mesh matching the input mesh-for-mesh (Cartography Shop export).
    pub lightmap: Option<Scene>,
    /// Extra directories searched for textures missing next to the model.
    pub search_dirs: Vec<PathBuf>,
}

#[derive(Default)]
struct Builder {
    bin: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
    images: Vec<Value>,
    image_index: HashMap<PathBuf, Option<usize>>,
    materials: Vec<Value>,
    material_index: HashMap<String, usize>,
    meshes: Vec<Value>,
    nodes: Vec<Value>,
    skins: Vec<Value>,
    node_by_name: HashMap<String, usize>,
    search_dirs: Vec<PathBuf>,
    warnings: Vec<String>,
}

pub fn write(scene: &Scene, opts: &Options, out: &Path) -> Result<Vec<String>, String> {
    let mut b = Builder { search_dirs: opts.search_dirs.clone(), ..Default::default() };
    let lm_meshes: Vec<&Mesh> = opts.lightmap.as_ref().map(|s| all_meshes(&s.root)).unwrap_or_default();
    let mut lm_iter = lm_meshes.into_iter();

    let joints = joint_names(&scene.root);

    // Pass 1: nodes (pruned to frames that carry meshes or joints).
    let root_index = b.add_frame(&scene.root, &joints).ok_or("model contains no meshes")?;

    // Pass 2: meshes + skins, now that joint node indices are known.
    let mut mesh_jobs = Vec::new();
    collect_mesh_nodes(&scene.root, &mut mesh_jobs);
    for (frame_name, mesh) in mesh_jobs {
        let lm = if opts.lightmap.is_some() {
            let lm = lm_iter.next().ok_or(format!("lightmap file is missing mesh for '{}'", mesh.name))?;
            if lm.positions.len() != mesh.positions.len() || lm.faces.len() != mesh.faces.len() {
                return Err(format!("lightmap mesh '{}' does not match base mesh", lm.name));
            }
            Some(lm)
        } else {
            None
        };
        let mesh_index = b.add_mesh(mesh, lm, opts)?;
        let node = b.node_by_name[&frame_name];
        // Skinned meshes get their own node directly under the root: glTF
        // ignores a skinned node's transform anyway, and Bevy 0.15's loader
        // never finishes if a skinned node is inside (or is) its own skeleton.
        let target = if !mesh.skin.is_empty() {
            let child = b.nodes.len();
            b.nodes.push(json!({ "name": format!("{}_skinned", mesh.name) }));
            push_child(&mut b.nodes[root_index], child);
            child
        } else if b.nodes[node].get("mesh").is_some() {
            // A frame can hold several meshes: extra ones become child nodes.
            let child = b.nodes.len();
            b.nodes.push(json!({ "name": mesh.name }));
            push_child(&mut b.nodes[node], child);
            child
        } else {
            node
        };
        b.nodes[target]["mesh"] = json!(mesh_index);
        if !mesh.skin.is_empty() {
            let skin = b.add_skin(mesh)?;
            b.nodes[target]["skin"] = json!(skin);
        }
    }

    let animations = b.add_animations(scene, opts.ticks_per_second);

    let mut doc = json!({
        "asset": { "version": "2.0", "generator": "xconv (Dark Messenger 2026)" },
        "scene": 0,
        "scenes": [{ "nodes": [root_index] }],
        "nodes": b.nodes,
        "meshes": b.meshes,
        "materials": b.materials,
        "accessors": b.accessors,
        "bufferViews": b.views,
        "buffers": [{ "byteLength": b.bin.len() }],
        "samplers": [{ "magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497 }],
    });
    if !b.images.is_empty() {
        doc["images"] = json!(b.images);
        doc["textures"] = json!((0..b.images.len()).map(|i| json!({ "sampler": 0, "source": i })).collect::<Vec<_>>());
    }
    if !b.skins.is_empty() {
        doc["skins"] = json!(b.skins);
    }
    if let Some(a) = animations {
        doc["animations"] = json!([a]);
    }

    write_glb(out, &serde_json::to_vec(&doc).unwrap(), &b.bin).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(b.warnings)
}

fn all_meshes(f: &Frame) -> Vec<&Mesh> {
    let mut v: Vec<&Mesh> = f.meshes.iter().collect();
    for c in &f.children {
        v.extend(all_meshes(c));
    }
    v
}

fn collect_mesh_nodes<'a>(f: &'a Frame, out: &mut Vec<(String, &'a Mesh)>) {
    for m in &f.meshes {
        out.push((f.name.clone(), m));
    }
    for c in &f.children {
        collect_mesh_nodes(c, out);
    }
}

fn joint_names(f: &Frame) -> Vec<String> {
    let mut v: Vec<String> = f.meshes.iter().flat_map(|m| m.skin.iter().map(|s| s.bone.clone())).collect();
    for c in &f.children {
        v.extend(joint_names(c));
    }
    v
}

fn push_child(node: &mut Value, child: usize) {
    match node.get_mut("children") {
        Some(Value::Array(a)) => a.push(json!(child)),
        _ => node["children"] = json!([child]),
    }
}

// ---------------------------------------------------------------- math ----

/// Mirrors a matrix across the Z axis (S * M * S with S = diag(1, 1, -1, 1)).
fn flip_matrix(m: &Mat4) -> Mat4 {
    let mut o = *m;
    for i in [2, 6, 8, 9, 11, 14] {
        o[i] = -o[i];
    }
    o
}

fn flip_vec(v: [f32; 3]) -> [f32; 3] {
    [v[0], v[1], -v[2]]
}

/// DirectX key (w, x, y, z) -> glTF (x, y, z, w), mirrored across Z.
fn flip_quat(q: [f32; 4]) -> [f32; 4] {
    let [w, x, y, z] = q;
    [-x, -y, z, w]
}

/// Splits a column-major matrix into translation, rotation (x,y,z,w) and scale.
fn decompose(m: &Mat4) -> ([f32; 3], [f32; 4], [f32; 3]) {
    let t = [m[12], m[13], m[14]];
    let col = |c: usize| [m[c * 4], m[c * 4 + 1], m[c * 4 + 2]];
    let len = |v: [f32; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let (c0, c1, c2) = (col(0), col(1), col(2));
    let det = c0[0] * (c1[1] * c2[2] - c1[2] * c2[1]) - c1[0] * (c0[1] * c2[2] - c0[2] * c2[1])
        + c2[0] * (c0[1] * c1[2] - c0[2] * c1[1]);
    let mut s = [len(c0), len(c1), len(c2)];
    if det < 0.0 {
        s[0] = -s[0];
    }
    let safe = |v: f32| if v.abs() < 1e-12 { 1.0 } else { v };
    let r = |c: [f32; 3], s: f32| [c[0] / safe(s), c[1] / safe(s), c[2] / safe(s)];
    let (x, y, z) = (r(c0, s[0]), r(c1, s[1]), r(c2, s[2]));
    // Rotation matrix element (row, col): x = col 0, y = col 1, z = col 2.
    let (m00, m10, m20) = (x[0], x[1], x[2]);
    let (m01, m11, m21) = (y[0], y[1], y[2]);
    let (m02, m12, m22) = (z[0], z[1], z[2]);
    let trace = m00 + m11 + m22;
    let q = if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        [(m21 - m12) / s, (m02 - m20) / s, (m10 - m01) / s, 0.25 * s]
    } else if m00 > m11 && m00 > m22 {
        let s = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
        [0.25 * s, (m01 + m10) / s, (m02 + m20) / s, (m21 - m12) / s]
    } else if m11 > m22 {
        let s = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
        [(m01 + m10) / s, 0.25 * s, (m12 + m21) / s, (m02 - m20) / s]
    } else {
        let s = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
        [(m02 + m20) / s, (m12 + m21) / s, 0.25 * s, (m10 - m01) / s]
    };
    let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    (t, [q[0] / n, q[1] / n, q[2] / n, q[3] / n], s)
}

fn quat_dot(a: [f32; 4], b: [f32; 4]) -> f32 {
    (a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]).abs()
}

// ------------------------------------------------------------- buffers ----

impl Builder {
    fn view(&mut self, bytes: &[u8], target: Option<u32>) -> usize {
        while !self.bin.len().is_multiple_of(4) {
            self.bin.push(0);
        }
        let mut v = json!({ "buffer": 0, "byteOffset": self.bin.len(), "byteLength": bytes.len() });
        if let Some(t) = target {
            v["target"] = json!(t);
        }
        self.bin.extend_from_slice(bytes);
        self.views.push(v);
        self.views.len() - 1
    }

    fn accessor_f32(&mut self, data: &[f32], ty: &str, width: usize, target: Option<u32>, min_max: bool) -> usize {
        let bytes: Vec<u8> = data.iter().flat_map(|f| f.to_le_bytes()).collect();
        let view = self.view(&bytes, target);
        let mut a = json!({ "bufferView": view, "componentType": 5126, "count": data.len() / width, "type": ty });
        if min_max {
            let mut min = vec![f32::MAX; width];
            let mut max = vec![f32::MIN; width];
            for chunk in data.chunks(width) {
                for (i, v) in chunk.iter().enumerate() {
                    min[i] = min[i].min(*v);
                    max[i] = max[i].max(*v);
                }
            }
            a["min"] = json!(min);
            a["max"] = json!(max);
        }
        self.accessors.push(a);
        self.accessors.len() - 1
    }

    fn accessor_u32(&mut self, data: &[u32]) -> usize {
        let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_le_bytes()).collect();
        let view = self.view(&bytes, Some(34963));
        self.accessors.push(json!({ "bufferView": view, "componentType": 5125, "count": data.len(), "type": "SCALAR" }));
        self.accessors.len() - 1
    }

    fn accessor_joints(&mut self, data: &[u16]) -> usize {
        let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_le_bytes()).collect();
        let view = self.view(&bytes, Some(34962));
        self.accessors.push(json!({ "bufferView": view, "componentType": 5123, "count": data.len() / 4, "type": "VEC4" }));
        self.accessors.len() - 1
    }

    // --------------------------------------------------------- textures ----

    fn image(&mut self, path: &Path) -> Option<usize> {
        if let Some(i) = self.image_index.get(path) {
            return *i;
        }
        let result = self.load_image(path);
        let index = match result {
            Ok((bytes, mime)) => {
                let view = self.view(&bytes, None);
                self.images.push(json!({ "bufferView": view, "mimeType": mime, "name": file_stem(path) }));
                Some(self.images.len() - 1)
            }
            Err(e) => {
                self.warnings.push(format!("texture {}: {e} (using material color)", path.display()));
                None
            }
        };
        self.image_index.insert(path.to_path_buf(), index);
        index
    }

    fn load_image(&self, path: &Path) -> Result<(Vec<u8>, &'static str), String> {
        let path = std::iter::once(path.to_path_buf())
            .chain(self.search_dirs.iter().filter_map(|d| Some(d.join(path.file_name()?))))
            .find_map(|p| resolve_case(&p))
            .ok_or("file not found")?;
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        if ext == "jpg" || ext == "jpeg" {
            return Ok((bytes, "image/jpeg"));
        }
        let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).map_err(|e| e.to_string())?;
        Ok((png, "image/png"))
    }

    fn material(&mut self, m: &Material, lightmap: Option<&Material>, double_sided: bool) -> usize {
        // Level materials carry their lightmap in the name: "material_3@one12".
        // The game attaches the matching lightmap image at load time.
        let name = match lightmap.and_then(|l| l.texture.as_deref()) {
            Some(lm) if m.color[3] > 0.0 => format!("{}@{}", m.name, file_stem(lm)),
            _ => m.name.clone(),
        };
        let key = format!("{name}|{double_sided}");
        if let Some(&i) = self.material_index.get(&key) {
            return i;
        }
        let texture = m.texture.as_deref().and_then(|t| self.image(t));
        let mut pbr = json!({ "baseColorFactor": m.color, "metallicFactor": 0.0, "roughnessFactor": 1.0 });
        if let Some(t) = texture {
            // DarkBASIC modulated the texture by vertex lighting only, so
            // textured faces use a white factor.
            pbr["baseColorFactor"] = json!([1.0, 1.0, 1.0, m.color[3]]);
            pbr["baseColorTexture"] = json!({ "index": t });
        }
        let mut mat = json!({ "name": name, "pbrMetallicRoughness": pbr, "doubleSided": double_sided });
        if m.color[3] < 0.99 {
            mat["alphaMode"] = json!("BLEND");
        }
        self.materials.push(mat);
        self.material_index.insert(key, self.materials.len() - 1);
        self.materials.len() - 1
    }

    // ------------------------------------------------------------ nodes ----

    fn add_frame(&mut self, f: &Frame, joints: &[String]) -> Option<usize> {
        let children: Vec<usize> = f.children.iter().filter_map(|c| self.add_frame(c, joints)).collect();
        let is_joint = joints.contains(&f.name);
        if f.meshes.is_empty() && children.is_empty() && !is_joint {
            return None;
        }
        let (t, r, s) = decompose(&flip_matrix(&f.matrix.unwrap_or(IDENTITY)));
        let mut node = json!({ "name": f.name, "translation": t, "rotation": r, "scale": s });
        if !children.is_empty() {
            node["children"] = json!(children);
        }
        self.nodes.push(node);
        let index = self.nodes.len() - 1;
        self.node_by_name.insert(f.name.clone(), index);
        Some(index)
    }

    // ----------------------------------------------------------- meshes ----

    fn add_mesh(&mut self, mesh: &Mesh, lm: Option<&Mesh>, opts: &Options) -> Result<usize, String> {
        let smooth = if mesh.normal_faces.is_empty() { smooth_normals(mesh) } else { Vec::new() };
        let skin = skin_per_vertex(mesh);
        if !mesh.skin.is_empty() && skin.iter().any(|w| w.is_empty()) {
            let n = skin.iter().filter(|w| w.is_empty()).count();
            self.warnings.push(format!("mesh '{}': {n} vertices have no bone weights (bound to first bone)", mesh.name));
        }

        // Group faces by output material (base material + lightmap image).
        let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
        for (fi, &mi) in mesh.face_materials.iter().enumerate() {
            let lm_mat = lm.map(|l| &l.materials[l.face_materials[fi]]);
            let material = self.material(&mesh.materials[mi], lm_mat, opts.double_sided);
            match groups.iter_mut().find(|g| g.0 == material) {
                Some(g) => g.1.push(fi),
                None => groups.push((material, vec![fi])),
            }
        }

        let mut primitives = Vec::new();
        for (material, faces) in groups {
            let mut remap: HashMap<(u32, u32), u32> = HashMap::new();
            let (mut pos, mut nrm, mut uv0, mut uv1) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
            let (mut joints, mut weights) = (Vec::<u16>::new(), Vec::<f32>::new());
            let mut indices = Vec::new();

            for &fi in &faces {
                let face = &mesh.faces[fi];
                let mut corner = |k: usize| -> u32 {
                    let p = face[k];
                    let n = mesh.normal_faces.get(fi).map(|nf| nf[k]).unwrap_or(p);
                    *remap.entry((p, n)).or_insert_with(|| {
                        pos.extend(flip_vec(mesh.positions[p as usize]));
                        let normal = if mesh.normal_faces.is_empty() { smooth[p as usize] } else { mesh.normals[n as usize] };
                        nrm.extend(normalize(flip_vec(normal)));
                        uv0.extend(mesh.uvs.get(p as usize).copied().unwrap_or([0.0, 0.0]));
                        if let Some(l) = lm {
                            uv1.extend(l.uvs.get(p as usize).copied().unwrap_or([0.0, 0.0]));
                        }
                        if !mesh.skin.is_empty() {
                            let w = &skin[p as usize];
                            for i in 0..4 {
                                let (j, wt) = w.get(i).copied().unwrap_or((0, if i == 0 && w.is_empty() { 1.0 } else { 0.0 }));
                                joints.push(j);
                                weights.push(wt);
                            }
                        }
                        (pos.len() / 3 - 1) as u32
                    })
                };
                // Fan triangulation with reversed winding for right-handed output.
                for k in 1..face.len().saturating_sub(1) {
                    let (a, b, c) = (corner(0), corner(k + 1), corner(k));
                    indices.extend([a, b, c]);
                }
            }
            if indices.is_empty() {
                continue;
            }

            let mut attrs = json!({
                "POSITION": self.accessor_f32(&pos, "VEC3", 3, Some(34962), true),
                "NORMAL": self.accessor_f32(&nrm, "VEC3", 3, Some(34962), false),
            });
            if !mesh.uvs.is_empty() {
                attrs["TEXCOORD_0"] = json!(self.accessor_f32(&uv0, "VEC2", 2, Some(34962), false));
            }
            if lm.is_some() {
                attrs["TEXCOORD_1"] = json!(self.accessor_f32(&uv1, "VEC2", 2, Some(34962), false));
            }
            if !mesh.skin.is_empty() {
                attrs["JOINTS_0"] = json!(self.accessor_joints(&joints));
                attrs["WEIGHTS_0"] = json!(self.accessor_f32(&weights, "VEC4", 4, Some(34962), false));
            }
            let idx = self.accessor_u32(&indices);
            primitives.push(json!({ "attributes": attrs, "indices": idx, "material": material }));
        }
        if primitives.is_empty() {
            return Err(format!("mesh '{}' has no faces", mesh.name));
        }
        self.meshes.push(json!({ "name": mesh.name, "primitives": primitives }));
        Ok(self.meshes.len() - 1)
    }

    fn add_skin(&mut self, mesh: &Mesh) -> Result<usize, String> {
        let mut joints = Vec::new();
        let mut ibm = Vec::new();
        for s in &mesh.skin {
            let node = *self.node_by_name.get(&s.bone).ok_or(format!("skin bone '{}' has no frame", s.bone))?;
            joints.push(node);
            ibm.extend(flip_matrix(&s.offset));
        }
        let acc = self.accessor_f32(&ibm, "MAT4", 16, None, false);
        self.skins.push(json!({ "joints": joints, "inverseBindMatrices": acc }));
        Ok(self.skins.len() - 1)
    }

    // ------------------------------------------------------- animations ----

    fn add_animations(&mut self, scene: &Scene, tps: f32) -> Option<Value> {
        let conjugate = rotation_keys_are_conjugated(scene);
        let mut samplers = Vec::new();
        let mut channels = Vec::new();
        for a in &scene.animations {
            let Some(&node) = self.node_by_name.get(&a.target) else { continue };
            let mut tracks: Vec<(&str, Vec<f32>, Vec<f32>, &str)> = Vec::new();

            if !a.matrix.is_empty() {
                let keys: Vec<(f32, ([f32; 3], [f32; 4], [f32; 3]))> =
                    a.matrix.iter().map(|(t, m)| (*t, decompose(&flip_matrix(m)))).collect();
                tracks.push(("translation", times(&keys, tps), keys.iter().flat_map(|k| k.1.0).collect(), "VEC3"));
                tracks.push(("rotation", times(&keys, tps), fix_quat_signs(keys.iter().map(|k| k.1.1)), "VEC4"));
                tracks.push(("scale", times(&keys, tps), keys.iter().flat_map(|k| k.1.2).collect(), "VEC3"));
            }
            if !a.translation.is_empty() {
                let keys = dedupe(&a.translation);
                tracks.push(("translation", times(&keys, tps), keys.iter().flat_map(|k| flip_vec(k.1)).collect(), "VEC3"));
            }
            if !a.rotation.is_empty() {
                let keys = dedupe(&a.rotation);
                let quats = keys.iter().map(|(_, q)| {
                    let q = if conjugate { [q[0], -q[1], -q[2], -q[3]] } else { *q };
                    flip_quat(q)
                });
                tracks.push(("rotation", times(&keys, tps), fix_quat_signs(quats), "VEC4"));
            }
            if !a.scale.is_empty() {
                let keys = dedupe(&a.scale);
                tracks.push(("scale", times(&keys, tps), keys.iter().flat_map(|k| k.1).collect(), "VEC3"));
            }

            for (path, t, values, ty) in tracks {
                let width = if ty == "VEC4" { 4 } else { 3 };
                let input = self.accessor_f32(&t, "SCALAR", 1, None, true);
                let output = self.accessor_f32(&values, ty, width, None, false);
                samplers.push(json!({ "input": input, "output": output, "interpolation": "LINEAR" }));
                channels.push(json!({ "sampler": samplers.len() - 1, "target": { "node": node, "path": path } }));
            }
        }
        (!channels.is_empty()).then(|| json!({ "name": "anim", "samplers": samplers, "channels": channels }))
    }
}

fn file_stem(p: &Path) -> String {
    p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string()
}

/// Windows-era asset paths are case-insensitive; find the real file.
fn resolve_case(p: &Path) -> Option<PathBuf> {
    if p.exists() {
        return Some(p.to_path_buf());
    }
    let dir = p.parent()?;
    let want = p.file_name()?.to_str()?.to_ascii_lowercase();
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|e| e.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.to_ascii_lowercase() == want))
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l < 1e-12 { [0.0, 1.0, 0.0] } else { [v[0] / l, v[1] / l, v[2] / l] }
}

/// Area-weighted vertex normals for meshes exported without MeshNormals.
fn smooth_normals(mesh: &Mesh) -> Vec<[f32; 3]> {
    let mut n = vec![[0.0f32; 3]; mesh.positions.len()];
    for face in &mesh.faces {
        for k in 1..face.len().saturating_sub(1) {
            let (a, b, c) = (mesh.positions[face[0] as usize], mesh.positions[face[k] as usize], mesh.positions[face[k + 1] as usize]);
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            // Left-handed clockwise winding: front face normal is u x v.
            let cr = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
            for i in [face[0], face[k], face[k + 1]] {
                for j in 0..3 {
                    n[i as usize][j] += cr[j];
                }
            }
        }
    }
    n.into_iter().map(normalize).collect()
}

/// Up to four strongest (joint, weight) pairs per vertex, normalized.
fn skin_per_vertex(mesh: &Mesh) -> Vec<Vec<(u16, f32)>> {
    let mut v: Vec<Vec<(u16, f32)>> = vec![Vec::new(); mesh.positions.len()];
    for (j, s) in mesh.skin.iter().enumerate() {
        for (i, w) in s.indices.iter().zip(&s.weights) {
            if *w > 0.0
                && let Some(slot) = v.get_mut(*i as usize) {
                    slot.push((j as u16, *w));
                }
        }
    }
    for w in &mut v {
        w.sort_by(|a, b| b.1.total_cmp(&a.1));
        w.truncate(4);
        let sum: f32 = w.iter().map(|x| x.1).sum();
        for x in w.iter_mut() {
            x.1 /= sum;
        }
    }
    v
}

fn dedupe<T: Copy>(keys: &[(f32, T)]) -> Vec<(f32, T)> {
    let mut out: Vec<(f32, T)> = Vec::new();
    for k in keys {
        match out.last_mut() {
            Some(last) if k.0 <= last.0 => *last = (last.0, k.1),
            _ => out.push(*k),
        }
    }
    out
}

fn times<T>(keys: &[(f32, T)], tps: f32) -> Vec<f32> {
    keys.iter().map(|k| k.0 / tps).collect()
}

/// Keeps consecutive quaternions in the same hemisphere so interpolation
/// takes the short way around.
fn fix_quat_signs(quats: impl Iterator<Item = [f32; 4]>) -> Vec<f32> {
    let mut out: Vec<f32> = Vec::new();
    let mut prev: Option<[f32; 4]> = None;
    for mut q in quats {
        if let Some(p) = prev
            && p[0] * q[0] + p[1] * q[1] + p[2] * q[2] + p[3] * q[3] < 0.0 {
                q = q.map(|c| -c);
            }
        out.extend(q);
        prev = Some(q);
    }
    out
}

/// DirectX exporters disagree on the handedness of rotation keys. Compare the
/// first key of every animated frame with that frame's bind matrix and pick
/// whichever reading matches more often.
fn rotation_keys_are_conjugated(scene: &Scene) -> bool {
    fn find<'a>(f: &'a Frame, name: &str) -> Option<&'a Frame> {
        if f.name == name {
            return Some(f);
        }
        f.children.iter().find_map(|c| find(c, name))
    }
    let (mut plain, mut conj) = (0.0f32, 0.0f32);
    for a in &scene.animations {
        let (Some((_, q)), Some(m)) = (a.rotation.first(), find(&scene.root, &a.target).and_then(|f| f.matrix)) else {
            continue;
        };
        let (_, r, _) = decompose(&m); // (x, y, z, w), still left-handed
        let [w, x, y, z] = *q;
        plain += quat_dot(r, [x, y, z, w]);
        conj += quat_dot(r, [-x, -y, -z, w]);
    }
    conj > plain
}

fn write_glb(path: &Path, json: &[u8], bin: &[u8]) -> std::io::Result<()> {
    let mut json = json.to_vec();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let mut bin = bin.to_vec();
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let total = 12 + 8 + json.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend(0x4654_6C67u32.to_le_bytes());
    out.extend(2u32.to_le_bytes());
    out.extend((total as u32).to_le_bytes());
    out.extend((json.len() as u32).to_le_bytes());
    out.extend(0x4E4F_534Au32.to_le_bytes());
    out.extend(&json);
    out.extend((bin.len() as u32).to_le_bytes());
    out.extend(0x004E_4942u32.to_le_bytes());
    out.extend(&bin);
    std::fs::write(path, out)
}
