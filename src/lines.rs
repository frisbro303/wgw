//! Curves: `vertices (n, 3)` joined in order (or by `segments (s, 2)`), optional `values (n)`; vertices and values
//! may have frames. Drawn as screen-space ribbons of constant width.
//! Parameters: `width` (CSS pixels, default 2), `closed` (join the last vertex to the first), `min`/`max`,
//! `color`/`colors`, `pickable` (off by default).

use glam::Vec3;
use glow::HasContext;

use crate::gl::{self, upload};
use crate::layer::{ray_distance, Hit, LayerData, Primitive, Style, View};
use crate::shaders::Shaders;

const STRIDE: i32 = 8 * 4; // a.xyz, b.xyz, value at a, value at b

#[derive(Default)]
pub struct Lines {
    gpu: Option<Gpu>,
    synced: Option<(u64, usize)>,
    count: i32,
    positions: Vec<f32>,
    segments: Vec<[u32; 2]>,
}

struct Gpu {
    vao: glow::VertexArray,
    instances: glow::Buffer,
    colormap: glow::Texture,
}

impl Primitive for Lines {
    fn sync(&mut self, gl: &glow::Context, data: &LayerData, k: usize, style: &Style) -> Result<(), String> {
        if self.synced == Some((data.version, k)) {
            return Ok(());
        }
        let positions = data.rows("vertices", 3, k, None)?.ok_or("lines need vertices")?;
        let n = positions.len() / 3;
        let values = data.rows("values", 1, k, Some(n))?;
        self.segments = match data.arrays.get("segments").and_then(|a| a.u32s()) {
            Some(s) => s.chunks_exact(2).map(|s| [s[0], s[1]]).collect(),
            None => {
                let mut s: Vec<[u32; 2]> = (1..n as u32).map(|i| [i - 1, i]).collect();
                if data.flag("closed", false) && n > 2 {
                    s.push([n as u32 - 1, 0]);
                }
                s
            }
        };
        if self.segments.iter().flatten().any(|&i| i as usize >= n) {
            return Err(format!("segments must be indices below {n}"));
        }
        let mut instances = Vec::with_capacity(self.segments.len() * 8);
        for &[a, b] in &self.segments {
            let (a, b) = (a as usize, b as usize);
            instances.extend_from_slice(&positions[3 * a..3 * a + 3]);
            instances.extend_from_slice(&positions[3 * b..3 * b + 3]);
            instances.push(values.map_or(0.0, |v| v[a]));
            instances.push(values.map_or(0.0, |v| v[b]));
        }
        let g = self.gpu.get_or_insert_with(|| unsafe {
            let vao = gl.create_vertex_array().expect("create vertex array");
            gl.bind_vertex_array(Some(vao));
            let instances = gl::buffer(gl);
            gl::attribute(gl, instances, 0, 3, STRIDE, 0, 1);
            gl::attribute(gl, instances, 1, 3, STRIDE, 12, 1);
            gl::attribute(gl, instances, 2, 1, STRIDE, 24, 1);
            gl::attribute(gl, instances, 3, 1, STRIDE, 28, 1);
            gl.bind_vertex_array(None);
            Gpu { vao, instances, colormap: gl::colormap(gl, None, &[[0.0; 4]]) }
        });
        upload(gl, glow::ARRAY_BUFFER, g.instances, &instances);
        gl::colormap(gl, Some(g.colormap), &data.stops(style));
        self.count = self.segments.len() as i32;
        self.positions = positions.to_vec();
        self.synced = Some((data.version, k));
        Ok(())
    }

    fn draw(&self, gl: &glow::Context, shaders: &Shaders, view: &View, data: &LayerData, _style: &Style) {
        let Some(g) = &self.gpu else { return };
        let p = &shaders.lines;
        self.setup(gl, p, view, data);
        let range = data.range();
        p.f2(gl, "uRange", range.x, range.y);
        gl::bind_texture(gl, p, "uMap", g.colormap);
        self.draw_instances(gl, g);
    }

    fn draw_ids(&self, gl: &glow::Context, shaders: &Shaders, view: &View, data: &LayerData, layer: u32) {
        let Some(g) = &self.gpu else { return };
        let p = &shaders.lines_ids;
        self.setup(gl, p, view, data);
        p.u1(gl, "uLayer", layer);
        self.draw_instances(gl, g);
    }

    /// The GPU gives the segment; its end nearer the ray is the vertex picked.
    fn hit(&mut self, id: u32, origin: Vec3, dir: Vec3) -> Hit {
        let at = |v: u32| Vec3::from_slice(&self.positions[3 * v as usize..3 * v as usize + 3]);
        let Some(&[a, b]) = self.segments.get(id as usize) else { return Hit { index: id, point: Vec3::ZERO } };
        let index = if ray_distance(origin, dir, at(a)) <= ray_distance(origin, dir, at(b)) { a } else { b };
        Hit { index, point: at(index) }
    }

    fn destroy(&mut self, gl: &glow::Context) {
        if let Some(g) = self.gpu.take() {
            unsafe {
                gl.delete_vertex_array(g.vao);
                gl.delete_buffer(g.instances);
                gl.delete_texture(g.colormap);
            }
        }
    }
}

impl Lines {
    fn setup(&self, gl: &glow::Context, p: &gl::Program, view: &View, data: &LayerData) {
        p.bind(gl);
        p.mat4(gl, "uModelView", &view.model_view);
        p.mat4(gl, "uProj", &view.proj);
        p.f2(gl, "uViewport", view.viewport.x, view.viewport.y);
        p.f1(gl, "uWidth", data.number("width", 2.0) as f32 * view.dpr);
    }

    fn draw_instances(&self, gl: &glow::Context, g: &Gpu) {
        unsafe {
            gl.bind_vertex_array(Some(g.vao));
            gl.draw_arrays_instanced(glow::TRIANGLES, 0, 6, self.count);
            gl.bind_vertex_array(None);
        }
    }
}
