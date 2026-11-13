# wgw: web geometry widgets

Interactive 3D figures for web pages: meshes, point clouds, curves and vector fields, drawn by a small Rust
core compiled to WebAssembly, and driven by **widgets**: short programs that build, color or reshape the scene
in response to sliders and clicks. Widgets can be written in Python (run in the browser with Pyodide, off the
page's thread) or JavaScript, and other languages can be plugged in.

```html
<script type="module" src="wgw/wgw.js"></script>
<div class="wgw" data-src="spot.wgw" data-widget="heat_widget.py"></div>
```

```python
# distance_widget.py
import numpy as np
from wgw import Mesh, Points, Slider, Widget

def widget(spot: Mesh) -> Widget:                  # once, with the figure's mesh file
    embedding = spot.evecs * np.exp(-spot.evals * 0.3)

    def on_pick(vertex, epsilon):                  # on a click, and whenever epsilon moves
        d2 = np.sum((embedding - embedding[vertex]) ** 2, axis=1)
        return {"mesh": spot.with_values(np.exp(-d2 / epsilon)), "dot": Points(spot.vertices[vertex])}

    return Widget(controls={"epsilon": Slider(0.001, 1.0, 0.02, log=True)}, on_pick=on_pick, grid=True)
```

## Build and try

Needs Rust with the `wasm32-unknown-unknown` target, and [wasm-pack](https://github.com/wasm-bindgen/wasm-pack).

```sh
./build.sh                       # -> dist/: the core (.wasm), the JS modules, wgw.py
./build.sh ../site/theme/wgw     # ... and copy dist/ into a site
python3 -m http.server           # then open http://localhost:8000/examples/
```

## The model: a scene of layers

A scene is a set of named **layers**. In Python each is a value of one of four types, made of **arrays** (NumPy)
and **parameters** (numbers, booleans, CSS colors):

| type                                     | parameters                                       |
|------------------------------------------|--------------------------------------------------|
| `Mesh(vertices, faces, values=None)`     | `edges` (the triangle grid), `flat`, `edge_color` |
| `Points(vertices, values=None)`          | `size` (CSS px), `lift`                          |
| `Lines(vertices, values=None, segments=None)` | `width` (CSS px), `closed`                  |
| `Vectors(origins, vectors, values=None)` | `scale`, `width` (fraction of the scene)         |

All take `color` (solid), `colors` (a colormap), `min`/`max` (the range of `values`, default 0 to 1), `visible`
and `pickable` (on for meshes and points). Give any array except `faces` and `segments` a leading **frames** axis
to animate it.

Layers are values: `layer.with_values(v)` and `layer.replace(width=3)` return new layers that share everything
else. A `Mesh` also carries extra arrays (`mesh.evecs`, from its file), geometry (`normals`, `face_normals`,
`face_areas`, `areas`, `area`, `edges`), `Mesh.load("spot.obj" or "spot.wgw")` and `mesh.save("spot.wgw", **arrays)`.

## Widgets

A widget file defines one function, `widget`, from the figure's mesh (or nothing, if the figure has none) to a
`Widget`: a value saying what the figure starts as, which controls it has, and how it responds to them.

```python
Widget(
    scene={"sphere": sphere},                     # layers to start with; a mesh file is already the layer "mesh"
    controls={"frequency": Slider(1, 6, 3),       # passed to the functions below as keyword arguments
              "epsilon": Slider(0.001, 1, 0.02, log=True),
              "arrows": Checkbox(True)},
    on_change=lambda frequency, epsilon, arrows: {...},   # at the start, and whenever a control changes
    on_pick=lambda vertex, frequency, epsilon, arrows: {...},  # on a click (or hover); add `layer` to learn which
    grid=True,                                    # meshes' triangle grids: True, False, or Checkbox(...) for readers
    fps=30, loop=False,                           # how frames play
)
```

`on_change` and `on_pick` return a scene: layers by name, each replacing the layer of that name exactly as given;
`None` removes one, and layers not named stay as they are. Work done once (a spectrum, an embedding) goes in
`widget` itself, and the closures keep it, so nothing lives in globals.

```python
from wgw import Checkbox, Lines, Mesh, Points, Slider, Vectors, Widget, icosphere, rescale

def widget() -> Widget:
    sphere = Mesh(*icosphere(3))

    def on_change(frequency, arrows):
        f, grad = ...
        return {"sphere": sphere.with_values(rescale(f)),
                "gradient": Vectors(sphere.vertices, grad, scale=0.04) if arrows else None}

    def on_pick(vertex, frequency, arrows):
        return {"picked": Points(sphere.vertices[vertex], size=14, color="#24837b")}

    return Widget(controls={"frequency": Slider(1, 6, 3), "arrows": Checkbox(True)},
                  on_change=on_change, on_pick=on_pick)
```

Arrays are sent to the page only when they're new objects, so returning `spot.with_values(...)` on every click
sends just the values. Treat arrays you've returned as fixed: make new ones rather than changing them in place.

A widget reads its own data, with relative paths from where the widget file is: `Mesh.load("meshes/spot.obj")`,
`np.load("spectrum.npz")`, `open("notes.txt")`. In the browser, each file is fetched from beside the widget the
first time it's opened, so the same code runs in the browser and locally. A figure's own mesh is better given as
`data-src`, though: the core shows it before Python has started.

Python widgets can import anything Pyodide ships (SciPy, ...), which loads on demand, and `wgw`'s shapes:
`torus`, `icosphere`, `surface`, `grid_faces`, and `rescale`. While a widget is busy, a newer call replaces the
waiting one, so dragging a slider never builds up a backlog.

A JavaScript widget is an ES module exporting `widget(mesh)`, which returns the same thing as an object:
`{scene, controls, onChange(controls), onPick(vertex, controls, layer), grid, fps, loop}`, with layers as
`{type: "mesh", vertices, faces, values, ...params}` (typed arrays) and controls as `[min, max, start, "log"?]`
or `true`/`false`. See `examples/ripple_widget.js`.

### Other languages

Widgets are run by a **runner**, chosen by file extension; `py` and `js` are built in (`js/runners.js`). Add one
for anything that can return patches, such as Rust or C++ compiled to WebAssembly, or a server:

```js
WGW.runners.wasm = {
  load(key, url, mesh) { /* -> Promise<{controls, functions: ["change", "pick"], grid, fps, loop, result: patch}> */ },
  call(key, name, index, layer, controls) { /* -> Promise<patch> */ },
};
```

## Mesh files

A figure's mesh (`data-src`) is fetched and parsed by the core, which shows it at once, before any widget has
started, and then hands it to the widget as `widget(mesh)`. The core reads Wavefront `.obj` (triangles and
polygons; texture coordinates and normals are ignored, so vertices are the file's `v` lines in order) and `.wgw`.

`.wgw` holds a mesh plus any arrays its widget needs, which the widget gets as attributes (`spot.evecs`): write one
with `Mesh(vertices, faces).save("spot.wgw", evecs=...)` (cast to float16 to halve the download). It's a small
JSON header and then the raw arrays, 8-byte aligned.

## Options

| attribute     | default   |                                                   |
|---------------|-----------|---------------------------------------------------|
| `data-up`     | `z`       | the axis that points up                           |
| `data-view`   | `-60,20`  | starting turn and tilt, degrees                   |
| `data-pick`   | `click`   | `click`, `hover` or `none`                        |
| `data-flat`   | off       | flat shading on the `mesh` layer                  |

Styling uses CSS custom properties on the element: `--wgw-colors` (the colors `values` run through),
`--wgw-accent` (points, curves and arrows without values), `--wgw-edge`, `--wgw-height`, and `--wgw-control`
(sliders and checkboxes; the text color by default). A theme change (a
`data-theme` or `class` on `<html>`, or the system setting) recolors every viewer.

Drag to turn, pinch or ctrl/cmd+scroll to zoom, double-click to reset; plain scrolling scrolls the page.

From JavaScript: `WGW.mount(el, options)` returns a viewer with `on("pick", fn)`, `apply(patch)` and `reset()`.

## Layout

```
src/            the Rust core: lib.rs (the Viewer the page talks to), layer.rs (the layer model and the
                Primitive trait), one module per layer type (mesh, points, lines, vectors), camera, shaders
js/             the page side: wgw.js (viewers, input, widgets), runners.js, python-worker.js, patch.js, io.js
python/wgw.py   Widget, the layer types, the file format and shapes, for offline scripts and widgets alike
examples/
```

A new layer type is one Rust module implementing `Primitive` (upload, draw, draw ids for picking, resolve a
hit), plus a line in `Kind`.

## Why it's fast

- Draws only when something changes, and only while on screen; content loads as a viewer nears the screen.
- Picking draws one scissored pixel of (layer, element) ids on the GPU, then refines on the CPU, checking only
  the triangles around the hit.
- Python runs in a worker, starting as soon as the page is idle; results come back as transferred typed arrays.
- Lines and arrows are instanced, so a field of thousands of arrows is one draw call.
