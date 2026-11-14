"""wgw for Python: interactive 3D figures of NumPy arrays and wgw widgets, in a window or inline in Jupyter.

A thin head: layers become the patches wgw's page code reads, and wgw-window (a small program in wgw's repository)
shows them, with the page code and its WebAssembly core in a webview, calling your function when a slider moves or
a point is picked. Drawing, turning, the controls and widgets are wgw's own.

    import numpy as np, wgw

    wgw.Figure("torus.wgw").show()                         # a widget (Rust, compiled to wasm), or a mesh file
    wgw.Figure(wgw.Mesh(vertices, faces, values=k)).show()  # arrays

    def scene(state):                                      # state["t"], state.pick
        return {"mesh": wgw.Mesh(vertices * state["t"], faces)}

    wgw.Figure(scene, {"t": wgw.Slider(0.5, 2, 1), "edges": False}).show()

Arrays with one more leading axis than usual (`(frames, n, 3)` vertices, `(frames, n)` values) are an animation.
"""
import json
import os
import shutil
import struct
import subprocess
import sys
import threading
import traceback
from pathlib import Path

import numpy as np

__all__ = ["Figure", "Mesh", "Points", "Lines", "Vectors", "Slider", "State", "encode", "read", "write"]

INTS = ("faces", "segments")  # stored as uint32; every other array as float32
PROTOCOL = "1"  # what this head speaks to wgw-window (docs/heads.typ)

# Each kind's parameters (besides values and range), as wgw's reference lists them.
COMMON = {"color", "colors", "min", "max", "visible", "pickable", "pane"}
PARAMS = {
    "mesh": COMMON | {"flat", "edges", "edge_color"},
    "points": COMMON | {"size", "lift"},
    "lines": COMMON | {"width", "closed"},
    "vectors": COMMON | {"scale", "width"},
}


# ---- layers


class Layer:
    """A layer: its type ("mesh", "points", "lines" or "vectors"), arrays and parameters (see wgw's reference).
    Its parameters and arrays' shapes are checked as it's made, so a mistake shows where it was made."""

    def __init__(self, type, arrays, values=None, range=None, **params):
        unknown = sorted(set(params) - PARAMS[type])
        if unknown:
            raise TypeError(f"a {type} layer has no parameter {', '.join(map(repr, unknown))}; "
                            f"its parameters are values, range, {', '.join(sorted(PARAMS[type]))}")
        self.type = type
        self.arrays = {k: np.asarray(v) for k, v in arrays.items() if v is not None}
        if values is not None:
            self.arrays["values"] = np.asarray(values)
        if range is not None:
            if len(range) != 2:
                raise ValueError(f"range is (min, max), not {range!r}")
            params["min"], params["max"] = range
        self.params = params
        self._check()

    def _check(self):
        def fail(text):
            raise ValueError(f"{self.type}: {text}")
        v = self.arrays["vertices"]
        if v.ndim not in (2, 3) or v.shape[-1] != 3:
            fail(f"vertices are (n, 3), or (frames, n, 3) for an animation, not {v.shape}")
        n = v.shape[-2]
        for key, a in self.arrays.items():
            if key in INTS:
                width = 3 if key == "faces" else 2
                if a.ndim != 2 or a.shape[1] != width:
                    fail(f"{key} are (k, {width}) vertex indices, not {a.shape}")
                if not np.issubdtype(a.dtype, np.integer) and not np.array_equal(a, np.round(a)):
                    fail(f"{key} are vertex indices, whole numbers")
                if a.size and (a.min() < 0 or a.max() >= n):
                    fail(f"{key} index the {n} vertices from 0, so 0 to {n - 1}, not {a.min()} to {a.max()}")
            elif key == "vectors" and a.shape[-2:] != (n, 3):
                fail(f"vectors are one per origin, ({n}, 3), not {a.shape}")
            elif key == "values" and (a.ndim not in (1, 2) or a.shape[-1] != n):
                fail(f"values are one per vertex, ({n},), or (frames, {n}) for an animation, not {a.shape}")

    def __repr__(self):
        shapes = ", ".join(f"{k}{list(a.shape)}" for k, a in self.arrays.items())
        return f"wgw.Layer({self.type!r}, {shapes})"


def Mesh(vertices, faces, **params):
    """A triangle mesh: `(n, 3)` vertices and `(m, 3)` faces (from 0). Parameters: values (per vertex), color,
    colors, range=(min, max), flat, edges, edge_color, pane, pickable, visible."""
    return Layer("mesh", {"vertices": vertices, "faces": faces}, **params)


def Points(vertices, **params):
    """Points, drawn as dots: `(n, 3)`. Parameters: values, color, colors, range, size, lift, pane, pickable,
    visible."""
    return Layer("points", {"vertices": vertices}, **params)


