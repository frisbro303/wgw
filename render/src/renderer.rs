//! The renderer: a scene of layers, its panes and cameras, input, picking and animation, drawn through a `Gl`.
//! On a page it's WebGL (the wgw crate's Viewer wraps it); for a still, the CPU renderer.

use std::rc::Rc;

use glam::{Vec2, Vec4, Vec4Swizzles};
use wgw_scene::array::{Data, NdArray};
use wgw_scene::camera::{self, Camera, Fit, Up};
use wgw_scene::mesh_file::MeshFile;

use crate::gl::{self, consts as glow, Gl, Rgba};
use crate::layer::{self, Kind, Layer, LayerData, Style, View};
use crate::shaders::Shaders;

const TURN_SPEED: f32 = 0.008; // radians per CSS pixel
const CLICK_SLOP: f32 = 6.0; // CSS pixels a press can move and still be a click
const GLIDE: f32 = 280.0; // ms for the glide after a drag to slow by a factor e

struct Animation {
    start: f64, // -inf: show the last frame
    fps: f64,
    looped: bool,
}

struct PickTarget {
    framebuffer: gl::Framebuffer,
    color: gl::Renderbuffer,
    depth: gl::Renderbuffer,
    size: (i32, i32),
}

pub struct Renderer {
    gl: Box<dyn Gl>,
    shaders: Rc<Shaders>,
    layers: Vec<Layer>,
    style: Style,
    /// The starting camera, which each pane's camera begins as.
    camera: Camera,
    /// Each pane's own camera, so dragging in one turns only it.
    cameras: Vec<Camera>,
    /// The pane the current press began in.
    active: usize,
    /// Each pane's fit: layers with `pane` = 1 are drawn in a second pane, beside the first, fitted on their own.
    fits: Vec<Option<Fit>>,
    /// The panes' relative widths, when they aren't equal.
    pane_widths: Vec<f32>,
    fitted: u64,
    animation: Animation,
    frame: usize,
    pointers: Vec<(i32, f32, f32)>,
    /// How far the current single-finger press has moved, while it may still be a click.
    press: Option<f32>,
    last_move: f64,
    last_frame: f64,
    size: (u32, u32),
    dpr: f32,
    pick_target: Option<PickTarget>,
    reduced_motion: bool,
    errors: Vec<String>,
    version: u64,
}

impl Renderer {
    pub fn new(gl: Box<dyn Gl>) -> Result<Renderer, String> {
        let shaders = Rc::new(Shaders::new(gl.as_ref())?);
        Ok(Renderer::with_shaders(gl, shaders))
    }

    /// A renderer using shaders already compiled on the same GL: renderers sharing one context (a page's figures)
    /// share them too. Each draw sets the program and state it needs, so they can take turns on the context.
    pub fn with_shaders(gl: Box<dyn Gl>, shaders: Rc<Shaders>) -> Renderer {
        let red = [0.72, 0.25, 0.18, 1.0];
        Renderer {
            gl,
            shaders,
            layers: Vec::new(),
            style: Style { colormap: vec![[0.85, 0.84, 0.81, 1.0], red], accent: red, edge: [0.0, 0.0, 0.0, 0.3], grid: false },
            camera: Camera::new(),
            cameras: Vec::new(),
            active: 0,
            fits: Vec::new(),
            pane_widths: Vec::new(),
            fitted: 0,
            animation: Animation { start: f64::NEG_INFINITY, fps: 30.0, looped: false },
            frame: 0,
            pointers: Vec::new(),
            press: None,
            last_move: 0.0,
            last_frame: 0.0,
            size: (1, 1),
            dpr: 1.0,
            pick_target: None,
            reduced_motion: false,
            errors: Vec::new(),
            version: 0,
        }
    }

    /// Carry on with a new GL, after the old one's context was lost: what was made on it died with it, so it's
    /// forgotten (not deleted), and the next frame uploads every layer again. The scene and cameras are kept.
    pub fn restore(&mut self, gl: Box<dyn Gl>, shaders: Rc<Shaders>) {
        self.gl = gl;
        self.shaders = shaders;
        self.pick_target = None;
        for layer in &mut self.layers {
            layer.primitive = layer::primitive(layer.data.kind);
        }
    }

