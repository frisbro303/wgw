//! The renderer's `Gl` on a page: WebGL2, through glow. Handles are indices into tables of glow's objects.

use std::cell::RefCell;

use glow::HasContext;
use wgw_render::gl::{self, Gl};

/// glow's objects of one kind, by the numeric handle the renderer holds.
struct Table<T>(RefCell<Vec<Option<T>>>);

impl<T: Copy> Table<T> {
    fn new() -> Self {
        Table(RefCell::new(Vec::new()))
    }

    fn add(&self, value: T) -> u32 {
        let mut v = self.0.borrow_mut();
        v.push(Some(value));
        v.len() as u32 - 1
    }

    fn get(&self, id: u32) -> T {
        self.0.borrow()[id as usize].expect("a deleted GL object")
    }

    fn take(&self, id: u32) -> T {
        self.0.borrow_mut()[id as usize].take().expect("a deleted GL object")
    }
}

pub struct WebGl {
    gl: glow::Context,
    vertex_arrays: Table<glow::VertexArray>,
    buffers: Table<glow::Buffer>,
    textures: Table<glow::Texture>,
    shaders: Table<glow::Shader>,
    programs: Table<glow::Program>,
    locations: RefCell<Vec<glow::UniformLocation>>,
    framebuffers: Table<glow::Framebuffer>,
    renderbuffers: Table<glow::Renderbuffer>,
}

impl WebGl {
    pub fn new(gl: glow::Context) -> Self {
        WebGl {
            gl,
            vertex_arrays: Table::new(),
            buffers: Table::new(),
            textures: Table::new(),
            shaders: Table::new(),
            programs: Table::new(),
            locations: RefCell::new(Vec::new()),
            framebuffers: Table::new(),
            renderbuffers: Table::new(),
        }
    }
}

