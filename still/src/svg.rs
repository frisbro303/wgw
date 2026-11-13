//! A scene drawn as SVG: the same camera, framing, panes, colors and lighting as the WebGL viewer (all from
//! wgw-scene), with every triangle, curve segment, point and arrow projected to the page and painted back to front.

use std::fmt::Write;

use glam::{Mat3, Mat4, Vec2, Vec3, Vec4Swizzles};
use wgw_scene::camera::{self, Camera, Up};
use wgw_scene::geometry::{segments, unique_edges, vertex_normals};
use wgw_scene::layer::{Kind, LayerData, Style};
use wgw_scene::light::{colormap, shade};
use wgw_scene::Rgba;

/// The figure: its size in CSS pixels, its up axis, and its starting view (turn and tilt in degrees, and a scale).
pub struct Frame {
    pub width: f32,
    pub height: f32,
    pub up: Up,
    pub view: [f32; 3],
}

enum Shape {
    Triangle([Vec2; 3], Rgba),
    Segment(Vec2, Vec2, Rgba, f32),
    Dot(Vec2, f32, Rgba),
}

/// Something to paint, and how far it is from the camera: farther ones are painted first.
struct Item {
    depth: f32,
    shape: Shape,
}

/// Where a pane's camera puts points: to the page, and to view space (for lighting and depth).
struct Projector {
    model_view: Mat4,
    proj: Mat4,
    x0: f32,
    width: f32,
    height: f32,
}

impl Projector {
    fn view(&self, p: Vec3) -> Vec3 {
        self.model_view.transform_point3(p)
    }

    fn page(&self, v: Vec3) -> Option<Vec2> {
        let clip = self.proj * v.extend(1.0);
        if clip.w <= 1e-6 || !clip.is_finite() {
            return None;
        }
        let ndc = clip.xyz() / clip.w;
        Some(Vec2::new(self.x0 + (ndc.x + 1.0) / 2.0 * self.width, (1.0 - ndc.y) / 2.0 * self.height))
    }
}