    // ---- the scene: layers patched by name

    /// Create a layer, or change its kind ("mesh", "points", "lines", "vectors"); "" keeps an existing layer's
    /// kind, and makes a new one points.
    pub fn set_layer(&mut self, name: &str, kind: &str) -> Result<(), String> {
        let parsed = Kind::parse(kind);
        if !kind.is_empty() && parsed.is_none() {
            return Err(format!("unknown layer type {kind:?}"));
        }
        match self.layers.iter_mut().find(|l| l.name == name) {
            Some(layer) => {
                if let Some(kind) = parsed.filter(|k| *k != layer.data.kind) {
                    layer.primitive.destroy(self.gl.as_ref());
                    layer.primitive = layer::primitive(kind);
                    layer.data.kind = kind;
                }
            }
            None => {
                let kind = parsed.unwrap_or(Kind::Points);
                self.layers.push(Layer { name: name.into(), data: LayerData::new(kind), primitive: layer::primitive(kind) });
            }
        }
        self.touch(name);
        Ok(())
    }

    pub fn remove_layer(&mut self, name: &str) {
        if let Some(i) = self.layers.iter().position(|l| l.name == name) {
            let mut layer = self.layers.remove(i);
            layer.primitive.destroy(self.gl.as_ref());
            self.version += 1;
        }
    }

    pub fn set_f32(&mut self, layer: &str, key: &str, data: Vec<f32>, shape: Vec<u32>) -> Result<(), String> {
        self.set_array(layer, key, Data::F32(data), shape)
    }

    pub fn set_u32(&mut self, layer: &str, key: &str, data: Vec<u32>, shape: Vec<u32>) -> Result<(), String> {
        self.set_array(layer, key, Data::U32(data), shape)
    }

    pub fn remove_array(&mut self, layer: &str, key: &str) {
        if let Some(l) = self.layer_mut(layer) {
            l.data.arrays.remove(key);
        }
        self.touch(layer);
    }

    /// Back to the defaults for every parameter of a layer, before it's given its new ones.
    pub fn clear_params(&mut self, layer: &str) {
        if let Some(l) = self.layer_mut(layer) {
            l.data.numbers.clear();
            l.data.colors.clear();
        }
        self.touch(layer);
    }

    /// Show a parsed mesh file as the mesh layer `name`.
    pub fn set_mesh_file(&mut self, name: &str, file: &MeshFile) -> Result<(), String> {
        let (vertices, vshape) = file.vertices()?;
        let (faces, fshape) = file.faces()?;
        self.set_layer(name, "mesh")?;
        self.set_array(name, "vertices", Data::F32(vertices), vshape.into_iter().map(|s| s as u32).collect())?;
        self.set_array(name, "faces", Data::U32(faces), fshape.into_iter().map(|s| s as u32).collect())
    }

    /// A number parameter; booleans are 0 and 1.
    pub fn set_number(&mut self, layer: &str, key: &str, value: f64) {
        if let Some(l) = self.layer_mut(layer) {
            l.data.numbers.insert(key.into(), value);
        }
        self.touch(layer);
    }

    /// A color parameter: one or more RGBA colors, 4 numbers in [0, 1] each.
    pub fn set_colors(&mut self, layer: &str, key: &str, rgba: Vec<f32>) {
        let colors = rgba.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect();
        if let Some(l) = self.layer_mut(layer) {
            l.data.colors.insert(key.into(), colors);
        }
        self.touch(layer);
    }

    pub fn has_layer(&self, name: &str) -> bool {
        self.layers.iter().any(|l| l.name == name)
    }

    pub fn layer_name(&self, i: usize) -> Option<String> {
        self.layers.get(i).map(|l| l.name.clone())
    }

    /// Errors from layers that couldn't be drawn since the last call, one per line.
    pub fn take_errors(&mut self) -> String {
        std::mem::take(&mut self.errors).join("\n")
    }

    // ---- look and feel

