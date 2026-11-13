//! A GL on the CPU: wgw-render's `Gl`, implemented without a GPU, so the page's renderer draws stills with the same
//! calls and the same shaders. Programs run their WGSL (the source the page's GLSL is generated from) in the
//! interpreter in shader.rs. Triangles are clipped at the near plane and rasterized with 4 samples per pixel (as
//! the page's antialiased canvas), depth tested, blended, and resolved to an RGBA image.

pub mod shader;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use naga::{Module, ShaderStage};
use wgw_render::gl::{self, consts as c, Gl};
use wgw_render::shaders::{ShaderDef, ALL, WGSL};

use shader::{Image, Io};

/// The standard 4x multisample pattern, as offsets from a pixel's center.
const SAMPLES: [(f32, f32); 4] = [(-0.125, -0.375), (0.375, -0.125), (-0.375, 0.125), (0.125, 0.375)];

#[derive(Clone, Copy, Default)]
struct Attribute {
    enabled: bool,
    buffer: u32,
    size: usize,
    stride: usize,
    offset: usize,
    divisor: u32,
}

#[derive(Clone, Default)]
struct VertexArray {
    attributes: [Attribute; 8],
    elements: Option<u32>,
}

struct Texture {
    width: usize,
    height: usize,
    texels: Vec<[f32; 4]>,
}

/// A varying: its location, number of components, and whether it's flat (taken from the provoking vertex).
#[derive(Clone, Copy)]
struct Varying {
    location: u32,
    n: usize,
    flat: bool,
}

/// A linked program: its two compiled entry points, and its varyings as the vertex shader writes them.
struct Program {
    vs: shader::Program,
    fs: shader::Program,
    varyings: Vec<Varying>,
}

/// A vertex after its shader: its clip position and its varyings' bits, flattened.
#[derive(Clone)]
struct Processed {
    clip: [f64; 4],
    vary: Vec<u32>,
}

#[derive(Default)]
struct State {
    buffers: HashMap<u32, Vec<u8>>,
    array_buffer: u32,
    uniform_buffer: Option<u32>,
    vertex_arrays: HashMap<u32, VertexArray>,
    vertex_array: u32,
    textures: HashMap<u32, Texture>,
    unit: u32,
    units: HashMap<u32, u32>,
    shaders: HashMap<u32, (u32, String)>,
    program_shaders: HashMap<u32, Vec<u32>>,
    programs: HashMap<u32, Rc<Program>>,
    program: Option<u32>,
    locations: Vec<(u32, String)>,
    modules: HashMap<&'static str, Rc<Module>>,
    errors: HashMap<u32, String>,
    depth_test: bool,
    blend: bool,
    scissor_test: bool,
    polygon_offset: bool,
    offset: (f32, f32),
    depth_write: bool,
    viewport: (i32, i32, i32, i32),
    scissor: (i32, i32, i32, i32),
    clear_color: [f32; 4],
    framebuffer: Option<u32>,
    next: u32,
}

/// The CPU GL, drawing into a `width` by `height` image with 4 samples per pixel. A handle: clones share it, so
/// one can be given to the renderer and another kept to read the image.
#[derive(Clone)]
pub struct SoftGl {
    state: Rc<RefCell<State>>,
    width: usize,
    height: usize,
    color: Rc<RefCell<Vec<[f32; 4]>>>,
    depth: Rc<RefCell<Vec<f32>>>,
}

impl SoftGl {
    pub fn new(width: usize, height: usize) -> Self {
        let n = width * height * SAMPLES.len();
        SoftGl {
            state: Rc::new(RefCell::new(State { depth_write: true, viewport: (0, 0, width as i32, height as i32), ..Default::default() })),
            width,
            height,
            color: Rc::new(RefCell::new(vec![[0.0; 4]; n])),
            depth: Rc::new(RefCell::new(vec![1.0; n])),
        }
    }

    fn id(&self) -> u32 {
        let mut s = self.state.borrow_mut();
        s.next += 1;
        s.next
    }

