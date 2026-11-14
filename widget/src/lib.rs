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
//!         Scene::new().layer("torus", Mesh::new(&self.mesh.vertices, &self.mesh.faces).color("var(--link)"))
//!     }
//! }
//!
//! export!(Torus);
//! ```
//!
//! An animation is frames: `Setup::frames(n)` asks for `scene` with `state.frame` from 0 to n - 1, which the page
//! plays in a loop; a still draws frame 0 alone. A layer that a later frame leaves out stays as it is in frame 0,
//! so a widget gives what never changes (a sphere, a grid) with frame 0 only, and it's sent once. The functions follow Typst's plugin protocol (two imports from
//! `typst_env`), so the same `.wasm` loads in Typst as a plugin and on a page in wgw's runner.

use std::borrow::Cow;
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
    arrays: HashMap<String, Array>,
}

/// One of a mesh file's other arrays, row-major, with its shape.
#[derive(Clone, Debug)]
pub struct Array {
    pub values: Values,
    pub shape: Vec<usize>,
}

/// An array's numbers in the type the file has them in: f64s stay f64s, and integers stay exact. The smaller types
/// are widened: f16 to f32, u8 and u16 to u32.
#[derive(Clone, Debug)]
pub enum Values {
    F32(Vec<f32>),
    F64(Vec<f64>),
    U32(Vec<u32>),
    I32(Vec<i32>),
}

impl Array {
    /// As f32s, converted if they're another type.
    pub fn f32s(&self) -> Cow<'_, [f32]> {
        match &self.values {
            Values::F32(v) => Cow::Borrowed(v),
            Values::F64(v) => v.iter().map(|&x| x as f32).collect(),
            Values::U32(v) => v.iter().map(|&x| x as f32).collect(),
            Values::I32(v) => v.iter().map(|&x| x as f32).collect(),
        }
    }
    /// As f64s, converted (exactly) if they're another type.
    pub fn f64s(&self) -> Cow<'_, [f64]> {
        match &self.values {
            Values::F64(v) => Cow::Borrowed(v),
            Values::F32(v) => v.iter().map(|&x| x as f64).collect(),
            Values::U32(v) => v.iter().map(|&x| x as f64).collect(),
            Values::I32(v) => v.iter().map(|&x| x as f64).collect(),
        }
    }
    /// As u32s, if they're unsigned integers (indices, say).
    pub fn u32s(&self) -> Option<&[u32]> {
        match &self.values {
            Values::U32(v) => Some(v),
            _ => None,
        }
    }
    /// As i32s, if they're signed integers.
    pub fn i32s(&self) -> Option<&[i32]> {
        match &self.values {
            Values::I32(v) => Some(v),
            _ => None,
        }
    }
}

