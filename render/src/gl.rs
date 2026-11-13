//! The GL calls wgw draws with, as a trait: implemented by WebGL (through glow) on a page, and by a CPU renderer
//! for stills, which runs the same shaders. Its methods are glow's, cut down to what wgw uses, with plain numeric
//! handles. Also: programs with their uniforms set by name, buffers, colormap textures.

use std::collections::HashMap;

use glam::Mat4;


macro_rules! handles {
    ($($name:ident),*) => { $(
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
        pub struct $name(pub u32);
    )* };
}
handles!(Buffer, VertexArray, Texture, Shader, ProgramId, UniformLocation, Framebuffer, Renderbuffer);

/// What wgw asks of a GL: glow's methods of the same names, cut down to the ones it uses.
#[allow(clippy::missing_safety_doc, clippy::too_many_arguments)]
pub trait Gl {
    unsafe fn create_vertex_array(&self) -> Result<VertexArray, String>;
    unsafe fn bind_vertex_array(&self, vao: Option<VertexArray>);
    unsafe fn delete_vertex_array(&self, vao: VertexArray);
    unsafe fn create_buffer(&self) -> Result<Buffer, String>;
    unsafe fn bind_buffer(&self, target: u32, buffer: Option<Buffer>);
    unsafe fn bind_buffer_base(&self, target: u32, index: u32, buffer: Option<Buffer>);
    unsafe fn delete_buffer(&self, buffer: Buffer);
    unsafe fn buffer_data_u8_slice(&self, target: u32, data: &[u8], usage: u32);
    unsafe fn buffer_data_size(&self, target: u32, size: i32, usage: u32);
    unsafe fn buffer_sub_data_u8_slice(&self, target: u32, offset: i32, data: &[u8]);
    unsafe fn enable_vertex_attrib_array(&self, index: u32);
    unsafe fn vertex_attrib_pointer_f32(&self, index: u32, size: i32, data_type: u32, normalized: bool, stride: i32, offset: i32);
    unsafe fn vertex_attrib_divisor(&self, index: u32, divisor: u32);
    unsafe fn create_texture(&self) -> Result<Texture, String>;
    unsafe fn bind_texture(&self, target: u32, texture: Option<Texture>);
    unsafe fn delete_texture(&self, texture: Texture);
    unsafe fn active_texture(&self, unit: u32);
    unsafe fn tex_parameter_i32(&self, target: u32, parameter: u32, value: i32);
    unsafe fn tex_image_2d(&self, target: u32, level: i32, internal_format: i32, width: i32, height: i32, border: i32,
                           format: u32, ty: u32, pixels: Option<&[u8]>);
    unsafe fn create_shader(&self, kind: u32) -> Result<Shader, String>;
    unsafe fn shader_source(&self, shader: Shader, source: &str);
    unsafe fn compile_shader(&self, shader: Shader);
    unsafe fn get_shader_compile_status(&self, shader: Shader) -> bool;
    unsafe fn get_shader_info_log(&self, shader: Shader) -> String;
    unsafe fn create_program(&self) -> Result<ProgramId, String>;
    unsafe fn attach_shader(&self, program: ProgramId, shader: Shader);
    unsafe fn link_program(&self, program: ProgramId);
    unsafe fn get_program_link_status(&self, program: ProgramId) -> bool;
    unsafe fn get_program_info_log(&self, program: ProgramId) -> String;
    unsafe fn use_program(&self, program: Option<ProgramId>);
    unsafe fn get_uniform_location(&self, program: ProgramId, name: &str) -> Option<UniformLocation>;
    unsafe fn uniform_1_i32(&self, location: Option<&UniformLocation>, x: i32);
    unsafe fn get_uniform_block_index(&self, program: ProgramId, name: &str) -> Option<u32>;
    unsafe fn uniform_block_binding(&self, program: ProgramId, index: u32, binding: u32);
    unsafe fn enable(&self, parameter: u32);
    unsafe fn disable(&self, parameter: u32);
    unsafe fn viewport(&self, x: i32, y: i32, width: i32, height: i32);
    unsafe fn scissor(&self, x: i32, y: i32, width: i32, height: i32);
    unsafe fn clear_color(&self, r: f32, g: f32, b: f32, a: f32);
    unsafe fn clear(&self, mask: u32);
    unsafe fn depth_mask(&self, write: bool);
    unsafe fn polygon_offset(&self, factor: f32, units: f32);
    unsafe fn blend_func_separate(&self, src_rgb: u32, dst_rgb: u32, src_alpha: u32, dst_alpha: u32);
    unsafe fn draw_elements(&self, mode: u32, count: i32, element_type: u32, offset: i32);
    unsafe fn draw_elements_instanced(&self, mode: u32, count: i32, element_type: u32, offset: i32, instances: i32);
    unsafe fn draw_arrays_instanced(&self, mode: u32, first: i32, count: i32, instances: i32);
    unsafe fn create_framebuffer(&self) -> Result<Framebuffer, String>;
    unsafe fn bind_framebuffer(&self, target: u32, framebuffer: Option<Framebuffer>);
    unsafe fn create_renderbuffer(&self) -> Result<Renderbuffer, String>;
    unsafe fn bind_renderbuffer(&self, target: u32, renderbuffer: Option<Renderbuffer>);
    unsafe fn renderbuffer_storage(&self, target: u32, format: u32, width: i32, height: i32);
    unsafe fn framebuffer_renderbuffer(&self, target: u32, attachment: u32, renderbuffer_target: u32, renderbuffer: Option<Renderbuffer>);
    unsafe fn read_pixels(&self, x: i32, y: i32, width: i32, height: i32, format: u32, ty: u32, pixels: &mut [u8]);
}

