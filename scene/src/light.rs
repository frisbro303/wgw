//! How the scene is colored and lit, as the WebGL shaders do it (src/shaders.rs in the wgw crate): values through
//! a layer's color stops, then a key light from the upper left, a weak fill from the lower right, a sky/ground
//! ambient and a little specular, in view space, both sides of a surface lit.

use glam::Vec3;

use crate::Rgba;

/// A value's color: `(value - min) / (max - min)`, clamped, run evenly through the stops.
pub fn colormap(stops: &[Rgba], value: f32, min: f32, max: f32) -> Rgba {
    let t = ((value - min) / (max - min)).clamp(0.0, 1.0);
    if t.is_nan() || stops.len() == 1 {
        return stops[0];
    }
    let x = t * (stops.len() - 1) as f32;
    let k = (x.floor() as usize).min(stops.len() - 2);
    let f = x - k as f32;
    std::array::from_fn(|c| stops[k][c] * (1.0 - f) + stops[k + 1][c] * f)
}

/// A base color lit at a point `pos` with normal `n`, both in view space (the camera at the origin, looking down -z).
pub fn shade(base: Rgba, n: Vec3, pos: Vec3) -> Rgba {
    let v = (-pos).normalize_or_zero();
    let n = if n.dot(v) < 0.0 { -n } else { n };
    let key = Vec3::new(-0.35, 0.55, 0.75).normalize();
    let fill = Vec3::new(0.6, -0.3, 0.5).normalize();
    let light = 0.16 + 0.2 * (0.5 + 0.5 * n.y) + 0.75 * n.dot(key).max(0.0) + 0.15 * n.dot(fill).max(0.0);
    let spec = n.dot((key + v).normalize()).max(0.0).powf(40.0);
    let lit = |c: f32| (c.powf(2.2) * light + 0.1 * spec).max(0.0).powf(1.0 / 2.2).min(1.0);
    [lit(base[0]), lit(base[1]), lit(base[2]), base[3]]
}
