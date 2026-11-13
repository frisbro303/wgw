//! Layers: what a scene is made of. Each has a kind, named arrays, and named parameters (numbers and colors).

use std::collections::HashMap;

use glam::{Vec2, Vec3};

use crate::array::NdArray;
use crate::Rgba;

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

/// The page's colors, read from CSS, and the viewer-wide settings.
pub struct Style {
    pub colormap: Vec<Rgba>,
    pub accent: Rgba,
    pub edge: Rgba,
    /// Draw meshes' triangle grids; a layer's own `edges` parameter overrides it.
    pub grid: bool,
}