    /// The page's colors, as flat RGBA: the colormap's stops, the accent for layers without values, the edge color.
    pub fn set_style(&mut self, colormap: Vec<f32>, accent: Vec<f32>, edge: Vec<f32>) {
        let rgba = |v: &[f32]| -> Rgba { [v[0], v[1], v[2], v[3]] };
        let stops: Vec<Rgba> = colormap.chunks_exact(4).map(rgba).collect();
        if !stops.is_empty() {
            self.style.colormap = stops;
        }
        if accent.len() == 4 {
            self.style.accent = rgba(&accent);
        }
        if edge.len() == 4 {
            self.style.edge = rgba(&edge);
        }
        let names: Vec<String> = self.layers.iter().map(|l| l.name.clone()).collect();
        names.iter().for_each(|n| self.touch(n));
    }

    /// Show meshes' triangle grids (unless a layer sets its own `edges`).
    pub fn set_grid(&mut self, on: bool) {
        self.style.grid = on;
        let names: Vec<String> = self.layers.iter().map(|l| l.name.clone()).collect();
        names.iter().for_each(|n| self.touch(n));
    }

    /// The up axis ("x", "y", "z"), the starting view (turn and tilt, degrees), and whether to keep motion down.
    /// `scale` magnifies the starting view: 2 shows the scene twice as large (it fits the view at 1).
    pub fn set_options(&mut self, up: &str, yaw: f32, pitch: f32, scale: f32, reduced_motion: bool) {
        self.camera.up = Up::parse(up);
        self.camera.home = (yaw.to_radians(), pitch.to_radians());
        self.camera.home_zoom = (1.0 / scale.max(0.25)).clamp(0.2, 4.0);
        self.camera.reset();
        self.cameras.clear();
        self.reduced_motion = reduced_motion;
    }

    /// Play the scene's frames from now at fps, once or looping; not animated, jump to the last frame. A loop that's
    /// already playing carries on from where it is, so new frames (a slider dragged) don't restart it each time.
    pub fn play(&mut self, now: f64, fps: f64, looped: bool, animate: bool) {
        let animate = looped || (animate && !self.reduced_motion);
        let playing = self.animation.looped && looped && self.animation.start.is_finite();
        let start = if playing { self.animation.start } else if animate { now } else { f64::NEG_INFINITY };
        self.animation = Animation { start, fps: fps.max(1.0), looped };
    }

    // ---- input, in CSS pixels from the canvas's top left

    pub fn pointer_down(&mut self, id: i32, x: f32, y: f32, now: f64) {
        self.pointers.retain(|p| p.0 != id);
        self.pointers.push((id, x, y));
        self.press = (self.pointers.len() == 1).then_some(0.0);
        if self.pointers.len() == 1 {
            let px = (x * self.dpr) as i32;
            self.active = (0..self.cameras.len()).find(|&i| { let (x0, w) = self.pane_rect(i); px >= x0 && px < x0 + w }).unwrap_or(0);
        }
        self.cam().velocity = (0.0, 0.0);
        self.last_move = now;
    }

    /// Returns whether the view changed (a drag turned it or a pinch zoomed it).
    pub fn pointer_move(&mut self, id: i32, x: f32, y: f32, now: f64) -> bool {
        let Some(i) = self.pointers.iter().position(|p| p.0 == id) else { return false };
        let (_, px, py) = self.pointers[i];
        let (dx, dy) = (x - px, y - py);
        if self.pointers.len() == 1 {
            let dt = (now - self.last_move).max(1.0) as f32;
            let blend = (dt / 40.0).min(1.0); // smoothed velocity, in radians per ms
            let camera = self.cam();
            camera.turn(-dx * TURN_SPEED, dy * TURN_SPEED);
            let v = camera.velocity;
            camera.velocity =
                (v.0 * (1.0 - blend) - dx * TURN_SPEED / dt * blend, v.1 * (1.0 - blend) + dy * TURN_SPEED / dt * blend);
            if let Some(moved) = &mut self.press {
                *moved += dx.abs() + dy.abs();
            }
        } else if self.pointers.len() == 2 {
            let (_, ox, oy) = self.pointers[1 - i];
            let (before, after) = ((px - ox).hypot(py - oy), (x - ox).hypot(y - oy));
            if after > 0.0 {
                self.cam().zoom_by(before / after);
            }
        }
        self.pointers[i] = (id, x, y);
        self.last_move = now;
        true
    }