def Lines(vertices, segments=None, **params):
    """A curve through `(n, 3)` vertices, or the `(k, 2)` segments given (from 0). Parameters: values, color,
    colors, range, width, closed, pane, pickable, visible."""
    return Layer("lines", {"vertices": vertices, "segments": segments}, **params)


def Vectors(origins, vectors, **params):
    """Arrows from `(n, 3)` origins along `(n, 3)` vectors. Parameters: values, color, colors, range, scale, width,
    pane, pickable, visible."""
    return Layer("vectors", {"vertices": origins, "vectors": vectors}, **params)


# ---- .wgw mesh files: a mesh and any arrays a widget needs (MeshData in wgw-widget)

DTYPES = {"float32": "f4", "float16": "f2", "float64": "f8", "uint32": "u4", "int32": "i4", "uint16": "u2", "uint8": "u1"}


def write(path, vertices, faces, **arrays):
    """Write a mesh, and any arrays its widget needs (a spectrum, say), to a .wgw file. vertices are stored as
    float32 and faces as uint32; other arrays keep their dtype, which must be float16, float32, float64, int32,
    uint8, uint16 or uint32 (cast to float16 to halve a download)."""
    arrays = {"vertices": np.asarray(vertices, np.float32), "faces": np.asarray(faces, np.uint32), **arrays}
    header, offset, blobs = {}, 0, []
    for name, array in arrays.items():
        array = np.ascontiguousarray(array)
        if array.dtype.name not in DTYPES:
            raise TypeError(f"{name}: dtype {array.dtype} can't be stored; use one of {', '.join(DTYPES)}")
        header[name] = {"dtype": DTYPES[array.dtype.name], "shape": list(array.shape), "offset": offset}
        blob = array.astype(array.dtype.newbyteorder("<"), copy=False).tobytes()
        blobs.append(blob + b"\0" * (-len(blob) % 8))
        offset += len(blobs[-1])
    text = json.dumps({"arrays": header}, separators=(",", ":")).encode()
    text += b" " * (-(len(text) + 8) % 8)
    Path(path).write_bytes(b"WGW1" + struct.pack("<I", len(text)) + text + b"".join(blobs))


def read(path):
    """A .wgw mesh file's arrays by name, vertices and faces among them."""
    data = Path(path).read_bytes()
    if data[:4] != b"WGW1":
        raise ValueError(f"{path} isn't a .wgw mesh file" + (" (it's a widget)" if data[:4] == b"\0asm" else ""))
    (length,) = struct.unpack("<I", data[4:8])
    header, start = json.loads(data[8:8 + length])["arrays"], 8 + length
    return {name: np.frombuffer(data, "<" + a["dtype"], int(np.prod(a["shape"])), start + a["offset"]).reshape(a["shape"])
            for name, a in header.items()}


def _layers(scene):
    """A scene as layers by name: a dict (None removes a layer), or one layer, named by its type."""
    if scene is None:
        return {}
    if isinstance(scene, Layer):
        return {scene.type: scene}
    if isinstance(scene, dict):
        return scene
    raise TypeError(f"a scene is a dict of layers by name, or a layer, not {type(scene).__name__}")


def encode(scene):
    """A scene as the patch bytes wgw's page reads: "WGW1", the header's length, a JSON header, the arrays."""
    header, blobs, offset = [], [], 0
    for name, layer in _layers(scene).items():
        if layer is None:
            header.append([name, None])
            continue
        arrays = {}
        for key, a in layer.arrays.items():
            dtype = "u4" if key in INTS else "f4"
            a = np.ascontiguousarray(a, "<" + dtype)
            arrays[key] = {"dtype": dtype, "shape": list(a.shape), "offset": offset}
            blobs.append(a.tobytes())
            offset += len(blobs[-1])
        header.append([name, {"type": layer.type, "params": layer.params, "remove": [], "arrays": arrays}])
    text = json.dumps({"layers": header}, default=_plain).encode()
    text += b" " * (-(8 + len(text)) % 4)  # the arrays start 4-byte aligned
    return b"WGW1" + struct.pack("<I", len(text)) + text + b"".join(blobs)


def _plain(x):
    if isinstance(x, (np.generic, np.ndarray)):
        return x.tolist()
    raise TypeError(f"{type(x).__name__} isn't a parameter value")


# ---- controls and state


class Slider:
    """A slider from min to max, starting at value; log moves evenly in powers of ten."""

    def __init__(self, min, max, value, step=None, log=False):
        self.spec = [min, max, value] + ([step] if step is not None else []) + (["log"] if log else [])


