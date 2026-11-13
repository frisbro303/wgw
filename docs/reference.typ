#import "@local/tdg:0.2.0": *
#show: doc.with("reference", title: "Reference", order: 4, description: "Layers and their parameters, a widget's contract for any language, and wgw's file format.")

= Layers

Every layer has arrays (per vertex, or with a leading frames axis) and parameters:

#table(
  columns: 3,
  [*parameter*], [*default*], [],
  [`color`], [], [one CSS color for the whole layer; `var(--name)` and `color-mix(...)` follow the page's theme],
  [`colors`], [the page's], [CSS colors for `values` to run through],
  [`min`, `max`], [`0`, `1`], [the range of `values` the colors span],
  [`visible`], [on], [],
  [`pickable`], [meshes and points], [whether clicks pick it],
  [`pane`], [`0`], [the view it's drawn in: `1` for a second view, beside the first, sized on its own],
)

Without `values` or `color`, meshes take the first of the page's colors, and other layers its accent.

#table(
  columns: 3,
  [*kind*], [*arrays*], [*parameters*],
  [`mesh`], [`vertices (n, 3)`, `faces (m, 3)`, `values (n)`], [`flat` (flat shading), `edges` (its triangle grid; by default the widget's `grid`), `edge_color`],
  [`points`], [`vertices (n, 3)`, `values (n)`], [`size` (CSS pixels across, 6), `lift` (toward the camera, as a fraction of the scene, so points on a surface sit on it; 0.02)],
  [`lines`], [`vertices (n, 3)`, `segments (s, 2)`, `values (n)`], [`width` (CSS pixels, 2), `closed` (join the last vertex to the first)],
  [`vectors`], [`vertices (n, 3)` (the origins), `vectors (n, 3)`, `values (n)`], [`scale` (1), `width` (the shaft's radius, as a fraction of the scene, 0.006)],
)

A vertex with a coordinate that isn't a number (NaN) isn't drawn, nor are the segments and faces that use it.

= The contract

A widget is a WebAssembly module exporting two functions, in
#link("https://github.com/astrale-sharp/wasm-minimal-protocol")[Typst's plugin protocol]: each takes its
arguments' lengths, reads the arguments through the import `typst_env.wasm_minimal_protocol_write_args_to_buffer`,
hands its result to `typst_env.wasm_minimal_protocol_send_result_to_host`, and returns 0 (or 1, with an error
message as the result). So a page and Typst run the same file. `wgw-widget` implements it for Rust.

- `setup(mesh)` returns JSON: `{controls, frames, fps, loop, grid, picked, panes, keepPicks}`. `controls` are by
  name: `[min, max, start, step?, "log"?]` for a slider (with `"log"`, it moves evenly in powers of ten), `true`
  or `false` for a checkbox. `grid` is a boolean, or `{"toggle": on}` for a checkbox readers can use. `picked` is a
  vertex of the layer `mesh` picked from the start.
- `scene(mesh, state)` returns a patch (below) for a state, JSON:
  `{controls: {name: value}, picks: [{vertex, layer, point, frame}], frame}`.

`mesh` is the figure's mesh file when it has one apart from the widget; it's empty otherwise, and after the first
call (the widget keeps it).

= Files and patches

wgw's container, for mesh files and for patches alike: `WGW1`, a little-endian u32 header length, a JSON header,
then raw little-endian arrays (`f4` floats, `u4` integers), each at an offset from the end of the header. A mesh
file's header is `{"arrays": {name: {dtype, shape, offset}}}`; a patch's:

```json
{"layers": [[name, null | {"type", "params": {...}, "remove": [...], "arrays": {key: {"dtype", "shape", "offset"}}}]]}
```

`null` removes a layer. Otherwise the layer becomes exactly what's given: its parameters are replaced, the arrays
in `remove` dropped and those in `arrays` set; arrays not mentioned stay as they were (a widget recoloring the
figure's mesh sends only `values`).
