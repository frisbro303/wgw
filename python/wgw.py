"""Python for wgw: widgets (Widget, Slider, Checkbox), layers (Mesh, Points, Lines, Vectors), the .wgw file
format, and shapes.

A widget is a function from the figure's mesh (or nothing) to a Widget:

    def widget(spot: Mesh) -> Widget:
        embedding = ...                                  # work done once, kept by the closures below
        def on_pick(vertex, epsilon):                    # controls arrive as keyword arguments
            return {"mesh": spot.with_values(...), "dot": Points(spot.vertices[vertex])}
        return Widget(controls={"epsilon": Slider(0.001, 1, 0.02, log=True)}, on_pick=on_pick, grid=True)


Runs both offline (to write .wgw files for a page) and in the browser, where wgw puts this file on the
widget's import path, so a widget can `from wgw import Mesh, Points`. Needs only NumPy.

The .wgw format is little-endian: b"WGW1", a uint32 header length, a JSON header
{"arrays": {name: {"dtype", "shape", "offset"}}}, then each array's raw bytes at its offset from the
end of the header, which is padded so every array starts on an 8-byte boundary.
"""
import json
import struct
from dataclasses import dataclass, field
from typing import Callable, Optional, Union

import numpy as np

MAGIC = b"WGW1"
DTYPES = {"float32": "f4", "float16": "f2", "float64": "f8", "uint32": "u4", "int32": "i4", "uint16": "u2", "uint8": "u1"}


def save(path, vertices, faces, **arrays):
    """Write a mesh, and any extra arrays its widget needs (say a spectrum), to an .wgw file.

    vertices (n, 3) are stored as float32 and faces (m, 3) as uint32; extra arrays keep their dtype
    (cast to float32 or float16 first to keep the file small), as long as it's one of DTYPES.
    """
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
    with open(path, "wb") as f:
        f.write(MAGIC + struct.pack("<I", len(text)) + text + b"".join(blobs))


def load(path):
    """Read an .wgw file back as a dict of arrays."""
    data = open(path, "rb").read()
    if data[:4] != MAGIC:
        raise ValueError(f"{path} is not an .wgw file")
    (length,) = struct.unpack("<I", data[4:8])
    header, start = json.loads(data[8:8 + length])["arrays"], 8 + length
    return {name: np.frombuffer(data, "<" + a["dtype"], int(np.prod(a["shape"])), start + a["offset"]).reshape(a["shape"])
            for name, a in header.items()}


def grid_faces(nu, nv, wrap_u=False, wrap_v=False):
    """Triangles over an nu by nv grid of vertices numbered u * nv + v, optionally closed up in u or v."""
    cu, cv = nu if wrap_u else nu - 1, nv if wrap_v else nv - 1
    u, v = np.meshgrid(np.arange(cu), np.arange(cv), indexing="ij")
    a, b = u * nv + v, ((u + 1) % nu) * nv + v
    c, d = u * nv + (v + 1) % nv, ((u + 1) % nu) * nv + (v + 1) % nv
    return np.concatenate([np.stack([a, b, d], -1), np.stack([a, d, c], -1)]).reshape(-1, 3)


def surface(f, u=(0, 1), v=(0, 1), n=64, wrap_u=False, wrap_v=False):
    """A parametric surface f(u, v) -> (x, y, z), sampled on an n by n grid; returns (vertices, faces, u, v).

    Wrapped directions leave out the endpoint, which would repeat the start.
    """
    us = np.linspace(*u, n, endpoint=not wrap_u)
    vs = np.linspace(*v, n, endpoint=not wrap_v)
    U, V = np.meshgrid(us, vs, indexing="ij")
    vertices = np.stack(np.broadcast_arrays(*f(U, V)), -1).reshape(-1, 3)
    return vertices, grid_faces(n, n, wrap_u, wrap_v), U.ravel(), V.ravel()


def torus(R=1.0, r=0.4, n=64):
    """A torus about the z axis; returns (vertices, faces, u, v) with u around the axis and v around the tube."""
    return surface(lambda u, v: ((R + r * np.cos(v)) * np.cos(u), (R + r * np.cos(v)) * np.sin(u), r * np.sin(v)),
                   (0, 2 * np.pi), (0, 2 * np.pi), n, wrap_u=True, wrap_v=True)


