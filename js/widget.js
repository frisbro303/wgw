// Widgets: WebAssembly modules (written in Rust with wgw-widget, or anything that compiles to WebAssembly). A
// widget exports two functions, in Typst's plugin protocol, so the same file runs here and in a Typst document:
//
//   setup(mesh)         -> JSON: {controls, frames, fps, loop, grid, picked, panes, keepPicks}
//   scene(mesh, state)  -> a patch (see wgw-scene's patch.rs): the layers for a state, {controls, picks, frame}
//
// `mesh` is the figure's mesh file, if it has one apart from the widget (empty after the first call: the widget
// keeps it). An animation is frames: scene is asked for each, and they're stacked into arrays with a frames axis. A
// layer a later frame leaves out stays as it is in frame 0, so what never changes is sent once.
//
// Widgets run in workers (widget-worker.js), so building scenes never holds up the page: a few workers, shared by
// the page's widgets, so a page of many figures doesn't start many. A widget's long work (an animation's frames)
// pauses between frames, so the others on its worker carry on, and so it can be cancelled.

const text = new TextDecoder(), utf8 = new TextEncoder();

const POOL = Math.max(1, Math.min(4, (globalThis.navigator?.hardwareConcurrency || 2) - 1));
const workers = [];  // each: {worker, widgets (how many it hosts), waiting (requests by id)}
let nextRequest = 0, nextWidget = 0;

function startWorker() {
  const w = { worker: new Worker(new URL("widget-worker.js", import.meta.url), { type: "module" }), widgets: 0, waiting: new Map() };
  w.worker.onmessage = ({ data }) => {
    const request = w.waiting.get(data.id);
    if (!request) return;  // its widget was stopped
    if ("partial" in data) return request.partial?.(data.partial);
    w.waiting.delete(data.id);
    if (data.error !== undefined) request.reject(new Error(data.error)); else request.resolve(data.result);
  };
  // If the worker can't start, everything waiting on it is rejected, so nothing hangs.
  w.worker.onerror = (e) => {
    for (const request of w.waiting.values()) request.reject(new Error(e.message || "the widgets' worker failed to start"));
    w.waiting.clear();
  };
  workers.push(w);
  return w;
}

// The worker with the fewest widgets, or a new one while the pool has room and every worker is busy.
function pickWorker() {
  const least = workers.reduce((a, b) => (b.widgets < a.widgets ? b : a), workers[0]);
  return !least || (least.widgets > 0 && workers.length < POOL) ? startWorker() : least;
}

// A widget, on one of the page's workers:
//   setup                               what the widget said about itself
//   scene(state, frames, first, onFirst) resolves to the patch of every frame, stacked; for an animation, onFirst
//                                       gets frame `first` alone as soon as it's built, to show while the rest are.
//                                       Resolves to null if cancelled.
//   cancel()                            stop building the scene asked for (an animation's frames are left)
//   terminate()                         drop the widget, rejecting anything it was asked
export async function loadWidget(bytes, mesh) {
  const w = pickWorker(), widget = nextWidget++;
  w.widgets++;
  const ask = (message, partial) => new Promise((resolve, reject) => {
    const id = nextRequest++;
    w.waiting.set(id, { resolve, reject, partial, widget });
    w.worker.postMessage({ ...message, id, widget });
  });
  let stopped = false;
  const terminate = () => {
    if (stopped) return;
    stopped = true;
    w.worker.postMessage({ type: "drop", widget });
    for (const [id, request] of w.waiting) {
      if (request.widget === widget) { request.reject(new Error("the widget was stopped")); w.waiting.delete(id); }
    }
    if (--w.widgets === 0) { w.worker.terminate(); workers.splice(workers.indexOf(w), 1); }
  };
  let setup;
  try {
    setup = await ask({ type: "load", bytes, mesh: mesh || null });
  } catch (err) {
    terminate();
    throw err;
  }
  return {
    setup,
    scene: (state, frames, first = 0, onFirst) => ask({ type: "scene", state, frames, first, mesh: null }, onFirst),
    cancel: () => { if (!stopped) w.worker.postMessage({ type: "cancel", widget }); },
    terminate,
  };
}

// A widget whose scenes a server makes (wgw for Python): `setup` as given, and each scene the patch's bytes in
// answer to the state, POSTed to `url` as JSON; an error's text is the response to a failed request. Cancelling
// drops the request (the server still finishes it).
export function remoteWidget(url, setup) {
  let request = null;
  return {
    setup,
    async scene(state) {
      const mine = (request = new AbortController());
      try {
        const res = await fetch(url, { method: "POST", body: JSON.stringify(state), signal: mine.signal });
        if (!res.ok) throw new Error((await res.text()) || `${url}: HTTP ${res.status}`);
        return decodePatch(new Uint8Array(await res.arrayBuffer()));
      } catch (err) {
        if (mine.signal.aborted) return null;
        throw err;
      }
    },
    cancel: () => request?.abort(),
    terminate: () => request?.abort(),
  };
}

// A widget module from its bytes, with its two functions as calls on byte arrays (in the worker). A panic stops
// the module mid-call (its memory left as it was), so the next call starts a fresh instance, given the mesh again.
// wgw-widget sends a panic's message before stopping; that's the error, rather than "unreachable".
export async function instantiate(bytes) {
  const module = await WebAssembly.compile(bytes);
  const empty = new Uint8Array(0);
  let args = empty, result = null, instance = null, memory, kept = empty;
  const env = {
    wasm_minimal_protocol_write_args_to_buffer(ptr) {
      new Uint8Array(memory.buffer, ptr, args.length).set(args);
    },
    wasm_minimal_protocol_send_result_to_host(ptr, len) {
      result = new Uint8Array(memory.buffer, ptr, len).slice();
    },
  };
  // The mesh for a call: as given, or, for a fresh instance, the last one given.
  const meshFor = (mesh) => {
    if (mesh && mesh.length) kept = mesh;
    return instance ? mesh || empty : kept;
  };
  const call = (name, ...parts) => {
    if (!instance) {
      instance = new WebAssembly.Instance(module, { typst_env: env });
      memory = instance.exports.memory;
    }
    const fn = instance.exports[name];
    if (typeof fn !== "function") throw new Error(`the widget has no ${name}()`);
    args = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
    let at = 0;
    for (const p of parts) { args.set(p, at); at += p.length; }
    result = null;
    let code;
    try {
      code = fn(...parts.map((p) => p.length));
    } catch (err) {
      instance = null;
      throw new Error(result ? text.decode(result) : `the widget stopped: ${err.message}`);
    }
    if (code !== 0) throw new Error(text.decode(result || empty));
    return result;
  };
  return {
    setup: (mesh) => JSON.parse(text.decode(call("setup", meshFor(mesh)))),
    scene: (mesh, state) => call("scene", meshFor(mesh), utf8.encode(JSON.stringify(state))),
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
// are taken from the first), parameters from the first. A layer only frame 0 has is the same in every frame.
export function stackFrames(patches) {
  if (patches.length === 1) return patches[0];
  const first = patches[0], out = { layers: {} };
  for (const [name, l] of Object.entries(first.layers)) {
    if (l === null) { out.layers[name] = null; continue; }
    if (!(name in patches[1].layers)) { out.layers[name] = l; continue; }  // unchanging: sent once
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
