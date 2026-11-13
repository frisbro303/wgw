//! Writing a wgw widget in Rust. A widget is one WebAssembly module: its code and any data it needs (a mesh, a
//! spectrum, embedded with `include_bytes!`). A page runs it live; a Typst document (through tdg) runs it for the
//! figure's still. Both call the same two functions:
//!
//! - `setup(mesh)`: what the widget is, as JSON: its controls, its frames, and how its figure starts. `mesh` is the
//!   figure's mesh file (`data-src`), if it has one; a widget with its data embedded ignores it.
//! - `scene(mesh, state)`: the scene for a state (the controls' values, the pick, the frame), as a patch (see
//!   wgw-scene's `patch`): the layers to show.
//!
//! A widget implements [`Widget`] and exports it with [`export!`]:
//!
//! ```ignore
//! use wgw_widget::*;
//!
//! struct Torus { mesh: MeshData }
//!
//! impl Widget for Torus {
//!     fn new(_mesh: Option<MeshData>) -> Self {
//!         Torus { mesh: MeshData::parse(include_bytes!("torus.obj")).unwrap() }
//!     }
//!     fn setup(&self) -> Setup {
//!         Setup::new().slider("twist", Slider::new(0.0, 1.0, 0.5))
//!     }
//!     fn scene(&self, state: &State) -> Scene {
//!         let twist = state.value("twist");
//!         Scene::new().layer("torus", Mesh::new(self.mesh.vertices.clone(), self.mesh.faces.clone()).color("var(--link)"))
//!     }
//! }
//!
//! export!(Torus);
//! ```
//!
//! An animation is frames: `Setup::frames(n)` asks for `scene` with `state.frame` from 0 to n - 1, which the page
//! plays in a loop; a still draws frame 0 alone. The functions follow Typst's plugin protocol (two imports from
//! `typst_env`), so the same `.wasm` loads in Typst as a plugin and on a page in wgw's runner.

use std::collections::HashMap;

use serde_json::{json, Map, Value};
pub use wgw_scene::array::{Data, NdArray};
use wgw_scene::mesh_file;
use wgw_scene::patch::{self, LayerPatch, Patch};

pub use serde_json;

// ---- what a widget sees

/// A mesh file's contents: vertices and faces, and any other arrays (a .wgw file can hold any).
#[derive(Clone, Debug, Default)]
pub struct MeshData {
    pub vertices: Vec<[f32; 3]>,
    pub faces: Vec<[u32; 3]>,
    arrays: HashMap<String, (Vec<f32>, Vec<usize>)>,
}

impl MeshData {
    /// A mesh file's bytes: Wavefront .obj, or .wgw (which may hold more arrays than vertices and faces).
    pub fn parse(bytes: &[u8]) -> Result<MeshData, String> {
        let format = if bytes.starts_with(b"WGW1") { "wgw" } else { "obj" };
        let file = mesh_file::parse(bytes, format)?;
        let (v, _) = file.vertices()?;
        let (f, _) = file.faces()?;
        let mut arrays = HashMap::new();
        for name in file.names() {
            if name == "vertices" || name == "faces" {
                continue;
            }
            let (Some(dtype), Some(shape), Some(raw)) = (file.dtype(&name), file.shape(&name), file.bytes(&name)) else { continue };
            let values: Vec<f32> = match dtype.as_str() {
                "f4" => raw.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect(),
                "f8" => raw.chunks_exact(8).map(|b| f64::from_le_bytes(b.try_into().unwrap()) as f32).collect(),
                "u4" => raw.chunks_exact(4).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as f32).collect(),
                "i4" => raw.chunks_exact(4).map(|b| i32::from_le_bytes(b.try_into().unwrap()) as f32).collect(),
                _ => continue,
            };
            arrays.insert(name, (values, shape.iter().map(|&s| s as usize).collect()));
        }
        Ok(MeshData {
            vertices: v.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect(),
            faces: f.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect(),
            arrays,
        })
    }

    /// One of the file's other arrays, as floats, with its shape: `mesh.array("evecs")`.
    pub fn array(&self, name: &str) -> Option<(&[f32], &[usize])> {
        self.arrays.get(name).map(|(v, s)| (v.as_slice(), s.as_slice()))
    }
}

/// Where a pick landed: the vertex (or element), its layer, and the point (in the scene's units); `frame` is the
/// animation frame that was on screen, to start something new in step with it.
#[derive(Clone, Debug)]
pub struct Pick {
    pub vertex: u32,
    pub layer: String,
    pub point: Option<[f32; 3]>,
    pub frame: u32,
}