    /// The image: its samples averaged, as straight (not premultiplied) RGBA bytes, top row first.
    pub fn image(&self) -> Vec<u8> {
        let color = self.color.borrow();
        let mut out = Vec::with_capacity(self.width * self.height * 4);
        for y in (0..self.height).rev() {
            for x in 0..self.width {
                let i = (y * self.width + x) * SAMPLES.len();
                let mut sum = [0.0f32; 4];
                for s in &color[i..i + SAMPLES.len()] {
                    for k in 0..4 {
                        sum[k] += s[k] / SAMPLES.len() as f32;
                    }
                }
                let a = sum[3];
                let un = |v: f32| if a > 0.0 { (v / a).clamp(0.0, 1.0) } else { 0.0 };
                out.extend_from_slice(&[un(sum[0]), un(sum[1]), un(sum[2]), a.clamp(0.0, 1.0)].map(|v| (v * 255.0).round() as u8));
            }
        }
        out
    }

    /// Errors from linking programs, if any.
    pub fn errors(&self) -> Vec<String> {
        self.state.borrow().errors.values().cloned().collect()
    }

    fn module(&self, name: &'static str) -> Result<Rc<Module>, String> {
        if let Some(m) = self.state.borrow().modules.get(name) {
            return Ok(m.clone());
        }
        let source = WGSL.iter().find(|w| w.0 == name).ok_or_else(|| format!("no shader {name}"))?.1;
        let module = Rc::new(naga::front::wgsl::parse_str(source).map_err(|e| e.emit_to_string(source))?);
        self.state.borrow_mut().modules.insert(name, module.clone());
        Ok(module)
    }

    fn link(&self, def: &ShaderDef) -> Result<Program, String> {
        let module = self.module(def.module)?;
        let info = shader::validate(&module)?;
        let entry = |name: &str, stage: ShaderStage| {
            module.entry_points.iter().position(|e| e.name == name && e.stage == stage).ok_or_else(|| format!("no entry point {name}"))
        };
        let vs = shader::compile(&module, &info, entry(def.entry_points.0, ShaderStage::Vertex)?)?;
        let fs = shader::compile(&module, &info, entry(def.entry_points.1, ShaderStage::Fragment)?)?;
        let varyings = vs.outputs.iter().filter_map(|o| match o {
            Io::Location { location, n, flat, .. } => Some(Varying { location: *location, n: *n, flat: *flat }),
            _ => None,
        }).collect();
        Ok(Program { vs, fs, varyings })
    }

    fn draw(&self, mode: u32, indices: Vec<u32>, instances: u32) {
        let (program, uniforms, texture, attributes, viewport) = {
            let s = self.state.borrow();
            if s.framebuffer.is_some() {
                return; // an offscreen pass (picking): a still has no use for it
            }
            let Some(program) = s.program.and_then(|p| s.programs.get(&p)).cloned() else { return };
            let vao = s.vertex_arrays.get(&s.vertex_array).cloned().unwrap_or_default();
            let texture = s.units.get(&0).and_then(|t| s.textures.get(t)).map(|t| (t.width, t.height, t.texels.clone()));
            let uniforms = s.uniform_buffer.and_then(|b| s.buffers.get(&b)).cloned().unwrap_or_default();
            (program, uniforms, texture, vao.attributes, s.viewport)
        };
        let image = texture.as_ref().map(|(w, h, t)| Image { width: *w, height: *h, texels: t });

        // Vertex shaders, once per vertex index and instance.
        let state = self.state.borrow();
        let mut regs = program.vs.registers(&uniforms);
        let mut fs_regs = program.fs.registers(&uniforms);
        for instance in 0..instances {
            let mut cache: HashMap<u32, Processed> = HashMap::new();
            let mut run = |index: u32| -> Processed {
                if let Some(p) = cache.get(&index) {
                    return p.clone();
                }
                for input in &program.vs.inputs {
                    match *input {
                        Io::VertexIndex(slot) => regs.0[slot] = index,
                        Io::InstanceIndex(slot) => regs.0[slot] = instance,
                        Io::Attribute { location, slot, n } => {
                            let a = &attributes[location as usize];
                            let element = if a.divisor == 0 { index } else { instance / a.divisor };
                            let stride = if a.stride == 0 { a.size * 4 } else { a.stride };
                            let data = state.buffers.get(&a.buffer).map_or(&[][..], |b| b);
                            for k in 0..n {
                                let at = a.offset + element as usize * stride + 4 * k;
                                regs.0[slot + k] = if a.enabled && k < a.size {
                                    data.get(at..at + 4).map_or(0, |w| u32::from_le_bytes(w.try_into().unwrap()))
                                } else {
                                    0
                                };
                            }
                        }
                        _ => {}
                    }
                }
                program.vs.run(&mut regs, image.as_ref(), &[]);
                let mut clip = [0.0; 4];
                let mut vary = Vec::new();
                for output in &program.vs.outputs {
                    match *output {
                        Io::Position(slot) => clip = std::array::from_fn(|k| f32::from_bits(regs.0[slot + k]) as f64),
                        Io::Location { slot, n, .. } => vary.extend_from_slice(&regs.0[slot..slot + n]),
                        _ => {}
                    }
                }
                let p = Processed { clip, vary };
                cache.insert(index, p.clone());
                p
            };
            match mode {
                c::TRIANGLES => {
                    for t in indices.chunks_exact(3) {
                        let tri = [run(t[0]), run(t[1]), run(t[2])];
                        for polygon in clip_near(&tri) {
                            let verts: Vec<([f64; 4], &Processed)> = polygon.iter().map(|p| (window(p, viewport), p)).collect();
                            // Flat varyings come from the provoking vertex, the last, as in GL.
                            self.raster(&program, &mut fs_regs, image.as_ref(), &verts, viewport, &state, &polygon[2].vary);
                        }
                    }
                }
                c::LINES => {
                    for l in indices.chunks_exact(2) {
                        let (a, b) = (run(l[0]), run(l[1]));
                        self.line(&program, &mut fs_regs, image.as_ref(), a, b, viewport, &state);
                    }
                }
                _ => {}
            }
        }
    }

