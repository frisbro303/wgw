//! wgw figures as SVG, for PDFs: a Typst plugin. Given a figure (its widget's source, its mesh file, its view and
//! the site's CSS), it runs the widget as the page would, builds the scene with wgw-scene, and draws it as SVG with
//! the viewer's camera, colors and lighting. Typst then places it like any image.

mod css;
mod run;
mod svg;

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;
use wasm_minimal_protocol::*;
use wgw_scene::array::{Data, NdArray};
use wgw_scene::camera::Up;
use wgw_scene::layer::{Kind, LayerData, Style};
use wgw_scene::mesh_file;

initiate_protocol!();

/// The figure, as tdg passes it.
#[derive(Deserialize)]
struct Figure {
    /// The mesh file's extension, "obj" or "wgw", when it has one.
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    up: Option<String>,
    /// Turn and tilt in degrees, then optionally a scale.
    #[serde(default)]
    view: Option<Vec<f32>>,
    #[serde(default)]
    flat: bool,
    /// The still's size in CSS pixels.
    width: f32,
    height: f32,
    /// The site's stylesheets, for its colors.
    #[serde(default)]
    css: String,
}

/// The figure as SVG. `widget` is a JavaScript widget's source (empty for none), `mesh` the mesh file (empty for
/// none), `figure` the rest, as JSON.
#[wasm_func]
pub fn still(figure: &[u8], widget: &[u8], mesh: &[u8]) -> Result<Vec<u8>, String> {
    let figure: Figure = serde_json::from_slice(figure).map_err(|e| format!("the figure: {e}"))?;
    let vars = css::variables(&figure.css);
    let mut layers: Vec<(String, LayerData)> = Vec::new();

    // The mesh file is the layer "mesh", and the widget gets its arrays.
    let mut mesh_json = None;
    if !mesh.is_empty() {
        let format = figure.format.as_deref().unwrap_or("obj");
        let file = mesh_file::parse(mesh, format)?;
        let (vertices, vshape) = file.vertices()?;
        let (faces, fshape) = file.faces()?;
        let mut layer = LayerData::new(Kind::Mesh);
        layer.arrays.insert("vertices".into(), NdArray::new(Data::F32(vertices), vshape)?);
        layer.arrays.insert("faces".into(), NdArray::new(Data::U32(faces), fshape)?);
        layers.push(("mesh".into(), layer));
        mesh_json = Some(mesh_arrays(&file));
    }

    let mut grid = false;
    let mut pane_widths = Vec::new();
    if !widget.is_empty() {
        let source = std::str::from_utf8(widget).map_err(|_| "the widget isn't text")?;
        let result: Value = serde_json::from_str(&run::widget(source, mesh_json.as_deref())?).map_err(|e| e.to_string())?;
        for patch in result["patches"].as_array().into_iter().flatten() {
            apply(&mut layers, patch, &vars, figure.flat)?;
        }
        grid = result["grid"].as_bool().unwrap_or(false);
        pane_widths = result["panes"].as_array().map(|p| p.iter().filter_map(|w| w.as_f64().map(|w| w as f32)).collect()).unwrap_or_default();
    }
    if figure.flat {
        if let Some((_, l)) = layers.iter_mut().find(|(n, _)| n == "mesh") {
            l.numbers.insert("flat".into(), 1.0);
        }
    }

    let style = page_style(&vars, grid);
    let view = figure.view.unwrap_or_default();
    let frame = svg::Frame {
        width: figure.width,
        height: figure.height,
        up: Up::parse(figure.up.as_deref().unwrap_or("z")),
        view: [view.first().copied().unwrap_or(-60.0), view.get(1).copied().unwrap_or(20.0), view.get(2).copied().unwrap_or(1.0)],
    };
    Ok(svg::render(&layers, &style, &frame, &pane_widths)?.into_bytes())
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

/// A mesh file's arrays as the page hands them to a widget, as JSON: {name: {data, shape, dtype}}.
fn mesh_arrays(file: &mesh_file::MeshFile) -> String {
    let mut out = serde_json::Map::new();
    for name in file.names() {
        let (Some(dtype), Some(shape), Some(bytes)) = (file.dtype(&name), file.shape(&name), file.bytes(&name)) else { continue };
        let numbers: Vec<f64> = match dtype.as_str() {
            "f4" => bytes.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap()) as f64).collect(),
            "f8" => bytes.chunks_exact(8).map(|b| f64::from_le_bytes(b.try_into().unwrap())).collect(),
            "u4" => bytes.chunks_exact(4).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as f64).collect(),
            "i4" => bytes.chunks_exact(4).map(|b| i32::from_le_bytes(b.try_into().unwrap()) as f64).collect(),
            "u2" => bytes.chunks_exact(2).map(|b| u16::from_le_bytes(b.try_into().unwrap()) as f64).collect(),
            "u1" => bytes.iter().map(|&b| b as f64).collect(),
            _ => continue,
        };
        let numbers: Vec<Value> = numbers.into_iter().map(|x| serde_json::Number::from_f64(x).map_or(Value::Null, Value::Number)).collect();
        out.insert(name, serde_json::json!({ "data": numbers, "shape": shape, "dtype": dtype }));
    }
    Value::Object(out).to_string()
}

