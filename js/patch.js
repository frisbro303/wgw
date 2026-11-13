// Patches: changes to a scene, in one shape whatever language a widget is written in.
//
//   {layers: {name: null | {type, arrays: {key: {data, shape}}, params: {key: value}, remove: [key]}}}
//
// null removes a layer. Otherwise the layer becomes exactly what's given: its parameters are replaced, the arrays
// in `remove` dropped, and those in `arrays` set; arrays not mentioned were sent before and are unchanged. Arrays
// are typed arrays with a shape, row-major; one more axis than the key expects (see RANK) is a leading frames axis.

export const ARRAYS = { values: Float32Array, vertices: Float32Array, vectors: Float32Array, faces: Uint32Array, segments: Uint32Array };
const WIDTH = { values: 1, vertices: 3, vectors: 3, faces: 3, segments: 2 };
export const RANK = { values: 1, vertices: 2, vectors: 2, faces: 2, segments: 2 };

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

// Per widget, (layer, array name) -> the array object last sent, so one returned again isn't sent again.
const sentBy = new Map();

export function markSent(key, layer, field, value) {
  if (!sentBy.has(key)) sentBy.set(key, new Map());
  sentBy.get(key).set(layer + "\0" + field, value);
}

// A JS widget's scene as a patch. A scene is layers by name, {type: "mesh" | "points" | "lines" | "vectors",
// ...arrays, ...params} or null to remove one; a single layer means {mesh: layer}. The same rules as Python's:
// each layer is replaced exactly as given, and arrays already sent (the same object) aren't sent again.
export function toPatch(key, scene) {
  if (scene == null) return null;
  if (typeof scene.type === "string") scene = { mesh: scene };
  if (!sentBy.has(key)) sentBy.set(key, new Map());
  const sent = sentBy.get(key), patch = { layers: {} };
  for (const [name, layer] of Object.entries(scene)) {
    const id = (field) => name + "\0" + field;
    if (layer === null) {
      patch.layers[name] = null;
      for (const field in ARRAYS) sent.delete(id(field));
      continue;
    }
    if (!layer.type) throw new Error(`${name}: a layer needs a type: "mesh", "points", "lines" or "vectors"`);
    const entry = { type: layer.type, arrays: {}, params: {}, remove: [] };
    for (const [k, v] of Object.entries(layer)) {
      if (k === "type" || v == null) continue;
      const field = k === "origins" ? "vertices" : k;
      if (!(field in ARRAYS)) entry.params[k] = v;
      else if (sent.get(id(field)) !== v) {
        sent.set(id(field), v);
        entry.arrays[field] = toArray(field, v);
      }
    }
    for (const field in ARRAYS) {
      const present = layer[field] != null || (field === "vertices" && layer.origins != null);
      if (!present && sent.has(id(field))) {
        sent.delete(id(field));
        entry.remove.push(field);
      }
    }
    patch.layers[name] = entry;
  }
  return patch;
}

// Whether a patch has frames to play.
export function animates(patch) {
  return Object.values(patch.layers).some((layer) =>
    layer && Object.entries(layer.arrays || {}).some(([key, a]) => a.shape.length > RANK[key] && a.shape[0] > 1));
}

// CONTROLS, from any language, as inputs: [min, max, start, step?] is a slider, with "log" anywhere after start
// for one that moves evenly in powers of ten (for parameters spanning orders of magnitude); true/false a checkbox.
// A log slider's min, max, value and step are in log10 units; widgets get 10 ** value.
export function controlInputs(raw) {
  const out = {};
  for (const [name, c] of Object.entries(raw || {})) {
    if (typeof c === "boolean") { out[name] = { type: "checkbox", value: c }; continue; }
    const log = c.slice(3).includes("log"), step = c.slice(3).find((x) => typeof x === "number");
    const [lo, hi, start] = [+c[0], +c[1], +c[2]].map(log ? Math.log10 : (x) => x);
    out[name] = { type: "range", log, min: lo, max: hi, value: start, step: log ? 0.01 : step !== undefined ? step : (hi - lo) / 200 };
  }
  return out;
}
