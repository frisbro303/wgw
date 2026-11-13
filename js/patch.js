// Patches, as the viewer applies them (widget.js decodes them from a widget's bytes):
//
//   {layers: {name: null | {type, arrays: {key: {data, shape}}, params: {key: value}, remove: [key]}}}
//
// null removes a layer. Otherwise the layer becomes exactly what's given: its parameters are replaced, the arrays
// in `remove` dropped, and those in `arrays` set; arrays not mentioned are unchanged. Arrays are typed arrays with a
// shape, row-major; one more axis than the key expects (see RANK) is a leading frames axis.

export const RANK = { values: 1, vertices: 2, vectors: 2, faces: 2, segments: 2 };

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
