//! What the page talks to: a `Gpu`, the one WebGL context a page's figures share, and a `Viewer` per figure,
//! wgw-render's Renderer drawing on it. A figure draws into the shared canvas's lower left corner and the page
//! copies that onto its own canvas, so a page can have any number of figures (browsers keep only about 16 WebGL
//! contexts alive at once).

use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wgw_render::shaders::Shaders;
use wgw_render::Renderer;

use crate::io;
use crate::webgl::WebGl;

/// The shared context, on a canvas the page keeps out of the document, and the shaders compiled on it.
#[wasm_bindgen]
pub struct Gpu {
    gl: WebGl,
    shaders: Rc<Shaders>,
}

#[wasm_bindgen]
impl Gpu {
    #[wasm_bindgen(constructor)]
    pub fn new(canvas: web_sys::HtmlCanvasElement) -> Result<Gpu, JsValue> {
        let options = js_sys::Object::new();
        js_sys::Reflect::set(&options, &"antialias".into(), &true.into())?;
        js_sys::Reflect::set(&options, &"premultipliedAlpha".into(), &true.into())?;
        let context = canvas
            .get_context_with_context_options("webgl2", &options)?
            .ok_or("WebGL2 isn't available in this browser")?
            .dyn_into::<web_sys::WebGl2RenderingContext>()?;
        let gl = WebGl::new(glow::Context::from_webgl2_context(context));
        let shaders = Rc::new(Shaders::new(&gl).map_err(js)?);
        Ok(Gpu { gl, shaders })
    }
}

/// A figure: its scene, cameras and input, drawn on the page's `Gpu`. Freeing it frees what it made there.
#[wasm_bindgen]
pub struct Viewer(Renderer);

fn js(e: String) -> JsValue {
    JsValue::from_str(&e)
}

#[wasm_bindgen]
impl Viewer {
    #[wasm_bindgen(constructor)]
    pub fn new(gpu: &Gpu) -> Viewer {
        Viewer(Renderer::with_shaders(Box::new(gpu.gl.clone()), gpu.shaders.clone()))
    }

    /// Carry on with a new Gpu, after the old one's context was lost and restored: the scene and cameras are
    /// kept, and everything is uploaded again on the next frame.
    pub fn restore(&mut self, gpu: &Gpu) {
        self.0.restore(Box::new(gpu.gl.clone()), gpu.shaders.clone())
    }

    // ---- the scene: layers patched by name (see wgw-render's Renderer)

    pub fn set_layer(&mut self, name: &str, kind: &str) -> Result<(), JsValue> {
        self.0.set_layer(name, kind).map_err(js)
    }
    pub fn remove_layer(&mut self, name: &str) {
        self.0.remove_layer(name)
    }
    pub fn set_f32(&mut self, layer: &str, key: &str, data: Vec<f32>, shape: Vec<u32>) -> Result<(), JsValue> {
        self.0.set_f32(layer, key, data, shape).map_err(js)
    }
    pub fn set_u32(&mut self, layer: &str, key: &str, data: Vec<u32>, shape: Vec<u32>) -> Result<(), JsValue> {
        self.0.set_u32(layer, key, data, shape).map_err(js)
    }
    pub fn remove_array(&mut self, layer: &str, key: &str) {
        self.0.remove_array(layer, key)
    }
    pub fn clear_params(&mut self, layer: &str) {
        self.0.clear_params(layer)
    }
    pub fn set_mesh_file(&mut self, name: &str, file: &io::MeshFile) -> Result<(), JsValue> {
        self.0.set_mesh_file(name, &file.0).map_err(js)
    }
    pub fn set_number(&mut self, layer: &str, key: &str, value: f64) {
        self.0.set_number(layer, key, value)
    }
    pub fn set_colors(&mut self, layer: &str, key: &str, rgba: Vec<f32>) {
        self.0.set_colors(layer, key, rgba)
    }
    pub fn has_layer(&self, name: &str) -> bool {
        self.0.has_layer(name)
    }
    pub fn layer_name(&self, i: usize) -> Option<String> {
        self.0.layer_name(i)
    }
    pub fn take_errors(&mut self) -> String {
        self.0.take_errors()
    }

    // ---- look, input, animation and drawing

    pub fn set_style(&mut self, colormap: Vec<f32>, accent: Vec<f32>, edge: Vec<f32>) {
        self.0.set_style(colormap, accent, edge)
    }
    pub fn set_grid(&mut self, on: bool) {
        self.0.set_grid(on)
    }
    pub fn set_options(&mut self, up: &str, yaw: f32, pitch: f32, scale: f32, reduced_motion: bool) {
        self.0.set_options(up, yaw, pitch, scale, reduced_motion)
    }
    pub fn play(&mut self, now: f64, fps: f64, looped: bool, animate: bool) {
        self.0.play(now, fps, looped, animate)
    }
    pub fn pointer_down(&mut self, id: i32, x: f32, y: f32, now: f64) {
        self.0.pointer_down(id, x, y, now)
    }
    pub fn pointer_move(&mut self, id: i32, x: f32, y: f32, now: f64) -> bool {
        self.0.pointer_move(id, x, y, now)
    }
    pub fn pointer_up(&mut self, id: i32, now: f64, cancelled: bool) -> bool {
        self.0.pointer_up(id, now, cancelled)
    }
    pub fn zoom_by(&mut self, factor: f32) {
        self.0.zoom_by(factor)
    }
    pub fn set_pane_widths(&mut self, widths: Vec<f32>) {
        self.0.set_pane_widths(widths)
    }
    pub fn current_frame(&self) -> u32 {
        self.0.current_frame()
    }
    pub fn reset(&mut self) {
        self.0.reset()
    }
    pub fn frame(&mut self, now: f64, width: u32, height: u32, dpr: f32) -> bool {
        self.0.frame(now, width, height, dpr)
    }
    pub fn pick(&mut self, x: f32, y: f32) -> Option<Vec<f64>> {
        self.0.pick(x, y)
    }
}