/// The GL constants wgw uses, with their OpenGL ES values.
pub mod consts {
    pub const ARRAY_BUFFER: u32 = 0x8892;
    pub const ELEMENT_ARRAY_BUFFER: u32 = 0x8893;
    pub const UNIFORM_BUFFER: u32 = 0x8A11;
    pub const DYNAMIC_DRAW: u32 = 0x88E8;
    pub const FLOAT: u32 = 0x1406;
    pub const UNSIGNED_INT: u32 = 0x1405;
    pub const UNSIGNED_BYTE: u32 = 0x1401;
    pub const TRIANGLES: u32 = 0x0004;
    pub const LINES: u32 = 0x0001;
    pub const TEXTURE_2D: u32 = 0x0DE1;
    pub const TEXTURE0: u32 = 0x84C0;
    pub const TEXTURE_MIN_FILTER: u32 = 0x2801;
    pub const TEXTURE_MAG_FILTER: u32 = 0x2800;
    pub const TEXTURE_WRAP_S: u32 = 0x2802;
    pub const TEXTURE_WRAP_T: u32 = 0x2803;
    pub const LINEAR: u32 = 0x2601;
    pub const CLAMP_TO_EDGE: u32 = 0x812F;
    pub const RGBA: u32 = 0x1908;
    pub const RGBA8: u32 = 0x8058;
    pub const VERTEX_SHADER: u32 = 0x8B31;
    pub const FRAGMENT_SHADER: u32 = 0x8B30;
    pub const DEPTH_TEST: u32 = 0x0B71;
    pub const SCISSOR_TEST: u32 = 0x0C11;
    pub const BLEND: u32 = 0x0BE2;
    pub const POLYGON_OFFSET_FILL: u32 = 0x8037;
    pub const SRC_ALPHA: u32 = 0x0302;
    pub const ONE_MINUS_SRC_ALPHA: u32 = 0x0303;
    pub const ONE: u32 = 1;
    pub const COLOR_BUFFER_BIT: u32 = 0x4000;
    pub const DEPTH_BUFFER_BIT: u32 = 0x0100;
    pub const FRAMEBUFFER: u32 = 0x8D40;
    pub const RENDERBUFFER: u32 = 0x8D41;
    pub const COLOR_ATTACHMENT0: u32 = 0x8CE0;
    pub const DEPTH_ATTACHMENT: u32 = 0x8D00;
    pub const DEPTH_COMPONENT24: u32 = 0x81A6;
}
use consts as glow;