/// What a scene is for: the controls' values, the picks so far (the latest last), and the frame.
#[derive(Clone, Debug, Default)]
pub struct State {
    values: Map<String, Value>,
    pub picks: Vec<Pick>,
    pub frame: u32,
}

impl State {
    /// A slider's value.
    pub fn value(&self, name: &str) -> f32 {
        self.values.get(name).and_then(Value::as_f64).unwrap_or(0.0) as f32
    }

    /// A checkbox's state.
    pub fn checked(&self, name: &str) -> bool {
        self.values.get(name).and_then(Value::as_bool).unwrap_or(false)
    }

    /// The latest pick, if any.
    pub fn pick(&self) -> Option<&Pick> {
        self.picks.last()
    }
}

// ---- what a widget says about itself

/// A slider: its range and starting value; `log` moves evenly in powers of ten.
#[derive(Clone, Debug)]
pub struct Slider {
    min: f64,
    max: f64,
    value: f64,
    step: Option<f64>,
    log: bool,
}

impl Slider {
    pub fn new(min: f64, max: f64, value: f64) -> Self {
        Slider { min, max, value, step: None, log: false }
    }
    pub fn step(mut self, step: f64) -> Self {
        self.step = Some(step);
        self
    }
    pub fn log(mut self) -> Self {
        self.log = true;
        self
    }
}

/// A widget's controls, frames and starting state.
#[derive(Clone, Debug, Default)]
pub struct Setup {
    controls: Map<String, Value>,
    frames: Option<u32>,
    fps: Option<f64>,
    looped: bool,
    grid: Option<Value>,
    picked: Option<u32>,
    panes: Option<Vec<f64>>,
    keep_picks: bool,
}

impl Setup {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn slider(mut self, name: &str, s: Slider) -> Self {
        let mut spec = vec![json!(s.min), json!(s.max), json!(s.value)];
        if let Some(step) = s.step {
            spec.push(json!(step));
        }
        if s.log {
            spec.push(json!("log"));
        }
        self.controls.insert(name.into(), Value::Array(spec));
        self
    }
    pub fn checkbox(mut self, name: &str, checked: bool) -> Self {
        self.controls.insert(name.into(), json!(checked));
        self
    }
    /// An animation of `n` frames (looping, at 30 per second unless set).
    pub fn frames(mut self, n: u32) -> Self {
        self.frames = Some(n);
        self.looped = true;
        self
    }
    pub fn fps(mut self, fps: f64) -> Self {
        self.fps = Some(fps);
        self
    }
    /// Play the frames once rather than looping.
    pub fn once(mut self) -> Self {
        self.looped = false;
        self
    }
    /// Draw meshes' triangle grids.
    pub fn grid(mut self, on: bool) -> Self {
        self.grid = Some(json!(on));
        self
    }
    /// A checkbox for readers to show meshes' grids, starting as given.
    pub fn grid_toggle(mut self, on: bool) -> Self {
        self.grid = Some(json!({ "toggle": on }));
        self
    }
    /// A vertex of the figure's mesh (the layer "mesh") picked from the start.
    pub fn picked(mut self, vertex: u32) -> Self {
        self.picked = Some(vertex);
        self
    }
    /// Layers' panes' relative widths, such as `[2.0, 1.0]`.
    pub fn panes(mut self, widths: &[f64]) -> Self {
        self.panes = Some(widths.to_vec());
        self
    }
    /// Keep every pick (each click adds one) rather than only the latest.
    pub fn keep_picks(mut self) -> Self {
        self.keep_picks = true;
        self
    }

    fn to_json(&self) -> Value {
        json!({
            "controls": self.controls,
            "frames": self.frames.unwrap_or(1),
            "fps": self.fps.unwrap_or(30.0),
            "loop": self.looped,
            "grid": self.grid.clone().unwrap_or(json!(false)),
            "picked": self.picked,
            "panes": self.panes,
            "keepPicks": self.keep_picks,
        })
    }
}

// ---- layers and scenes

/// A layer: a kind, arrays and parameters (see wgw's docs for each kind's).
#[derive(Clone, Debug)]
pub struct Layer(LayerPatch);

fn flat3<T: Copy>(rows: &[[T; 3]]) -> Vec<T> {
    rows.iter().flatten().copied().collect()
}

impl Layer {
    fn new(kind: &str) -> Self {
        Layer(LayerPatch { kind: kind.into(), ..Default::default() })
    }

