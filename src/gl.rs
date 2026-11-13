//! Small helpers over glow: programs with their uniforms set by name, buffers, colormap textures.

use std::collections::HashMap;

use glam::Mat4;
use glow::HasContext;

pub use wgw_scene::Rgba;

use crate::shaders::ShaderDef;

/// A compiled program. Its uniforms live in one uniform block (as naga writes WGSL uniforms for WebGL), held in a
/// buffer of its own; the setters write a uniform by name into it. Textures are plain sampler uniforms.
pub struct Program {
    pub program: glow::Program,
    def: &'static ShaderDef,
    block: glow::Buffer,
    samplers: HashMap<&'static str, glow::UniformLocation>,
}

impl Program {
    pub fn new(gl: &glow::Context, def: &'static ShaderDef) -> Result<Self, String> {
        unsafe {
            let program = gl.create_program()?;
            for (kind, source) in [(glow::VERTEX_SHADER, def.vertex), (glow::FRAGMENT_SHADER, def.fragment)] {
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
            // Every stage's block reads the program's one buffer, at binding 0.
            for name in def.blocks {
                if let Some(index) = gl.get_uniform_block_index(program, name) {
                    gl.uniform_block_binding(program, index, 0);
                }
            }
            let block = gl.create_buffer()?;
            gl.bind_buffer(glow::UNIFORM_BUFFER, Some(block));
            gl.buffer_data_size(glow::UNIFORM_BUFFER, def.size.max(16) as i32, glow::DYNAMIC_DRAW);
            let samplers = def.textures.iter()
                .filter_map(|&(name, glsl)| gl.get_uniform_location(program, glsl).map(|l| (name, l)))
                .collect();
            Ok(Program { program, def, block, samplers })
        }
    }

    pub fn bind(&self, gl: &glow::Context) {
        unsafe {
            gl.use_program(Some(self.program));
            gl.bind_buffer_base(glow::UNIFORM_BUFFER, 0, Some(self.block));
        }
    }

    /// Write a uniform's bytes into the block; uniforms the program doesn't have are skipped.
    fn set(&self, gl: &glow::Context, name: &str, bytes: &[u8]) {
        if let Some(&(_, offset, size)) = self.def.uniforms.iter().find(|u| u.0 == name) {
            let n = bytes.len().min(size as usize);
            unsafe {
                gl.bind_buffer(glow::UNIFORM_BUFFER, Some(self.block));
                gl.buffer_sub_data_u8_slice(glow::UNIFORM_BUFFER, offset as i32, &bytes[..n]);
            }
        }
    }

    pub fn mat4(&self, gl: &glow::Context, name: &str, m: &Mat4) {
        self.set(gl, name, bytemuck::cast_slice(&m.to_cols_array()));
    }

    pub fn f1(&self, gl: &glow::Context, name: &str, x: f32) {
        self.set(gl, name, bytemuck::bytes_of(&x));
    }

    pub fn f2(&self, gl: &glow::Context, name: &str, x: f32, y: f32) {
        self.set(gl, name, bytemuck::cast_slice(&[x, y]));
    }

    pub fn f4(&self, gl: &glow::Context, name: &str, v: Rgba) {
        self.set(gl, name, bytemuck::cast_slice(&v));
    }

    /// An integer uniform, or a texture's unit.
    pub fn i1(&self, gl: &glow::Context, name: &str, x: i32) {
        match self.samplers.get(name) {
            Some(l) => unsafe { gl.uniform_1_i32(Some(l), x) },
            None => self.set(gl, name, bytemuck::bytes_of(&x)),
        }
    }

    pub fn u1(&self, gl: &glow::Context, name: &str, x: u32) {
        self.set(gl, name, bytemuck::bytes_of(&x));
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