    /// A 1-pixel line, as GL draws LINES: two triangles a pixel wide in window space.
    #[allow(clippy::too_many_arguments)]
    fn line(&self, program: &Program, regs: &mut shader::Registers, image: Option<&Image>, a: Processed, b: Processed,
            viewport: (i32, i32, i32, i32), state: &State) {
        if a.clip[3] <= 1e-6 || b.clip[3] <= 1e-6 {
            return;
        }
        let (wa, wb) = (window(&a, viewport), window(&b, viewport));
        let (dx, dy) = (wb[0] - wa[0], wb[1] - wa[1]);
        let len = (dx * dx + dy * dy).sqrt().max(1e-9);
        let (nx, ny) = (-dy / len * 0.5, dx / len * 0.5);
        let corner = |w: [f64; 4], s: f64| [w[0] + nx * s, w[1] + ny * s, w[2], w[3]];
        let quad = [(corner(wa, 1.0), &a), (corner(wb, 1.0), &b), (corner(wb, -1.0), &b), (corner(wa, -1.0), &a)];
        for t in [[0, 1, 2], [0, 2, 3]] {
            let verts: Vec<([f64; 4], &Processed)> = t.iter().map(|&i| quad[i]).collect();
            self.raster(program, regs, image, &verts, viewport, state, &b.vary);
        }
    }

