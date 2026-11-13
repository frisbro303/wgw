//! Drawing layers with WebGL (the layers themselves are in wgw-scene). A kind of layer is drawn by a `Primitive`:
//! it uploads a layer's arrays to the GPU, draws them, and draws ids for picking.
//! Adding a kind means one new module implementing `Primitive`, and a line in `Kind`.

use glam::{Mat4, Vec2, Vec3};

use crate::shaders::Shaders;
use crate::{lines, mesh, points, vectors};

pub use wgw_scene::layer::{Kind, LayerData, Style};

/// The primitive that draws a kind of layer.
pub fn primitive(kind: Kind) -> Box<dyn Primitive> {
    match kind {
        Kind::Mesh => Box::new(mesh::Mesh::default()),
        Kind::Points => Box::new(points::Points::default()),
        Kind::Lines => Box::new(lines::Lines::default()),
        Kind::Vectors => Box::new(vectors::Vectors::default()),
    }
}

/// What a primitive needs to draw: matrices, the viewport in device pixels, and the scene's size.
pub struct View {
    pub model_view: Mat4,
    pub proj: Mat4,
    pub viewport: Vec2,
    pub dpr: f32,
    /// The fitted scene's radius in the layers' own units, for sizes given relative to the scene.
    pub radius: f32,
}

pub struct Hit {
    pub index: u32,
    pub point: Vec3,
}

pub trait Primitive {
    /// Upload frame k of the layer, if it changed since the last call.
    fn sync(&mut self, gl: &glow::Context, data: &LayerData, k: usize, style: &Style) -> Result<(), String>;
    fn draw(&self, gl: &glow::Context, shaders: &Shaders, view: &View, data: &LayerData, style: &Style);
    /// Draw (layer, element) ids for picking.
    fn draw_ids(&self, gl: &glow::Context, shaders: &Shaders, view: &View, data: &LayerData, layer: u32);
    /// Turn the element id the GPU found under the cursor, and the ray through it (in the layer's units), into a hit.
    fn hit(&mut self, id: u32, origin: Vec3, dir: Vec3) -> Hit;
    fn destroy(&mut self, gl: &glow::Context);
}

pub struct Layer {
    pub name: String,
    pub data: LayerData,
    pub primitive: Box<dyn Primitive>,
}

/// Distance from point p to the ray o + t d (t >= 0).
pub fn ray_distance(o: Vec3, d: Vec3, p: Vec3) -> f32 {
    let t = (p - o).dot(d) / d.length_squared();
    (o + d * t.max(0.0) - p).length()
}
