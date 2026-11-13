//! Layers: what a scene is made of. Each has a kind, named arrays, and named parameters (numbers and colors).
//! A kind is a `Primitive`: it uploads a layer's arrays to the GPU, draws them, and draws ids for picking.
//! Adding a kind means one new module implementing `Primitive`, and a line in `Kind`.

use std::collections::HashMap;

use glam::{Mat4, Vec2, Vec3};

use crate::array::NdArray;
use crate::gl::Rgba;
use crate::shaders::Shaders;
use crate::{lines, mesh, points, vectors};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Mesh,
    Points,
    Lines,
    Vectors,
}

impl Kind {
    pub fn parse(s: &str) -> Option<Kind> {
        match s {
            "mesh" => Some(Kind::Mesh),
            "points" => Some(Kind::Points),
            "lines" => Some(Kind::Lines),
            "vectors" => Some(Kind::Vectors),
            _ => None,
        }
    }

    pub fn primitive(self) -> Box<dyn Primitive> {
        match self {
            Kind::Mesh => Box::new(mesh::Mesh::default()),
            Kind::Points => Box::new(points::Points::default()),
            Kind::Lines => Box::new(lines::Lines::default()),
            Kind::Vectors => Box::new(vectors::Vectors::default()),
        }
    }
}

/// How many axes an array has per frame, by its key; one more is a leading frames axis.
pub fn rank(key: &str) -> usize {
    match key {
        "values" => 1,
        _ => 2,
    }
}

pub struct LayerData {
    pub kind: Kind,
    pub arrays: HashMap<String, NdArray>,
    pub numbers: HashMap<String, f64>,
    pub colors: HashMap<String, Vec<Rgba>>,
    /// Bumped on every change, so primitives know when to upload again.
    pub version: u64,
}

impl LayerData {
    pub fn new(kind: Kind) -> Self {
        LayerData { kind, arrays: HashMap::new(), numbers: HashMap::new(), colors: HashMap::new(), version: 0 }
    }

    pub fn number(&self, key: &str, default: f64) -> f64 {
        self.numbers.get(key).copied().unwrap_or(default)
    }

    pub fn flag(&self, key: &str, default: bool) -> bool {
        self.numbers.get(key).map(|x| *x != 0.0).unwrap_or(default)
    }

    pub fn frames(&self) -> usize {
        self.arrays.iter().map(|(k, a)| if k == "faces" || k == "segments" { 1 } else { a.frames(rank(k)) }).max().unwrap_or(1)
    }

    /// Frame k of a float array with rows of `width` numbers, checked to have `rows` rows when given.
    pub fn rows(&self, key: &str, width: usize, k: usize, rows: Option<usize>) -> Result<Option<&[f32]>, String> {
        let Some(a) = self.arrays.get(key) else { return Ok(None) };
        let r = rank(key);
        if width > 1 && a.shape.last() != Some(&width) {
            return Err(format!("{key} must have rows of {width}, got shape {:?}", a.shape));
        }
        let data = a.f32_frame(r, k).ok_or_else(|| format!("{key} must be floats"))?;
        if let Some(n) = rows {
            if data.len() != n * width {
                return Err(format!("{key} has {} rows for {n} vertices", data.len() / width));
            }
        }
        Ok(Some(data))
    }

    pub fn range(&self) -> Vec2 {
        Vec2::new(self.number("min", 0.0) as f32, self.number("max", 1.0) as f32)
    }

    /// The colors values run through: a solid `color`, the layer's `colors`, or the page's colormap (for meshes
    /// and anything with values) or accent (for points, lines and vectors without values).
    pub fn stops(&self, style: &Style) -> Vec<Rgba> {
        if let Some(c) = self.colors.get("color") {
            return vec![c[0]];
        }
        if let Some(c) = self.colors.get("colors") {
            return c.clone();
        }
        if self.kind == Kind::Mesh || self.arrays.contains_key("values") { style.colormap.clone() } else { vec![style.accent] }
    }

    /// Every point the layer covers in frame k, for fitting the camera.
    pub fn for_each_point(&self, k: usize, mut f: impl FnMut(Vec3)) {
        let Ok(Some(p)) = self.rows("vertices", 3, k, None) else { return };
        let tips = if self.kind == Kind::Vectors { self.rows("vectors", 3, k, Some(p.len() / 3)).ok().flatten() } else { None };
        let scale = self.number("scale", 1.0) as f32;
        for (i, c) in p.chunks_exact(3).enumerate() {
            let v = Vec3::new(c[0], c[1], c[2]);
            f(v);
            if let Some(t) = tips {
                f(v + Vec3::new(t[3 * i], t[3 * i + 1], t[3 * i + 2]) * scale);
            }
        }
    }
}

/// The page's colors, read from CSS.
pub struct Style {
    pub colormap: Vec<Rgba>,
    pub accent: Rgba,
    pub edge: Rgba,
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
