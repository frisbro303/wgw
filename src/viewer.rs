//! The Viewer the page talks to: a scene of layers, its camera, input, picking and animation.

use crate::{array, camera, gl, io, layer, shaders};
use glam::{Vec2, Vec3, Vec4, Vec4Swizzles};
use glow::HasContext;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use array::{Data, NdArray};
use camera::{Camera, Fit, Up};
use gl::Rgba;
use layer::{Kind, Layer, LayerData, Style, View};
use shaders::Shaders;

const TURN_SPEED: f32 = 0.008; // radians per CSS pixel
const CLICK_SLOP: f32 = 6.0; // CSS pixels a press can move and still be a click
const GLIDE: f32 = 280.0; // ms for the glide after a drag to slow by a factor e

struct Animation {
    start: f64, // -inf: show the last frame
    fps: f64,
    looped: bool,
}

struct PickTarget {
    framebuffer: glow::Framebuffer,
    color: glow::Renderbuffer,
    depth: glow::Renderbuffer,
    size: (i32, i32),
}

#[wasm_bindgen]
pub struct Viewer {
    gl: glow::Context,
    shaders: Shaders,
    layers: Vec<Layer>,
    style: Style,
    camera: Camera,
    fit: Option<Fit>,
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

#[wasm_bindgen]
impl Viewer {
    #[wasm_bindgen(constructor)]
    pub fn new(canvas: web_sys::HtmlCanvasElement) -> Result<Viewer, JsValue> {
        let options = js_sys::Object::new();
        js_sys::Reflect::set(&options, &"antialias".into(), &true.into())?;
        js_sys::Reflect::set(&options, &"premultipliedAlpha".into(), &true.into())?;
        let context = canvas
            .get_context_with_context_options("webgl2", &options)?
            .ok_or("WebGL2 isn't available in this browser")?
            .dyn_into::<web_sys::WebGl2RenderingContext>()?;
        let gl = glow::Context::from_webgl2_context(context);
        let shaders = Shaders::new(&gl).map_err(|e| JsValue::from_str(&e))?;
        let red = [0.72, 0.25, 0.18, 1.0];
        Ok(Viewer {
            gl,
            shaders,
            layers: Vec::new(),
            style: Style { colormap: vec![[0.85, 0.84, 0.81, 1.0], red], accent: red, edge: [0.0, 0.0, 0.0, 0.3], grid: false },
            camera: Camera::new(),
            fit: None,
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
        })
    }

    // ---- the scene: layers patched by name

    /// Create a layer, or change its kind ("mesh", "points", "lines", "vectors"); "" keeps an existing layer's
    /// kind, and makes a new one points.
    pub fn set_layer(&mut self, name: &str, kind: &str) -> Result<(), JsValue> {
        let parsed = Kind::parse(kind);
        if !kind.is_empty() && parsed.is_none() {
            return Err(format!("unknown layer type {kind:?}").into());
        }
        match self.layers.iter_mut().find(|l| l.name == name) {
            Some(layer) => {
                if let Some(kind) = parsed.filter(|k| *k != layer.data.kind) {
                    layer.primitive.destroy(&self.gl);
                    layer.primitive = kind.primitive();
                    layer.data.kind = kind;
                }
            }
            None => {
                let kind = parsed.unwrap_or(Kind::Points);
                self.layers.push(Layer { name: name.into(), data: LayerData::new(kind), primitive: kind.primitive() });
            }
        }
        self.touch(name);
        Ok(())
    }

    pub fn remove_layer(&mut self, name: &str) {
        if let Some(i) = self.layers.iter().position(|l| l.name == name) {
            let mut layer = self.layers.remove(i);
            layer.primitive.destroy(&self.gl);
            self.version += 1;
        }
    }

    pub fn set_f32(&mut self, layer: &str, key: &str, data: Vec<f32>, shape: Vec<u32>) -> Result<(), JsValue> {
        self.set_array(layer, key, Data::F32(data), shape)
    }

