"""wgw for Python: widgets, layers, meshes, shapes and `.wgw` files.

Widgets import it in the browser, where wgw puts it on their path; offline scripts can import it as
`python/wgw.py`. It needs only NumPy.
"""
import json
import struct
from dataclasses import dataclass, field
from typing import Callable, Optional, Union

import numpy as np

MAGIC = b"WGW1"
DTYPES = {"float32": "f4", "float16": "f2", "float64": "f8", "uint32": "u4", "int32": "i4", "uint16": "u2", "uint8": "u1"}


def save(path, vertices, faces, **arrays):
    """Write a mesh, and any arrays its widget needs, to a `.wgw` file.

    Args:
        path: Where to write it.
        vertices: `(n, 3)` positions, stored as float32.
        faces: `(m, 3)` vertex indices, stored as uint32.
        **arrays: Extra arrays by name (a spectrum, say), which a widget gets as the mesh's attributes. They keep
            their dtype, which must be float16, float32, float64, int32, uint8, uint16 or uint32; cast to
            float16 to halve a download.
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
    """Read a `.wgw` file.

    Returns:
        Its arrays by name, `vertices` and `faces` included.
    """
    data = open(path, "rb").read()
    if data[:4] != MAGIC:
        raise ValueError(f"{path} is not an .wgw file")
    (length,) = struct.unpack("<I", data[4:8])
    header, start = json.loads(data[8:8 + length])["arrays"], 8 + length
    return {name: np.frombuffer(data, "<" + a["dtype"], int(np.prod(a["shape"])), start + a["offset"]).reshape(a["shape"])
            for name, a in header.items()}


def grid_faces(nu, nv, wrap_u=False, wrap_v=False):
    """Triangles over a grid of vertices numbered `u * nv + v`.

    Args:
        nu: The grid's size along u.
        nv: The grid's size along v.
        wrap_u: Join the last row to the first, as on a cylinder or torus.
        wrap_v: Join the last column to the first.

    Returns:
        `(m, 3)` faces, two per grid cell.
    """
    cu, cv = nu if wrap_u else nu - 1, nv if wrap_v else nv - 1
    u, v = np.meshgrid(np.arange(cu), np.arange(cv), indexing="ij")
    a, b = u * nv + v, ((u + 1) % nu) * nv + v
    c, d = u * nv + (v + 1) % nv, ((u + 1) % nu) * nv + (v + 1) % nv
    return np.concatenate([np.stack([a, b, d], -1), np.stack([a, d, c], -1)]).reshape(-1, 3)


def surface(f, u=(0, 1), v=(0, 1), n=64, wrap_u=False, wrap_v=False):
    """A parametric surface, sampled on a grid.

    Args:
        f: `f(u, v) -> (x, y, z)`, taking and returning arrays.
        u: The range of the first parameter.
        v: The range of the second.
        n: Samples along each parameter.
        wrap_u: Whether the surface closes up in u; the range's end is then left out, as it would repeat the start.
        wrap_v: Whether it closes up in v.

    Returns:
        `(vertices, faces, u, v)`, with each vertex's parameters.
    """
    us = np.linspace(*u, n, endpoint=not wrap_u)
    vs = np.linspace(*v, n, endpoint=not wrap_v)
    U, V = np.meshgrid(us, vs, indexing="ij")
    vertices = np.stack(np.broadcast_arrays(*f(U, V)), -1).reshape(-1, 3)
    return vertices, grid_faces(n, n, wrap_u, wrap_v), U.ravel(), V.ravel()


def torus(R=1.0, r=0.4, n=64):
    """A torus about the z axis.

    Args:
        R: The distance from the axis to the tube's center.
        r: The tube's radius.
        n: Samples around the axis and around the tube.

    Returns:
        `(vertices, faces, u, v)`, with `u` the angle around the axis and `v` around the tube.
    """
    return surface(lambda u, v: ((R + r * np.cos(v)) * np.cos(u), (R + r * np.cos(v)) * np.sin(u), r * np.sin(v)),
                   (0, 2 * np.pi), (0, 2 * np.pi), n, wrap_u=True, wrap_v=True)


def icosphere(subdivisions=3):
    """The unit sphere, as an icosahedron with each triangle split in four, `subdivisions` times.

    Returns:
        `(vertices, faces)`: 642 vertices at 3 subdivisions, 2562 at 4.
    """
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
    """Values mapped linearly onto [0, 1], the range colors span by default.

    Args:
        values: Any array.
        lo: The value that maps to 0 (by default, the smallest); values below are clipped.
        hi: The value that maps to 1 (by default, the largest); values above are clipped.
    """
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
    """What every layer has: arrays (per vertex, or per frame and vertex to animate) and parameters.

    Every layer takes these parameters as keyword arguments:

    | parameter | default | |
    |---|---|---|
    | `color` | | one CSS color for the whole layer, which may be the page's: `"var(--link)"` follows its theme |
    | `colors` | the figure's | a list of CSS colors for `values` to run through, `var(...)` too |
    | `min`, `max` | `0`, `1` | the range of `values` the colors span |
    | `visible` | `True` | |
    | `pickable` | meshes and points | whether clicks pick it |
    | `pane` | `0` | which pane it's drawn in: layers with `pane=1` get a second view, beside the first, sized on its own |

    Without `values` or `color`, meshes take the first of the figure's colors, and other layers its accent.
    """

    kind = None
    fields = ()  # the array fields, in constructor order

    def __init__(self, **params):
        self.params = params  # numbers, booleans and CSS colors; see the README for each kind's

    def replace(self, **changes):
        """A copy with some arrays or parameters changed.

        The rest are shared, not copied, and shared arrays aren't sent to the page again.
        """
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
        """A copy colored by new values, `(n)` or `(frames, n)`, sharing everything else."""
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
    """A triangle mesh.

    A figure's mesh file reaches its widget as a `Mesh`, with the file's extra arrays as attributes: `spot.evecs`,
    or `spot["evecs"]`.

    Args:
        vertices: `(n, 3)` positions, or `(frames, n, 3)` to animate.
        faces: `(m, 3)` vertex indices.
        values: `(n)` or `(frames, n)`, for color.
        data: Extra arrays by name.
        **params: `flat` (flat shading), `edges` (this mesh's triangle grid, overriding the widget's `grid`),
            `edge_color`, and those of every [layer][wgw.Layer].
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
        """A mesh from a file.

        Args:
            path: A `.wgw` file (whose extra arrays become the mesh's), or a Wavefront `.obj` (polygons are split
                into triangles; texture coordinates and normals are ignored). In a widget, relative to the widget.
        """
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
        """Write the mesh, its extra arrays and these ones to a `.wgw` file."""
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
        """Each face's area `(m)`."""
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
        """The total area."""
        return float(self.face_areas.sum())

    def _cross(self):
        v = self.vertices[self.faces]
        return np.cross(v[:, 1] - v[:, 0], v[:, 2] - v[:, 0])