    /// Returns whether the press was a click (it barely moved).
    pub fn pointer_up(&mut self, id: i32, now: f64, cancelled: bool) -> bool {
        let before = self.pointers.len();
        self.pointers.retain(|p| p.0 != id);
        if self.pointers.len() == before {
            return false;
        }
        let click = !cancelled && self.press.is_some_and(|moved| moved < CLICK_SLOP);
        if click || now - self.last_move > 60.0 || self.reduced_motion {
            self.cam().velocity = (0.0, 0.0); // held still before letting go: no glide
        }
        self.press = None;
        click
    }

    /// Turn every pane's camera, by radians (from the keyboard).
    pub fn turn_by(&mut self, dyaw: f32, dpitch: f32) {
        for camera in &mut self.cameras {
            camera.velocity = (0.0, 0.0);
            camera.turn(dyaw, dpitch);
        }
    }

    /// A pane's center, in CSS pixels from the canvas's top left (where the keyboard picks).
    pub fn pane_center(&self, pane: usize) -> Option<Vec<f32>> {
        if pane >= self.fits.len().max(1) {
            return None;
        }
        let (x0, w) = self.pane_rect(pane);
        Some(vec![(x0 as f32 + w as f32 / 2.0) / self.dpr, self.size.1 as f32 / 2.0 / self.dpr])
    }

    pub fn zoom_by(&mut self, factor: f32) {
        for camera in &mut self.cameras {
            camera.zoom_by(factor);
        }
    }

    /// The panes' relative widths, `[2, 1]` for a first pane twice as wide as the second; empty for equal panes.
    pub fn set_pane_widths(&mut self, widths: Vec<f32>) {
        self.pane_widths = widths;
    }

    /// How many frames the scene's animation has (1 for none).
    pub fn frame_count(&self) -> usize {
        self.layers.iter().map(|l| l.data.frames()).max().unwrap_or(1)
    }

    /// The animation frame on screen, so a widget can start something new in step with what's playing.
    pub fn current_frame(&self) -> u32 {
        self.frame as u32
    }

    pub fn reset(&mut self) {
        self.cameras.clear();
        self.fits.clear();
    }

    // ---- drawing

    /// Advance the glide and the animation, upload what changed, and draw into a canvas of width by height device
    /// pixels. Returns whether something is still moving, so the page asks for another frame.
    pub fn frame(&mut self, now: f64, width: u32, height: u32, dpr: f32) -> bool {
        self.size = (width.max(1), height.max(1));
        self.dpr = dpr;
        let dt = if self.last_frame > 0.0 { (now - self.last_frame).min(50.0) as f32 } else { 16.0 };
        self.last_frame = now;
        let mut moving = false;

        for camera in &mut self.cameras {
            let (vy, vp) = camera.velocity;
            if self.pointers.is_empty() && vy.abs() + vp.abs() > 2e-5 {
                camera.turn(vy * dt, vp * dt);
                let decay = (-dt / GLIDE).exp();
                camera.velocity = (vy * decay, vp * decay);
                moving = true;
            }
        }

        let frames = self.layers.iter().map(|l| l.data.frames()).max().unwrap_or(1);
        let a = &self.animation;
        let playing = a.start != f64::NEG_INFINITY;
        self.frame = if !playing {
            frames - 1
        } else {
            let k = ((now - a.start) * a.fps / 1000.0).floor() as i64;
            if a.looped { k.rem_euclid(frames as i64) as usize } else { k.clamp(0, frames as i64 - 1) as usize }
        };
        if frames > 1 && playing && (a.looped || self.frame < frames - 1) {
            moving = true;
        }

        self.sync();
        self.refit();
        self.draw();
        if !moving {
            self.last_frame = 0.0;
        }
        moving
    }