    fn array(mut self, key: &str, data: Data, shape: Vec<usize>) -> Self {
        self.0.arrays.retain(|(k, _)| k != key);
        self.0.arrays.push((key.into(), NdArray::new(data, shape).expect("an array's shape matches its data")));
        self
    }

    /// Any parameter: a number, boolean or CSS color.
    pub fn param(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.0.params.insert(key.into(), value.into());
        self
    }

    /// Per-vertex values, colored through the layer's colors.
    pub fn values(self, values: Vec<f32>) -> Self {
        let n = values.len();
        self.array("values", Data::F32(values), vec![n])
    }
    /// One CSS color for the whole layer; `var(--name)` follows the page's theme.
    pub fn color(self, css: &str) -> Self {
        self.param("color", css)
    }
    /// The colors values run through.
    pub fn colors(self, css: &[&str]) -> Self {
        self.param("colors", Value::Array(css.iter().map(|c| json!(c)).collect()))
    }
    /// The range of values the colors span (0 to 1 by default).
    pub fn range(self, min: f64, max: f64) -> Self {
        self.param("min", min).param("max", max)
    }
    pub fn width(self, px: f64) -> Self {
        self.param("width", px)
    }
    pub fn size(self, px: f64) -> Self {
        self.param("size", px)
    }
    /// The pane it's drawn in: 1 for a second view beside the first.
    pub fn pane(self, pane: u32) -> Self {
        self.param("pane", pane)
    }
    pub fn pickable(self, on: bool) -> Self {
        self.param("pickable", on)
    }
    pub fn visible(self, on: bool) -> Self {
        self.param("visible", on)
    }
    pub fn flat(self, on: bool) -> Self {
        self.param("flat", on)
    }
}

/// A triangle mesh.
pub struct Mesh;
impl Mesh {
    pub fn new(vertices: Vec<[f32; 3]>, faces: Vec<[u32; 3]>) -> Layer {
        let (n, m) = (vertices.len(), faces.len());
        Layer::new("mesh").array("vertices", Data::F32(flat3(&vertices)), vec![n, 3]).array("faces", Data::U32(flat3(&faces)), vec![m, 3])
    }
    /// The figure's own mesh (from its `src`), changed only in its parameters and values.
    pub fn existing() -> Layer {
        Layer::new("mesh")
    }
}

/// Points, drawn as dots.
pub struct Points;
impl Points {
    pub fn new(vertices: Vec<[f32; 3]>) -> Layer {
        let n = vertices.len();
        Layer::new("points").array("vertices", Data::F32(flat3(&vertices)), vec![n, 3])
    }
}

/// A curve, or any set of segments.
pub struct Lines;
impl Lines {
    pub fn new(vertices: Vec<[f32; 3]>) -> Layer {
        let n = vertices.len();
        Layer::new("lines").array("vertices", Data::F32(flat3(&vertices)), vec![n, 3])
    }
}

impl Layer {
    /// Which vertices to join, in pairs; by default, each to the next.
    pub fn segments(self, pairs: Vec<[u32; 2]>) -> Self {
        let n = pairs.len();
        self.array("segments", Data::U32(pairs.iter().flatten().copied().collect()), vec![n, 2])
    }
}

/// Arrows: a vector field.
pub struct Vectors;
impl Vectors {
    pub fn new(origins: Vec<[f32; 3]>, vectors: Vec<[f32; 3]>) -> Layer {
        let n = origins.len();
        Layer::new("vectors").array("vertices", Data::F32(flat3(&origins)), vec![n, 3]).array("vectors", Data::F32(flat3(&vectors)), vec![n, 3])
    }
}

/// A scene: layers by name, in order; a name given `remove` is taken away.
#[derive(Clone, Debug, Default)]
pub struct Scene(Patch);

impl Scene {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn layer(mut self, name: &str, layer: Layer) -> Self {
        self.0.layers.push((name.into(), Some(layer.0)));
        self
    }
    pub fn remove(mut self, name: &str) -> Self {
        self.0.layers.push((name.into(), None));
        self
    }
}

// ---- the widget and its exports

/// A widget: built from the figure's mesh (if any), it says what it is, and draws scenes.
pub trait Widget: Sized {
    fn new(mesh: Option<MeshData>) -> Self;
    fn setup(&self) -> Setup;
    fn scene(&self, state: &State) -> Scene;
}

#[doc(hidden)]
pub mod runtime {
    use super::*;
    use std::cell::RefCell;

