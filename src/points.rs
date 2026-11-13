//! Point clouds: `vertices (n, 3)`, optional `values (n)`, either with frames. Drawn as round, shaded sprites.
//! Parameters: `size` (CSS pixels, default 6), `lift` (toward the camera, as a fraction of the scene's size,
//! default 0.02, so points on a surface sit on top of it), `min`/`max`, `color`/`colors`, `pickable` (on by default).

use glam::Vec3;
use glow::HasContext;

use crate::gl::{self, upload};
use crate::layer::{Hit, LayerData, Primitive, Style, View};
use crate::shaders::Shaders;

#[derive(Default)]
pub struct Points {
    gpu: Option<Gpu>,
    synced: Option<(u64, usize)>,
    count: i32,
    positions: Vec<f32>,
}

struct Gpu {
    vao: glow::VertexArray,
    position: glow::Buffer,
    value: glow::Buffer,
    colormap: glow::Texture,
}

impl Primitive for Points {
    fn sync(&mut self, gl: &glow::Context, data: &LayerData, k: usize, style: &Style) -> Result<(), String> {
        if self.synced == Some((data.version, k)) {
            return Ok(());
        }
        let positions = data.rows("vertices", 3, k, None)?.ok_or("points need vertices")?;
        let n = positions.len() / 3;
        let values = data.rows("values", 1, k, Some(n))?.map(|v| v.to_vec()).unwrap_or_else(|| vec![0.0; n]);
        let g = self.gpu.get_or_insert_with(|| unsafe {
            let vao = gl.create_vertex_array().expect("create vertex array");
            gl.bind_vertex_array(Some(vao));
            let (position, value) = (gl::buffer(gl), gl::buffer(gl));
            gl::attribute(gl, position, 0, 3, 0, 0, 1);  // per point: each is an instanced square
            gl::attribute(gl, value, 2, 1, 0, 0, 1);
            gl.bind_vertex_array(None);
            Gpu { vao, position, value, colormap: gl::colormap(gl, None, &[[0.0; 4]]) }
        });
        upload(gl, glow::ARRAY_BUFFER, g.position, positions);
        upload(gl, glow::ARRAY_BUFFER, g.value, &values);
        gl::colormap(gl, Some(g.colormap), &data.stops(style));
        self.count = n as i32;
        self.positions = positions.to_vec();
        self.synced = Some((data.version, k));
        Ok(())
    }

    fn draw(&self, gl: &glow::Context, shaders: &Shaders, view: &View, data: &LayerData, _style: &Style) {
        let Some(g) = &self.gpu else { return };
        let p = &shaders.points;
        p.bind(gl);
        p.mat4(gl, "uModelView", &view.model_view);
        p.mat4(gl, "uProj", &view.proj);
        p.f1(gl, "uSize", data.number("size", 6.0) as f32 * view.dpr);
        p.f1(gl, "uLift", data.number("lift", 0.02) as f32);
        p.f2(gl, "uViewport", view.viewport.x, view.viewport.y);
        let range = data.range();
        p.f2(gl, "uRange", range.x, range.y);
        gl::bind_texture(gl, p, "uMap", g.colormap);
        unsafe {
            gl.bind_vertex_array(Some(g.vao));
            gl.draw_arrays_instanced(glow::TRIANGLES, 0, 6, self.count);
            gl.bind_vertex_array(None);
        }
    }

    fn draw_ids(&self, gl: &glow::Context, shaders: &Shaders, view: &View, data: &LayerData, layer: u32) {
        let Some(g) = &self.gpu else { return };
        let p = &shaders.points_ids;
        p.bind(gl);
        p.mat4(gl, "uModelView", &view.model_view);
        p.mat4(gl, "uProj", &view.proj);
        p.f1(gl, "uSize", data.number("size", 6.0) as f32 * view.dpr);
        p.f1(gl, "uLift", data.number("lift", 0.02) as f32);
        p.f2(gl, "uViewport", view.viewport.x, view.viewport.y);
        p.u1(gl, "uLayer", layer);
        unsafe {
            gl.bind_vertex_array(Some(g.vao));
            gl.draw_arrays_instanced(glow::TRIANGLES, 0, 6, self.count);
            gl.bind_vertex_array(None);
        }
    }

    fn hit(&mut self, id: u32, _origin: Vec3, _dir: Vec3) -> Hit {
        let i = 3 * id as usize;
        let point = self.positions.get(i..i + 3).map(Vec3::from_slice).unwrap_or(Vec3::ZERO);
        Hit { index: id, point }
    }

    fn destroy(&mut self, gl: &glow::Context) {
        if let Some(g) = self.gpu.take() {
            unsafe {
                gl.delete_vertex_array(g.vao);
                gl.delete_buffer(g.position);
                gl.delete_buffer(g.value);
                gl.delete_texture(g.colormap);
            }
        }
    }
}