pub fn render(layers: &[(String, LayerData)], style: &Style, frame: &Frame, pane_widths: &[f32]) -> Result<String, String> {
    let pane_of = |l: &LayerData| l.number("pane", 0.0).max(0.0) as usize;
    let panes = layers.iter().map(|(_, l)| pane_of(l)).max().map_or(1, |p| p + 1);
    let weight = |k: usize| pane_widths.get(k).copied().filter(|w| *w > 0.0).unwrap_or(1.0);
    let total: f32 = (0..panes).map(weight).sum();
    let edge = |k: usize| ((0..k).map(weight).sum::<f32>() / total * frame.width).round();

    let mut svg = String::new();
    let (w, h) = (frame.width, frame.height);
    write!(svg, r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}">"#).unwrap();
    for pane in 0..panes {
        let (x0, pw) = (edge(pane), edge(pane + 1) - edge(pane));
        let in_pane: Vec<&(String, LayerData)> = layers.iter().filter(|(_, l)| pane_of(l) == pane).collect();
        let Some(fit) = camera::fit(in_pane.iter().map(|(_, l)| l), None) else { continue };
        let mut cam = Camera::new();
        cam.up = frame.up;
        cam.home = (frame.view[0].to_radians(), frame.view[1].to_radians());
        cam.home_zoom = (1.0 / frame.view[2].max(0.25)).clamp(0.2, 4.0);
        cam.reset();
        let (view, proj) = cam.matrices(pw / h);
        let projector = Projector { model_view: view * fit.model(), proj, x0, width: pw, height: h };

        let mut items = Vec::new();
        for (name, layer) in &in_pane {
            if !layer.flag("visible", true) {
                continue;
            }
            add_layer(&mut items, layer, style, &projector, fit.radius).map_err(|e| format!("{name}: {e}"))?;
        }
        items.sort_by(|a, b| b.depth.total_cmp(&a.depth));

        write!(svg, r#"<clipPath id="pane{pane}"><rect x="{x0}" y="0" width="{pw}" height="{h}"/></clipPath><g clip-path="url(#pane{pane})" stroke-linecap="round" stroke-linejoin="round">"#).unwrap();
        for item in items {
            match item.shape {
                Shape::Triangle([a, b, c], color) => {
                    // Outlined in its own color, so neighbouring triangles meet without hairline gaps.
                    let fill = css_color(color);
                    write!(svg, r#"<path d="M{:.1} {:.1}L{:.1} {:.1}L{:.1} {:.1}Z" fill="{fill}" stroke="{fill}" stroke-width=".5"{}/>"#,
                           a.x, a.y, b.x, b.y, c.x, c.y, opacity(color, "")).unwrap();
                }
                Shape::Segment(a, b, color, width) => {
                    write!(svg, r#"<path d="M{:.1} {:.1}L{:.1} {:.1}" stroke="{}" stroke-width="{width:.2}" fill="none"{}/>"#,
                           a.x, a.y, b.x, b.y, css_color(color), opacity(color, "stroke-")).unwrap();
                }
                Shape::Dot(p, r, color) => {
                    let rim = shade(color, Vec3::new(0.75, -0.6, 0.3).normalize(), Vec3::NEG_Z);
                    write!(svg, r#"<circle cx="{:.1}" cy="{:.1}" r="{r:.1}" fill="{}" stroke="{}" stroke-width="{:.2}"/>"#,
                           p.x, p.y, css_color(shade(color, Vec3::new(-0.2, 0.3, 0.93).normalize(), Vec3::NEG_Z)), css_color(rim), r * 0.25).unwrap();
                }
            }
        }
        svg.push_str("</g>");
    }
    svg.push_str("</svg>");
    Ok(svg)
}

fn css_color(c: Rgba) -> String {
    let byte = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", byte(c[0]), byte(c[1]), byte(c[2]))
}

fn opacity(c: Rgba, prefix: &str) -> String {
    if c[3] < 0.999 { format!(r#" {prefix}opacity="{:.3}""#, c[3]) } else { String::new() }
}

// Curves and edges sit just in front of a surface they lie on, as the WebGL viewer nudges them.
const LIFT: f32 = 0.004;

fn add_layer(items: &mut Vec<Item>, l: &LayerData, style: &Style, p: &Projector, radius: f32) -> Result<(), String> {
    let positions = l.rows("vertices", 3, 0, None)?.ok_or("no vertices")?;
    let n = positions.len() / 3;
    let at = |i: usize| Vec3::new(positions[3 * i], positions[3 * i + 1], positions[3 * i + 2]);
    let values = l.rows("values", 1, 0, Some(n))?;
    let value = |i: usize| values.map_or(0.0, |v| v[i]);
    let stops = l.stops(style);
    let range = l.range();
    let color = |v: f32| colormap(&stops, v, range.x, range.y);
    let normal_matrix = Mat3::from_mat4(p.model_view);

    match l.kind {
        Kind::Mesh => {
            let faces = l.arrays.get("faces").and_then(|a| a.u32s()).ok_or("a mesh needs faces")?;
            if faces.iter().any(|&f| f as usize >= n) {
                return Err(format!("faces must be indices below {n}"));
            }
            let flat = l.flag("flat", false);
            let normals = vertex_normals(positions, faces);
            let view: Vec<Vec3> = (0..n).map(|i| p.view(at(i))).collect();
            let page: Vec<Option<Vec2>> = view.iter().map(|&v| p.page(v)).collect();
            for f in faces.chunks_exact(3) {
                let [a, b, c] = [f[0] as usize, f[1] as usize, f[2] as usize];
                let (Some(pa), Some(pb), Some(pc)) = (page[a], page[b], page[c]) else { continue };
                let face_normal = (view[b] - view[a]).cross(view[c] - view[a]).normalize_or_zero();
                let lit = |i: usize| {
                    let n = if flat { face_normal } else { (normal_matrix * Vec3::from_slice(&normals[3 * i..3 * i + 3])).normalize_or_zero() };
                    shade(color(value(i)), n, view[i])
                };
                let (ca, cb, cc) = (lit(a), lit(b), lit(c));
                let mean: Rgba = std::array::from_fn(|k| (ca[k] + cb[k] + cc[k]) / 3.0);
                let depth = -(view[a].z + view[b].z + view[c].z) / 3.0;
                items.push(Item { depth, shape: Shape::Triangle([pa, pb, pc], mean) });
            }
            if l.flag("edges", style.grid) {
                let edge = l.colors.get("edge_color").map(|c| c[0]).unwrap_or(style.edge);
                for e in unique_edges(faces).chunks_exact(2) {
                    let (a, b) = (e[0] as usize, e[1] as usize);
                    if let (Some(pa), Some(pb)) = (page[a], page[b]) {
                        items.push(Item { depth: -(view[a].z + view[b].z) / 2.0 - LIFT, shape: Shape::Segment(pa, pb, edge, 0.6) });
                    }
                }
            }
        }
        Kind::Lines => {
            let width = l.number("width", 2.0) as f32;
            for [a, b] in segments(l, n)? {
                let (a, b) = (a as usize, b as usize);
                let (va, vb) = (p.view(at(a)), p.view(at(b)));
                let (Some(pa), Some(pb)) = (p.page(va), p.page(vb)) else { continue };
                let c = color((value(a) + value(b)) / 2.0);
                items.push(Item { depth: -(va.z + vb.z) / 2.0 - LIFT, shape: Shape::Segment(pa, pb, c, width) });
            }
        }
        Kind::Points => {
            let (size, lift) = (l.number("size", 6.0) as f32, l.number("lift", 0.02) as f32);
            for i in 0..n {
                let v = p.view(at(i));
                if let Some(pv) = p.page(v) {
                    items.push(Item { depth: -v.z - lift, shape: Shape::Dot(pv, size / 2.0, color(value(i))) });
                }
            }
        }
        Kind::Vectors => {
            let vectors = l.rows("vectors", 3, 0, Some(n))?.ok_or("vectors need vectors")?;
            let (scale, width) = (l.number("scale", 1.0) as f32, l.number("width", 0.006) as f32 * radius);
            for i in 0..n {
                let (o, v) = (at(i), Vec3::from_slice(&vectors[3 * i..3 * i + 3]) * scale);
                let len = v.length();
                if len < 1e-12 {
                    continue;
                }
                let head = (0.4 * len).min(6.0 * width);
                let (vo, vb, vt) = (p.view(o), p.view(o + v * (1.0 - head / len)), p.view(o + v));
                let (Some(po), Some(pb), Some(pt)) = (p.page(vo), p.page(vb), p.page(vt)) else { continue };
                // The shaft's and head's widths on the page, from the scene's units at the arrow's base.
                let px = |r: f32| {
                    let side = p.view(o + v.any_orthonormal_vector() * r);
                    p.page(side).map_or(1.0, |s| (s - po).length())
                };
                let c = shade(color(value(i)), Vec3::Z, vo);
                let dir = (pt - pb).normalize_or_zero();
                let side = Vec2::new(-dir.y, dir.x) * px(2.5 * width);
                items.push(Item { depth: -(vo.z + vb.z) / 2.0, shape: Shape::Segment(po, pb, c, 2.0 * px(width)) });
                items.push(Item { depth: -(vb.z + vt.z) / 2.0, shape: Shape::Triangle([pb + side, pt, pb - side], c) });
            }
        }
    }
    Ok(())
}