impl Gl for WebGl {
    unsafe fn create_vertex_array(&self) -> Result<gl::VertexArray, String> {
        Ok(gl::VertexArray(self.vertex_arrays.add(self.gl.create_vertex_array()?)))
    }
    unsafe fn bind_vertex_array(&self, vao: Option<gl::VertexArray>) {
        self.gl.bind_vertex_array(vao.map(|v| self.vertex_arrays.get(v.0)))
    }
    unsafe fn delete_vertex_array(&self, vao: gl::VertexArray) {
        self.gl.delete_vertex_array(self.vertex_arrays.take(vao.0))
    }
    unsafe fn create_buffer(&self) -> Result<gl::Buffer, String> {
        Ok(gl::Buffer(self.buffers.add(self.gl.create_buffer()?)))
    }
    unsafe fn bind_buffer(&self, target: u32, buffer: Option<gl::Buffer>) {
        self.gl.bind_buffer(target, buffer.map(|b| self.buffers.get(b.0)))
    }
    unsafe fn bind_buffer_base(&self, target: u32, index: u32, buffer: Option<gl::Buffer>) {
        self.gl.bind_buffer_base(target, index, buffer.map(|b| self.buffers.get(b.0)))
    }
    unsafe fn delete_buffer(&self, buffer: gl::Buffer) {
        self.gl.delete_buffer(self.buffers.take(buffer.0))
    }
    unsafe fn buffer_data_u8_slice(&self, target: u32, data: &[u8], usage: u32) {
        self.gl.buffer_data_u8_slice(target, data, usage)
    }
    unsafe fn buffer_data_size(&self, target: u32, size: i32, usage: u32) {
        self.gl.buffer_data_size(target, size, usage)
    }
    unsafe fn buffer_sub_data_u8_slice(&self, target: u32, offset: i32, data: &[u8]) {
        self.gl.buffer_sub_data_u8_slice(target, offset, data)
    }
    unsafe fn enable_vertex_attrib_array(&self, index: u32) {
        self.gl.enable_vertex_attrib_array(index)
    }
    unsafe fn vertex_attrib_pointer_f32(&self, index: u32, size: i32, data_type: u32, normalized: bool, stride: i32, offset: i32) {
        self.gl.vertex_attrib_pointer_f32(index, size, data_type, normalized, stride, offset)
    }
    unsafe fn vertex_attrib_divisor(&self, index: u32, divisor: u32) {
        self.gl.vertex_attrib_divisor(index, divisor)
    }
    unsafe fn create_texture(&self) -> Result<gl::Texture, String> {
        Ok(gl::Texture(self.textures.add(self.gl.create_texture()?)))
    }
    unsafe fn bind_texture(&self, target: u32, texture: Option<gl::Texture>) {
        self.gl.bind_texture(target, texture.map(|t| self.textures.get(t.0)))
    }
    unsafe fn delete_texture(&self, texture: gl::Texture) {
        self.gl.delete_texture(self.textures.take(texture.0))
    }
    unsafe fn active_texture(&self, unit: u32) {
        self.gl.active_texture(unit)
    }
    unsafe fn tex_parameter_i32(&self, target: u32, parameter: u32, value: i32) {
        self.gl.tex_parameter_i32(target, parameter, value)
    }
    unsafe fn tex_image_2d(&self, target: u32, level: i32, internal_format: i32, width: i32, height: i32, border: i32,
                           format: u32, ty: u32, pixels: Option<&[u8]>) {
        self.gl.tex_image_2d(target, level, internal_format, width, height, border, format, ty, glow::PixelUnpackData::Slice(pixels))
    }
    unsafe fn create_shader(&self, kind: u32) -> Result<gl::Shader, String> {
        Ok(gl::Shader(self.shaders.add(self.gl.create_shader(kind)?)))
    }
    unsafe fn shader_source(&self, shader: gl::Shader, source: &str) {
        self.gl.shader_source(self.shaders.get(shader.0), source)
    }
    unsafe fn compile_shader(&self, shader: gl::Shader) {
        self.gl.compile_shader(self.shaders.get(shader.0))
    }
    unsafe fn get_shader_compile_status(&self, shader: gl::Shader) -> bool {
        self.gl.get_shader_compile_status(self.shaders.get(shader.0))
    }
    unsafe fn get_shader_info_log(&self, shader: gl::Shader) -> String {
        self.gl.get_shader_info_log(self.shaders.get(shader.0))
    }
    unsafe fn create_program(&self) -> Result<gl::ProgramId, String> {
        Ok(gl::ProgramId(self.programs.add(self.gl.create_program()?)))
    }
    unsafe fn attach_shader(&self, program: gl::ProgramId, shader: gl::Shader) {
        self.gl.attach_shader(self.programs.get(program.0), self.shaders.get(shader.0))
    }
    unsafe fn link_program(&self, program: gl::ProgramId) {
        self.gl.link_program(self.programs.get(program.0))
    }
    unsafe fn get_program_link_status(&self, program: gl::ProgramId) -> bool {
        self.gl.get_program_link_status(self.programs.get(program.0))
    }
    unsafe fn get_program_info_log(&self, program: gl::ProgramId) -> String {
        self.gl.get_program_info_log(self.programs.get(program.0))
    }
    unsafe fn use_program(&self, program: Option<gl::ProgramId>) {
        self.gl.use_program(program.map(|p| self.programs.get(p.0)))
    }
    unsafe fn get_uniform_location(&self, program: gl::ProgramId, name: &str) -> Option<gl::UniformLocation> {
        let location = self.gl.get_uniform_location(self.programs.get(program.0), name)?;
        let mut locations = self.locations.borrow_mut();
        locations.push(location);
        Some(gl::UniformLocation(locations.len() as u32 - 1))
    }
    unsafe fn uniform_1_i32(&self, location: Option<&gl::UniformLocation>, x: i32) {
        let locations = self.locations.borrow();
        self.gl.uniform_1_i32(location.map(|l| &locations[l.0 as usize]), x)
    }
    unsafe fn get_uniform_block_index(&self, program: gl::ProgramId, name: &str) -> Option<u32> {
        self.gl.get_uniform_block_index(self.programs.get(program.0), name)
    }
    unsafe fn uniform_block_binding(&self, program: gl::ProgramId, index: u32, binding: u32) {
        self.gl.uniform_block_binding(self.programs.get(program.0), index, binding)
    }
    unsafe fn enable(&self, parameter: u32) {
        self.gl.enable(parameter)
    }
    unsafe fn disable(&self, parameter: u32) {
        self.gl.disable(parameter)
    }
    unsafe fn viewport(&self, x: i32, y: i32, width: i32, height: i32) {
        self.gl.viewport(x, y, width, height)
    }
    unsafe fn scissor(&self, x: i32, y: i32, width: i32, height: i32) {
        self.gl.scissor(x, y, width, height)
    }
    unsafe fn clear_color(&self, r: f32, g: f32, b: f32, a: f32) {
        self.gl.clear_color(r, g, b, a)
    }
    unsafe fn clear(&self, mask: u32) {
        self.gl.clear(mask)
    }
    unsafe fn depth_mask(&self, write: bool) {
        self.gl.depth_mask(write)
    }
    unsafe fn polygon_offset(&self, factor: f32, units: f32) {
        self.gl.polygon_offset(factor, units)
    }
    unsafe fn blend_func_separate(&self, src_rgb: u32, dst_rgb: u32, src_alpha: u32, dst_alpha: u32) {
        self.gl.blend_func_separate(src_rgb, dst_rgb, src_alpha, dst_alpha)
    }
    unsafe fn draw_elements(&self, mode: u32, count: i32, element_type: u32, offset: i32) {
        self.gl.draw_elements(mode, count, element_type, offset)
    }
    unsafe fn draw_elements_instanced(&self, mode: u32, count: i32, element_type: u32, offset: i32, instances: i32) {
        self.gl.draw_elements_instanced(mode, count, element_type, offset, instances)
    }
    unsafe fn draw_arrays_instanced(&self, mode: u32, first: i32, count: i32, instances: i32) {
        self.gl.draw_arrays_instanced(mode, first, count, instances)
    }
    unsafe fn create_framebuffer(&self) -> Result<gl::Framebuffer, String> {
        Ok(gl::Framebuffer(self.framebuffers.add(self.gl.create_framebuffer()?)))
    }
    unsafe fn bind_framebuffer(&self, target: u32, framebuffer: Option<gl::Framebuffer>) {
        self.gl.bind_framebuffer(target, framebuffer.map(|f| self.framebuffers.get(f.0)))
    }
    unsafe fn create_renderbuffer(&self) -> Result<gl::Renderbuffer, String> {
        Ok(gl::Renderbuffer(self.renderbuffers.add(self.gl.create_renderbuffer()?)))
    }
    unsafe fn bind_renderbuffer(&self, target: u32, renderbuffer: Option<gl::Renderbuffer>) {
        self.gl.bind_renderbuffer(target, renderbuffer.map(|r| self.renderbuffers.get(r.0)))
    }
    unsafe fn renderbuffer_storage(&self, target: u32, format: u32, width: i32, height: i32) {
        self.gl.renderbuffer_storage(target, format, width, height)
    }
    unsafe fn framebuffer_renderbuffer(&self, target: u32, attachment: u32, renderbuffer_target: u32, renderbuffer: Option<gl::Renderbuffer>) {
        self.gl.framebuffer_renderbuffer(target, attachment, renderbuffer_target, renderbuffer.map(|r| self.renderbuffers.get(r.0)))
    }
    unsafe fn read_pixels(&self, x: i32, y: i32, width: i32, height: i32, format: u32, ty: u32, pixels: &mut [u8]) {
        self.gl.read_pixels(x, y, width, height, format, ty, glow::PixelPackData::Slice(Some(pixels)))
    }
}
