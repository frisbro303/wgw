//! wgw figures without a browser: a figure's scene is drawn by wgw-render's Renderer, the same code the page uses,
//! on a GL that runs on the CPU with the same shaders (soft/), as a PNG of what the page first shows (`png`), or a
//! video's frames (`video`): an animation played through, or a figure turned about its up axis. `save` writes
//! either to a file.
//!
//! Optional, so a host that only wants stills (tdg's Typst plugin, which runs widgets itself) stays small:
//!   video    animated PNG and GIF files, and other video formats by ffmpeg (video.rs)
//!   widgets  running a widget's WebAssembly module to draw it, as the page does (widget.rs)

mod css;
pub mod soft;
#[cfg(feature = "video")]
pub mod video;
#[cfg(feature = "widgets")]
pub mod widget;

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
    /// A CSS color behind the figure; transparent if none.
    #[serde(default)]
    pub background: Option<String>,
}

/// A video: frames per second, and how many frames (by default, an animation's, or `seconds` of a turn); a
/// figure turns `spin` degrees about its up axis over the whole video (360 by default if it doesn't animate).
#[derive(Deserialize, Default)]
pub struct Video {
    #[serde(default)]
    pub fps: Option<f64>,
    #[serde(default)]
    pub frames: Option<u32>,
    #[serde(default)]
    pub seconds: Option<f64>,
    #[serde(default)]
    pub spin: Option<f32>,
}

/// An image: its size in pixels, and its pixels, RGBA (not premultiplied), top row first.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// A drawn figure, ready for frames.
struct Stage {
    renderer: Renderer,
    gl: soft::SoftGl,
    width: u32,
    height: u32,
    dpr: f32,
    background: Option<[f32; 4]>,
    fps: f64,
}

impl Stage {
    fn shoot(&mut self, now: f64) -> Result<Image, String> {
        self.renderer.frame(now, self.width, self.height, self.dpr);
        let errors = self.renderer.take_errors();
        if !errors.is_empty() {
            return Err(errors);
        }
        if let Some(e) = self.gl.errors().into_iter().next() {
            return Err(e);
        }
        let mut rgba = self.gl.image();
        if let Some(bg) = self.background {
            for px in rgba.chunks_exact_mut(4) {
                let a = px[3] as f32 / 255.0;
                for k in 0..3 {
                    px[k] = (px[k] as f32 * a + bg[k] * 255.0 * (1.0 - a)).round() as u8;
                }
                px[3] = 255;
            }
        }
        Ok(Image { width: self.width, height: self.height, rgba })
    }
}

/// What to draw: a .wgw file or .obj (a widget, which needs the "widgets" feature, or a mesh), or a scene: a widget's
/// setup (its JSON, for the frame rate and grid) and a patch.
pub enum Source<'a> {
    File(&'a [u8]),
    Scene { setup: &'a [u8], patch: &'a [u8] },
}

/// Save a figure by its path's extension: .png, a still; .apng and .gif, videos; .mp4, .webm, .mov and .mkv, videos
/// by ffmpeg (on the PATH). A widget is drawn with its controls as set up, or as `controls` (JSON: name to value)
/// gives. Videos, and widgets, need the features of those names; a video is on a white background unless the figure
/// gives one (an animated PNG stays transparent).
pub fn save(path: &std::path::Path, figure: Figure, video: &Video, source: Source, controls: &Value) -> Result<(), String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    #[cfg(feature = "widgets")]
    let ran;
    let (mesh, setup, scene): (&[u8], &[u8], &[u8]) = match source {
        Source::File(bytes) if bytes.starts_with(b"\0asm") => {
            #[cfg(feature = "widgets")]
            {
                ran = widget::run(bytes, controls)?;
                (&[], &ran.0, &ran.1)
            }
            #[cfg(not(feature = "widgets"))]
            return Err("drawing a widget needs wgw-headless's \"widgets\" feature".into());
        }
        Source::File(bytes) => (bytes, &[], &[]),
        Source::Scene { setup, patch } => (&[], setup, patch),
    };
    #[cfg(not(feature = "widgets"))]
    let _ = controls;
    let bytes = match ext.as_str() {
        "png" => png(&figure, mesh, setup, scene)?,
        "apng" | "gif" | "mp4" | "webm" | "mov" | "mkv" => {
            #[cfg(feature = "video")]
            {
                let mut figure = figure;
                if ext != "apng" && figure.background.is_none() {
                    figure.background = Some("white".into());
                }
                let (frames, fps) = self::video(&figure, mesh, setup, scene, video)?;
                match ext.as_str() {
                    "gif" => video::gif(&frames, fps)?,
                    "apng" => video::apng(&frames, fps)?,
                    _ => return video::ffmpeg(path, &frames, fps),
                }
            }
            #[cfg(not(feature = "video"))]
            {
                let _ = video;
                return Err(format!("saving .{ext} needs wgw-headless's \"video\" feature"));
            }
        }
        _ => return Err(format!("can't save as .{ext}: use .png, .apng, .gif, .mp4, .webm or .mov")),
    };
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// The figure as a PNG, drawn by the page's renderer on the CPU. Either `mesh` is a mesh file (.obj or .wgw), or
/// `setup` and `scene` are what a widget's `setup` and `scene` returned (its JSON, and a patch) for its first frame.
pub fn png(figure: &Figure, mesh: &[u8], setup: &[u8], scene: &[u8]) -> Result<Vec<u8>, String> {
    let image = stage(figure, mesh, setup, scene)?.shoot(0.0)?;
    encode_png(image.width, image.height, &image.rgba)
}

/// A video's frames: the animation played through at its own speed (or the video's), and turned by `spin`.
pub fn video(figure: &Figure, mesh: &[u8], setup: &[u8], scene: &[u8], video: &Video) -> Result<(Vec<Image>, f64), String> {
    let mut stage = stage(figure, mesh, setup, scene)?;
    let fps = video.fps.unwrap_or(stage.fps).max(1.0);
    let animated = stage.renderer.frame_count();
    let frames = video.frames.map(|n| n as usize).unwrap_or(if animated > 1 && video.seconds.is_none() {
        animated
    } else {
        (video.seconds.unwrap_or(4.0) * fps).round().max(1.0) as usize
    });
    let spin = video.spin.unwrap_or(if animated > 1 { 0.0 } else { 360.0 }).to_radians();
    stage.renderer.play(0.0, fps, true, true);
    let mut out = Vec::with_capacity(frames);
    for k in 0..frames {
        if k > 0 {
            stage.renderer.turn_by(spin / frames as f32, 0.0);
        }
        out.push(stage.shoot(k as f64 * 1000.0 / fps + 0.5)?);
    }
    Ok((out, fps))
}

fn stage(figure: &Figure, mesh: &[u8], setup: &[u8], scene: &[u8]) -> Result<Stage, String> {
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
    let background = figure.background.as_deref().and_then(|c| css::color(&css::substitute(c, &vars), &vars));
    let fps = setup["fps"].as_f64().unwrap_or(30.0);
    Ok(Stage { renderer: r, gl, width, height, dpr, background, fps })
}

/// An image as a PNG.
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
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