    pub fn set_u32(&mut self, layer: &str, key: &str, data: Vec<u32>, shape: Vec<u32>) -> Result<(), JsValue> {
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
    pub fn set_mesh_file(&mut self, name: &str, file: &io::MeshFile) -> Result<(), JsValue> {
        let err = |e: String| JsValue::from_str(&e);
        let (vertices, vshape) = file.vertices().map_err(err)?;
        let (faces, fshape) = file.faces().map_err(err)?;
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
    pub fn set_options(&mut self, up: &str, yaw: f32, pitch: f32, reduced_motion: bool) {
        self.camera.up = Up::parse(up);
        self.camera.home = (yaw.to_radians(), pitch.to_radians());
        self.camera.reset();
        self.reduced_motion = reduced_motion;
    }

    /// Play the scene's frames from now at fps, once or looping; not animated, jump to the last frame.
    pub fn play(&mut self, now: f64, fps: f64, looped: bool, animate: bool) {
        let animate = looped || (animate && !self.reduced_motion);
        self.animation = Animation { start: if animate { now } else { f64::NEG_INFINITY }, fps: fps.max(1.0), looped };
    }

    // ---- input, in CSS pixels from the canvas's top left

    pub fn pointer_down(&mut self, id: i32, x: f32, y: f32, now: f64) {
        self.pointers.retain(|p| p.0 != id);
        self.pointers.push((id, x, y));
        self.press = (self.pointers.len() == 1).then_some(0.0);
        self.camera.velocity = (0.0, 0.0);
        self.last_move = now;
    }

    /// Returns whether the view changed (a drag turned it or a pinch zoomed it).
    pub fn pointer_move(&mut self, id: i32, x: f32, y: f32, now: f64) -> bool {
        let Some(i) = self.pointers.iter().position(|p| p.0 == id) else { return false };
        let (_, px, py) = self.pointers[i];
        let (dx, dy) = (x - px, y - py);
        if self.pointers.len() == 1 {
            self.camera.turn(-dx * TURN_SPEED, dy * TURN_SPEED);
            let dt = (now - self.last_move).max(1.0) as f32;
            let blend = (dt / 40.0).min(1.0); // smoothed velocity, in radians per ms
            let v = self.camera.velocity;
            self.camera.velocity =
                (v.0 * (1.0 - blend) - dx * TURN_SPEED / dt * blend, v.1 * (1.0 - blend) + dy * TURN_SPEED / dt * blend);
            if let Some(moved) = &mut self.press {
                *moved += dx.abs() + dy.abs();
            }
        } else if self.pointers.len() == 2 {
            let (_, ox, oy) = self.pointers[1 - i];
            let (before, after) = ((px - ox).hypot(py - oy), (x - ox).hypot(y - oy));
            if after > 0.0 {
                self.camera.zoom_by(before / after);
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
            self.camera.velocity = (0.0, 0.0); // held still before letting go: no glide
        }
        self.press = None;
        click
    }

    pub fn zoom_by(&mut self, factor: f32) {
        self.camera.zoom_by(factor);
    }

    pub fn reset(&mut self) {
        self.camera.reset();
        self.fit = None;
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

        let (vy, vp) = self.camera.velocity;
        if self.pointers.is_empty() && vy.abs() + vp.abs() > 2e-5 {
            self.camera.turn(vy * dt, vp * dt);
            let decay = (-dt / GLIDE).exp();
            self.camera.velocity = (vy * decay, vp * decay);
            moving = true;
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
        let fit = self.fit?;
        let (w, h) = (self.size.0 as i32, self.size.1 as i32);
        let px = (x * self.dpr).floor() as i32;
        let py = h - 1 - (y * self.dpr).floor() as i32;
        if px < 0 || py < 0 || px >= w || py >= h {
            return None;
        }
        let view = self.view(fit);
        let mut pixel = [0u8; 4];
        let gl = &self.gl;
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
            gl.viewport(0, 0, w, h);
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(px, py, 1, 1); // only the pixel under the pointer is drawn
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            gl.enable(glow::DEPTH_TEST);
            for (i, layer) in self.layers.iter().enumerate() {
                let default = matches!(layer.data.kind, Kind::Mesh | Kind::Points);
                if layer.data.flag("pickable", default) && layer.data.flag("visible", true) {
                    layer.primitive.draw_ids(gl, &self.shaders, &view, &layer.data, i as u32);
                }
            }
            gl.read_pixels(px, py, 1, 1, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut pixel)));
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
        let ndc = Vec2::new(2.0 * (px as f32 + 0.5) / w as f32 - 1.0, 2.0 * (py as f32 + 0.5) / h as f32 - 1.0);
        let near = inverse * Vec4::new(ndc.x, ndc.y, -1.0, 1.0);
        let far = inverse * Vec4::new(ndc.x, ndc.y, 1.0, 1.0);
        let (near, far) = (near.xyz() / near.w, far.xyz() / far.w);
        let hit = self.layers.get_mut(layer)?.primitive.hit(id, near, far - near);
        Some(vec![layer as f64, hit.index as f64, hit.point.x as f64, hit.point.y as f64, hit.point.z as f64])
    }
}

impl Viewer {
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

    fn set_array(&mut self, layer: &str, key: &str, data: Data, shape: Vec<u32>) -> Result<(), JsValue> {
        let shape = shape.into_iter().map(|s| s as usize).collect();
        let array = NdArray::new(data, shape).map_err(|e| JsValue::from_str(&format!("{layer}.{key}: {e}")))?;
        let l = self.layer_mut(layer).ok_or_else(|| JsValue::from_str(&format!("no layer {layer:?}")))?;
        l.data.arrays.insert(key.into(), array);
        self.touch(layer);
        Ok(())
    }

    fn sync(&mut self) {
        for layer in &mut self.layers {
            let k = self.frame.min(layer.data.frames() - 1);
            if let Err(e) = layer.primitive.sync(&self.gl, &layer.data, k, &self.style) {
                let message = format!("{}: {}", layer.name, e);
                if !self.errors.contains(&message) {
                    self.errors.push(message);
                }
            }
        }
    }

    /// Fit the camera to the scene when it first has something in it, and widen the fit whenever the scene grows
    /// out of it; it never shrinks, so a shape that changes size reads as changing size.
    fn refit(&mut self) {
        let scene: u64 = self.layers.iter().map(|l| l.data.version).sum::<u64>() + self.layers.len() as u64;
        if self.fit.is_some() && scene == self.fitted {
            return;
        }
        self.fitted = scene;
        let frame = self.frame;
        let layers = &self.layers;
        let each = |f: &mut dyn FnMut(Vec3)| {
            for l in layers {
                l.data.for_each_point(frame.min(l.data.frames() - 1), &mut *f);
            }
        };
        let (mut lo, mut hi, mut any) = (Vec3::INFINITY, Vec3::NEG_INFINITY, false);
        each(&mut |p| {
            lo = lo.min(p);
            hi = hi.max(p);
            any = true;
        });
        if !any {
            return;
        }
        if let Some(fit) = self.fit {
            let mut outside = false;
            each(&mut |p| outside |= p.distance(fit.center) > fit.radius * 1.0001);
            if !outside {
                return;
            }
        }
        let center = (lo + hi) / 2.0;
        let mut radius = 0f32;
        each(&mut |p| radius = radius.max(p.distance(center)));
        self.fit = Some(Fit { center, radius: if radius > 0.0 { radius } else { 1.0 } });
    }

    fn view(&self, fit: Fit) -> View {
        let (view, proj) = self.camera.matrices(self.size.0 as f32 / self.size.1 as f32);
        View {
            model_view: view * fit.model(),
            proj,
            viewport: Vec2::new(self.size.0 as f32, self.size.1 as f32),
            dpr: self.dpr,
            radius: fit.radius,
        }
    }

    fn draw(&mut self) {
        let gl = &self.gl;
        unsafe {
            gl.viewport(0, 0, self.size.0 as i32, self.size.1 as i32);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            gl.enable(glow::DEPTH_TEST);
        }
        let Some(fit) = self.fit else { return };
        let view = self.view(fit);
        for layer in &self.layers {
            if layer.data.flag("visible", true) {
                layer.primitive.draw(gl, &self.shaders, &view, &layer.data, &self.style);
            }
        }
    }
}
