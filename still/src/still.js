// Runs a widget the way the page does (js/runners.js), once, for a still: build it from the figure's mesh, apply
// its starting scene, run on_change with the controls' starting values, and on_pick for its starting pick. Its
// scenes become patches with the page's own js/patch.js. Only the first frame of an animation is kept.
import { toPatch, controlInputs, RANK } from "./patch.js";
import * as module from "../widget/widget.js";

const TYPES = { f4: Float32Array, f8: Float64Array, u4: Uint32Array, i4: Int32Array, u2: Uint16Array, u1: Uint8Array, f2: Float32Array };

// The figure's mesh file as the page hands it to a widget: {name: {data, shape, dtype}}.
function meshArrays(raw) {
  if (!raw) return null;
  return Object.fromEntries(Object.entries(raw).map(([name, a]) => [name, { data: TYPES[a.dtype].from(a.data), shape: a.shape, dtype: a.dtype }]));
}

// A patch with only its first frame, as plain arrays (NaN, which JSON can't hold, as null).
function firstFrame(patch) {
  if (!patch) return null;
  for (const layer of Object.values(patch.layers)) {
    if (!layer) continue;
    for (const [key, a] of Object.entries(layer.arrays)) {
      let { data, shape } = a;
      if (shape.length > RANK[key]) {
        shape = shape.slice(1);
        data = data.subarray(0, shape.reduce((x, y) => x * y, 1));
      }
      layer.arrays[key] = { data: Array.from(data, (x) => (Number.isFinite(x) ? x : null)), shape, int: data instanceof Uint32Array };
    }
  }
  return patch;
}

export async function run(meshJson) {
  const make = typeof module.widget === "function" ? module.widget : module.default;
  if (typeof make !== "function") throw new Error("a widget module exports function widget(mesh)");
  const mesh = meshArrays(meshJson ? JSON.parse(meshJson) : null);
  const w = await make(mesh);
  const patches = [toPatch("still", w.scene || {})];

  const toggle = w.grid && typeof w.grid === "object";
  const controls = {};
  for (const [name, c] of Object.entries(controlInputs(w.controls))) controls[name] = c.type === "checkbox" ? c.value : c.log ? 10 ** c.value : c.value;
  if (toggle) controls.grid = !!w.grid.toggle;

  if (typeof w.onChange === "function") patches.push(toPatch("still", await w.onChange({ ...controls })));
  if (w.picked != null && typeof w.onPick === "function") {
    const v = mesh && mesh.vertices ? Array.from(mesh.vertices.data.subarray(3 * w.picked, 3 * w.picked + 3)) : null;
    patches.push(toPatch("still", await w.onPick(w.picked, { ...controls }, "mesh", { point: v, frame: 0 })));
  }
  return JSON.stringify({
    patches: patches.map(firstFrame),
    grid: toggle ? !!w.grid.toggle : !!w.grid,
    panes: w.panes || null,
  });
}