def icosphere(subdivisions=3):
    """The unit sphere, from an icosahedron with each triangle split in four `subdivisions` times."""
    t = (1 + 5 ** 0.5) / 2
    vertices = [(-1, t, 0), (1, t, 0), (-1, -t, 0), (1, -t, 0), (0, -1, t), (0, 1, t),
                (0, -1, -t), (0, 1, -t), (t, 0, -1), (t, 0, 1), (-t, 0, -1), (-t, 0, 1)]
    faces = [(0, 11, 5), (0, 5, 1), (0, 1, 7), (0, 7, 10), (0, 10, 11), (1, 5, 9), (5, 11, 4), (11, 10, 2), (10, 7, 6),
             (7, 1, 8), (3, 9, 4), (3, 4, 2), (3, 2, 6), (3, 6, 8), (3, 8, 9), (4, 9, 5), (2, 4, 11), (6, 2, 10),
             (8, 6, 7), (9, 8, 1)]
    vertices, faces = np.array(vertices, float), np.array(faces)
    for _ in range(subdivisions):
        edges = np.sort(faces[:, [0, 1, 1, 2, 2, 0]].reshape(-1, 2), axis=1)
        unique, inverse = np.unique(edges, axis=0, return_inverse=True)
        mid = len(vertices) + inverse.reshape(-1, 3)  # the new vertex on edges (a, b), (b, c), (c, a)
        vertices = np.concatenate([vertices, vertices[unique].mean(axis=1)])
        a, b, c = faces.T
        ab, bc, ca = mid.T
        faces = np.concatenate([np.stack(f, -1) for f in ((a, ab, ca), (b, bc, ab), (c, ca, bc), (ab, bc, ca))])
    return vertices / np.linalg.norm(vertices, axis=1, keepdims=True), faces


def rescale(values, lo=None, hi=None):
    """Values mapped linearly onto [0, 1], the range the colormap spans, from [lo, hi] (their min and max by default)."""
    values = np.asarray(values, float)
    lo = values.min() if lo is None else lo
    hi = values.max() if hi is None else hi
    return np.clip((values - lo) / (hi - lo or 1), 0, 1)


# ---- Layers: what a scene is made of, and what widgets return.
#
# A widget returns a dict of layers by name, to add or replace them; None removes one. A layer's arrays are sent to
# the page only when they're new objects: one returned again (say the vertices of SPOT.with_values(...)) is not sent
# again. So treat arrays you've returned as fixed: make a new array rather than changing one in place.


class Layer:
    """A layer of the scene: named arrays (per vertex, or per frame and vertex to animate) and parameters."""

    kind = None
    fields = ()  # the array fields, in constructor order

    def __init__(self, **params):
        self.params = params  # numbers, booleans and CSS colors; see the README for each kind's

    def replace(self, **changes):
        """A copy with some arrays or parameters changed; the rest are shared, not copied."""
        new = object.__new__(type(self))
        new.__dict__.update(self.__dict__)
        new.params = dict(self.params)
        for key, value in changes.items():
            if key in self.fields:
                setattr(new, key, value)
            else:
                new.params[key] = value
        return new

    def with_values(self, values, **params):
        """A copy colored by new per-vertex values, sharing everything else."""
        return self.replace(values=values, **params)

    def _wgw_layer(self):
        """The layer as wgw sends it: its kind, arrays and parameters."""
        out = {"type": self.kind, **self.params}
        for name in self.fields:
            value = getattr(self, name)
            if value is not None:
                out["vertices" if name == "origins" else name] = value
        return out

    def __repr__(self):
        arrays = ", ".join(f"{f}={getattr(self, f).shape}" for f in self.fields if getattr(self, f) is not None)
        return f"{type(self).__name__}({arrays}{''.join(f', {k}={v!r}' for k, v in self.params.items())})"


class Mesh(Layer):
    """A triangle mesh: vertices (n, 3), faces (m, 3), and values (n) for color, in [min, max] (default [0, 1]).

    Extra arrays (from a .wgw file, or given as `data`) are attributes: mesh.evecs, or mesh["evecs"]. Parameters:
    edges, flat, color, colors, min, max, edge_color, pickable, visible.
    """

    kind = "mesh"
    fields = ("vertices", "faces", "values")

    def __init__(self, vertices, faces, values=None, data=None, **params):
        super().__init__(**params)
        self.vertices = np.asarray(vertices, dtype=float)
        self.faces = np.asarray(faces, dtype=np.int64)
        self.values = None if values is None else np.asarray(values, dtype=float)
        self.data = dict(data or {})

    def __getattr__(self, name):
        data = self.__dict__.get("data", {})
        if name in data:
            return data[name]
        raise AttributeError(f"Mesh has no {name!r}; its extra arrays are {sorted(data) or 'none'}")

    def __getitem__(self, name):
        return getattr(self, name)

    @classmethod
    def load(cls, path):
        """A mesh from a .wgw file (with its extra arrays) or a Wavefront .obj (polygons split into triangles)."""
        path = str(path)
        if path.endswith(".obj"):
            vertices, faces = [], []
            for line in open(path):
                parts = line.split()
                if parts[:1] == ["v"]:
                    vertices.append([float(x) for x in parts[1:4]])
                elif parts[:1] == ["f"]:
                    corners = [int(p.split("/")[0]) for p in parts[1:]]
                    corners = [c - 1 if c > 0 else len(vertices) + c for c in corners]  # 1-based, or negative
                    faces += [[corners[0], corners[k], corners[k + 1]] for k in range(1, len(corners) - 1)]
            return cls(vertices, faces)
        arrays = dict(load(path))
        return cls(arrays.pop("vertices"), arrays.pop("faces"), data=arrays)

    def save(self, path, **arrays):
        """Write the mesh, its extra arrays and these ones to a .wgw file."""
        save(path, self.vertices, self.faces, **{**self.data, **arrays})

    @property
    def edges(self):
        """Each edge once, as (e, 2) vertex pairs, the smaller index first."""
        pairs = np.sort(self.faces[:, [0, 1, 1, 2, 2, 0]].reshape(-1, 2), axis=1)
        return np.unique(pairs, axis=0)

    @property
    def face_normals(self):
        """Unit normals (m, 3), by the faces' winding."""
        n = self._cross()
        return n / np.linalg.norm(n, axis=1, keepdims=True)

    @property
    def face_areas(self):
        return 0.5 * np.linalg.norm(self._cross(), axis=1)

    @property
    def normals(self):
        """Area-weighted unit vertex normals (n, 3)."""
        n = np.zeros_like(self.vertices)
        cross = self._cross()
        for k in range(3):
            np.add.at(n, self.faces[:, k], cross)
        return n / np.maximum(np.linalg.norm(n, axis=1, keepdims=True), 1e-300)

    @property
    def areas(self):
        """Each vertex's share of the area (n): a third of each face around it (the lumped mass matrix)."""
        a = np.zeros(len(self.vertices))
        for k in range(3):
            np.add.at(a, self.faces[:, k], self.face_areas / 3)
        return a

    @property
    def area(self):
        return float(self.face_areas.sum())

    def _cross(self):
        v = self.vertices[self.faces]
        return np.cross(v[:, 1] - v[:, 0], v[:, 2] - v[:, 0])


