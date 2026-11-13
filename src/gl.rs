//! Small helpers over glow: programs with their uniforms looked up by name, buffers, colormap textures.

use std::collections::HashMap;

use glam::Mat4;
use glow::HasContext;

pub type Rgba = [f32; 4];

pub struct Program {
    pub program: glow::Program,
    uniforms: HashMap<String, glow::UniformLocation>,
}

impl Program {
    pub fn new(gl: &glow::Context, vs: &str, fs: &str) -> Result<Self, String> {
        unsafe {
            let program = gl.create_program()?;
            for (kind, source) in [(glow::VERTEX_SHADER, vs), (glow::FRAGMENT_SHADER, fs)] {
                let shader = gl.create_shader(kind)?;
                gl.shader_source(shader, source);
                gl.compile_shader(shader);
                if !gl.get_shader_compile_status(shader) {
                    return Err(gl.get_shader_info_log(shader));
                }
                gl.attach_shader(program, shader);
            }
            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                return Err(gl.get_program_info_log(program));
            }
            let mut uniforms = HashMap::new();
            for i in 0..gl.get_active_uniforms(program) {
                if let Some(u) = gl.get_active_uniform(program, i) {
                    if let Some(location) = gl.get_uniform_location(program, &u.name) {
                        uniforms.insert(u.name, location);
                    }
                }
            }
            Ok(Program { program, uniforms })
        }
    }

    pub fn bind(&self, gl: &glow::Context) {
        unsafe { gl.use_program(Some(self.program)) }
    }

    // Setters skip uniforms the compiler optimized out.
    pub fn mat4(&self, gl: &glow::Context, name: &str, m: &Mat4) {
        if let Some(l) = self.uniforms.get(name) {
            unsafe { gl.uniform_matrix_4_f32_slice(Some(l), false, &m.to_cols_array()) }
        }
    }

    pub fn f1(&self, gl: &glow::Context, name: &str, x: f32) {
        if let Some(l) = self.uniforms.get(name) {
            unsafe { gl.uniform_1_f32(Some(l), x) }
        }
    }

    pub fn f2(&self, gl: &glow::Context, name: &str, x: f32, y: f32) {
        if let Some(l) = self.uniforms.get(name) {
            unsafe { gl.uniform_2_f32(Some(l), x, y) }
        }
    }

    pub fn f4(&self, gl: &glow::Context, name: &str, v: Rgba) {
        if let Some(l) = self.uniforms.get(name) {
            unsafe { gl.uniform_4_f32(Some(l), v[0], v[1], v[2], v[3]) }
        }
    }

    pub fn i1(&self, gl: &glow::Context, name: &str, x: i32) {
        if let Some(l) = self.uniforms.get(name) {
            unsafe { gl.uniform_1_i32(Some(l), x) }
        }
    }

    pub fn u1(&self, gl: &glow::Context, name: &str, x: u32) {
        if let Some(l) = self.uniforms.get(name) {
            unsafe { gl.uniform_1_u32(Some(l), x) }
        }
    }
}

pub fn buffer(gl: &glow::Context) -> glow::Buffer {
    unsafe { gl.create_buffer().expect("create buffer") }
}

pub fn upload<T: bytemuck::Pod>(gl: &glow::Context, target: u32, buffer: glow::Buffer, data: &[T]) {
    unsafe {
        gl.bind_buffer(target, Some(buffer));
        gl.buffer_data_u8_slice(target, bytemuck::cast_slice(data), glow::DYNAMIC_DRAW);
    }
}

/// A float attribute read from `buffer`: `size` components at `offset` bytes, every `stride` bytes;
/// `divisor` 1 makes it per instance.
pub fn attribute(gl: &glow::Context, buffer: glow::Buffer, location: u32, size: i32, stride: i32, offset: i32, divisor: u32) {
    unsafe {
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(buffer));
        gl.enable_vertex_attrib_array(location);
        gl.vertex_attrib_pointer_f32(location, size, glow::FLOAT, false, stride, offset);
        gl.vertex_attrib_divisor(location, divisor);
    }
}

/// 256 texels running evenly through the color stops.
pub fn colormap(gl: &glow::Context, texture: Option<glow::Texture>, stops: &[Rgba]) -> glow::Texture {
    let mut texels = vec![0u8; 256 * 4];
    let stops: Vec<Rgba> = if stops.len() == 1 { vec![stops[0], stops[0]] } else { stops.to_vec() };
    for i in 0..256 {
        let x = i as f32 / 255.0 * (stops.len() - 1) as f32;
        let k = (x.floor() as usize).min(stops.len() - 2);
        let t = x - k as f32;
        for c in 0..4 {
            texels[i * 4 + c] = ((stops[k][c] * (1.0 - t) + stops[k + 1][c] * t) * 255.0).round() as u8;
        }
    }
    unsafe {
        let texture = texture.unwrap_or_else(|| gl.create_texture().expect("create texture"));
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
        gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, 256, 1, 0, glow::RGBA, glow::UNSIGNED_BYTE,
                        glow::PixelUnpackData::Slice(Some(&texels)));
        texture
    }
}

pub fn bind_texture(gl: &glow::Context, program: &Program, name: &str, texture: glow::Texture) {
    unsafe {
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
    }
    program.i1(gl, name, 0);
}