/// An IEEE half-precision float, as f32.
fn f16_to_f32(h: u16) -> f32 {
    let (sign, exp, frac) = ((h >> 15) as u32, ((h >> 10) & 0x1f) as u32, (h & 0x3ff) as u32);
    let magnitude = match exp {
        0 => frac as f32 * 2f32.powi(-24), // subnormal
        0x1f => if frac == 0 { f32::INFINITY } else { f32::NAN },
        _ => f32::from_bits(((exp + 112) << 23) | (frac << 13)),
    };
    if sign == 1 { -magnitude } else { magnitude }
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
            let values = match dtype.as_str() {
                "f4" => Values::F32(raw.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect()),
                "f8" => Values::F64(raw.chunks_exact(8).map(|b| f64::from_le_bytes(b.try_into().unwrap())).collect()),
                "f2" => Values::F32(raw.chunks_exact(2).map(|b| f16_to_f32(u16::from_le_bytes([b[0], b[1]]))).collect()),
                "u4" => Values::U32(raw.chunks_exact(4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).collect()),
                "u2" => Values::U32(raw.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]]) as u32).collect()),
                "u1" => Values::U32(raw.iter().map(|&b| b as u32).collect()),
                "i4" => Values::I32(raw.chunks_exact(4).map(|b| i32::from_le_bytes(b.try_into().unwrap())).collect()),
                _ => continue,
            };
            arrays.insert(name, Array { values, shape: shape.iter().map(|&s| s as usize).collect() });
        }
        Ok(MeshData {
            vertices: v.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect(),
            faces: f.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect(),
            arrays,
        })
    }

    /// One of the file's other arrays: `mesh.array("evecs")`, then `.f32s()` (or `.f64s()`, `.u32s()`, `.i32s()`)
    /// and `.shape`.
    pub fn array(&self, name: &str) -> Option<&Array> {
        self.arrays.get(name)
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
    /// A slider's value. Panics if the widget has no slider of that name (a typo shows on the figure, rather than
    /// reading as 0).
    pub fn value(&self, name: &str) -> f32 {
        match self.values.get(name) {
            Some(v) => v.as_f64().unwrap_or_else(|| panic!("the control {name:?} is a checkbox: use state.checked")) as f32,
            None => panic!("no slider named {name:?}; the controls are {}", self.names()),
        }
    }

    /// A checkbox's state. Panics if the widget has no checkbox of that name.
    pub fn checked(&self, name: &str) -> bool {
        match self.values.get(name) {
            Some(v) => v.as_bool().unwrap_or_else(|| panic!("the control {name:?} is a slider: use state.value")),
            None => panic!("no checkbox named {name:?}; the controls are {}", self.names()),
        }
    }

    fn names(&self) -> String {
        let mut names: Vec<_> = self.values.keys().map(|k| format!("{k:?}")).collect();
        names.sort();
        if names.is_empty() { "none".into() } else { names.join(", ") }
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
    /// An animation of `n` frames (looping, at 30 per second unless set). Layers a frame after the first leaves out
    /// stay as they are in frame 0.
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

    /// Each control's starting value, as a state has it.
    fn defaults(&self) -> Map<String, Value> {
        self.controls.iter().map(|(name, spec)| (name.clone(), match spec {
            Value::Array(s) => s[2].clone(),
            other => other.clone(),
        })).collect()
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

    /// Per-vertex values, colored through the layer's colors: a `Vec<f32>` (moved in) or a slice (copied).
    pub fn values(self, values: impl Into<Vec<f32>>) -> Self {
        let values = values.into();
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

// The layers take their arrays as anything that's a slice of rows (`Vec`s, `&Vec`s, slices, arrays), so a widget
// passes what it keeps by reference, `Mesh::new(&self.vertices, &self.faces)`, rather than cloning it.

/// A triangle mesh.
pub struct Mesh;
impl Mesh {
    pub fn new(vertices: impl AsRef<[[f32; 3]]>, faces: impl AsRef<[[u32; 3]]>) -> Layer {
        let (vertices, faces) = (vertices.as_ref(), faces.as_ref());
        let (n, m) = (vertices.len(), faces.len());
        Layer::new("mesh").array("vertices", Data::F32(flat3(vertices)), vec![n, 3]).array("faces", Data::U32(flat3(faces)), vec![m, 3])
    }
    /// The figure's own mesh (from its `src`), changed only in its parameters and values.
    pub fn existing() -> Layer {
        Layer::new("mesh")
    }
}

/// Points, drawn as dots.
pub struct Points;
impl Points {
    pub fn new(vertices: impl AsRef<[[f32; 3]]>) -> Layer {
        let vertices = vertices.as_ref();
        let n = vertices.len();
        Layer::new("points").array("vertices", Data::F32(flat3(vertices)), vec![n, 3])
    }
}

/// A curve, or any set of segments.
pub struct Lines;
impl Lines {
    pub fn new(vertices: impl AsRef<[[f32; 3]]>) -> Layer {
        let vertices = vertices.as_ref();
        let n = vertices.len();
        Layer::new("lines").array("vertices", Data::F32(flat3(vertices)), vec![n, 3])
    }
}

impl Layer {
    /// Which vertices to join, in pairs; by default, each to the next.
    pub fn segments(self, pairs: impl AsRef<[[u32; 2]]>) -> Self {
        let pairs = pairs.as_ref();
        let n = pairs.len();
        self.array("segments", Data::U32(pairs.iter().flatten().copied().collect()), vec![n, 2])
    }
}

/// Arrows: a vector field.
pub struct Vectors;
impl Vectors {
    pub fn new(origins: impl AsRef<[[f32; 3]]>, vectors: impl AsRef<[[f32; 3]]>) -> Layer {
        let (origins, vectors) = (origins.as_ref(), vectors.as_ref());
        let n = origins.len();
        Layer::new("vectors").array("vertices", Data::F32(flat3(origins)), vec![n, 3]).array("vectors", Data::F32(flat3(vectors)), vec![n, 3])
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
        // Every control has a value: the page sends them all, and any a host leaves out (a still, say) starts as
        // the setup says.
        let mut values = w.setup().defaults();
        values.extend(v["controls"].as_object().cloned().unwrap_or_default());
        let state = State {
            values,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A .wgw file of one triangle and these extra arrays: (name, dtype, shape, bytes).
    fn wgw(extra: &[(&str, &str, &[usize], Vec<u8>)]) -> Vec<u8> {
        let vertices: Vec<u8> = [0f32, 0., 0., 1., 0., 0., 0., 1., 0.].iter().flat_map(|x| x.to_le_bytes()).collect();
        let faces: Vec<u8> = [0u32, 1, 2].iter().flat_map(|x| x.to_le_bytes()).collect();
        let mut arrays = vec![("vertices", "f4", &[3usize, 3][..], vertices), ("faces", "u4", &[1, 3][..], faces)];
        arrays.extend(extra.iter().map(|(n, d, s, b)| (*n, *d, *s, b.clone())));
        let (mut header, mut data) = (Map::new(), Vec::new());
        for (name, dtype, shape, bytes) in arrays {
            header.insert(name.into(), json!({ "dtype": dtype, "shape": shape, "offset": data.len() }));
            data.extend(bytes);
            data.resize(data.len().next_multiple_of(8), 0);
        }
        let header = json!({ "arrays": header }).to_string();
        let mut out = b"WGW1".to_vec();
        out.extend((header.len() as u32).to_le_bytes());
        out.extend(header.as_bytes());
        out.extend(data);
        out
    }

    #[test]
    fn arrays_keep_their_type() {
        let big = [16_777_217u32, 3];
        let file = wgw(&[
            ("ids", "u4", &[2], big.iter().flat_map(|x| x.to_le_bytes()).collect()),
            ("exact", "f8", &[1], 0.1f64.to_le_bytes().to_vec()),
            ("half", "f2", &[3], [0x3c00u16, 0xc000, 0x0001].iter().flat_map(|x| x.to_le_bytes()).collect()),
            ("bytes", "u1", &[2], vec![7, 255]),
        ]);
        let mesh = MeshData::parse(&file).unwrap();
        assert_eq!(mesh.faces, vec![[0, 1, 2]]);
        assert_eq!(mesh.array("ids").unwrap().u32s(), Some(&big[..])); // above 2^24, still exact
        assert_eq!(mesh.array("exact").unwrap().f64s()[0], 0.1);
        assert_eq!(mesh.array("half").unwrap().f32s().as_ref(), &[1.0, -2.0, 2f32.powi(-24)]);
        assert_eq!(mesh.array("bytes").unwrap().u32s(), Some(&[7, 255][..]));
        assert_eq!(mesh.array("ids").unwrap().shape, vec![2]);
        assert!(mesh.array("nothing").is_none());
    }

    struct Probe;
    impl Widget for Probe {
        fn new(_: Option<MeshData>) -> Self { Probe }
        fn setup(&self) -> Setup {
            Setup::new().slider("epsilon", Slider::new(0.001, 1.0, 0.02).log()).checkbox("smooth", true)
        }
        fn scene(&self, state: &State) -> Scene {
            let at = [[state.value("epsilon"), state.checked("smooth") as u8 as f32, 0.0]];
            Scene::new().layer("dot", Points::new(at))
        }
    }

    fn state(values: Value) -> State {
        State { values: values.as_object().unwrap().clone(), ..Default::default() }
    }

    #[test]
    fn controls_by_name() {
        let s = state(json!({ "epsilon": 0.5, "smooth": false }));
        assert_eq!(s.value("epsilon"), 0.5);
        assert!(!s.checked("smooth"));
    }

    #[test]
    #[should_panic(expected = "no slider named \"epsilom\"; the controls are \"epsilon\", \"smooth\"")]
    fn a_misspelled_slider_panics() {
        state(json!({ "epsilon": 0.5, "smooth": false })).value("epsilom");
    }

    #[test]
    #[should_panic(expected = "is a checkbox")]
    fn a_checkbox_read_as_a_slider_panics() {
        state(json!({ "smooth": false })).value("smooth");
    }

    #[test]
    fn controls_left_out_start_as_set_up() {
        // A host that sends no controls (a still) gets each control's starting value.
        let bytes = runtime::scene(&Probe, br#"{"controls": {"smooth": false}}"#).unwrap();
        let patch = patch::decode(&bytes).unwrap();
        let (_, dot) = &patch.layers[0];
        let Data::F32(v) = &dot.as_ref().unwrap().arrays[0].1.data else { panic!("f32 vertices") };
        assert_eq!(v, &vec![0.02, 0.0, 0.0]);
    }
}
