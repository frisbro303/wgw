#import "@local/tdg:0.2.0": *
#show: doc.with("javascript", title: "JavaScript", order: 5, description: "wgw widgets in JavaScript, the page API, patches, and other languages.")

= Widgets in JavaScript

A `.js` widget is an ES module exporting `widget(mesh)` (or a default export), returning what a Python `Widget`
holds, as an object. It runs on the page, with no Pyodide to load. This one builds and animates a sheet:

#widget(widget: path("../examples/ripple_widget.js"), view: (-60, 30), pick: "none")
#source-code(path("../examples/ripple_widget.js"))

#table(
  columns: 2,
  [*field*], [],
  [`scene`], [layers to start with, by name],
  [`controls`], [`[min, max, value]`, with a step and/or `"log"` after it, for sliders; `true`/`false` for checkboxes],
  [`onChange(controls)`], [returns a scene; at the start and whenever a control changes],
  [`onPick(vertex, controls, layer, {point, frame})`], [returns a scene; on a click. `point` is the exact spot clicked, in the scene's units, and `frame` the animation frame on screen, to start something new in step with it],
  [`grid`, `fps`, `loop`], [as in Python; `grid` may be `{toggle: true}` for a checkbox],
  [`panes`], [the panes' relative widths, such as `[2, 1]`, for layers split between panes with their `pane` parameter],
)

`mesh` is the figure's mesh file as arrays, `{vertices: {data, shape, dtype}, faces: ..., ...}`, or `null`.
Functions may be `async`.

A layer is `{type: "mesh" | "points" | "lines" | "vectors", ...arrays, ...parameters}`, with the arrays and
parameters of the #link("/reference/#wgw.Layer")[Python layers] (`origins` for vectors' starts). Arrays are typed
arrays, read in rows of 3 (`segments` in rows of 2), or `{data, shape}` to give a frames axis. Scenes follow the
same rules as in Python, and arrays are only sent again when they're new objects.

= The page API

`wgw.js` sets `window.WGW`:

#table(
  columns: 2,
  [*member*], [],
  [`WGW.mount(element, options)`], [make an element a figure (or get its figure); returns it],
  [`WGW.loadMesh(url)`], [fetch and parse a mesh file in the core; returns a `MeshFile` (call `free()` when done)],
  [`WGW.runners`], [the widget runners, by file extension],
  [`WGW.toPatch(key, scene)`], [a JS scene as a patch],
)

`options` are the #link("/figures/#attributes")[attributes] without `data-`: `src`, `widget`, `up`, `view`
(`[turn, tilt]`), `pick`, `flat`. A figure (also `element.wgw`) has `on("pick", fn)`, which calls
`fn({layer, index, point})` on each pick, with or without a widget; `apply(patch, animate)`, to change the scene
directly; `reset()`, for the starting camera; and `ready`, a promise, once the core has started.

= Patches

Whatever language a widget is in, the page receives *patches*:

```js
{ layers: { name: null | { type, arrays: { key: { data, shape } }, params: { key: value }, remove: [key] } } }
```

`null` removes a layer. Otherwise the layer gets exactly the given parameters, drops the arrays in `remove`, sets
those in `arrays`, and keeps the arrays not mentioned (sent before, unchanged). Arrays are `Float32Array`s
(`Uint32Array`s for `faces` and `segments`), row-major; one more axis than the key has per frame (`values` 1, the
rest 2) is a frames axis.

= Other languages

Widgets are run by a *runner*, chosen by the widget file's extension; `py` and `js` are built in
(`js/runners.js`). A runner for, say, Rust compiled to WebAssembly, or a server:

```js
WGW.runners.wasm = {
  preload() {},                                     // optional: start up early
  async load(key, url, mesh) {
    return { controls, functions: ["change", "pick"], grid, fps, loop, result: patch };
  },
  async call(key, name, index, layer, controls) {   // name: "change" or "pick"
    return patch;
  },
};
```

`key` names one figure's widget; `mesh` is the figure's arrays, as JS widgets get them; `controls` is in the
JavaScript form above.
