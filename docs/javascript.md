# JavaScript

## Widgets in JavaScript

A `.js` widget is an ES module exporting `widget(mesh)` (or a default export), returning what a Python `Widget`
holds, as an object. It runs on the page, with no Pyodide to load.

```js
export function widget(mesh) {
  const faces = new Uint32Array([...]);              // made once: the same object every time, so sent once
  return {
    controls: { frequency: [1, 8, 4], epsilon: [0.001, 1, 0.02, "log"], arrows: true },
    onChange: ({ frequency, epsilon, arrows }) => ({
      sheet: { type: "mesh", vertices: new Float32Array([...]), faces, values: new Float32Array([...]) },
    }),
    onPick: (vertex, controls, layer) => null,
    grid: false,                                      // or {toggle: true} for a checkbox
    loop: true,
  };
}
```

| field      |                                                                                          |
|------------|------------------------------------------------------------------------------------------|
| `scene`    | layers to start with, by name                                                            |
| `controls` | `[min, max, value]`, with a step and/or `"log"` after it, for sliders; `true`/`false` for checkboxes |
| `onChange(controls)` | returns a scene; at the start and whenever a control changes                   |
| `onPick(vertex, controls, layer)` | returns a scene; on a click                                       |
| `grid`, `fps`, `loop` | as in Python                                                                  |

`mesh` is the figure's mesh file as arrays, `{vertices: {data, shape, dtype}, faces: ..., ...}`, or `null`.
Functions may be `async`.

A layer is `{type: "mesh" | "points" | "lines" | "vectors", ...arrays, ...parameters}`, with the arrays and
parameters of the [Python layers](reference.md#layers) (`origins` for vectors' starts). Arrays are typed arrays,
read in rows of 3 (`segments` in rows of 2), or `{data, shape}` to give a frames axis. Scenes follow the same rules
as in Python, and arrays are only sent again when they're new objects. This one builds and animates a sheet:

<div class="wgw" data-widget="examples/ripple_widget.js" data-view="-60,30" data-pick="none"></div>

```js
--8<-- "examples/ripple_widget.js"
```

## The page API

`wgw.js` sets `window.WGW`:

| member                       |                                                                       |
|------------------------------|-----------------------------------------------------------------------|
| `WGW.mount(element, options)` | make an element a figure (or get its figure); returns it             |
| `WGW.loadMesh(url)`          | fetch and parse a mesh file in the core; returns a `MeshFile` (call `free()` when done) |
| `WGW.runners`                | the widget runners, by file extension                                 |
| `WGW.toPatch(key, scene)`    | a JS scene as a patch                                                 |

`options` are the [attributes](figures.md#attributes) without `data-`: `src`, `widget`, `up`, `view` (`[turn, tilt]`),
`pick`, `flat`. A figure (also `element.wgw`) has:

| member                      |                                                                       |
|-----------------------------|-----------------------------------------------------------------------|
| `figure.on("pick", fn)`     | `fn({layer, index, point})` on each pick, with or without a widget   |
| `figure.apply(patch, animate)` | change the scene directly                                         |
| `figure.reset()`            | the starting camera                                                   |
| `figure.ready`              | a promise, once the core has started                                  |

## Patches

Whatever language a widget is in, the page receives **patches**:

```js
{ layers: { name: null | { type, arrays: { key: { data, shape } }, params: { key: value }, remove: [key] } } }
```

`null` removes a layer. Otherwise the layer gets exactly the given parameters, drops the arrays in `remove`, sets
those in `arrays`, and keeps the arrays not mentioned (sent before, unchanged). Arrays are `Float32Array`s
(`Uint32Array`s for `faces` and `segments`), row-major; one more axis than the key has per frame (`values` 1, the
rest 2) is a frames axis.

## Other languages

Widgets are run by a **runner**, chosen by the widget file's extension; `py` and `js` are built in
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