class State(dict):
    """What a scene is for: the controls' values by name (state["t"]), and the picks so far, the latest last, each
    {"vertex", "layer", "point", "frame"}."""

    def __init__(self, values, picks):
        super().__init__(values)
        self.picks = picks

    @property
    def pick(self):
        """The latest pick, or None."""
        return self.picks[-1] if self.picks else None

    def __missing__(self, name):
        raise KeyError(f"no control named {name!r}; the controls are {', '.join(map(repr, self)) or 'none'}")


# ---- figures


class Figure:
    """A figure.

    scene: a .wgw file (a path or its bytes: a widget, or a mesh); its layers (a dict by name, or one layer); or a
        function of a State returning them, called again whenever a control moves or a point is picked.
    controls: sliders and checkboxes (True or False) by name, for a function.
    up: the axis that points up: "x", "y" or "z".
    view: the starting camera: (turn, tilt) in degrees, or (turn, tilt, scale).
    pick: when a pick reaches the function: "click", "hover" or "none".
    height: the figure's height in CSS pixels.
    fps, loop: an animation's frames per second, and whether it loops.
    grid: draw meshes' triangle grids; grid_toggle: a checkbox for readers to show them, starting as given.
    picked: a vertex of the layer "mesh" picked from the start. keep_picks: keep every pick, not only the latest.
    panes: the views' relative widths, such as (2, 1), for layers with pane=1.
    flat: flat shading for the layer "mesh".
    """

    def __init__(self, scene, controls=None, *, title="wgw", up="z", view=(-60, 20), pick="click", height=420,
                 fps=30, loop=True, grid=False, grid_toggle=None, picked=None, keep_picks=False, panes=None,
                 flat=False):
        if up not in ("x", "y", "z"):
            raise ValueError(f'up is "x", "y" or "z", not {up!r}')
        if pick not in ("click", "hover", "none"):
            raise ValueError(f'pick is "click", "hover" or "none", not {pick!r}')
        if isinstance(scene, (str, os.PathLike)):
            title, scene = Path(scene).name, Path(scene).read_bytes()
        self.scene, self.controls, self.title, self.height = scene, dict(controls or {}), title, height
        self.options = {"up": up, "view": list(view), "pick": pick, "flat": flat}
        self.setup = {
            "controls": {k: c.spec if isinstance(c, Slider) else bool(c) for k, c in self.controls.items()},
            "frames": 1, "fps": fps, "loop": loop, "keepPicks": keep_picks,
            "grid": {"toggle": bool(grid_toggle)} if grid_toggle is not None else bool(grid),
            "picked": picked, "panes": list(panes) if panes is not None else None,
        }
        self._lock = threading.Lock()
        self._urls = {}  # by window: where it's served
        self.closed = threading.Event()

    def render(self, lines):
        """The patch's bytes for a state, as wgw-window sends it: `control <value> <name>` and
        `pick <vertex> <frame> <x> <y> <z> <layer>` lines."""
        values = {k: c.spec[2] if isinstance(c, Slider) else bool(c) for k, c in self.controls.items()}
        picks = []
        for line in lines.splitlines():
            word = line.split(" ", 6)
            if word[0] == "control":
                values[" ".join(word[2:])] = word[1] == "true" if word[1] in ("true", "false") else float(word[1])
            elif word[0] == "pick":
                picks.append({"vertex": int(word[1]), "frame": int(word[2]), "point": [float(x) for x in word[3:6]], "layer": word[6]})
        with self._lock:
            return encode(self.scene(State(values, picks)))

    def _send(self, window):
        if window not in self._urls:
            if isinstance(self.scene, (bytes, bytearray)):
                kind, body = "src", bytes(self.scene)
            elif callable(self.scene):
                kind, body = "callback", b""
            else:
                kind, body = "scene", encode(self.scene)
            header = {"type": "figure", "title": self.title, "options": self.options, "setup": self.setup,
                      "height": self.height, "body": kind}
            self._urls[window] = window.figure(self, header, body)
        return self._urls[window]

    def save(self, path, *, width=800, height=None, scale=2, background=None, controls=None, fps=None, frames=None,
             seconds=None, spin=None):
        """Save the figure, headless: an image (.png) or a video (.gif, .apng; .mp4, .webm or .mov with ffmpeg on the
        PATH), drawn on the CPU by wgw's own renderer. width and height are in CSS pixels, scale pixels to each. A
        function's figure (or a widget's) is drawn with its controls as set up, or as `controls` gives. A video plays
        the animation through, or, for a figure that doesn't animate, turns it `spin` degrees (360) in `seconds` (4)."""
        controls = dict(controls or {})
        if isinstance(self.scene, (bytes, bytearray)):
            kind, body = "src", bytes(self.scene)
        elif callable(self.scene):
            values = {k: c.spec[2] if isinstance(c, Slider) else bool(c) for k, c in self.controls.items()}
            kind, body = "scene", encode(self.scene(State({**values, **controls}, [])))
        else:
            kind, body = "scene", encode(self.scene)
        figure = {"width": width, "height": height or self.height, "scale": scale, "background": background,
                  "up": self.options["up"], "view": self.options["view"], "flat": self.options["flat"]}
        video = {k: v for k, v in {"fps": fps, "frames": frames, "seconds": seconds, "spin": spin}.items() if v is not None}
        _window(serve=True).save({"type": "save", "path": str(Path(path).resolve()), "figure": figure, "video": video,
                                  "controls": controls, "setup": self.setup, "body": kind}, body)

    def _repr_html_(self):
        bar = 40 if self.setup["controls"] else 0  # the controls, under the figure
        url = self._send(_window(serve=True))
        return f'<iframe src="{url}" style="width:100%;height:{self.height + bar}px;border:0" title="{self.title}"></iframe>'

    def show(self, block=None):
        """Show the figure: inline in Jupyter; otherwise in a window, which closes when the program ends. A script
        (not an interactive session) waits here until the window is closed, or Ctrl+C."""
        if _in_jupyter():
            from IPython.display import display
            return display(self)
        self._send(_window(serve=False))
        if block is None:
            block = not hasattr(sys, "ps1") and not sys.flags.interactive
        if block:
            try:
                while not self.closed.wait(0.5):
                    pass
            except KeyboardInterrupt:
                pass


