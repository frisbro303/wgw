<h1>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/banner-dark.svg">
    <img src="assets/banner-light.svg" height="56" alt="wgw: Web Geometry Widgets">
  </picture>
</h1>

Interactive 3D figures for web pages and PDFs: meshes, point clouds, curves and vector fields, drawn by a Rust
core compiled to WebAssembly and driven by **widgets**: small WebAssembly modules, written in Rust, that respond to
sliders and clicks. A figure is one `.wgw` file, a widget with its data inside, or a mesh.

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
            .layer("mesh", Mesh::new(self.spot.vertices.clone(), self.spot.faces.clone()).values(values))
            .layer("dot", Points::new(vec![self.spot.vertices[v]]))
    }
}

export!(Distance);
```

In a [tdg](https://github.com/frisbro303/tdg) site, `#widget(path("distance.wgw"))` makes the same figure, and its
PDF shows the figure's first frame, drawn by wgw's own renderer with the page's shaders.

## Build

Needs Rust with the `wasm32-unknown-unknown` target and [wasm-pack](https://github.com/wasm-bindgen/wasm-pack).

```sh
./build.sh                     # -> dist/
./build.sh ../site/theme/wgw   # ... and copy it into a site
cargo install --path cargo-wgw                       # cargo wgw: builds widgets into .wgw files
cargo wgw --manifest-path examples/torus/Cargo.toml   # the example widget -> examples/torus.wgw
python3 -m http.server         # examples at http://localhost:8000/examples/
```

## Docs

**https://frisbro303.github.io/wgw/**, with live figures: [figures](https://frisbro303.github.io/wgw/figures/),
[writing widgets](https://frisbro303.github.io/wgw/widgets/), the
[reference](https://frisbro303.github.io/wgw/reference/),
[the page API](https://frisbro303.github.io/wgw/javascript/) and
[internals](https://frisbro303.github.io/wgw/internals/). They're Typst pages in [`docs/`](docs), a
[tdg](https://github.com/frisbro303/tdg) site: `typst compile --root . --features html,bundle --format bundle docs/site.typ docs/_site`.
