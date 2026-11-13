// Runs Python widgets with Pyodide, off the page's thread. One worker serves every viewer on a page.
// Messages in: {type: "init", base, helper}, then {id, type: "load", key, source, arrays} and
// {id, type: "call", key, name, index, layer, controls}. Out: {id, result} or {id, error}; results are patches
// (see patch.js), their arrays transferred rather than copied.

const PRELUDE = `
import inspect, json
import numpy as np

_widgets = {}
_ARRAYS = {"values": np.float32, "vertices": np.float32, "vectors": np.float32, "faces": np.uint32, "segments": np.uint32}


def _json(o):
    return o.item() if hasattr(o, "item") else o.tolist() if hasattr(o, "tolist") else str(o)


def _layer(name, fields, arrays):
    layer = {"params": {}}
    for key, value in fields.items():
        if value is None:
            continue
        if key == "type":
            layer["type"] = value
        elif key in _ARRAYS:
            arrays.append((name, key, np.ascontiguousarray(value, dtype=_ARRAYS[key])))
        else:
            layer["params"][key] = value
    return layer


def _out(result):
    """A widget's result as a patch: JSON for the layers' types and parameters, and the arrays to transfer."""
    if result is None:
        return None
    if not isinstance(result, dict):
        result = {"values": result}
    meta, arrays, loose = {"layers": {}}, [], {}
    for key, value in result.items():
        if key in ("loop", "fps"):
            meta[key] = value
        elif value is None:
            meta["layers"][key] = None
        elif isinstance(value, dict):
            meta["layers"][key] = _layer(key, value, arrays)
        else:
            loose[key] = value
    if loose:
        extra = _layer("mesh", loose, arrays)
        mesh = meta["layers"].get("mesh") or {"params": {}}
        mesh["params"].update(extra["params"])
        meta["layers"]["mesh"] = mesh
    return {"meta": json.dumps(meta, default=_json), "arrays": arrays}


def _load(key, source, arrays):
    ns = {"__name__": "widget"}
    exec(compile(source, key, "exec"), ns)
    _widgets[key] = ns
    mesh = None
    if arrays is not None:
        mesh = {}
        for name, a in arrays.to_py(depth=1).items():
            mesh[name] = np.frombuffer(a.data.to_bytes(), dtype="<" + a.dtype).reshape(a.shape.to_py()).copy()
    result = ns["setup"](mesh) if callable(ns.get("setup")) else None
    return {
        "controls": json.dumps(dict(ns.get("CONTROLS", {})), default=_json),
        "functions": json.dumps([f for f in ("update", "pick") if callable(ns.get(f))]),
        "result": _out(result),
    }


def _call(key, name, index, layer, controls):
    fn, kwargs = _widgets[key][name], controls.to_py()
    if name == "update":
        return _out(fn(**kwargs))
    if "layer" in inspect.signature(fn).parameters:
        kwargs["layer"] = layer
    return _out(fn(int(index), **kwargs))
`;

let start;
const ready = new Promise((resolve) => { start = resolve; }).then(async ({ base, helper }) => {
  importScripts(base + "pyodide.js");
  const py = await loadPyodide({ indexURL: base });
  await py.loadPackage("numpy");
  if (helper) py.FS.writeFile("/home/pyodide/wgw.py", helper);
  py.runPython(PRELUDE);
  return py;
});

// A patch out of Python's memory: each array copied once, into a buffer the page then owns.
function exported(proxy, transfer) {
  if (proxy == null) return null;
  const patch = JSON.parse(proxy.get("meta"));
  const arrays = proxy.get("arrays");
  for (const item of arrays) {
    const [name, key, array] = [item.get(0), item.get(1), item.get(2)];
    const buffer = array.getBuffer(), data = buffer.data.slice();
    patch.layers[name] = patch.layers[name] || { params: {} };
    (patch.layers[name].arrays = patch.layers[name].arrays || {})[key] = { data, shape: Array.from(buffer.shape) };
    transfer.push(data.buffer);
    buffer.release();
    array.destroy();
    item.destroy();
  }
  arrays.destroy();
  proxy.destroy();
  return patch;
}

async function handle(m) {
  try {
    const py = await ready, transfer = [];
    if (m.type === "load") {
      await py.loadPackagesFromImports(m.source);
      const info = py.globals.get("_load")(m.key, m.source, m.arrays);
      const reply = {
        controls: JSON.parse(info.get("controls")),
        functions: JSON.parse(info.get("functions")),
        result: exported(info.get("result"), transfer),
      };
      info.destroy();
      postMessage({ id: m.id, result: reply }, transfer);
    } else {
      const result = py.globals.get("_call")(m.key, m.name, m.index, m.layer, m.controls);
      postMessage({ id: m.id, result: exported(result, transfer) }, transfer);
    }
  } catch (err) {
    postMessage({ id: m.id, error: String((err && err.message) || err) });
  }
}

let queue = Promise.resolve();
onmessage = (e) => {
  if (e.data.type === "init") start(e.data);
  else queue = queue.then(() => handle(e.data));
};