    /// What's under a point (CSS pixels from the canvas's top left): [layer index, element index, x, y, z], or
    /// nothing. The element is a vertex for meshes, points and curves, and a vector's index for vector fields.
    pub fn pick(&mut self, x: f32, y: f32) -> Option<Vec<f64>> {
        let (w, h) = (self.size.0 as i32, self.size.1 as i32);
        let px = (x * self.dpr).floor() as i32;
        let py = h - 1 - (y * self.dpr).floor() as i32;
        if px < 0 || py < 0 || px >= w || py >= h {
            return None;
        }
        let pane = (0..self.fits.len()).find(|&i| { let (x0, pw) = self.pane_rect(i); px >= x0 && px < x0 + pw })?;
        let (x0, pw) = self.pane_rect(pane);
        let view = self.view(self.fits[pane]?, pane);
        let mut pixel = [0u8; 4];
        let gl = self.gl.as_ref();
        unsafe {
            if self.pick_target.as_ref().map(|t| t.size) != Some((w, h)) {
                let t = self.pick_target.take().unwrap_or_else(|| PickTarget {
                    framebuffer: gl.create_framebuffer().expect("create framebuffer"),
                    color: gl.create_renderbuffer().expect("create renderbuffer"),
                    depth: gl.create_renderbuffer().expect("create renderbuffer"),
                    size: (0, 0),
                });
                gl.bind_renderbuffer(glow::RENDERBUFFER, Some(t.color));
                gl.renderbuffer_storage(glow::RENDERBUFFER, glow::RGBA8, w, h);
                gl.bind_renderbuffer(glow::RENDERBUFFER, Some(t.depth));
                gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT24, w, h);
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(t.framebuffer));
                gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, Some(t.color));
                gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(t.depth));
                self.pick_target = Some(PickTarget { size: (w, h), ..t });
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.pick_target.as_ref().unwrap().framebuffer));
            gl.viewport(x0, 0, pw, h);
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(px, py, 1, 1); // only the pixel under the pointer is drawn
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            gl.enable(glow::DEPTH_TEST);
            for (i, layer) in self.layers.iter().enumerate() {
                let default = matches!(layer.data.kind, Kind::Mesh | Kind::Points);
                if pane_of(layer) == pane && layer.data.flag("pickable", default) && layer.data.flag("visible", true) {
                    layer.primitive.draw_ids(gl, &self.shaders, &view, &layer.data, i as u32);
                }
            }
            gl.read_pixels(px, py, 1, 1, glow::RGBA, glow::UNSIGNED_BYTE, &mut pixel);
            gl.disable(glow::SCISSOR_TEST);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }
        if pixel[3] == 0 {
            return None;
        }
        let layer = pixel[3] as usize - 1;
        let id = pixel[0] as u32 | (pixel[1] as u32) << 8 | (pixel[2] as u32) << 16;

        // The ray through the pixel, in the layers' own units.
        let inverse = (view.proj * view.model_view).inverse();
        let ndc = Vec2::new(2.0 * ((px - x0) as f32 + 0.5) / pw as f32 - 1.0, 2.0 * (py as f32 + 0.5) / h as f32 - 1.0);
        let near = inverse * Vec4::new(ndc.x, ndc.y, -1.0, 1.0);
        let far = inverse * Vec4::new(ndc.x, ndc.y, 1.0, 1.0);
        let (near, far) = (near.xyz() / near.w, far.xyz() / far.w);
        let hit = self.layers.get_mut(layer)?.primitive.hit(id, near, far - near);
        Some(vec![layer as f64, hit.index as f64, hit.point.x as f64, hit.point.y as f64, hit.point.z as f64])
    }
}

