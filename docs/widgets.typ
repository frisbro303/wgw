#import "@local/tdg:0.2.0": *
#show: doc.with("widgets", title: "Writing widgets", order: 3, description: "Widgets in Rust with wgw-widget: controls, scenes, picks, frames, panes and embedded data.")

A widget is a small WebAssembly module that builds a figure's scene. It's written in Rust with the `wgw-widget`
crate (any language that compiles to WebAssembly can follow the #link("/reference/#the-contract")[contract]),
compiled to a `.wasm` file, and used as the figure's `.wgw` file. The same file runs live on a page and, in a tdg
site, for the figure's still in a PDF.

= A widget

A crate with `crate-type = ["cdylib"]` and `wgw-widget` as a dependency, and a type implementing `Widget`:

```rust
use wgw_widget::*;

struct Torus;

impl Widget for Torus {
    fn new(_mesh: Option<MeshData>) -> Self { Torus }           // built once
    fn setup(&self) -> Setup {                                    // its controls and how it starts
        Setup::new().slider("R", Slider::new(0.5, 2.0, 1.0))
    }
    fn scene(&self, state: &State) -> Scene {                     // the scene for a state
        let big = state.value("R");
        Scene::new().layer("torus", Mesh::new(vertices(big), faces()).values(curvature(big)))
    }
}

export!(Torus);
```

Build it with `cargo wgw` (after `rustup target add wasm32-unknown-unknown` and
`cargo install --git https://github.com/frisbro303/wgw cargo-wgw`). It builds every widget crate in the package or
workspace in release for `wasm32-unknown-unknown` and writes each one's `.wgw` file to `out`, a path relative to
the crate, or else `target/wgw/<crate>.wgw`:

```toml
[package.metadata.wgw]             # in the crate's Cargo.toml
out = "../figures/torus.wgw"

[workspace.metadata.wgw]           # or once for a workspace: {dir} is each crate's folder, {crate} its name
out = "../../figures/{dir}.wgw"
```

`cargo wgw -p <crate>` builds just that one.

- `setup` says what the widget is: its controls, frames, panes, grid and starting pick.
- `scene` builds the scene for a `State`: the controls' values (`state.value("R")`, `state.checked("smooth")`),
  the picks (`state.pick()`, `state.picks`) and the animation frame (`state.frame`). It's called at the start and
  whenever a control or a pick changes; it returns every layer it shows.

= Layers

A scene is layers by name: `Scene::new().layer("name", layer)`, or `.remove("name")`. Each kind is built from its
arrays, then given parameters:

```rust
Mesh::new(vertices, faces)      // Vec<[f32; 3]>, Vec<[u32; 3]>
Points::new(vertices)
Lines::new(vertices).segments(pairs)   // each vertex to the next, unless given pairs
Vectors::new(origins, vectors)
```

- `.values(v)`: a number per vertex, colored through the layer's colors (`.colors(&[...])`, or the page's) over
  `.range(min, max)` (0 to 1 by default); or `.color("...")`, one color.
- Colors are CSS: `"var(--link)"` and `color-mix(...)` follow the page's theme.
- `.width(px)` for curves, `.size(px)` for points, `.flat(true)` for meshes, `.pane(1)` for a second view beside
  the first, `.pickable(false)`, `.visible(false)`, and `.param(name, value)` for any other (see the
  #link("/reference/")[reference]).

= Data

A widget carries its data: embed a mesh file, and anything else as extra arrays in a `.wgw` file, with
`include_bytes!`. Spot and its Laplace-Beltrami spectrum, in one file:

```rust
fn new(_mesh: Option<MeshData>) -> Self {
    let spot = MeshData::parse(include_bytes!("spot.wgw")).unwrap();   // .wgw or .obj
    let (evecs, shape) = spot.array("evecs").unwrap();                // extra arrays, as floats
    ...
}
```

`spot.vertices` and `spot.faces` are its mesh. `new` runs once; keep whatever `scene` needs.

= Picks

A click on a pickable layer (meshes and points, unless `.pickable(false)`) reaches `scene` as `state.pick()`: the
vertex, its layer, the point clicked and the frame on screen. `Setup::picked(vertex)` starts with a vertex of the
layer `mesh` picked, so the figure opens showing something; `Setup::keep_picks()` keeps every click
(`state.picks`) rather than the latest. With `data-pick="hover"`, picks follow the mouse.

= Frames

`Setup::frames(n)` makes an animation: `scene` is asked for each frame, `state.frame` from 0 to n - 1, and the
page plays them in a loop (`.fps(20.0)`, or `.once()`). A still draws frame 0 alone, so widgets never build frames
they don't need. A layer that a later frame leaves out stays as it is in frame 0: give what never changes (a
sphere, a grid) with frame 0 only, and it's sent once. Scenes are built in a worker, off the page's thread.

= Panes

Layers with `.pane(1)` are drawn in a second view beside the first, with its own camera and fit;
`Setup::panes(&[2.0, 1.0])` gives the first two thirds of the width. Each pane turns on its own.
