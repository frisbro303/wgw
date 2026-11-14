//! wgw for Rust programs: a [`Widget`], a [`Scene`] or a `.wgw` file in a window. A thin head over wgw-window (in
//! wgw's repository, found as `WGW_WINDOW` or on the PATH), which shows the figure in a webview, running wgw's page
//! code and core; a widget's scenes are made here, natively, by the same code that compiles to a `.wgw` file.
//!
//! ```no_run
//! use wgw_show::{Figure, Save};
//! use wgw_widget::*;
//!
//! # struct Torus;
//! # impl Widget for Torus {
//! #     fn new(_: Option<MeshData>) -> Self { Torus }
//! #     fn setup(&self) -> Setup { Setup::new() }
//! #     fn scene(&self, _: &State) -> Scene { Scene::new() }
//! # }
//! Figure::widget(Torus::new(None)).title("torus").show()?;           // its sliders call its scene, here
//! Figure::scene(Scene::new().layer("dots", Points::new([[0.0, 0.0, 0.0]]))).show()?;
//! Figure::file("examples/torus.wgw")?.show()?;                        // a widget compiled to wasm, or a mesh
//! Figure::widget(Torus::new(None)).save("torus.mp4", &Save::default().seconds(2.0))?;  // headless
//! # Ok::<(), std::io::Error>(())
//! ```
//!
//! `show` waits until the figure's window is closed; the window closes when the program ends. `save` draws the
//! figure headless, on the CPU, as an image or a video.

use std::io::{self, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::{json, Value};
use wgw_scene::patch;
use wgw_widget::{runtime, Scene, Widget};

/// A widget's scene: the state's JSON in, the patch's bytes out.
type MakeScene<'a> = Box<dyn Fn(&[u8]) -> Result<Vec<u8>, String> + 'a>;

enum Source<'a> {
    File(Vec<u8>),
    Scene(Vec<u8>),
    Widget { setup: Value, scene: MakeScene<'a> },
}

/// How to save a figure: its size in CSS pixels (800 by its height, by default), image pixels to each (2), a CSS
/// background color (transparent for PNG, white for videos), the controls' values (as set up by default), and for
/// a video, its frames per second, how many frames or seconds, and how far it turns (360 degrees over 4 seconds, for
/// a figure that doesn't animate; an animation plays through).
#[derive(Clone)]
pub struct Save {
    pub width: f64,
    pub height: Option<f64>,
    pub scale: f64,
    pub background: Option<String>,
    pub controls: serde_json::Map<String, Value>,
    pub fps: Option<f64>,
    pub frames: Option<u32>,
    pub seconds: Option<f64>,
    pub spin: Option<f64>,
}

impl Default for Save {
    fn default() -> Self {
        Save { width: 800.0, height: None, scale: 2.0, background: None, controls: Default::default(), fps: None, frames: None, seconds: None, spin: None }
    }
}

impl Save {
    pub fn size(mut self, width: f64, height: f64) -> Self {
        (self.width, self.height) = (width, Some(height));
        self
    }
    pub fn scale(mut self, scale: f64) -> Self {
        self.scale = scale;
        self
    }
    pub fn background(mut self, css: &str) -> Self {
        self.background = Some(css.into());
        self
    }
    /// A control's value: a number for a slider, a bool for a checkbox.
    pub fn control(mut self, name: &str, value: impl Into<Value>) -> Self {
        self.controls.insert(name.into(), value.into());
        self
    }
    pub fn fps(mut self, fps: f64) -> Self {
        self.fps = Some(fps);
        self
    }
    pub fn frames(mut self, n: u32) -> Self {
        self.frames = Some(n);
        self
    }
    pub fn seconds(mut self, s: f64) -> Self {
        self.seconds = Some(s);
        self
    }
    /// Degrees to turn about the up axis over the video.
    pub fn spin(mut self, degrees: f64) -> Self {
        self.spin = Some(degrees);
        self
    }
}

fn spawn(args: &[&str]) -> io::Result<std::process::Child> {
    let program = std::env::var("WGW_WINDOW").unwrap_or_else(|_| "wgw-window".into());
    // The protocol this head speaks to wgw-window (docs/heads.typ).
    Command::new(&program).args(["--protocol", "1"]).args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().map_err(|e| {
        io::Error::new(e.kind(), format!("{program}: {e} (build it in wgw's repository: cargo install --path window)"))
    })
}

/// A figure, with its window's options.
pub struct Figure<'a> {
    source: Source<'a>,
    title: String,
    options: Value,
    height: u32,
}

