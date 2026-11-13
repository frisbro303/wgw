// Runs Python widgets with Pyodide, off the page's thread. One worker serves every viewer on a page.
// Messages in: {type: "init", base, helper}, then {id, type: "load", key, url, source, arrays} and
// {id, type: "call", key, name: "change" | "pick", index, layer, controls}. Out: {id, result} or {id, error};
// results are patches (see patch.js), their arrays transferred rather than copied.

const PRELUDE = `
import builtins, inspect, io, json, os, posixpath
from urllib.parse import urljoin
import numpy as np
from js import XMLHttpRequest
from wgw import Checkbox, Layer, Mesh, Widget

# Files a widget reads by relative path (open, np.load, Mesh.load, ...) come from beside the widget on the web: each
# widget runs in a folder of its own, and the first time it opens a file that isn't there, it's fetched into it.
_open, _folders = builtins.open, {}


def _enter(key):
    os.chdir(_folders[key][0])


def _fetch(path):
    folder, url = _folders[next(k for k, f in _folders.items() if f[0] == os.getcwd())]
    request = XMLHttpRequest.new()
    request.open("GET", urljoin(url, path), False)  # synchronous: allowed in a worker, and open() can't wait
    request.responseType = "arraybuffer"
    request.send(None)
    if request.status != 200:
        raise FileNotFoundError(f"{path}: {urljoin(url, path)} answered HTTP {request.status}")
    target = posixpath.normpath(posixpath.join(folder, path))
    os.makedirs(posixpath.dirname(target), exist_ok=True)
    with _open(target, "wb") as f:
        f.write(request.response.to_bytes())


def _open_fetching(file, mode="r", *args, **kwargs):
    if isinstance(file, (str, os.PathLike)) and set(mode) <= set("rbt"):
        path = os.fspath(file)
        if not posixpath.isabs(path) and not os.path.exists(path) and os.getcwd() in (f[0] for f in _folders.values()):
            _fetch(path)
    return _open(file, mode, *args, **kwargs)


builtins.open = io.open = _open_fetching

_widgets = {}
_sent = {}  # per widget, (layer, array name) -> the array object last sent, so it isn't sent again
_ARRAYS = {"values": np.float32, "vertices": np.float32, "vectors": np.float32, "faces": np.uint32, "segments": np.uint32}


def _json(o):
    return o.item() if hasattr(o, "item") else o.tolist() if hasattr(o, "tolist") else str(o)


def _out(key, scene):
    """A scene (layers by name) as a patch: JSON for kinds and parameters, and the arrays not already sent."""
    if scene is None:
        return None
    if isinstance(scene, Layer):
        scene = {"mesh": scene}
    if not isinstance(scene, dict):
        raise TypeError(f"widget functions return a layer, a dict of layers by name, or None; got {type(scene).__name__}")
    sent = _sent.setdefault(key, {})
    meta, arrays = {"layers": {}}, []
    for name, layer in scene.items():
        if layer is None:
            meta["layers"][name] = None
            for k in [k for k in sent if k[0] == name]:
                del sent[k]
            continue
        if not isinstance(layer, Layer):
            raise TypeError(f"{name}: expected a Mesh, Points, Lines or Vectors, got {type(layer).__name__}")
        fields = layer._wgw_layer()
        entry = {"type": fields.pop("type"), "params": {}, "remove": []}
        for k, v in fields.items():
            if k not in _ARRAYS:
                entry["params"][k] = v
            elif sent.get((name, k)) is not v:
                sent[(name, k)] = v
                arrays.append((name, k, np.ascontiguousarray(v, dtype=_ARRAYS[k])))
        for k in _ARRAYS:  # arrays the layer had before but not now
            if k not in fields and (name, k) in sent:
                del sent[(name, k)]
                entry["remove"].append(k)
        meta["layers"][name] = entry
    return {"meta": json.dumps(meta, default=_json), "arrays": arrays}


def _load(key, url, source, arrays):
    folder = f"/widgets/{len(_folders)}"
    os.makedirs(folder, exist_ok=True)
    _folders[key] = (folder, url)
    _enter(key)
    ns = {"__name__": "widget", "__file__": posixpath.join(folder, posixpath.basename(url))}
    exec(compile(source, key, "exec"), ns)
    make = ns.get("widget")
    if not callable(make):
        raise TypeError("a widget file defines widget(mesh) -> Widget")
    mesh = None
    if arrays is not None:
        data = {}
        for name, a in arrays.to_py(depth=1).items():
            data[name] = np.frombuffer(a.data.to_bytes(), dtype="<" + a.dtype).reshape(a.shape.to_py()).copy()
        mesh = Mesh(data.pop("vertices"), data.pop("faces"), data=data)
        _sent[key] = {("mesh", "vertices"): mesh.vertices, ("mesh", "faces"): mesh.faces}  # the page shows them already
    w = make(mesh) if inspect.signature(make).parameters else make()
    if not isinstance(w, Widget):
        raise TypeError(f"widget() returns a Widget, got {type(w).__name__}")
    _widgets[key] = w
    controls = {name: c._spec() for name, c in w.controls.items()}
    if isinstance(w.grid, Checkbox):
        controls["grid"] = w.grid._spec()  # the viewer's own checkbox
    grid = w.grid.value if isinstance(w.grid, Checkbox) else w.grid
    return {
        "controls": json.dumps(controls, default=_json),
        "functions": json.dumps([f for f in ("change", "pick") if getattr(w, "on_" + f)]),
        "options": json.dumps({"grid": bool(grid), "fps": w.fps, "loop": bool(w.loop)}),
        "result": _out(key, w.scene),
    }


def _call(key, name, index, layer, controls):
    _enter(key)
    w, kwargs = _widgets[key], controls.to_py()
    if name == "change":
        return _out(key, w.on_change(**kwargs))
    if "layer" in inspect.signature(w.on_pick).parameters:
        kwargs["layer"] = layer
    return _out(key, w.on_pick(int(index), **kwargs))
`;

let start;
const ready = new Promise((resolve) => { start = resolve; }).then(async ({ base, helper }) => {
  importScripts(base + "pyodide.js");
  const py = await loadPyodide({ indexURL: base });
  await py.loadPackage("numpy");
  if (!helper) throw new Error("wgw.py, which Python widgets need, wasn't found beside wgw.js");
  py.FS.writeFile("/home/pyodide/wgw.py", helper);
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
      const info = py.globals.get("_load")(m.key, m.url, m.source, m.arrays);
      const reply = {
        controls: JSON.parse(info.get("controls")),
        functions: JSON.parse(info.get("functions")),
        ...JSON.parse(info.get("options")),
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
