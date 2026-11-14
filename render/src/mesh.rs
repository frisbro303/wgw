//! Triangle meshes: `vertices (n, 3)`, `faces (m, 3)`, optional `values (n)`; any but faces may have frames.
//! Parameters: `edges` (draw triangle edges; by default the viewer's grid setting), `flat` (flat shading), `min`/`max` (the values' range),
//! `color`/`colors`, `edge_color`, `pickable` (on by default).

use glam::Vec3;

use crate::gl::{self, consts as glow, upload, Gl};
use wgw_scene::geometry::{unique_edges, vertex_normals};

use crate::layer::{Hit, LayerData, Primitive, Style, View};
use crate::shaders::Shaders;

#[derive(Default)]
pub struct Mesh {
    gpu: Option<Gpu>,
    synced: Option<(u64, usize)>,
    count: i32,
    edge_count: i32,
    /// The current frame's positions and the faces, kept for picking.
    positions: Vec<f32>,
    faces: Vec<u32>,
    around: Option<VertexFaces>,
}

struct Gpu {
    vao: gl::VertexArray,
    position: gl::Buffer,
    normal: gl::Buffer,
    value: gl::Buffer,
    faces: gl::Buffer,
    edges: gl::Buffer,
    colormap: gl::Texture,
}

impl Mesh {
    fn gpu(&mut self, gl: &dyn Gl) -> &Gpu {
        self.gpu.get_or_insert_with(|| unsafe {
            let vao = gl.create_vertex_array().expect("create vertex array");
            gl.bind_vertex_array(Some(vao));
            let (position, normal, value) = (gl::buffer(gl), gl::buffer(gl), gl::buffer(gl));
            gl::attribute(gl, position, 0, 3, 0, 0, 0);
            gl::attribute(gl, normal, 1, 3, 0, 0, 0);
            gl::attribute(gl, value, 2, 1, 0, 0, 0);
            gl.bind_vertex_array(None);
            Gpu { vao, position, normal, value, faces: gl::buffer(gl), edges: gl::buffer(gl), colormap: gl::colormap(gl, None, &[[0.0; 4]]) }
        })
    }
}

impl Primitive for Mesh {
    fn sync(&mut self, gl: &dyn Gl, data: &LayerData, k: usize, style: &Style) -> Result<(), String> {
        if self.synced == Some((data.version, k)) {
            return Ok(());
        }
        let changed = self.synced.map(|s| s.0) != Some(data.version);
        let positions = data.rows("vertices", 3, k, None)?.ok_or("a mesh needs vertices")?;
        let n = positions.len() / 3;
        let faces = data.arrays.get("faces").and_then(|a| a.u32s()).ok_or("a mesh needs faces (m, 3) of vertex indices")?;
        if faces.len() % 3 != 0 || faces.iter().any(|&i| i as usize >= n) {
            return Err(format!("faces must be (m, 3) indices below {n}"));
        }
        let values = match data.rows("values", 1, k, Some(n))? {
            Some(v) => v.to_vec(),
            None => vec![0.0; n],
        };
        let normals = vertex_normals(positions, faces);
        self.gpu(gl);
        let g = self.gpu.as_ref().unwrap();
        unsafe { gl.bind_vertex_array(Some(g.vao)) };
        upload(gl, glow::ARRAY_BUFFER, g.position, positions);
        upload(gl, glow::ARRAY_BUFFER, g.normal, &normals);
        upload(gl, glow::ARRAY_BUFFER, g.value, &values);
        if changed {
            gl::colormap(gl, Some(g.colormap), &data.stops(style));
            // The faces and their edges go up only when the faces change, not with every new set of values. The
            // first upload compares against no faces at all.
            if self.faces != faces || self.count == 0 {
                upload(gl, glow::ELEMENT_ARRAY_BUFFER, g.faces, faces);
                self.count = faces.len() as i32;
                self.faces = faces.to_vec();
                self.around = None;
                self.edge_count = 0;
            }
            if data.flag("edges", style.grid) && self.edge_count == 0 && !faces.is_empty() {
                let edges = unique_edges(faces);
                upload(gl, glow::ELEMENT_ARRAY_BUFFER, g.edges, &edges);
                self.edge_count = edges.len() as i32;
            }
        }
        unsafe { gl.bind_vertex_array(None) };
        self.positions = positions.to_vec();
        self.synced = Some((data.version, k));
        Ok(())
    }