impl<'a> Figure<'a> {
    fn new(source: Source<'a>) -> Self {
        Figure { source, title: "wgw".into(), options: json!({ "up": "z", "view": [-60, 20], "pick": "click" }), height: 420 }
    }

    /// A widget, run here: its setup (controls, frames) as it says, and its scene for each state.
    pub fn widget<W: Widget + 'a>(widget: W) -> Self {
        let setup: Value = serde_json::from_slice(&runtime::setup(&widget)).unwrap_or(json!({}));
        let frames = setup["frames"].as_u64().unwrap_or(1).max(1);
        Figure::new(Source::Widget {
            setup,
            scene: Box::new(move |state: &[u8]| {
                let mut state: Value = serde_json::from_slice(state).map_err(|e| e.to_string())?;
                let patches = (0..frames)
                    .map(|k| {
                        state["frame"] = json!(k);
                        runtime::scene(&widget, state.to_string().as_bytes()).and_then(|bytes| patch::decode(&bytes))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(patch::encode(&patch::stack(patches)))
            }),
        })
    }

    /// A scene, as it is.
    pub fn scene(scene: Scene) -> Self {
        Figure::new(Source::Scene(scene.to_bytes()))
    }

    /// A .wgw file: a widget (Rust, compiled to WebAssembly by `cargo wgw`), or a mesh.
    pub fn file(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        Ok(Figure::new(Source::File(std::fs::read(path)?)).title(&path.file_name().unwrap_or_default().to_string_lossy()))
    }

    pub fn title(mut self, title: &str) -> Self {
        self.title = title.into();
        self
    }
    /// The axis that points up: "x", "y" or "z".
    pub fn up(mut self, axis: &str) -> Self {
        self.options["up"] = json!(axis);
        self
    }
    /// The starting camera: turn and tilt, in degrees.
    pub fn view(mut self, turn: f64, tilt: f64) -> Self {
        self.options["view"] = json!([turn, tilt]);
        self
    }
    /// When a pick reaches the widget: "click", "hover" or "none".
    pub fn pick(mut self, when: &str) -> Self {
        self.options["pick"] = json!(when);
        self
    }
    /// The figure's height, in CSS pixels.
    pub fn height(mut self, px: u32) -> Self {
        self.height = px;
        self
    }

    /// Save the figure, headless: an image (.png) or a video (.gif, .apng; .mp4, .webm or .mov with ffmpeg on the
    /// PATH), drawn on the CPU by wgw's own renderer.
    pub fn save(&self, path: impl AsRef<Path>, how: &Save) -> io::Result<()> {
        let path = std::path::absolute(path.as_ref())?;
        let (kind, body, setup) = match &self.source {
            Source::File(bytes) => ("src", bytes.clone(), json!(null)),
            Source::Scene(bytes) => ("scene", bytes.clone(), json!({ "controls": {} })),
            Source::Widget { setup, scene } => {
                let state = json!({ "controls": how.controls, "picks": [] }).to_string();
                ("scene", scene(state.as_bytes()).map_err(io::Error::other)?, setup.clone())
            }
        };
        let figure = json!({ "width": how.width, "height": how.height.unwrap_or(self.height as f64), "scale": how.scale,
            "background": how.background, "up": self.options["up"], "view": self.options["view"], "flat": false });
        let video = json!({ "fps": how.fps, "frames": how.frames, "seconds": how.seconds, "spin": how.spin });
        let mut child = spawn(&["--serve"])?;
        let mut to = child.stdin.take().unwrap();
        let mut from = BufReader::new(child.stdout.take().unwrap());
        write_frame(&mut to, &json!({ "type": "save", "request": 1, "path": path, "figure": figure, "video": video,
            "controls": how.controls, "setup": setup, "body": kind }), &body)?;
        let (header, _) = read_frame(&mut from)?;
        drop(to);
        child.wait()?;
        match header.strip_prefix("failed 1 ") {
            Some(why) => Err(io::Error::other(format!("wgw: can't save {}: {why}", path.display()))),
            None => Ok(()),
        }
    }

    /// Show the figure in a window, and wait until it's closed, making the widget's scenes meanwhile.
    pub fn show(self) -> io::Result<()> {
        let mut child = spawn(&[])?;
        let mut to = child.stdin.take().unwrap();
        let mut from = BufReader::new(child.stdout.take().unwrap());
        let (kind, body, setup) = match &self.source {
            Source::File(bytes) => ("src", bytes.clone(), json!(null)),
            Source::Scene(bytes) => ("scene", bytes.clone(), json!({ "controls": {} })),
            Source::Widget { setup, .. } => ("callback", Vec::new(), setup.clone()),
        };
        let header = json!({ "type": "figure", "id": "1", "title": self.title, "options": self.options, "setup": setup,
            "height": self.height, "body": kind });
        write_frame(&mut to, &header, &body)?;
        loop {
            let (header, body) = match read_frame(&mut from) {
                Ok(frame) => frame,
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e),
            };
            let words: Vec<&str> = header.splitn(3, ' ').collect();
            match (words[0], &self.source) {
                ("closed", _) => break,
                ("scene", Source::Widget { scene, .. }) => {
                    let request: u64 = words[1].parse().unwrap_or(0);
                    match scene(state_json(&String::from_utf8_lossy(&body)).to_string().as_bytes()) {
                        Ok(patch) => write_frame(&mut to, &json!({ "type": "scene", "request": request }), &patch)?,
                        Err(message) => {
                            eprintln!("wgw: {message}");
                            write_frame(&mut to, &json!({ "type": "error", "request": request, "message": message }), &[])?
                        }
                    }
                }
                _ => {}
            }
        }
        drop(to); // closing its stdin ends it
        child.wait()?;
        Ok(())
    }
}

