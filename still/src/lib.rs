//! wgw figures as images, without a browser: a figure's scene (a widget's first frame, or a mesh) is drawn by
//! wgw-render's Renderer, the same code the page uses, on a GL that runs on the CPU with the same shaders (soft/).
//! The result is a PNG of what the page first shows. Running the widget is the host's job (a widget is a
//! WebAssembly module: tdg runs it as a Typst plugin); this takes what it returns.

mod css;
pub mod soft;

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;
use wgw_render::Renderer;
use wgw_scene::array::Data;
use wgw_scene::layer::Style;
use wgw_scene::{mesh_file, patch};

/// A figure: what the page reads from a figure's data- attributes, its size, and the site's CSS for its colors.
#[derive(Deserialize)]
pub struct Figure {
    #[serde(default)]
    pub up: Option<String>,
    /// Turn and tilt in degrees, then optionally a scale.
    #[serde(default)]
    pub view: Option<Vec<f32>>,
    #[serde(default)]
    pub flat: bool,
    /// The figure's size in CSS pixels, and how many image pixels to each (2 by default).
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub scale: Option<f32>,
    /// The site's stylesheets, for its colors (light theme).
    #[serde(default)]
    pub css: String,
}

/// The figure as a PNG, drawn by the page's renderer on the CPU. Either `mesh` is a mesh file (.obj or .wgw), or
/// `setup` and `scene` are what a widget's `setup` and `scene` returned (its JSON, and a patch) for its first frame.
pub fn png(figure: &Figure, mesh: &[u8], setup: &[u8], scene: &[u8]) -> Result<Vec<u8>, String> {
    let vars = css::variables(&figure.css);
    let setup: Value = if setup.is_empty() { Value::Null } else { serde_json::from_slice(setup).map_err(|e| format!("the widget's setup: {e}"))? };
    let grid = match &setup["grid"] {
        Value::Bool(b) => *b,
        Value::Object(o) => o.get("toggle").and_then(Value::as_bool).unwrap_or(false),
        _ => false,
    };
    let panes: Vec<f32> = setup["panes"].as_array().map(|p| p.iter().filter_map(|w| w.as_f64().map(|w| w as f32)).collect()).unwrap_or_default();

    let dpr = figure.scale.unwrap_or(2.0);
    let (width, height) = ((figure.width * dpr).round() as u32, (figure.height * dpr).round() as u32);
    let gl = soft::SoftGl::new(width as usize, height as usize);
    let mut r = Renderer::new(Box::new(gl.clone()))?;

    let style = page_style(&vars, grid);
    let flat = |s: &[[f32; 4]]| s.iter().flatten().copied().collect::<Vec<f32>>();
    r.set_style(flat(&style.colormap), style.accent.to_vec(), style.edge.to_vec());
    r.set_grid(grid);
    let view = figure.view.clone().unwrap_or_default();
    let at = |i: usize, d: f32| view.get(i).copied().unwrap_or(d);
    r.set_options(figure.up.as_deref().unwrap_or("z"), at(0, -60.0), at(1, 20.0), at(2, 1.0), true);
    r.set_pane_widths(panes);
    if !mesh.is_empty() {
        let format = if mesh.starts_with(b"WGW1") { "wgw" } else { "obj" };
        r.set_mesh_file("mesh", &mesh_file::parse(mesh, format)?)?;
        if figure.flat {
            r.set_number("mesh", "flat", 1.0);
        }
    }
    if !scene.is_empty() {
        apply(&mut r, &patch::decode(scene)?, &vars, figure.flat)?;
    }
    r.frame(0.0, width, height, dpr);
    let errors = r.take_errors();
    if !errors.is_empty() {
        return Err(errors);
    }
    if let Some(e) = gl.errors().into_iter().next() {
        return Err(e);
    }
    encode_png(width, height, &gl.image())
}

fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

/// Apply a patch as the page does (Viewer.apply in js/wgw.js).
fn apply(r: &mut Renderer, p: &patch::Patch, vars: &HashMap<String, String>, flat: bool) -> Result<(), String> {
    for (name, layer) in &p.layers {
        let Some(l) = layer else {
            r.remove_layer(name);
            continue;
        };
        r.set_layer(name, &l.kind)?;
        r.clear_params(name);
        if name == "mesh" && flat {
            r.set_number(name, "flat", 1.0);
        }
        for key in &l.remove {
            r.remove_array(name, key);
        }
        for (key, a) in &l.arrays {
            let shape = a.shape.iter().map(|&s| s as u32).collect();
            match &a.data {
                Data::F32(v) => r.set_f32(name, key, v.clone(), shape)?,
                Data::U32(v) => r.set_u32(name, key, v.clone(), shape)?,
            }
        }
        for (key, value) in &l.params {
            match value {
                Value::Number(x) => r.set_number(name, key, x.as_f64().unwrap_or(0.0)),
                Value::Bool(b) => r.set_number(name, key, if *b { 1.0 } else { 0.0 }),
                Value::String(_) | Value::Array(_) => {
                    let list: Vec<&str> = match value {
                        Value::String(s) => vec![s.as_str()],
                        _ => value.as_array().unwrap().iter().filter_map(Value::as_str).collect(),
                    };
                    let colors: Vec<f32> = list.iter().filter_map(|c| css::color(c, vars)).flatten().collect();
                    if colors.len() == 4 * list.len() && !colors.is_empty() {
                        r.set_colors(name, key, colors);
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

/// The page's figure colors, from its CSS as js/style.js reads them: --wgw-colors, --wgw-accent and --wgw-edge.
fn page_style(vars: &HashMap<String, String>, grid: bool) -> Style {
    let list = vars.get("--wgw-colors").map(|v| css::split_list(&css::substitute(v, vars))).unwrap_or_default();
    let mut colormap: Vec<_> = list.iter().filter_map(|c| css::color(c, vars)).collect();
    if colormap.is_empty() {
        colormap = vec![[0.847, 0.839, 0.812, 1.0], [0.722, 0.251, 0.184, 1.0]];
    }
    let accent = vars.get("--wgw-accent").and_then(|c| css::color(c, vars)).unwrap_or(*colormap.last().unwrap());
    let edge = vars.get("--wgw-edge").and_then(|c| css::color(c, vars)).unwrap_or([0.0, 0.0, 0.0, 0.3]);
    Style { colormap, accent, edge, grid }
}