def _in_jupyter():
    try:
        from IPython import get_ipython
        return get_ipython() is not None and "IPKernelApp" in get_ipython().config
    except ImportError:
        return False


# ---- wgw-window: started once (for windows, or serving for Jupyter), spoken to in frames over its stdin/stdout


def _program():
    found = os.environ.get("WGW_WINDOW") or shutil.which("wgw-window")
    built = Path(__file__).resolve().parents[3] / "target" / "release" / "wgw-window"  # in wgw's repository
    if found or built.exists():
        return found or str(built)
    raise RuntimeError("wgw needs wgw-window: in wgw's repository, run ./build.sh, then "
                       "cargo install --path window (or set WGW_WINDOW to its path)")


class _Window:
    def __init__(self, serve):
        self.process = subprocess.Popen([_program(), "--protocol", PROTOCOL] + (["--serve"] if serve else []),
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE)
        self.figures, self.urls, self.saves, self.lock, self.next = {}, {}, {}, threading.Lock(), 0
        threading.Thread(target=self.listen, daemon=True).start()

    def write(self, header, body=b""):
        text = json.dumps(header).encode()
        with self.lock:
            self.process.stdin.write(struct.pack("<I", len(text)) + text + struct.pack("<I", len(body)) + body)
            self.process.stdin.flush()

    def read(self):
        def part():
            n = self.process.stdout.read(4)
            if len(n) < 4:
                raise EOFError
            return self.process.stdout.read(struct.unpack("<I", n)[0]).decode()
        return part().split(" ", 2), part()

    def figure(self, fig, header, body):
        with self.lock:
            self.next += 1
            id = str(self.next)
        self.figures[id] = fig
        self.urls[id] = got = threading.Event()
        self.write({**header, "id": id}, body)
        got.wait()
        return self.urls[id]

    def save(self, header, body):
        with self.lock:
            self.next += 1
            request = self.next
        self.saves[request] = done = [threading.Event(), None]
        self.write({**header, "request": request}, body)
        done[0].wait()
        if done[1] is not None:
            raise RuntimeError(f"wgw: can't save {header['path']}: {done[1]}")

    def listen(self):
        while True:  # "url <id> <url>", "closed <id>", "scene <request> <id>" with the state
            try:
                words, body = self.read()
            except EOFError:
                return
            if words[0] == "url":
                got, self.urls[words[1]] = self.urls[words[1]], words[2]
                got.set()
            elif words[0] in ("saved", "failed"):
                done = self.saves.pop(int(words[1]))
                done[1] = words[2] if words[0] == "failed" else None
                done[0].set()
            elif words[0] == "closed":
                self.figures[words[1]].closed.set()
            elif words[0] == "scene":
                threading.Thread(target=self.scene, args=(int(words[1]), words[2], body), daemon=True).start()

    def scene(self, request, id, state):
        try:
            body = self.figures[id].render(state)
        except Exception as err:
            traceback.print_exc()
            return self.write({"type": "error", "request": request, "message": f"{type(err).__name__}: {err}"})
        self.write({"type": "scene", "request": request}, body)


_windows = {}


def _window(serve):
    if serve not in _windows:
        _windows[serve] = _Window(serve)
    return _windows[serve]