class Points(Layer):
    """A point cloud, vertices (n, 3), drawn as round dots `size` CSS pixels across; values (n) for color."""

    kind = "points"
    fields = ("vertices", "values")

    def __init__(self, vertices, values=None, **params):
        super().__init__(**params)
        self.vertices = np.atleast_2d(np.asarray(vertices, dtype=float))
        self.values = None if values is None else np.asarray(values, dtype=float)


class Lines(Layer):
    """A curve through vertices (n, 3) in order (closed=True joins the ends), or the segments (s, 2) between them;
    `width` CSS pixels wide; values (n) for color."""

    kind = "lines"
    fields = ("vertices", "segments", "values")

    def __init__(self, vertices, values=None, segments=None, **params):
        super().__init__(**params)
        self.vertices = np.asarray(vertices, dtype=float)
        self.segments = None if segments is None else np.asarray(segments, dtype=np.int64)
        self.values = None if values is None else np.asarray(values, dtype=float)


class Vectors(Layer):
    """Arrows from origins (n, 3) along vectors (n, 3), scaled by `scale`; values (n) for color."""

    kind = "vectors"
    fields = ("origins", "vectors", "values")

    def __init__(self, origins, vectors, values=None, **params):
        super().__init__(**params)
        self.origins = np.asarray(origins, dtype=float)
        self.vectors = np.asarray(vectors, dtype=float)
        self.values = None if values is None else np.asarray(values, dtype=float)


# ---- Widgets


@dataclass(frozen=True)
class Slider:
    """A slider from min to max starting at value; log=True moves it evenly in powers of ten."""

    min: float
    max: float
    value: float
    step: Optional[float] = None
    log: bool = False

    def _spec(self):
        return [self.min, self.max, self.value] + ([self.step] if self.step else []) + (["log"] if self.log else [])


@dataclass(frozen=True)
class Checkbox:
    value: bool = False

    def _spec(self):
        return bool(self.value)


Scene = dict  # layers by name: {"mesh": Mesh(...), "dot": Points(...), "gone": None}


@dataclass(frozen=True)
class Widget:
    """What a widget is: its starting scene, its controls, and functions from controls (and a picked vertex) to
    changes of the scene.

    scene      layers to start with, by name; the figure's mesh file, if any, is already shown as "mesh"
    controls   Sliders and Checkboxes by name; their values are passed to on_change and on_pick as keyword arguments
    on_change  (**controls) -> Scene, run at the start and whenever a control changes
    on_pick    (vertex, **controls) -> Scene, run when a vertex is clicked (or hovered, with pick: "hover"); add a
               `layer` argument to be told which layer it was in
    grid       draw meshes' triangle grids: True, False, or a Checkbox for readers to toggle it
    fps, loop  how a scene's frames play (arrays with a leading frames axis animate)

    A returned Scene replaces the layers it names, each exactly as given; layers it doesn't name are kept, and
    None removes one. Returning a single layer means {"mesh": layer}.
    """

    scene: Scene = field(default_factory=dict)
    controls: dict = field(default_factory=dict)
    on_change: Optional[Callable[..., Scene]] = None
    on_pick: Optional[Callable[..., Scene]] = None
    grid: Union[bool, Checkbox] = False
    fps: float = 30
    loop: bool = False