/// wgw-window's state lines (`control <value> <name>`, `pick <vertex> <frame> <x> <y> <z> <layer>`) as the JSON a
/// widget reads.
fn state_json(lines: &str) -> Value {
    let (mut controls, mut picks) = (serde_json::Map::new(), Vec::new());
    for line in lines.lines() {
        let word: Vec<&str> = line.splitn(7, ' ').collect();
        match word[..] {
            ["control", value, ref name @ ..] => {
                controls.insert(name.join(" "), serde_json::from_str(value).unwrap_or(Value::Null));
            }
            ["pick", vertex, frame, x, y, z, layer] => {
                let n = |s: &str| s.parse::<f64>().ok();
                picks.push(json!({ "vertex": n(vertex), "frame": n(frame), "point": [n(x), n(y), n(z)], "layer": layer }));
            }
            _ => {}
        }
    }
    json!({ "controls": controls, "picks": picks })
}

fn write_frame(to: &mut impl Write, header: &Value, body: &[u8]) -> io::Result<()> {
    let text = header.to_string();
    to.write_all(&(text.len() as u32).to_le_bytes())?;
    to.write_all(text.as_bytes())?;
    to.write_all(&(body.len() as u32).to_le_bytes())?;
    to.write_all(body)?;
    to.flush()
}

fn read_frame(from: &mut impl Read) -> io::Result<(String, Vec<u8>)> {
    let mut part = || -> io::Result<Vec<u8>> {
        let mut n = [0u8; 4];
        from.read_exact(&mut n)?;
        let mut bytes = vec![0u8; u32::from_le_bytes(n) as usize];
        from.read_exact(&mut bytes)?;
        Ok(bytes)
    };
    Ok((String::from_utf8_lossy(&part()?).into_owned(), part()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgw_scene::array::Data;
    use wgw_widget::*;

    struct Spin;
    impl Widget for Spin {
        fn new(_: Option<MeshData>) -> Self {
            Spin
        }
        fn setup(&self) -> Setup {
            Setup::new().slider("r", Slider::new(0.0, 2.0, 1.0)).frames(3)
        }
        fn scene(&self, s: &State) -> Scene {
            let x = s.value("r") * s.frame as f32;
            Scene::new().layer("dot", Points::new([[x, 0.0, 0.0]])).layer("still", Points::new([[0.0, 1.0, 0.0]]))
        }
    }

    #[test]
    fn a_widgets_frames_are_stacked() {
        let Source::Widget { scene, .. } = Figure::widget(Spin).source else { unreachable!() };
        let state = state_json("control 2 r\npick 4 0 0.5 0 1 the mesh\n");
        assert_eq!(state["picks"][0]["layer"], "the mesh");
        let patch = patch::decode(&scene(state.to_string().as_bytes()).unwrap()).unwrap();
        let dot = patch.layers.iter().find(|(n, _)| n == "dot").unwrap().1.as_ref().unwrap();
        let vertices = &dot.arrays.iter().find(|(k, _)| k == "vertices").unwrap().1;
        assert_eq!(vertices.shape, vec![3, 1, 3]);
        assert!(matches!(&vertices.data, Data::F32(v) if v[0] == 0.0 && v[3] == 2.0 && v[6] == 4.0));
    }
}
