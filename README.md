<h1>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/banner-dark.svg">
    <img src="assets/banner-light.svg" height="56" alt="wgw: Wasm Geometry Widgets">
  </picture>
</h1>

Interactive 3D figures: meshes, point clouds, curves and vector fields, with sliders and clicks. One renderer,
written in Rust, draws them wherever they're needed:

- **on web pages**, compiled to WebAssembly and drawn with WebGL;
- **in PDFs**, through Typst and [tdg](https://github.com/frisbro303/tdg), drawn on the CPU;
- **from programs** in Python, Julia, R, Lean and Rust: in a window, inline in Jupyter, or saved headless as
  images and videos.

A figure is driven by a **widget**: a small WebAssembly module, written in Rust, that turns sliders and clicks into
a scene, and runs the same in all of these. Or by a function in your own language; or it's just data, a mesh or
arrays. A figure's file, `.wgw`, is a widget with its data inside, or a mesh.

wgw is in beta: its API and file format still change, without deprecations.

## On a web page

```html
<script type="module" src="wgw/wgw.js"></script>
<div class="wgw" data-src="distance.wgw" data-up="y"></div>
```

Any number of figures share one WebGL context. They can be turned with the mouse or the keyboard, and they recover
if the GPU resets. Widgets run in a few workers shared by the page, so the page never waits on them.

## A widget, in Rust

```rust
// Click Spot to color it by diffusion distance from the clicked vertex.
use wgw_widget::*;

struct Distance { spot: MeshData, embedding: Vec<f32>, k: usize }

impl Widget for Distance {
    fn new(_mesh: Option<MeshData>) -> Self {
        let spot = MeshData::parse(include_bytes!("spot.wgw")).unwrap();  // Spot and its spectrum, embedded
        let (embedding, k) = diffusion_map(&spot);
        Distance { spot, embedding, k }
    }
    fn setup(&self) -> Setup {
        Setup::new().slider("epsilon", Slider::new(0.001, 1.0, 0.02).log()).picked(925).grid(true)
    }
    fn scene(&self, state: &State) -> Scene {
        let v = state.pick().unwrap().vertex as usize;
        let at = &self.embedding[v * self.k..(v + 1) * self.k];
        let values: Vec<f32> = self.embedding.chunks_exact(self.k)
            .map(|x| (-x.iter().zip(at).map(|(a, b)| (a - b).powi(2)).sum::<f32>() / state.value("epsilon")).exp())
            .collect();
        Scene::new()
            .layer("mesh", Mesh::new(&self.spot.vertices, &self.spot.faces).values(values))
            .layer("dot", Points::new([self.spot.vertices[v]]))
    }
}

export!(Distance);
```

`cargo wgw` builds it into `distance.wgw`. That one file runs on a web page, in a PDF and from any of the heads
below. A scene is named layers (meshes, points, lines and vectors), each made of arrays and parameters; arrays
with an extra leading axis are an animation.

## From your program

The heads, in [`heads/`](heads), are thin wrappers for Python, Julia, R, Lean and Rust. A figure is a `.wgw` file,
arrays as layers, or a function of the controls, which is called again whenever a slider moves or a point is
picked.

```python
import wgw

wgw.Figure("torus.wgw").show()                                   # a widget, or a mesh file
wgw.Figure(wgw.Mesh(vertices, faces, values=curvature)).show()   # arrays
fig = wgw.Figure(lambda s: {"mesh": wgw.Mesh(vertices * s["t"], faces)}, {"t": wgw.Slider(0.5, 2, 1)})
fig.show()                                                       # a window, or inline in Jupyter
fig.save("torus.mp4", seconds=4)                                 # headless
wgw.write("spot.wgw", vertices, faces, evecs=evecs)              # a mesh file for a widget
```

```julia
using Wgw
display(Figure(s -> Dict("mesh" => Mesh(vertices .* s["t"], faces)), "t" => Slider(0.5, 2, 1)))
```

```r
wgw_figure(function(s) list(mesh = wgw_mesh(vertices * s$t, faces)), list(t = wgw_slider(0.5, 2, 1)))
```

| head | folder | show | save |
|---|---|---|---|
| Python | `heads/python` | `fig.show()`, or inline in Jupyter | `fig.save(path)` |
| Julia | `heads/julia` (`Wgw`) | `display(fig)`, or inline in Jupyter and Pluto | `savefig(fig, path)` |
| R | `heads/r` | printing a figure, or `wgw_show` | `wgw_save(fig, path)` |
| Lean 4 | `heads/lean` | `fig.display` | `fig.save path` |
| Rust | `heads/rust` (`wgw-show`) | `Figure::show` | `Figure::save` |

- **Windows:** a figure opens in a window with a webview, which closes when its program ends, so a script waits on
  showing a figure until its window is closed.
- **Saving:** headless, with no window or GPU. `.png` is a still; `.gif`, `.apng`, `.mp4`, `.webm` and `.mov` are
  videos (the last three by `ffmpeg`, if it's installed). A video plays an animation through, or turns a figure
  once around.
- **Counting:** Julia and R count faces, segments and picks from 1, as those languages do; the others from 0.
- **Rust:** `wgw_show::Figure::widget(w)` runs a widget natively, the same code `cargo wgw` compiles to a `.wgw`
  file.

Every head starts `wgw-window`, one small Rust program that shows, serves and saves figures, and talks to it over a
small protocol. `tests/heads/` checks every head against the same input.

## In a PDF

In a [tdg](https://github.com/frisbro303/tdg) site, `#widget(path("distance.wgw"))` makes the figure live in the
HTML and a still in the PDF. Typst runs the widget, and wgw's renderer draws its first frame on the CPU in tdg's
plugin, with the same shaders as the page.

## How it fits together

| folder | crate | what it is |
|---|---|---|
| `scene/` | `wgw-scene` | layers, patches (how a scene changes), `.obj` and `.wgw` files, the camera |
| `render/`, `shaders/` | `wgw-render` | the renderer, over a small GL interface, with its shaders written once in WGSL |
| `src/`, `js/` | `wgw` | the web page: the renderer on WebGL, compiled to WebAssembly, and the page code |
| `widget/`, `cargo-wgw/` | `wgw-widget`, `cargo-wgw` | writing widgets in Rust, and building them into `.wgw` files |
| `headless/` | `wgw-headless` | figures without a browser: the renderer on a CPU GL; stills, videos, running widgets |
| `window/` | `wgw-window` | the heads' program: windows with webviews, serving for Jupyter, saving |
| `heads/` | | the heads: Python, Julia, R, Lean and Rust (`wgw-show`) |

Every part speaks one format, the **patch**: named layers, each a JSON description followed by its raw arrays.
Widgets and heads make patches; the page, `wgw-headless` and `wgw-window` draw them.

## Install and build

`wgw-window` is attached, built for Linux, macOS and Windows, to tagged releases; the heads find it as
`WGW_WINDOW`, or on the PATH. Built with `--no-default-features`, it has no windows and needs no webview, for saving
on machines with no display. Until wgw is on package registries, install the heads from this repository:
`pip install heads/python`, `Pkg.develop(path="heads/julia")`, `R CMD INSTALL heads/r`, a path dependency on
`heads/lean` or `heads/rust`.

Building needs Rust with the `wasm32-unknown-unknown` target, `wasm-bindgen-cli` at the version in `Cargo.lock`
(`build.sh` says which), and optionally `wasm-opt` from [binaryen](https://github.com/WebAssembly/binaryen) for a
smaller core.

```sh
./build.sh                              # the page code and core -> dist/
./build.sh ../site/theme/wgw            # ... and copy it into a site
cargo install --path cargo-wgw          # cargo wgw: builds widgets into .wgw files
cargo wgw -p torus                      # the example widget -> examples/torus.wgw
cargo install --path window             # wgw-window, for the heads (after build.sh: it carries dist/)
python3 -m http.server                  # examples at http://localhost:8000/examples/
```

Tests: `cargo test`, `tests/heads/run.sh` (every head) and `tests/versions.sh`. The version is set in
`Cargo.toml`; the heads' packages carry it too, set by hand, and `tests/versions.sh` checks they agree. CI runs all
three.

## Docs

**https://frisbro303.github.io/wgw/**, with live figures: [figures](https://frisbro303.github.io/wgw/figures/),
[writing widgets](https://frisbro303.github.io/wgw/widgets/), the
[reference](https://frisbro303.github.io/wgw/reference/),
[the page API](https://frisbro303.github.io/wgw/javascript/),
[from other languages](https://frisbro303.github.io/wgw/heads/) and
[internals](https://frisbro303.github.io/wgw/internals/). They're Typst pages in [`docs/`](docs), a
[tdg](https://github.com/frisbro303/tdg) site: `typst compile --root . --features html,bundle --format bundle
docs/site.typ docs/_site`.
