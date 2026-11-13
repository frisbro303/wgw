#import "@local/tdg:0.2.0": *
#show: doc.with("internals", title: "Internals", order: 6, description: "How wgw works, how to extend it, and how its docs are built.")

= How a figure works

+ `wgw.js` finds `.wgw` elements, loads the core (`core/wgw_core_bg.wasm`) if there are any, and gives each
  figure a canvas and a `Viewer` from the core.
+ When a figure nears the screen, the page fetches its `data-src`, the core parses it (`src/io.rs`) and draws it.
+ The widget's runner loads it. Python widgets run in one worker shared by the page (`js/python-worker.js`),
  which starts Pyodide as soon as the page is idle; JavaScript widgets are imported on the page.
+ Controls and clicks call the widget, whose scenes become patches, which the page applies to the core.

The core holds the scene and does the drawing, camera, picking and animation; the page handles the DOM, input
events, controls and widgets. Between them are named n-dimensional arrays (typed arrays with a shape).

= Layout

```
src/lib.rs          the modules: the data model and parsers are plain Rust; drawing is built for wasm only
src/viewer.rs       the Viewer the page talks to: layers, camera, input, picking, animation, the frame loop
src/layer.rs        the layer model, and the Primitive trait each layer type implements
src/mesh.rs, points.rs, lines.rs, vectors.rs     one module per layer type
src/io.rs           .obj and .wgw parsing
src/array.rs        arrays with shapes and frames
src/camera.rs, shaders.rs, gl.rs
js/wgw.js           figures: the DOM, input, controls, applying patches
js/runners.js       running widgets: Python (in python-worker.js) and JavaScript
js/patch.js         patches, and JS scenes as patches; controls as inputs
js/io.js, style.js  fetching mesh files; reading CSS colors
python/wgw.py       Widget, controls, layers, shapes and .wgw files
examples/           fields (every layer type), hover, heat (animated), torus (built in Python), ripple (JavaScript)
docs/               these docs, as Typst pages for tdg
```

= A new layer type

+ A Rust module implementing `Primitive` (`src/layer.rs`): `sync` (upload frame k of the layer's arrays when they
  changed), `draw`, `draw_ids` (the picking pass), `hit` (turn the id under the cursor into an element), and
  `destroy`.
+ Its shaders in `src/shaders.rs`, a line in `Kind`, and its arrays' rank in `layer::rank` if any isn't 2.
+ A class in `python/wgw.py`, and its array names in `ARRAYS` (`js/patch.js`) and `_ARRAYS`
  (`js/python-worker.js`) if new.

= Performance

- Figures draw only when something changes (input, new data, animation, a glide), and only while on screen.
- The mesh file is parsed in the core and drawn before any widget starts.
- Widgets run in a worker. A newer call replaces a waiting one; results come back as transferred typed arrays,
  and arrays already sent (the same object) aren't sent again.
- Picking draws one scissored pixel of (layer, element) ids, then refines on the CPU, testing only the triangles
  around the hit.
- Curves and arrows are instanced: thousands of arrows are one draw call.

= Building and testing

```sh
./build.sh [folder]   # wasm-pack builds the core into dist/core; the JS modules and wgw.py are copied beside it
cargo test            # the parsers, natively
```

The examples page (`examples/index.html`) loads `dist/`. There are no browser tests in the repository yet.

= The docs

These pages are Typst, and #link("https://github.com/frisbro303/tdg")[tdg] makes them a site in one
`typst compile`: `docs/site.typ` sets it up and includes the pages. Each live figure's source is read from
`examples/`, so what's shown is what runs, and the Python reference is laid out by `api(...)` from `wgw.py`'s
docstrings, which tdg's plugin reads.

```sh
typst compile --root . --features html,bundle --format bundle docs/site.typ docs/_site
typst watch --root . --features html,bundle --format bundle docs/site.typ docs/_site   # rebuilt as you edit
python3 -m http.server -d docs/_site                                                   # http://localhost:8000
```

On every push to `main`, `.github/workflows/docs.yml` builds wgw and the site, and publishes it to GitHub Pages.
