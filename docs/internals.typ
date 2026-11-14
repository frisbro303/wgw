#import "@local/tdg:0.1.0": *
#show: docs.doc.with("internals", title: "Internals", order: 6, description: "How wgw works, how to extend it, and how its docs are built.")

= How a figure works

+ `wgw.js` finds `.wgw` elements, loads the core (`core/wgw_core_bg.wasm`) if there are any, and gives each
  figure a canvas and a `Viewer` from the core.
+ When a figure nears the screen, the page fetches its `data-src`. A mesh is parsed by the core
  (`scene/src/mesh_file.rs`) and drawn; a widget (a WebAssembly module) is instantiated (`js/widget.js`).
+ The widget's `setup` gives its controls; its `scene`, called at the start and on each control or pick (once per
  frame for an animation), gives patches, which the page applies to the core.

The core holds the scene and does the drawing, cameras, picking and animation; the page handles the DOM, input
events, controls and widgets. Between them are named n-dimensional arrays (typed arrays with a shape).

A still (a figure in a PDF) is the same scene drawn by the same renderer: wgw-still runs the core's renderer on a
GL implemented on the CPU (`still/src/soft/`), which runs the same WGSL shaders, compiled to a register machine
from naga's IR. tdg runs the widget as a Typst plugin and hands its first frame to wgw-still in its own plugin.

= Layout

```
shaders/            the shaders, in WGSL: common.wgsl (lighting, colormaps, picking ids) and one file per layer type
scene/              wgw-scene: arrays, layers, .obj and .wgw files, patches, the camera and framing
render/             wgw-render: the renderer (layers, panes, cameras, picking, animation) over a small GL trait,
                    with the shaders translated to GLSL for WebGL by naga (render/build.rs)
src/                the page's core: the renderer on WebGL (through glow), for the page's Viewer
widget/             wgw-widget: writing widgets in Rust
still/              wgw-still: stills on the CPU (the renderer on a CPU GL that runs the shaders)
js/wgw.js           figures: the DOM, input, controls, applying patches
js/widget.js        running widgets, and decoding their patches
js/patch.js, io.js, style.js   controls as inputs; fetching and parsing mesh files; reading CSS colors
examples/           torus (a widget in Rust), spot (a mesh)
docs/               these docs, as Typst pages for tdg
```

= A new layer type

+ A Rust module implementing `Primitive` (`render/src/layer.rs`): `sync` (upload frame k of the layer's arrays when they
  changed), `draw`, `draw_ids` (the picking pass), `hit` (turn the id under the cursor into an element), and
  `destroy`.
+ Its shaders in `shaders/<type>.wgsl` and its programs in `render/build.rs`, a line in `Kind`
  (`scene/src/layer.rs`), and its arrays' rank in `layer::rank` if any isn't 2.
+ A builder in `widget/src/lib.rs` (like `Lines`), and its arrays' names in `RANK` (`js/patch.js`) if new.

= Performance

- Figures draw only when something changes (input, new data, animation, a glide), and only while on screen.
- The mesh file is parsed in the core and drawn before any widget starts.
- Widgets are WebAssembly, run on the page. A newer request replaces a waiting one; a patch's arrays are viewed
  in place, not copied.
- Picking draws one scissored pixel of (layer, element) ids, then refines on the CPU, testing only the triangles
  around the hit.
- Curves and arrows are instanced: thousands of arrows are one draw call.

= Building and testing

```sh
./build.sh [folder]     # the core into dist/core (cargo, wasm-bindgen, wasm-opt); the JS beside it
cargo test              # every crate, natively
cargo wgw -p torus      # the example widget, examples/torus.wgw
```

The examples page (`examples/index.html`) loads `dist/` and `examples/torus.wgw` (the built torus widget). There
are no browser tests in the repository yet.

= The docs

These pages are Typst, and #link("https://github.com/frisbro303/tdg")[tdg] makes them a site in one
`typst compile`: `docs/site.typ` sets it up and includes the pages. Each live figure's source is read from
`examples/`, so what's shown is what runs.

```sh
typst compile --root . --features html,bundle --format bundle docs/site.typ docs/_site
typst watch --root . --features html,bundle --format bundle docs/site.typ docs/_site   # rebuilt as you edit
python3 -m http.server -d docs/_site                                                   # http://localhost:8000
```

On every push to `main`, `.github/workflows/docs.yml` builds wgw, the example widget and the site, and publishes
it to GitHub Pages.
