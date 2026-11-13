// Widgets: WebAssembly modules (written in Rust with wgw-widget, or anything that compiles to WebAssembly). A
// widget exports two functions, in Typst's plugin protocol, so the same file runs here and in a Typst document:
//
//   setup(mesh)         -> JSON: {controls, frames, fps, loop, grid, picked, panes, keepPicks}
//   scene(mesh, state)  -> a patch (see wgw-scene's patch.rs): the layers for a state, {controls, picks, frame}
//
// `mesh` is the figure's mesh file, if it has one apart from the widget (empty after the first call: the widget
// keeps it). An animation is frames: scene is asked for each, and they're stacked into arrays with a frames axis.

const text = new TextDecoder(), utf8 = new TextEncoder();

// A widget module from its bytes, with its two functions as async-free calls on byte arrays.
export async function loadWidget(bytes) {
  let args = new Uint8Array(0), result = null;
  let memory;
  const env = {
    wasm_minimal_protocol_write_args_to_buffer(ptr) {
      new Uint8Array(memory.buffer, ptr, args.length).set(args);
    },
    wasm_minimal_protocol_send_result_to_host(ptr, len) {
      result = new Uint8Array(memory.buffer, ptr, len).slice();
    },
  };
  const { instance } = await WebAssembly.instantiate(bytes, { typst_env: env });
  memory = instance.exports.memory;
  const call = (name, ...parts) => {
    const fn = instance.exports[name];
    if (typeof fn !== "function") throw new Error(`the widget has no ${name}()`);
    args = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
    let at = 0;
    for (const p of parts) { args.set(p, at); at += p.length; }
    result = null;
    const code = fn(...parts.map((p) => p.length));
    if (code !== 0) throw new Error(text.decode(result || new Uint8Array()));
    return result;
  };
  return {
    setup: (mesh) => JSON.parse(text.decode(call("setup", mesh || new Uint8Array()))),
    scene: (mesh, state) => decodePatch(call("scene", mesh || new Uint8Array(), utf8.encode(JSON.stringify(state)))),
  };
}

const TYPES = { f4: Float32Array, u4: Uint32Array };

// A patch from its bytes ("WGW1", the header's length, a JSON header, the arrays) as the viewer applies it:
// {layers: {name: null | {type, params, remove, arrays: {key: {data, shape}}}}}.
export function decodePatch(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (text.decode(bytes.subarray(0, 4)) !== "WGW1") throw new Error("the widget's result isn't a patch");
  const length = view.getUint32(4, true), start = 8 + length;
  const header = JSON.parse(text.decode(bytes.subarray(8, start)));
  const data = bytes.slice(start).buffer;  // aligned at 0, so each array can be viewed in place
  const layers = {};
  for (const [name, l] of header.layers) {
    if (l === null) { layers[name] = null; continue; }
    const arrays = {};
    for (const [key, a] of Object.entries(l.arrays)) {
      const n = a.shape.reduce((x, y) => x * y, 1);
      arrays[key] = { data: new TYPES[a.dtype](data, a.offset, n), shape: a.shape };
    }
    layers[name] = { type: l.type, params: l.params, remove: l.remove, arrays };
  }
  return { layers };
}

// Frames' patches as one: each array gets a leading frames axis (faces and segments, the same in every frame,
// are taken from the first), parameters from the first.
export function stackFrames(patches) {
  if (patches.length === 1) return patches[0];
  const first = patches[0], out = { layers: {} };
  for (const [name, l] of Object.entries(first.layers)) {
    if (l === null) { out.layers[name] = null; continue; }
    const arrays = {};
    for (const [key, a] of Object.entries(l.arrays)) {
      if (key === "faces" || key === "segments") { arrays[key] = a; continue; }
      const data = new a.data.constructor(a.data.length * patches.length);
      patches.forEach((p, k) => data.set(p.layers[name].arrays[key].data, k * a.data.length));
      arrays[key] = { data, shape: [patches.length, ...a.shape] };
    }
    out.layers[name] = { ...l, arrays };
  }
  return out;
}

// Whether bytes are a widget (a WebAssembly module) rather than a mesh file.
export const isWidget = (bytes) => bytes.length >= 4 && bytes[0] === 0 && bytes[1] === 0x61 && bytes[2] === 0x73 && bytes[3] === 0x6d;