class Points(Layer):
    """Points, drawn as round dots.

    Args:
        vertices: `(n, 3)`, `(3)` for one point, or `(frames, n, 3)`.
        values: `(n)` or `(frames, n)`, for color.
        **params: `size` (CSS pixels across, 6), `lift` (how far toward the camera, as a fraction of the scene,
            so points on a surface sit on top of it; 0.02), and those of every [layer][wgw.Layer].
    """

    kind = "points"
    fields = ("vertices", "values")

    def __init__(self, vertices, values=None, **params):
        super().__init__(**params)
        self.vertices = np.atleast_2d(np.asarray(vertices, dtype=float))
        self.values = None if values is None else np.asarray(values, dtype=float)


class Lines(Layer):
    """A curve, or any set of segments.

    Args:
        vertices: `(n, 3)` or `(frames, n, 3)`.
        values: `(n)` or `(frames, n)`, for color.
        segments: `(s, 2)` pairs of vertices to join; by default, each vertex to the next.
        **params: `width` (CSS pixels, 2), `closed` (join the last vertex to the first), and those of every
            [layer][wgw.Layer].
    """

    kind = "lines"
    fields = ("vertices", "segments", "values")

    def __init__(self, vertices, values=None, segments=None, **params):
        super().__init__(**params)
        self.vertices = np.asarray(vertices, dtype=float)
        self.segments = None if segments is None else np.asarray(segments, dtype=np.int64)
        self.values = None if values is None else np.asarray(values, dtype=float)