    /// Rasterize a triangle given in window space ([x, y, depth, 1/w] per corner): each pixel whose samples it
    /// covers runs the fragment shader once, at its center, and writes the covered samples that pass the depth test.
    #[allow(clippy::too_many_arguments)]
    fn raster(&self, program: &Program, regs: &mut shader::Registers, image: Option<&Image>, v: &[([f64; 4], &Processed)],
              viewport: (i32, i32, i32, i32), state: &State, provoking: &[u32]) {
        let ([x0, y0, z0, q0], [x1, y1, z1, q1], [x2, y2, z2, q2]) = (v[0].0, v[1].0, v[2].0);
        let area = (x1 - x0) * (y2 - y0) - (x2 - x0) * (y1 - y0);
        if area.abs() < 1e-12 {
            return;
        }
        let (vx, vy, vw, vh) = viewport;
        let (mut lo_x, mut hi_x) = (vx, vx + vw);
        let (mut lo_y, mut hi_y) = (vy, vy + vh);
        if state.scissor_test {
            let (sx, sy, sw, sh) = state.scissor;
            lo_x = lo_x.max(sx);
            hi_x = hi_x.min(sx + sw);
            lo_y = lo_y.max(sy);
            hi_y = hi_y.min(sy + sh);
        }
        let bx0 = (x0.min(x1).min(x2).floor() as i32).max(lo_x).max(0);
        let bx1 = (x0.max(x1).max(x2).ceil() as i32).min(hi_x).min(self.width as i32);
        let by0 = (y0.min(y1).min(y2).floor() as i32).max(lo_y).max(0);
        let by1 = (y0.max(y1).max(y2).ceil() as i32).min(hi_y).min(self.height as i32);
        if bx0 >= bx1 || by0 >= by1 {
            return;
        }
        // Barycentric weights at a point, and depth (affine in window space).
        let bary = |x: f64, y: f64| {
            let w0 = ((x1 - x) * (y2 - y) - (x2 - x) * (y1 - y)) / area;
            let w1 = ((x2 - x) * (y0 - y) - (x0 - x) * (y2 - y)) / area;
            (w0, w1, 1.0 - w0 - w1)
        };
        let depth_bias = if state.polygon_offset {
            let dzdx = ((z1 - z0) * (y2 - y0) - (z2 - z0) * (y1 - y0)) / area;
            let dzdy = ((x1 - x0) * (z2 - z0) - (x2 - x0) * (z1 - z0)) / area;
            state.offset.0 as f64 * dzdx.abs().max(dzdy.abs()) + state.offset.1 as f64 / (1u32 << 24) as f64
        } else {
            0.0
        };
        // Varyings at a point, perspective-correct; flat ones from the provoking vertex.
        let flat: Vec<bool> = program.varyings.iter().flat_map(|v| std::iter::repeat_n(v.flat, v.n)).collect();
        let vary_at = |x: f64, y: f64, out: &mut Vec<u32>| -> f64 {
            let (w0, w1, w2) = bary(x, y);
            let q = w0 * q0 + w1 * q1 + w2 * q2;
            out.clear();
            for (i, &is_flat) in flat.iter().enumerate() {
                out.push(if is_flat {
                    provoking[i]
                } else {
                    let at = |p: &Processed| f32::from_bits(p.vary[i]) as f64;
                    (((w0 * q0 * at(v[0].1) + w1 * q1 * at(v[1].1) + w2 * q2 * at(v[2].1)) / q) as f32).to_bits()
                });
            }
            q
        };
        // Where each varying's components start, by location, and the fragment shader's inputs.
        let mut offsets = HashMap::new();
        let mut at = 0;
        for v in &program.varyings {
            offsets.insert(v.location, at);
            at += v.n;
        }
        let fs = &program.fs;
        let (mut here, mut right, mut up) = (Vec::new(), Vec::new(), Vec::new());
        let mut gradients = vec![0f32; if fs.derivatives { 2 * fs.input_len } else { 0 }];
        let mut color = self.color.borrow_mut();
        let mut depth = self.depth.borrow_mut();
        for py in by0..by1 {
            for px in bx0..bx1 {
                let (cx, cy) = (px as f64 + 0.5, py as f64 + 0.5);
                let mut covered = [false; 4];
                let mut sample_depth = [0.0f64; 4];
                let base = (py as usize * self.width + px as usize) * SAMPLES.len();
                for (s, (ox, oy)) in SAMPLES.iter().enumerate() {
                    let (w0, w1, w2) = bary(cx + *ox as f64, cy + *oy as f64);
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    let z = w0 * z0 + w1 * z1 + w2 * z2 + depth_bias;
                    if !(0.0..=1.0).contains(&z) {
                        continue;
                    }
                    if state.depth_test && z >= depth[base + s] as f64 {
                        continue;
                    }
                    covered[s] = true;
                    sample_depth[s] = z;
                }
                if !covered.iter().any(|&c| c) {
                    continue;
                }
                let q = vary_at(cx, cy, &mut here);
                let (w0, w1, w2) = bary(cx, cy);
                let frag = [cx, cy, w0 * z0 + w1 * z1 + w2 * z2, q];
                for input in &fs.inputs {
                    match *input {
                        Io::Position(slot) => (0..4).for_each(|k| regs.0[slot + k] = (frag[k] as f32).to_bits()),
                        Io::Location { location, slot, n, .. } => {
                            if let Some(&o) = offsets.get(&location) {
                                regs.0[slot..slot + n].copy_from_slice(&here[o..o + n]);
                            }
                        }
                        _ => {}
                    }
                }
                if fs.derivatives {
                    vary_at(cx + 1.0, cy, &mut right);
                    vary_at(cx, cy + 1.0, &mut up);
                    for input in &fs.inputs {
                        if let Io::Location { location, slot, n, .. } = *input {
                            let Some(&o) = offsets.get(&location) else { continue };
                            for k in 0..n {
                                let d = |other: &Vec<u32>| f32::from_bits(other[o + k]) - f32::from_bits(here[o + k]);
                                gradients[slot - fs.input_base + k] = d(&right);
                                gradients[fs.input_len + slot - fs.input_base + k] = d(&up);
                            }
                        }
                    }
                }
                if !fs.run(regs, image, &gradients) {
                    continue;
                }
                let Some(Io::Location { slot, .. }) = fs.outputs.first().cloned() else { continue };
                let rgba: Vec<f32> = (0..4).map(|k| f32::from_bits(regs.0[slot + k])).collect();
                let src = [rgba[0], rgba[1], rgba[2], rgba[3]];
                for s in 0..SAMPLES.len() {
                    if !covered[s] {
                        continue;
                    }
                    let dst = &mut color[base + s];
                    *dst = if state.blend {
                        let a = src[3];
                        [src[0] * a + dst[0] * (1.0 - a), src[1] * a + dst[1] * (1.0 - a), src[2] * a + dst[2] * (1.0 - a), a + dst[3] * (1.0 - a)]
                    } else {
                        src
                    };
                    if state.depth_write {
                        depth[base + s] = sample_depth[s] as f32;
                    }
                }
            }
        }
    }
}