    fn draw(&self, gl: &dyn Gl, shaders: &Shaders, view: &View, data: &LayerData, style: &Style) {
        let Some(g) = &self.gpu else { return };
        let p = &shaders.mesh;
        let edges = data.flag("edges", style.grid) && self.edge_count > 0;
        p.bind(gl);
        p.mat4(gl, "uModelView", &view.model_view);
        p.mat4(gl, "uProj", &view.proj);
        let range = data.range();
        p.f2(gl, "uRange", range.x, range.y);
        p.i1(gl, "uFlat", data.flag("flat", false) as i32);
        gl::bind_texture(gl, p, "uMap", g.colormap);
        unsafe {
            gl.bind_vertex_array(Some(g.vao));
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(g.faces));
            if edges {
                gl.enable(glow::POLYGON_OFFSET_FILL);
                gl.polygon_offset(1.0, 1.0);
            }
            gl.draw_elements(glow::TRIANGLES, self.count, glow::UNSIGNED_INT, 0);
            gl.disable(glow::POLYGON_OFFSET_FILL);
            if edges {
                let e = &shaders.edges;
                e.bind(gl);
                e.mat4(gl, "uModelView", &view.model_view);
                e.mat4(gl, "uProj", &view.proj);
                e.f4(gl, "uColor", data.colors.get("edge_color").map(|c| c[0]).unwrap_or(style.edge));
                gl.enable(glow::BLEND);
                gl.blend_func_separate(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA, glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
                gl.depth_mask(false);
                gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(g.edges));
                gl.draw_elements(glow::LINES, self.edge_count, glow::UNSIGNED_INT, 0);
                gl.depth_mask(true);
                gl.disable(glow::BLEND);
            }
            gl.bind_vertex_array(None);
        }
    }

    fn draw_ids(&self, gl: &dyn Gl, shaders: &Shaders, view: &View, _data: &LayerData, layer: u32) {
        let Some(g) = &self.gpu else { return };
        let p = &shaders.mesh_ids;
        p.bind(gl);
        p.mat4(gl, "uModelView", &view.model_view);
        p.mat4(gl, "uProj", &view.proj);
        p.u1(gl, "uLayer", layer);
        unsafe {
            gl.bind_vertex_array(Some(g.vao));
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(g.faces));
            gl.draw_elements(glow::TRIANGLES, self.count, glow::UNSIGNED_INT, 0);
            gl.bind_vertex_array(None);
        }
    }

    /// The GPU gives a vertex of the hit triangle; the ray against the faces around it finds the hit point, and
    /// its nearest corner is the vertex picked.
    fn hit(&mut self, id: u32, origin: Vec3, dir: Vec3) -> Hit {
        let n = self.positions.len() / 3;
        let corner = |p: &[f32], v: u32| Vec3::new(p[3 * v as usize], p[3 * v as usize + 1], p[3 * v as usize + 2]);
        let around = self.around.get_or_insert_with(|| VertexFaces::new(&self.faces, n));
        let (mut best, mut face) = (f32::INFINITY, None);
        for &f in around.of(id) {
            let f = f as usize;
            let [a, b, c] = [0, 1, 2].map(|i| corner(&self.positions, self.faces[3 * f + i]));
            if let Some(t) = ray_triangle(origin, dir, a, b, c) {
                if t < best {
                    best = t;
                    face = Some(f);
                }
            }
        }
        let Some(f) = face else { return Hit { index: id, point: corner(&self.positions, id) } };
        let point = origin + dir * best;
        let index = (0..3)
            .map(|i| self.faces[3 * f + i])
            .min_by(|&u, &v| {
                corner(&self.positions, u).distance(point).total_cmp(&corner(&self.positions, v).distance(point))
            })
            .unwrap();
        Hit { index, point }
    }

    fn destroy(&mut self, gl: &dyn Gl) {
        if let Some(g) = self.gpu.take() {
            unsafe {
                gl.delete_vertex_array(g.vao);
                for b in [g.position, g.normal, g.value, g.faces, g.edges] {
                    gl.delete_buffer(b);
                }
                gl.delete_texture(g.colormap);
            }
        }
    }
}

/// Area-weighted vertex normals.
/// For each vertex, the faces around it, packed: faces[start[v]..start[v + 1]].
struct VertexFaces {
    start: Vec<u32>,
    faces: Vec<u32>,
}

impl VertexFaces {
    fn new(indices: &[u32], n: usize) -> Self {
        let mut start = vec![0u32; n + 1];
        for &v in indices {
            start[v as usize + 1] += 1;
        }
        for v in 0..n {
            start[v + 1] += start[v];
        }
        let mut cursor = start.clone();
        let mut faces = vec![0u32; indices.len()];
        for (i, &v) in indices.iter().enumerate() {
            faces[cursor[v as usize] as usize] = (i / 3) as u32;
            cursor[v as usize] += 1;
        }
        VertexFaces { start, faces }
    }

    fn of(&self, v: u32) -> &[u32] {
        let v = v as usize;
        if v + 1 >= self.start.len() {
            return &[];
        }
        &self.faces[self.start[v] as usize..self.start[v + 1] as usize]
    }
}

/// Where the ray o + t d first meets triangle (a, b, c), from either side (Moller-Trumbore).
fn ray_triangle(o: Vec3, d: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let (e1, e2) = (b - a, c - a);
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let s = o - a;
    let u = s.dot(p) / det;
    let q = s.cross(e1);
    let v = d.dot(q) / det;
    let eps = 1e-5;
    if u < -eps || v < -eps || u + v > 1.0 + eps {
        return None;
    }
    let t = e2.dot(q) / det;
    (t > 0.0).then_some(t)
}