pub use wgw_scene::Rgba;

use crate::shaders::ShaderDef;

/// A compiled program. Its uniforms live in one uniform block (as naga writes WGSL uniforms for WebGL), held in a
/// buffer of its own; the setters write a uniform by name into it. Textures are plain sampler uniforms.
pub struct Program {
    pub program: ProgramId,
    def: &'static ShaderDef,
    block: Buffer,
    samplers: HashMap<&'static str, UniformLocation>,
}

impl Program {
    pub fn new(gl: &dyn Gl, def: &'static ShaderDef) -> Result<Self, String> {
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

    pub fn bind(&self, gl: &dyn Gl) {
        unsafe {
            gl.use_program(Some(self.program));
            gl.bind_buffer_base(glow::UNIFORM_BUFFER, 0, Some(self.block));
        }
    }

    /// Write a uniform's bytes into the block; uniforms the program doesn't have are skipped.
    fn set(&self, gl: &dyn Gl, name: &str, bytes: &[u8]) {
        if let Some(&(_, offset, size)) = self.def.uniforms.iter().find(|u| u.0 == name) {
            let n = bytes.len().min(size as usize);
            unsafe {
                gl.bind_buffer(glow::UNIFORM_BUFFER, Some(self.block));
                gl.buffer_sub_data_u8_slice(glow::UNIFORM_BUFFER, offset as i32, &bytes[..n]);
            }
        }
    }

    pub fn mat4(&self, gl: &dyn Gl, name: &str, m: &Mat4) {
        self.set(gl, name, bytemuck::cast_slice(&m.to_cols_array()));
    }

    pub fn f1(&self, gl: &dyn Gl, name: &str, x: f32) {
        self.set(gl, name, bytemuck::bytes_of(&x));
    }

    pub fn f2(&self, gl: &dyn Gl, name: &str, x: f32, y: f32) {
        self.set(gl, name, bytemuck::cast_slice(&[x, y]));
    }

    pub fn f4(&self, gl: &dyn Gl, name: &str, v: Rgba) {
        self.set(gl, name, bytemuck::cast_slice(&v));
    }

    /// An integer uniform, or a texture's unit.
    pub fn i1(&self, gl: &dyn Gl, name: &str, x: i32) {
        match self.samplers.get(name) {
            Some(l) => unsafe { gl.uniform_1_i32(Some(l), x) },
            None => self.set(gl, name, bytemuck::bytes_of(&x)),
        }
    }

    pub fn u1(&self, gl: &dyn Gl, name: &str, x: u32) {
        self.set(gl, name, bytemuck::bytes_of(&x));
    }
}

pub fn buffer(gl: &dyn Gl) -> Buffer {
    unsafe { gl.create_buffer().expect("create buffer") }
}

pub fn upload<T: bytemuck::Pod>(gl: &dyn Gl, target: u32, buffer: Buffer, data: &[T]) {
    unsafe {
        gl.bind_buffer(target, Some(buffer));
        gl.buffer_data_u8_slice(target, bytemuck::cast_slice(data), glow::DYNAMIC_DRAW);
    }
}

/// A float attribute read from `buffer`: `size` components at `offset` bytes, every `stride` bytes;
/// `divisor` 1 makes it per instance.
pub fn attribute(gl: &dyn Gl, buffer: Buffer, location: u32, size: i32, stride: i32, offset: i32, divisor: u32) {
    unsafe {
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(buffer));
        gl.enable_vertex_attrib_array(location);
        gl.vertex_attrib_pointer_f32(location, size, glow::FLOAT, false, stride, offset);
        gl.vertex_attrib_divisor(location, divisor);
    }
}

/// 256 texels running evenly through the color stops.
pub fn colormap(gl: &dyn Gl, texture: Option<Texture>, stops: &[Rgba]) -> Texture {
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
                        Some(&texels));
        texture
    }
}

pub fn bind_texture(gl: &dyn Gl, program: &Program, name: &str, texture: Texture) {
    unsafe {
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
    }
    program.i1(gl, name, 0);
}