/// A clip-space vertex in window space: [x, y, depth in [0, 1], 1/w].
fn window(p: &Processed, (vx, vy, vw, vh): (i32, i32, i32, i32)) -> [f64; 4] {
    let [x, y, z, w] = p.clip;
    [vx as f64 + (x / w + 1.0) / 2.0 * vw as f64, vy as f64 + (y / w + 1.0) / 2.0 * vh as f64, (z / w + 1.0) / 2.0, 1.0 / w]
}

/// A triangle clipped to the near plane (z >= -w), as triangles. Varyings are interpolated linearly in clip space.
fn clip_near(tri: &[Processed; 3]) -> Vec<[Processed; 3]> {
    let inside = |p: &Processed| p.clip[2] + p.clip[3] >= 0.0;
    if tri.iter().all(inside) {
        return vec![tri.clone()];
    }
    if !tri.iter().any(inside) {
        return Vec::new();
    }
    let mut polygon: Vec<Processed> = Vec::new();
    for i in 0..3 {
        let (a, b) = (&tri[i], &tri[(i + 1) % 3]);
        if inside(a) {
            polygon.push(a.clone());
        }
        if inside(a) != inside(b) {
            let (da, db) = (a.clip[2] + a.clip[3], b.clip[2] + b.clip[3]);
            let t = da / (da - db);
            let lerp = |x: f64, y: f64| x + (y - x) * t;
            polygon.push(Processed {
                clip: std::array::from_fn(|k| lerp(a.clip[k], b.clip[k])),
                vary: a.vary.iter().zip(&b.vary).map(|(x, y)| (lerp(f32::from_bits(*x) as f64, f32::from_bits(*y) as f64) as f32).to_bits()).collect(),
            });
        }
    }
    // Keep the original last vertex last where it survived, so flat varyings come from it.
    (1..polygon.len() - 1).map(|i| [polygon[0].clone(), polygon[i].clone(), polygon[i + 1].clone()]).collect()
}

fn indices_from(bytes: &[u8], offset: i32, count: i32) -> Vec<u32> {
    let at = offset.max(0) as usize;
    (0..count.max(0) as usize).map(|i| bytes.get(at + 4 * i..at + 4 * i + 4).map_or(0, |w| u32::from_le_bytes(w.try_into().unwrap()))).collect()
}

