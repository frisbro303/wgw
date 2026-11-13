//! Vector fields: arrows from `vertices (n, 3)` along `vectors (n, 3)`, optional `values (n)`; all may have frames.
//! Parameters: `scale` (multiplies the vectors, default 1), `width` (shaft radius as a fraction of the scene's
//! radius, default 0.006), `min`/`max`, `color`/`colors`, `pickable` (off by default).

use glam::Vec3;
use glow::HasContext;

use crate::gl::{self, upload};
use crate::layer::{Hit, LayerData, Primitive, Style, View};
use crate::shaders::Shaders;

const SIDES: u32 = 12;
const STRIDE: i32 = 7 * 4; // origin.xyz, vector.xyz, value

#[derive(Default)]
pub struct Vectors {
    gpu: Option<Gpu>,
    synced: Option<(u64, usize)>,
    count: i32,
    origins: Vec<f32>,
}

struct Gpu {
    vao: glow::VertexArray,
    arrow: glow::Buffer,
    arrow_indices: glow::Buffer,
    arrow_count: i32,
    instances: glow::Buffer,
    colormap: glow::Texture,
}

/// The arrow every instance draws: rings of (cos, sin, part), see VECTORS_VS for the parts.
fn arrow() -> (Vec<f32>, Vec<u32>) {
    let mut corners = Vec::new();
    for part in 0..6 {
        for i in 0..SIDES {
            let a = i as f32 / SIDES as f32 * std::f32::consts::TAU;
            corners.extend_from_slice(&[a.cos(), a.sin(), part as f32]);
        }
    }
    let ring = |part: u32, i: u32| part * SIDES + i % SIDES;
    let mut indices = Vec::new();
    for i in 0..SIDES {
        let j = i + 1;
        // The shaft and the cone, as quads between rings; the cone's back, as a fan from the center ring.
        for (lo, hi) in [(0, 1), (2, 3)] {
            indices.extend_from_slice(&[ring(lo, i), ring(lo, j), ring(hi, j), ring(lo, i), ring(hi, j), ring(hi, i)]);
        }
        indices.extend_from_slice(&[ring(4, i), ring(5, i), ring(4, j)]);
    }
    (corners, indices)
}

impl Primitive for Vectors {
    fn sync(&mut self, gl: &glow::Context, data: &LayerData, k: usize, style: &Style) -> Result<(), String> {
        if self.synced == Some((data.version, k)) {
            return Ok(());
        }
        let origins = data.rows("vertices", 3, k, None)?.ok_or("vectors need vertices, where the arrows start")?;
        let n = origins.len() / 3;
        let vectors = data.rows("vectors", 3, k, Some(n))?.ok_or("vectors need vectors (n, 3)")?;
        let values = data.rows("values", 1, k, Some(n))?;
        let mut instances = Vec::with_capacity(n * 7);
        for i in 0..n {
            instances.extend_from_slice(&origins[3 * i..3 * i + 3]);
            instances.extend_from_slice(&vectors[3 * i..3 * i + 3]);
            instances.push(values.map_or(0.0, |v| v[i]));
        }
        let g = self.gpu.get_or_insert_with(|| unsafe {
            let vao = gl.create_vertex_array().expect("create vertex array");
            gl.bind_vertex_array(Some(vao));
            let (corners, indices) = arrow();
            let (arrow, arrow_indices, instances) = (gl::buffer(gl), gl::buffer(gl), gl::buffer(gl));
            upload(gl, glow::ARRAY_BUFFER, arrow, &corners);
            gl::attribute(gl, arrow, 0, 3, 0, 0, 0);
            upload(gl, glow::ELEMENT_ARRAY_BUFFER, arrow_indices, &indices);
            gl::attribute(gl, instances, 1, 3, STRIDE, 0, 1);
            gl::attribute(gl, instances, 2, 3, STRIDE, 12, 1);
            gl::attribute(gl, instances, 3, 1, STRIDE, 24, 1);
            gl.bind_vertex_array(None);
            Gpu { vao, arrow, arrow_indices, arrow_count: indices.len() as i32, instances, colormap: gl::colormap(gl, None, &[[0.0; 4]]) }
        });
        upload(gl, glow::ARRAY_BUFFER, g.instances, &instances);
        gl::colormap(gl, Some(g.colormap), &data.stops(style));
        self.count = n as i32;
        self.origins = origins.to_vec();
        self.synced = Some((data.version, k));
        Ok(())
    }

    fn draw(&self, gl: &glow::Context, shaders: &Shaders, view: &View, data: &LayerData, _style: &Style) {
        let Some(g) = &self.gpu else { return };
        let p = &shaders.vectors;
        self.setup(gl, p, view, data);
        let range = data.range();
        p.f2(gl, "uRange", range.x, range.y);
        gl::bind_texture(gl, p, "uMap", g.colormap);
        self.draw_instances(gl, g);
    }

    fn draw_ids(&self, gl: &glow::Context, shaders: &Shaders, view: &View, data: &LayerData, layer: u32) {
        let Some(g) = &self.gpu else { return };
        let p = &shaders.vectors_ids;
        self.setup(gl, p, view, data);
        p.u1(gl, "uLayer", layer);
        self.draw_instances(gl, g);
    }

    fn hit(&mut self, id: u32, _origin: Vec3, _dir: Vec3) -> Hit {
        let i = 3 * id as usize;
        Hit { index: id, point: self.origins.get(i..i + 3).map(Vec3::from_slice).unwrap_or(Vec3::ZERO) }
    }

    fn destroy(&mut self, gl: &glow::Context) {
        if let Some(g) = self.gpu.take() {
            unsafe {
                gl.delete_vertex_array(g.vao);
                for b in [g.arrow, g.arrow_indices, g.instances] {
                    gl.delete_buffer(b);
                }
                gl.delete_texture(g.colormap);
            }
        }
    }
}

impl Vectors {
    fn setup(&self, gl: &glow::Context, p: &gl::Program, view: &View, data: &LayerData) {
        p.bind(gl);
        p.mat4(gl, "uModelView", &view.model_view);
        p.mat4(gl, "uProj", &view.proj);
        p.f1(gl, "uScale", data.number("scale", 1.0) as f32);
        p.f1(gl, "uWidth", data.number("width", 0.006) as f32 * view.radius);
    }

    fn draw_instances(&self, gl: &glow::Context, g: &Gpu) {
        unsafe {
            gl.bind_vertex_array(Some(g.vao));
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(g.arrow_indices));
            gl.draw_elements_instanced(glow::TRIANGLES, g.arrow_count, glow::UNSIGNED_INT, 0, self.count);
            gl.bind_vertex_array(None);
        }
    }
}