/// Apply a patch as the page does (Viewer.apply in js/wgw.js): null removes a layer; otherwise the layer gets
/// exactly the given parameters, drops the arrays in `remove`, and sets those in `arrays`.
fn apply(layers: &mut Vec<(String, LayerData)>, patch: &Value, vars: &HashMap<String, String>, flat: bool) -> Result<(), String> {
    let Some(entries) = patch.get("layers").and_then(Value::as_object) else { return Ok(()) };
    for (name, layer) in entries {
        if layer.is_null() {
            layers.retain(|(n, _)| n != name);
            continue;
        }
        let kind_name = layer["type"].as_str().unwrap_or("");
        let kind = Kind::parse(kind_name).ok_or_else(|| format!("{name}: unknown layer type {kind_name:?}"))?;
        let index = match layers.iter().position(|(n, _)| n == name) {
            Some(i) => i,
            None => {
                layers.push((name.clone(), LayerData::new(kind)));
                layers.len() - 1
            }
        };
        let data = &mut layers[index].1;
        data.kind = kind;
        data.numbers.clear();
        data.colors.clear();
        if name == "mesh" && flat {
            data.numbers.insert("flat".into(), 1.0);
        }
        for key in layer["remove"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            data.arrays.remove(key);
        }
        for (key, a) in layer["arrays"].as_object().into_iter().flatten() {
            let shape: Vec<usize> = a["shape"].as_array().into_iter().flatten().filter_map(|s| s.as_u64().map(|s| s as usize)).collect();
            let numbers = a["data"].as_array().ok_or_else(|| format!("{name}.{key}: no data"))?;
            let array = if a["int"].as_bool().unwrap_or(false) {
                Data::U32(numbers.iter().map(|x| x.as_f64().unwrap_or(0.0) as u32).collect())
            } else {
                Data::F32(numbers.iter().map(|x| x.as_f64().map_or(f32::NAN, |x| x as f32)).collect())
            };
            data.arrays.insert(key.clone(), NdArray::new(array, shape).map_err(|e| format!("{name}.{key}: {e}"))?);
        }
        for (key, value) in layer["params"].as_object().into_iter().flatten() {
            match value {
                Value::Number(x) => {
                    data.numbers.insert(key.clone(), x.as_f64().unwrap_or(0.0));
                }
                Value::Bool(b) => {
                    data.numbers.insert(key.clone(), if *b { 1.0 } else { 0.0 });
                }
                Value::String(_) | Value::Array(_) => {
                    let list: Vec<&str> = match value {
                        Value::String(s) => vec![s.as_str()],
                        _ => value.as_array().unwrap().iter().filter_map(Value::as_str).collect(),
                    };
                    let colors: Vec<_> = list.iter().filter_map(|c| css::color(c, vars)).collect();
                    if colors.len() == list.len() && !colors.is_empty() {
                        data.colors.insert(key.clone(), colors);
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

// Typst plugins are pure functions with no source of randomness; the engine only uses it to seed hashing, so it
// gets a fixed stream (getrandom's custom backend, chosen in .cargo/config.toml).
#[no_mangle]
unsafe extern "Rust" fn __getrandom_v04_custom(dest: *mut u8, len: usize) -> Result<(), getrandom::Error> {
    for i in 0..len {
        *dest.add(i) = (i as u8).wrapping_mul(97).wrapping_add(13);
    }
    Ok(())
}

// The same, for the crates that ask through getrandom 0.3.
#[no_mangle]
unsafe extern "Rust" fn __getrandom_v03_custom(dest: *mut u8, len: usize) -> Result<(), getrandom03::Error> {
    for i in 0..len {
        *dest.add(i) = (i as u8).wrapping_mul(97).wrapping_add(13);
    }
    Ok(())
}