class Vectors(Layer):
    """Arrows: a vector field.

    Args:
        origins: `(n, 3)` or `(frames, n, 3)`, where the arrows start.
        vectors: `(n, 3)` or `(frames, n, 3)`.
        values: `(n)` or `(frames, n)`, for color.
        **params: `scale` (multiplies the vectors, 1), `width` (the shaft's radius, as a fraction of the scene,
            0.006; the head is in proportion), and those of every [layer][wgw.Layer].
    """

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
    """A slider.

    Attributes:
        min: Its lowest value; with `log`, it must be positive.
        max: Its highest value.
        value: Where it starts.
        step: Its step; by default, a 200th of the range.
        log: Move evenly in powers of ten (in steps of 0.01 of a decade), for a parameter spanning orders of
            magnitude.
    """

    min: float
    max: float
    value: float
    step: Optional[float] = None
    log: bool = False

    def _spec(self):
        return [self.min, self.max, self.value] + ([self.step] if self.step else []) + (["log"] if self.log else [])


@dataclass(frozen=True)
class Checkbox:
    """A checkbox, starting checked or not."""

    value: bool = False

    def _spec(self):
        return bool(self.value)


Scene = dict  # layers by name: {"mesh": Mesh(...), "dot": Points(...), "gone": None}


@dataclass(frozen=True)
class Widget:
    """A widget: what a figure starts as, its controls, and how it responds to them.

    A widget file's `widget(mesh)` (or `widget()`, for a figure without a mesh) returns one. Every field is
    optional: a widget with no controls, no `on_pick` or no grid just leaves them out, and `Widget(scene=...)` alone
    is a figure built in Python that doesn't respond to anything.

    `on_change` and `on_pick` return a **scene**: layers by name. Each replaces the layer of that name exactly as
    given; `None` removes one; layers not named are kept. A single layer means `{"mesh": layer}`, and `None`
    changes nothing.

    Attributes:
        scene: Layers to start with, by name. The figure's mesh file, if any, is already shown as `"mesh"`.
        controls: [Sliders][wgw.Slider] and [checkboxes][wgw.Checkbox] by name. Their values are passed to
            `on_change` and `on_pick` as keyword arguments.
        on_change: `(**controls) -> scene`, run at the start and whenever a control changes.
        on_pick: `(vertex, **controls) -> scene`, run when a vertex is clicked (or hovered, with
            `data-pick="hover"`), and again with the same vertex whenever a control changes. Give it a `layer`
            parameter to be told which layer the vertex is in.
        grid: Draw meshes' triangle grids: `True`, `False`, or a `Checkbox` for readers to toggle them.
        picked: A vertex of the figure's mesh to start with picked: `on_pick` runs for it at the start, so the
            figure opens showing something, and controls act on it until a reader picks another.
        fps: The rate animation frames play at (arrays with a leading frames axis animate).
        loop: Loop animations, rather than playing them once.
    """

    scene: Scene = field(default_factory=dict)
    controls: dict = field(default_factory=dict)
    on_change: Optional[Callable[..., Scene]] = None
    on_pick: Optional[Callable[..., Scene]] = None
    grid: Union[bool, Checkbox] = False
    picked: Optional[int] = None
    fps: float = 30
    loop: bool = False
