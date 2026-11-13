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
# heat_widget.py
import numpy as np
from wgw import rescale
CONTROLS = {"time": (0.01, 1.0, 0.15)}

def setup(mesh):                 # once, with the mesh file's arrays as NumPy arrays
    global EVALS, EVECS
    EVALS, EVECS = mesh["evals"], mesh["evecs"]

def pick(vertex, time):          # on click: heat from the vertex, as colors in [0, 1]
    return rescale(EVECS @ (np.exp(-EVALS * time) * EVECS[vertex]))
```

## Build and try

Needs Rust with the `wasm32-unknown-unknown` target, and [wasm-pack](https://github.com/wasm-bindgen/wasm-pack).

```sh
./build.sh                       # -> dist/: the core (.wasm), the JS modules, wgw.py
./build.sh ../site/theme/wgw     # ... and copy dist/ into a site
python3 -m http.server           # then open http://localhost:8000/examples/
```

## The model: a scene of layers, made of named arrays

A scene is a set of named **layers**. Each has a type, named **arrays** (n-dimensional, like NumPy's) and
**parameters** (numbers, booleans, CSS colors):

| type      | arrays                                            | parameters                                   |
|-----------|---------------------------------------------------|----------------------------------------------|
| `mesh`    | `vertices (n,3)`, `faces (m,3)`, `values (n)`      | `edges`, `flat`, `edge_color`                |
| `points`  | `vertices (n,3)`, `values (n)`                     | `size` (CSS px)                              |
| `lines`   | `vertices (n,3)`, `values (n)`, `segments (s,2)`   | `width` (CSS px), `closed`                   |
| `vectors` | `vertices (n,3)` (origins), `vectors (n,3)`, `values (n)` | `scale`, `width` (fraction of the scene) |

All types also take `color` (solid), `colors` (a colormap), `min`/`max` (the range of `values`, default 0 to 1),
`visible`, and `pickable` (on for meshes and points). Give any array except `faces` and `segments` a leading
**frames** axis to animate it.

## Widgets

A widget defines any of:

```python
CONTROLS = {"frequency": (1, 6, 3), "arrows": True}   # (min, max, start[, step]) is a slider; a bool, a checkbox

def setup(mesh): ...                     # once; mesh is the data-src file's arrays (or None)
def update(frequency, arrows): ...       # at start, and whenever a control changes
def pick(vertex, frequency, arrows): ... # on click (or hover, with data-pick="hover"); add a `layer` argument
                                         # to also be told which layer was picked
```

Each returns a **patch** to the scene: a dict of layers by name, built with `wgw.mesh`, `wgw.points`,
`wgw.lines` and `wgw.vectors`. `None` removes a layer, and a dict without `type` changes just what it names.
Shorthand: a bare array, or `values`/`vertices`/`faces` at the top level, goes to the layer named `mesh`. Add
`"loop": True` or `"fps": 24` to control animation.

```python
from wgw import icosphere, mesh, vectors, lines, points
V, F = icosphere(3)

def update(frequency, arrows):
    f, grad = ...
    return {"sphere": mesh(V, F, f), "gradient": vectors(V, grad, scale=0.04) if arrows else None}

def pick(vertex, frequency, arrows):
    return {"picked": points(V[[vertex]], size=14, color="#24837b")}
```

Python widgets can `from wgw import ...` (shapes: `torus`, `icosphere`, `surface`, `grid_faces`; `rescale`) and
import any package Pyodide ships, which loads on demand. A JavaScript widget is an ES module exporting the same
names, with controls as one object: `update({frequency, arrows})`, `pick(vertex, controls, layer)`.

While a widget is busy, a newer call replaces the waiting one, so dragging a slider never builds up a backlog.

### Other languages

Widgets are run by a **runner**, chosen by file extension; `py` and `js` are built in (`js/runners.js`). Add one
for anything that can return patches, such as Rust or C++ compiled to WebAssembly, or a server:

```js
WGW.runners.wasm = {
  load(key, url, mesh) { /* -> Promise<{controls, functions: ["update", "pick"], result: patch}> */ },
  call(key, name, index, layer, controls) { /* -> Promise<patch> */ },
};
```

## Mesh files

`wgw.save("spot.wgw", vertices, faces, **arrays)` writes a mesh, plus any arrays its widget needs (a spectrum,
say; cast to float16 to halve the download). The format is a small JSON header and then the raw arrays, 8-byte
aligned, so the page reads them without parsing. JSON files (`{"vertices": [...], "faces": [...]}`) work too.

## Options

| attribute     | default   |                                                   |
|---------------|-----------|---------------------------------------------------|
| `data-up`     | `z`       | the axis that points up                           |
| `data-view`   | `-60,20`  | starting turn and tilt, degrees                   |
| `data-pick`   | `click`   | `click`, `hover` or `none`                        |
| `data-edges`  | off       | triangle edges on the `mesh` layer                |
| `data-flat`   | off       | flat shading on the `mesh` layer                  |
| `data-fps`    | `30`      | animation frame rate                              |

Styling uses CSS custom properties on the element: `--wgw-colors` (the colors `values` run through),
`--wgw-accent` (points, curves and arrows without values), `--wgw-edge`, `--wgw-height`. A theme change (a
`data-theme` or `class` on `<html>`, or the system setting) recolors every viewer.

Drag to turn, pinch or ctrl/cmd+scroll to zoom, double-click to reset; plain scrolling scrolls the page.

From JavaScript: `WGW.mount(el, options)` returns a viewer with `on("pick", fn)`, `apply(patch)` and `reset()`.

## Layout

```
src/            the Rust core: lib.rs (the Viewer the page talks to), layer.rs (the layer model and the
                Primitive trait), one module per layer type (mesh, points, lines, vectors), camera, shaders
js/             the page side: wgw.js (viewers, input, widgets), runners.js, python-worker.js, patch.js, io.js
python/wgw.py   file format, layer constructors and shapes, for offline scripts and widgets alike
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