    #[link(wasm_import_module = "typst_env")]
    extern "C" {
        fn wasm_minimal_protocol_send_result_to_host(ptr: *const u8, len: usize);
        fn wasm_minimal_protocol_write_args_to_buffer(ptr: *mut u8);
    }

    /// The call's arguments, by their lengths.
    pub fn args(lengths: &[usize]) -> Vec<Vec<u8>> {
        let mut buffer = vec![0u8; lengths.iter().sum()];
        unsafe { wasm_minimal_protocol_write_args_to_buffer(buffer.as_mut_ptr()) };
        let mut out = Vec::new();
        let mut at = 0;
        for &n in lengths {
            out.push(buffer[at..at + n].to_vec());
            at += n;
        }
        out
    }

    /// Hand a result (or an error message) to the host: 0 for a result, 1 for an error.
    pub fn send(result: Result<Vec<u8>, String>) -> i32 {
        let (bytes, code) = match result {
            Ok(b) => (b, 0),
            Err(e) => (e.into_bytes(), 1),
        };
        unsafe { wasm_minimal_protocol_send_result_to_host(bytes.as_ptr(), bytes.len()) };
        code
    }

    /// The widget, built once per mesh and kept: a page calls `scene` many times with the same mesh (or none, to
    /// mean the same as before).
    pub fn with<W: Widget, R>(cell: &'static std::thread::LocalKey<RefCell<Option<(u64, W)>>>, mesh: &[u8], f: impl FnOnce(&W) -> R) -> Result<R, String> {
        let key = if mesh.is_empty() { None } else { Some(hash(mesh)) };
        cell.with(|c| {
            let mut c = c.borrow_mut();
            let fresh = match (&*c, key) {
                (Some((k, _)), Some(key)) => *k != key,
                (Some(_), None) => false,
                (None, _) => true,
            };
            if fresh {
                let data = if mesh.is_empty() { None } else { Some(MeshData::parse(mesh)?) };
                *c = Some((key.unwrap_or(0), W::new(data)));
            }
            Ok(f(&c.as_ref().unwrap().1))
        })
    }

    fn hash(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf29ce484222325u64, |h, &b| (h ^ b as u64).wrapping_mul(0x100000001b3))
    }

    pub fn setup<W: Widget>(w: &W) -> Vec<u8> {
        w.setup().to_json().to_string().into_bytes()
    }

    /// The state from its JSON: {"controls": {...}, "picks": [{vertex, layer, point, frame}], "frame": k}.
    pub fn scene<W: Widget>(w: &W, state: &[u8]) -> Result<Vec<u8>, String> {
        let v: Value = if state.is_empty() { json!({}) } else { serde_json::from_slice(state).map_err(|e| format!("the state: {e}"))? };
        let picks = v["picks"].as_array().into_iter().flatten().map(|p| Pick {
            vertex: p["vertex"].as_u64().unwrap_or(0) as u32,
            layer: p["layer"].as_str().unwrap_or("mesh").to_string(),
            point: p["point"].as_array().filter(|a| a.len() == 3).map(|a| [0, 1, 2].map(|i| a[i].as_f64().unwrap_or(0.0) as f32)),
            frame: p["frame"].as_u64().unwrap_or(0) as u32,
        }).collect();
        let state = State {
            values: v["controls"].as_object().cloned().unwrap_or_default(),
            picks,
            frame: v["frame"].as_u64().unwrap_or(0) as u32,
        };
        Ok(patch::encode(&w.scene(&state).0))
    }
}

/// Export a [`Widget`] as the module's `setup` and `scene`.
#[macro_export]
macro_rules! export {
    ($widget:ty) => {
        thread_local! {
            static WIDGET: ::std::cell::RefCell<Option<(u64, $widget)>> = ::std::cell::RefCell::new(None);
        }

        #[no_mangle]
        pub extern "C" fn setup(mesh_len: usize) -> i32 {
            let args = $crate::runtime::args(&[mesh_len]);
            $crate::runtime::send($crate::runtime::with(&WIDGET, &args[0], |w: &$widget| $crate::runtime::setup(w)))
        }

        #[no_mangle]
        pub extern "C" fn scene(mesh_len: usize, state_len: usize) -> i32 {
            let args = $crate::runtime::args(&[mesh_len, state_len]);
            $crate::runtime::send($crate::runtime::with(&WIDGET, &args[0], |w: &$widget| $crate::runtime::scene(w, &args[1])).and_then(|r| r))
        }
    };
}