impl Renderer {
    fn layer_mut(&mut self, name: &str) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|l| l.name == name)
    }

    fn touch(&mut self, name: &str) {
        self.version += 1;
        let version = self.version;
        if let Some(l) = self.layer_mut(name) {
            l.data.version = version;
        }
    }

    fn set_array(&mut self, layer: &str, key: &str, data: Data, shape: Vec<u32>) -> Result<(), String> {
        let shape = shape.into_iter().map(|s| s as usize).collect();
        let array = NdArray::new(data, shape).map_err(|e| format!("{layer}.{key}: {e}"))?;
        let l = self.layer_mut(layer).ok_or_else(|| format!("no layer {layer:?}"))?;
        l.data.arrays.insert(key.into(), array);
        self.touch(layer);
        Ok(())
    }

    fn sync(&mut self) {
        for layer in &mut self.layers {
            let k = self.frame.min(layer.data.frames() - 1);
            if let Err(e) = layer.primitive.sync(self.gl.as_ref(), &layer.data, k, &self.style) {
                let message = format!("{}: {}", layer.name, e);
                if !self.errors.contains(&message) {
                    self.errors.push(message);
                }
            }
        }
    }

    /// Fit the camera to each pane's layers when they first have something in them, and widen a fit whenever its
    /// layers grow out of it, in any frame; it never shrinks, so a shape that changes size reads as changing size.
    fn refit(&mut self) {
        let scene: u64 = self.layers.iter().map(|l| l.data.version).sum::<u64>() + self.layers.len() as u64;
        let panes = self.layers.iter().map(pane_of).max().map_or(1, |p| p + 1);
        if self.fits.len() == panes && self.fits.iter().all(Option::is_some) && scene == self.fitted {
            return;
        }
        self.fitted = scene;
        self.fits.resize(panes, None);
        let start = self.camera.clone();
        self.cameras.resize_with(panes, || start.clone());
        for (pane, slot) in self.fits.iter_mut().enumerate() {
            *slot = camera::fit(self.layers.iter().filter(|l| pane_of(l) == pane).map(|l| &l.data), *slot);
        }
    }

    /// Pane i's left edge and width, in device pixels: the panes split the canvas side by side, by their widths.
    fn pane_rect(&self, i: usize) -> (i32, i32) {
        let n = self.fits.len().max(1);
        let weight = |k: usize| self.pane_widths.get(k).copied().filter(|w| *w > 0.0).unwrap_or(1.0);
        let total: f32 = (0..n).map(weight).sum();
        let edge = |k: usize| ((0..k).map(weight).sum::<f32>() / total * self.size.0 as f32).round() as i32;
        (edge(i), edge(i + 1) - edge(i))
    }

    fn view(&self, fit: Fit, pane: usize) -> View {
        let (_, pw) = self.pane_rect(pane);
        let (pw, h) = (pw.max(1) as f32, self.size.1 as f32);
        let (view, proj) = self.cameras.get(pane).unwrap_or(&self.camera).matrices(pw / h);
        View { model_view: view * fit.model(), proj, viewport: Vec2::new(pw, h), dpr: self.dpr, radius: fit.radius }
    }

    fn draw(&mut self) {
        let gl = self.gl.as_ref();
        unsafe {
            gl.viewport(0, 0, self.size.0 as i32, self.size.1 as i32);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            gl.enable(glow::DEPTH_TEST);
        }
        for pane in 0..self.fits.len() {
            let Some(fit) = self.fits[pane] else { continue };
            let (x0, pw) = self.pane_rect(pane);
            unsafe { gl.viewport(x0, 0, pw, self.size.1 as i32) };
            let view = self.view(fit, pane);
            for layer in self.layers.iter().filter(|l| pane_of(l) == pane) {
                if layer.data.flag("visible", true) {
                    layer.primitive.draw(gl, &self.shaders, &view, &layer.data, &self.style);
                }
            }
        }
    }
}

impl Renderer {
    /// The camera of the pane being dragged (the starting camera before any pane exists).
    fn cam(&mut self) -> &mut Camera {
        let i = self.active.min(self.cameras.len().saturating_sub(1));
        if self.cameras.is_empty() { &mut self.camera } else { &mut self.cameras[i] }
    }
}

/// A renderer frees what it made on its GL, which may outlive it (a context shared by a page's figures). The
/// shaders are freed with the GL.
impl Drop for Renderer {
    fn drop(&mut self) {
        let gl = self.gl.as_ref();
        for layer in &mut self.layers {
            layer.primitive.destroy(gl);
        }
        if let Some(t) = self.pick_target.take() {
            unsafe {
                gl.delete_framebuffer(t.framebuffer);
                gl.delete_renderbuffer(t.color);
                gl.delete_renderbuffer(t.depth);
            }
        }
    }
}

/// Which pane a layer is drawn in: its `pane` parameter, 0 (the first) by default.
fn pane_of(layer: &Layer) -> usize {
    layer.data.number("pane", 0.0).max(0.0) as usize
}