impl Gl for SoftGl {
    unsafe fn create_vertex_array(&self) -> Result<gl::VertexArray, String> {
        let id = self.id();
        self.state.borrow_mut().vertex_arrays.insert(id, VertexArray::default());
        Ok(gl::VertexArray(id))
    }
    unsafe fn bind_vertex_array(&self, vao: Option<gl::VertexArray>) {
        self.state.borrow_mut().vertex_array = vao.map_or(0, |v| v.0);
    }
    unsafe fn delete_vertex_array(&self, vao: gl::VertexArray) {
        self.state.borrow_mut().vertex_arrays.remove(&vao.0);
    }
    unsafe fn create_buffer(&self) -> Result<gl::Buffer, String> {
        let id = self.id();
        self.state.borrow_mut().buffers.insert(id, Vec::new());
        Ok(gl::Buffer(id))
    }
    unsafe fn bind_buffer(&self, target: u32, buffer: Option<gl::Buffer>) {
        let mut s = self.state.borrow_mut();
        let id = buffer.map_or(0, |b| b.0);
        match target {
            c::ARRAY_BUFFER => s.array_buffer = id,
            c::ELEMENT_ARRAY_BUFFER => {
                let vao = s.vertex_array;
                s.vertex_arrays.entry(vao).or_default().elements = buffer.map(|b| b.0);
            }
            c::UNIFORM_BUFFER => s.uniform_buffer = buffer.map(|b| b.0),
            _ => {}
        }
    }
    unsafe fn bind_buffer_base(&self, _target: u32, _index: u32, buffer: Option<gl::Buffer>) {
        self.state.borrow_mut().uniform_buffer = buffer.map(|b| b.0);
    }
    unsafe fn delete_buffer(&self, buffer: gl::Buffer) {
        self.state.borrow_mut().buffers.remove(&buffer.0);
    }
    unsafe fn buffer_data_u8_slice(&self, target: u32, data: &[u8], _usage: u32) {
        if let Some(b) = self.bound(target) {
            self.state.borrow_mut().buffers.insert(b, data.to_vec());
        }
    }
    unsafe fn buffer_data_size(&self, target: u32, size: i32, _usage: u32) {
        if let Some(b) = self.bound(target) {
            self.state.borrow_mut().buffers.insert(b, vec![0; size.max(0) as usize]);
        }
    }
    unsafe fn buffer_sub_data_u8_slice(&self, target: u32, offset: i32, data: &[u8]) {
        if let Some(b) = self.bound(target) {
            let mut s = self.state.borrow_mut();
            let buffer = s.buffers.entry(b).or_default();
            let end = offset as usize + data.len();
            if buffer.len() < end {
                buffer.resize(end, 0);
            }
            buffer[offset as usize..end].copy_from_slice(data);
        }
    }
    unsafe fn enable_vertex_attrib_array(&self, index: u32) {
        let mut s = self.state.borrow_mut();
        let vao = s.vertex_array;
        s.vertex_arrays.entry(vao).or_default().attributes[index as usize].enabled = true;
    }
    unsafe fn vertex_attrib_pointer_f32(&self, index: u32, size: i32, _data_type: u32, _normalized: bool, stride: i32, offset: i32) {
        let mut s = self.state.borrow_mut();
        let (vao, buffer) = (s.vertex_array, s.array_buffer);
        let a = &mut s.vertex_arrays.entry(vao).or_default().attributes[index as usize];
        a.buffer = buffer;
        a.size = size as usize;
        a.stride = stride as usize;
        a.offset = offset as usize;
    }
    unsafe fn vertex_attrib_divisor(&self, index: u32, divisor: u32) {
        let mut s = self.state.borrow_mut();
        let vao = s.vertex_array;
        s.vertex_arrays.entry(vao).or_default().attributes[index as usize].divisor = divisor;
    }
    unsafe fn create_texture(&self) -> Result<gl::Texture, String> {
        let id = self.id();
        self.state.borrow_mut().textures.insert(id, Texture { width: 0, height: 0, texels: Vec::new() });
        Ok(gl::Texture(id))
    }
    unsafe fn bind_texture(&self, _target: u32, texture: Option<gl::Texture>) {
        let mut s = self.state.borrow_mut();
        let unit = s.unit;
        match texture {
            Some(t) => s.units.insert(unit, t.0),
            None => s.units.remove(&unit),
        };
    }
    unsafe fn delete_texture(&self, texture: gl::Texture) {
        self.state.borrow_mut().textures.remove(&texture.0);
    }
    unsafe fn active_texture(&self, unit: u32) {
        self.state.borrow_mut().unit = unit - c::TEXTURE0;
    }
    unsafe fn tex_parameter_i32(&self, _target: u32, _parameter: u32, _value: i32) {} // always linear, clamped
    unsafe fn tex_image_2d(&self, _target: u32, _level: i32, _internal_format: i32, width: i32, height: i32, _border: i32,
                           _format: u32, _ty: u32, pixels: Option<&[u8]>) {
        let mut s = self.state.borrow_mut();
        let unit = s.unit;
        let Some(&t) = s.units.get(&unit) else { return };
        let texels = pixels.unwrap_or(&[]).chunks_exact(4).map(|p| [p[0], p[1], p[2], p[3]].map(|b| b as f32 / 255.0)).collect();
        s.textures.insert(t, Texture { width: width as usize, height: height as usize, texels });
    }
    unsafe fn create_shader(&self, kind: u32) -> Result<gl::Shader, String> {
        let id = self.id();
        self.state.borrow_mut().shaders.insert(id, (kind, String::new()));
        Ok(gl::Shader(id))
    }
    unsafe fn shader_source(&self, shader: gl::Shader, source: &str) {
        if let Some(s) = self.state.borrow_mut().shaders.get_mut(&shader.0) {
            s.1 = source.to_string();
        }
    }
    unsafe fn compile_shader(&self, _shader: gl::Shader) {}
    unsafe fn get_shader_compile_status(&self, _shader: gl::Shader) -> bool {
        true
    }
    unsafe fn get_shader_info_log(&self, _shader: gl::Shader) -> String {
        String::new()
    }
    unsafe fn create_program(&self) -> Result<gl::ProgramId, String> {
        Ok(gl::ProgramId(self.id()))
    }
    unsafe fn attach_shader(&self, program: gl::ProgramId, shader: gl::Shader) {
        self.state.borrow_mut().program_shaders.entry(program.0).or_default().push(shader.0);
    }
    unsafe fn link_program(&self, program: gl::ProgramId) {
        // The program is the one whose generated GLSL these are; it runs from its WGSL.
        let sources: Vec<(u32, String)> = {
            let s = self.state.borrow();
            s.program_shaders.get(&program.0).into_iter().flatten().filter_map(|sh| s.shaders.get(sh).cloned()).collect()
        };
        let source = |kind: u32| sources.iter().find(|s| s.0 == kind).map(|s| s.1.as_str()).unwrap_or("");
        let def = ALL.iter().find(|d| d.vertex == source(c::VERTEX_SHADER) && d.fragment == source(c::FRAGMENT_SHADER));
        let linked = match def {
            Some(def) => self.link(def),
            None => Err("a program that isn't one of wgw's".to_string()),
        };
        let mut s = self.state.borrow_mut();
        match linked {
            Ok(p) => {
                s.programs.insert(program.0, Rc::new(p));
            }
            Err(e) => {
                s.errors.insert(program.0, e);
            }
        }
    }
    unsafe fn get_program_link_status(&self, program: gl::ProgramId) -> bool {
        !self.state.borrow().errors.contains_key(&program.0)
    }
    unsafe fn get_program_info_log(&self, program: gl::ProgramId) -> String {
        self.state.borrow().errors.get(&program.0).cloned().unwrap_or_default()
    }
    unsafe fn use_program(&self, program: Option<gl::ProgramId>) {
        self.state.borrow_mut().program = program.map(|p| p.0);
    }
    unsafe fn get_uniform_location(&self, program: gl::ProgramId, name: &str) -> Option<gl::UniformLocation> {
        let mut s = self.state.borrow_mut();
        s.locations.push((program.0, name.to_string()));
        Some(gl::UniformLocation(s.locations.len() as u32 - 1))
    }
    unsafe fn uniform_1_i32(&self, _location: Option<&gl::UniformLocation>, _x: i32) {} // textures are on unit 0
    unsafe fn get_uniform_block_index(&self, _program: gl::ProgramId, _name: &str) -> Option<u32> {
        Some(0)
    }
    unsafe fn uniform_block_binding(&self, _program: gl::ProgramId, _index: u32, _binding: u32) {}
    unsafe fn enable(&self, parameter: u32) {
        self.set_flag(parameter, true);
    }
    unsafe fn disable(&self, parameter: u32) {
        self.set_flag(parameter, false);
    }
    unsafe fn viewport(&self, x: i32, y: i32, width: i32, height: i32) {
        self.state.borrow_mut().viewport = (x, y, width, height);
    }
    unsafe fn scissor(&self, x: i32, y: i32, width: i32, height: i32) {
        self.state.borrow_mut().scissor = (x, y, width, height);
    }
    unsafe fn clear_color(&self, r: f32, g: f32, b: f32, a: f32) {
        self.state.borrow_mut().clear_color = [r, g, b, a];
    }
    unsafe fn clear(&self, mask: u32) {
        if self.state.borrow().framebuffer.is_some() {
            return;
        }
        if mask & c::COLOR_BUFFER_BIT != 0 {
            let clear = self.state.borrow().clear_color;
            self.color.borrow_mut().fill(clear);
        }
        if mask & c::DEPTH_BUFFER_BIT != 0 {
            self.depth.borrow_mut().fill(1.0);
        }
    }
    unsafe fn depth_mask(&self, write: bool) {
        self.state.borrow_mut().depth_write = write;
    }
    unsafe fn polygon_offset(&self, factor: f32, units: f32) {
        self.state.borrow_mut().offset = (factor, units);
    }
    unsafe fn blend_func_separate(&self, _src_rgb: u32, _dst_rgb: u32, _src_alpha: u32, _dst_alpha: u32) {} // wgw only blends "over"
    unsafe fn draw_elements(&self, mode: u32, count: i32, _element_type: u32, offset: i32) {
        self.draw_elements_instanced(mode, count, _element_type, offset, 1);
    }
    unsafe fn draw_elements_instanced(&self, mode: u32, count: i32, _element_type: u32, offset: i32, instances: i32) {
        let indices = {
            let s = self.state.borrow();
            let elements = s.vertex_arrays.get(&s.vertex_array).and_then(|v| v.elements);
            let bytes = elements.and_then(|e| s.buffers.get(&e)).map_or(&[][..], |b| b);
            indices_from(bytes, offset, count)
        };
        self.draw(mode, indices, instances.max(0) as u32);
    }
    unsafe fn draw_arrays_instanced(&self, mode: u32, first: i32, count: i32, instances: i32) {
        self.draw(mode, (first as u32..(first + count) as u32).collect(), instances.max(0) as u32);
    }
    unsafe fn create_framebuffer(&self) -> Result<gl::Framebuffer, String> {
        Ok(gl::Framebuffer(self.id()))
    }
    unsafe fn bind_framebuffer(&self, _target: u32, framebuffer: Option<gl::Framebuffer>) {
        self.state.borrow_mut().framebuffer = framebuffer.map(|f| f.0);
    }
    unsafe fn create_renderbuffer(&self) -> Result<gl::Renderbuffer, String> {
        Ok(gl::Renderbuffer(self.id()))
    }
    unsafe fn bind_renderbuffer(&self, _target: u32, _renderbuffer: Option<gl::Renderbuffer>) {}
    unsafe fn renderbuffer_storage(&self, _target: u32, _format: u32, _width: i32, _height: i32) {}
    unsafe fn framebuffer_renderbuffer(&self, _target: u32, _attachment: u32, _rb_target: u32, _rb: Option<gl::Renderbuffer>) {}
    unsafe fn read_pixels(&self, x: i32, y: i32, width: i32, height: i32, _format: u32, _ty: u32, pixels: &mut [u8]) {
        let image = self.image();
        for row in 0..height.max(0) as usize {
            for col in 0..width.max(0) as usize {
                let (px, py) = (x as usize + col, self.height - 1 - (y as usize + row));
                let (from, to) = ((py * self.width + px) * 4, (row * width as usize + col) * 4);
                if let (Some(src), Some(dst)) = (image.get(from..from + 4), pixels.get_mut(to..to + 4)) {
                    dst.copy_from_slice(src);
                }
            }
        }
    }
}

impl SoftGl {
    fn bound(&self, target: u32) -> Option<u32> {
        let s = self.state.borrow();
        match target {
            c::ARRAY_BUFFER => Some(s.array_buffer).filter(|&b| b != 0),
            c::ELEMENT_ARRAY_BUFFER => s.vertex_arrays.get(&s.vertex_array).and_then(|v| v.elements),
            c::UNIFORM_BUFFER => s.uniform_buffer,
            _ => None,
        }
    }

    fn set_flag(&self, parameter: u32, on: bool) {
        let mut s = self.state.borrow_mut();
        match parameter {
            c::DEPTH_TEST => s.depth_test = on,
            c::BLEND => s.blend = on,
            c::SCISSOR_TEST => s.scissor_test = on,
            c::POLYGON_OFFSET_FILL => s.polygon_offset = on,
            _ => {}
        }
    }
}
