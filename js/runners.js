// Runners run widgets, one per language, chosen by the widget file's extension. A runner is
//   preload()                                 optional: start up early (Python takes seconds)
//   load(key, url, mesh) -> Promise<{controls, functions, result}>
//   call(key, name, index, layer, controls) -> Promise<patch>
// where mesh is the mesh file's arrays ({name: {data, shape, dtype}}) or null, controls the widget's CONTROLS as
// written, functions which of "update" and "pick" it has, and results patches (see patch.js).
// Add a language with WGW.runners.ext = {...}: Rust or C++ compiled to WebAssembly, a server, ...

import { toPatch } from "./patch.js";

const PYODIDE = "https://cdn.jsdelivr.net/pyodide/v0.26.4/full/";
const HERE = new URL(".", import.meta.url).href;

async function text(url) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  return res.text();
}

// ---- Python, in one worker shared by every viewer

let worker = null, nextId = 0;
const waiting = new Map();

function python() {
  if (worker) return;
  worker = new Worker(new URL("./python-worker.js", import.meta.url));
  worker.onmessage = (e) => {
    const w = waiting.get(e.data.id);
    waiting.delete(e.data.id);
    if (e.data.error) w.reject(new Error(e.data.error)); else w.resolve(e.data.result);
  };
  // wgw.py, from beside this file, so widgets can `from wgw import ...`.
  fetch(HERE + "wgw.py").then((r) => (r.ok ? r.text() : null), () => null)
    .then((helper) => worker.postMessage({ type: "init", base: PYODIDE, helper }));
}

function request(message) {
  python();
  return new Promise((resolve, reject) => {
    message.id = ++nextId;
    waiting.set(message.id, { resolve, reject });
    worker.postMessage(message);
  });
}

const py = {
  preload: python,
  async load(key, url, mesh) {
    const source = await text(url);
    let arrays = null;
    if (mesh) {
      arrays = {};
      for (const [name, a] of Object.entries(mesh)) {  // each sliced out, so only its own bytes are copied
        arrays[name] = { data: a.data.slice(), shape: a.shape, dtype: a.dtype };
      }
    }
    return request({ type: "load", key, source, arrays });
  },
  call(key, name, index, layer, controls) {
    return request({ type: "call", key, name, index, layer, controls });
  },
};

// ---- JavaScript: an ES module exporting CONTROLS, setup(mesh), update(controls), pick(index, controls, layer)

const modules = new Map();

const js = {
  async load(key, url, mesh) {
    const module = await import(new URL(url, location.href).href);
    modules.set(key, module);
    const result = module.setup ? await module.setup(mesh) : null;
    return {
      controls: module.CONTROLS || {},
      functions: ["update", "pick"].filter((f) => typeof module[f] === "function"),
      result: toPatch(result),
    };
  },
  async call(key, name, index, layer, controls) {
    const module = modules.get(key);
    return toPatch(await (name === "update" ? module.update(controls) : module.pick(index, controls, layer)));
  },
};

export const runners = { py, js };

export function runnerFor(url) {
  const ext = url.split(/[?#]/)[0].split(".").pop();
  if (!runners[ext]) throw new Error(`no runner for .${ext} widgets`);
  return runners[ext];
}
