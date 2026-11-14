<h1>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/banner-dark.svg">
    <img src="assets/banner-light.svg" height="56" alt="wgw: Web Geometry Widgets">
  </picture>
</h1>

Interactive 3D figures for web pages and PDFs: meshes, point clouds, curves and vector fields, drawn by a Rust
core compiled to WebAssembly and driven by **widgets**: small WebAssembly modules, written in Rust, that respond to
sliders and clicks. A figure is one `.wgw` file, a widget with its data inside, or a mesh.

wgw is in beta: its API and file format still change, without deprecations.

```html
<script type="module" src="wgw/wgw.js"></script>
<div class="wgw" data-src="distance.wgw" data-up="y"></div>
```

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
        let values = self.embedding.chunks_exact(self.k)
            .map(|x| (-x.iter().zip(at).map(|(a, b)| (a - b).powi(2)).sum::<f32>() / state.value("epsilon")).exp())
            .collect();
        Scene::new()
            .layer("mesh", Mesh::new(&self.spot.vertices, &self.spot.faces).values(values))
            .layer("dot", Points::new(vec![self.spot.vertices[v]]))
    }
}

export!(Distance);
```

In a [tdg](https://github.com/frisbro303/tdg) site, `#widget(path("distance.wgw"))` makes the same figure, and its
PDF shows the figure's first frame, drawn by wgw's own renderer with the page's shaders.

## Python, Julia, R, Lean and Rust

The same figures in a window, from a program: the heads in [`heads/`](heads), for Python, Julia, R, Lean and Rust
(`wgw-show`), are thin wrappers over `wgw-window` ([`window/`](window)), a small Rust
program that shows each figure in a webview, running wgw's page code and core, and asks the head for a new scene
when a slider moves or a point is picked. A figure is a `.wgw` file (a widget or a mesh), arrays as layers, or a
function of the controls:

```python
import wgw
wgw.Figure("examples/torus.wgw").show()
wgw.Figure(lambda s: {"mesh": wgw.Mesh(vertices * s["t"], faces)}, {"t": wgw.Slider(0.5, 2, 1)}).show()
```

```julia
using Wgw
display(Figure(s -> Dict("mesh" => Mesh(vertices .* s["t"], faces)), "t" => Slider(0.5, 2, 1)))
```

```r
wgw_figure(function(s) list(mesh = wgw_mesh(vertices * s$t, faces)), list(t = wgw_slider(0.5, 2, 1)))
```

In Rust, `wgw_show::Figure::widget(w).show()` runs a widget natively, the same code `cargo wgw` compiles to a
`.wgw` file (`cargo run -p wgw-show --example torus`); `heads/lean/Examples/Torus.lean` is the Lean version
(`lake exe torus`). A window closes when its program ends, so a script waits on `show` until it's closed; Python's
figures also show inline in Jupyter. Each release has `wgw-window` built for Linux, macOS and Windows; from the
repository, `./build.sh` (it carries `dist/` inside), then `cargo install --path window`, or point `WGW_WINDOW` at
`target/release/wgw-window`. Built with `--no-default-features` it has no windows and needs no webview, for saving
on machines with no display.

Every head also saves a figure headless, as an image or a video, drawn on the CPU by wgw's own renderer
(`wgw-headless`): `fig.save("torus.png")`, `fig.save("torus.mp4", seconds=4)` in Python; `savefig` in Julia,
`wgw_save` in R, `Figure.save` in Lean and `Figure::save` in Rust. `wgw-headless` does all of it: PNG, animated PNG
and GIF itself, MP4, WebM and MOV by `ffmpeg` if it's installed. A video plays an animation through, or turns a
figure that doesn't animate once around; widgets (`.wgw`) run in `wgw-headless`'s WebAssembly interpreter, with the
controls as set up or as given.

In Typst, [tdg](https://github.com/frisbro303/tdg)'s `#widget` makes a figure live in HTML and a still in PDF, drawn
by wgw-headless in tdg's plugin. The heads and their protocol:
[docs](https://frisbro303.github.io/wgw/heads/); `tests/heads/run.sh` checks every head against the same input.

## Build

Needs Rust with the `wasm32-unknown-unknown` target, `wasm-bindgen-cli` at the version in `Cargo.lock` (`build.sh`
says which), and optionally `wasm-opt` from [binaryen](https://github.com/WebAssembly/binaryen) for a smaller core.
The version is set once, in `Cargo.toml`.

```sh
./build.sh                     # -> dist/
./build.sh ../site/theme/wgw   # ... and copy it into a site
cargo install --path cargo-wgw                       # cargo wgw: builds widgets into .wgw files
cargo wgw -p torus                                   # the example widget -> examples/torus.wgw
python3 -m http.server         # examples at http://localhost:8000/examples/
```

## Docs

**https://frisbro303.github.io/wgw/**, with live figures: [figures](https://frisbro303.github.io/wgw/figures/),
[writing widgets](https://frisbro303.github.io/wgw/widgets/), the
[reference](https://frisbro303.github.io/wgw/reference/),
[the page API](https://frisbro303.github.io/wgw/javascript/) and
[internals](https://frisbro303.github.io/wgw/internals/). They're Typst pages in [`docs/`](docs), a
[tdg](https://github.com/frisbro303/tdg) site: `typst compile --root . --features html,bundle --format bundle docs/site.typ docs/_site`.
