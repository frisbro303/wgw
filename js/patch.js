// Patches: what widgets return, in one shape whatever language they're written in.
//
//   {layers: {name: null | {type?, arrays: {key: {data, shape}}, params: {key: value}}}, loop?, fps?}
//
// null removes a layer; a layer without type changes only the arrays and parameters it names. Arrays are typed
// arrays with a shape, row-major; one more axis than the key expects (see RANK) is a leading frames axis.

export const ARRAYS = { values: Float32Array, vertices: Float32Array, vectors: Float32Array, faces: Uint32Array, segments: Uint32Array };
const WIDTH = { values: 1, vertices: 3, vectors: 3, faces: 3, segments: 2 };
export const RANK = { values: 1, vertices: 2, vectors: 2, faces: 2, segments: 2 };

function isArray(v) {
  return Array.isArray(v) || ArrayBuffer.isView(v) || (v != null && typeof v === "object" && "data" in v && "shape" in v);
}

// An array as {data, shape}: from {data, shape}, a typed array (rows read off its key), or nested lists.
function toArray(key, v) {
  let data = v.data !== undefined && v.shape ? v.data : v, shape = v.shape;
  if (Array.isArray(data) && !shape) {
    shape = [];
    for (let level = data; Array.isArray(level); level = level[0]) shape.push(level.length);
    data = data.flat(Infinity);
  }
  if (!(data instanceof ARRAYS[key])) data = ARRAYS[key].from(data);
  if (!shape || (shape.length === 1 && WIDTH[key] > 1)) shape = WIDTH[key] > 1 ? [data.length / WIDTH[key], WIDTH[key]] : [data.length];
  return { data, shape: Array.from(shape) };
}

function toLayer(fields) {
  const layer = { arrays: {}, params: {} };
  for (const [key, value] of Object.entries(fields)) {
    if (value == null) continue;
    if (key === "type") layer.type = value;
    else if (key in ARRAYS) layer.arrays[key] = toArray(key, value);
    else layer.params[key] = value;
  }
  return layer;
}

// A JS widget's result as a patch: a bare array is the "mesh" layer's values; top-level objects are layers by
// name; anything else at the top level goes to the "mesh" layer.
export function toPatch(result) {
  if (result == null) return null;
  if (isArray(result)) result = { values: result };
  const patch = { layers: {} }, loose = {};
  for (const [key, value] of Object.entries(result)) {
    if (key === "loop" || key === "fps") patch[key] = value;
    else if (value === null) patch.layers[key] = null;
    else if (typeof value === "object" && !isArray(value)) patch.layers[key] = toLayer(value);
    else loose[key] = value;
  }
  if (Object.keys(loose).length) {
    const extra = toLayer(loose), mesh = patch.layers.mesh || { arrays: {}, params: {} };
    patch.layers.mesh = { ...mesh, arrays: { ...mesh.arrays, ...extra.arrays }, params: { ...mesh.params, ...extra.params } };
  }
  return patch;
}

// Whether a patch has frames to play.
export function animates(patch) {
  return !!patch.loop || Object.values(patch.layers).some((layer) =>
    layer && Object.entries(layer.arrays || {}).some(([key, a]) => a.shape.length > RANK[key] && a.shape[0] > 1));
}

// CONTROLS, from any language, as inputs: [min, max, start, step?] is a slider, true/false a checkbox.
export function controlInputs(raw) {
  const out = {};
  for (const [name, c] of Object.entries(raw || {})) {
    if (typeof c === "boolean") out[name] = { type: "checkbox", value: c };
    else {
      const [lo, hi] = [+c[0], +c[1]];
      out[name] = { type: "range", min: lo, max: hi, value: +c[2], step: c[3] !== undefined ? +c[3] : (hi - lo) / 200 };
    }
  }
  return out;
}
