"""Python for wgw: the .wgw file format, layers for widgets to return, and shapes to build them from.

Runs both offline (to write .wgw files for a page) and in the browser, where wgw puts this file on the
widget's import path, so a widget can `from wgw import torus, vectors`. Needs only NumPy.

The .wgw format is little-endian: b"WGW1", a uint32 header length, a JSON header
{"arrays": {name: {"dtype", "shape", "offset"}}}, then each array's raw bytes at its offset from the
end of the header, which is padded so every array starts on an 8-byte boundary.
"""
import json
import struct

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


# Layers. A widget returns a dict of these by name, to add or replace layers of the scene; None removes one, and a
# dict without "type" changes only what it names (just "values", say). Bare arrays at the top level ("values",
# "vertices", "faces") go to the layer named "mesh". Parameters (numbers, booleans, CSS colors) are listed in the
# Rust modules for each kind: src/mesh.rs, points.rs, lines.rs, vectors.rs.

def mesh(vertices, faces, values=None, **params):
    """A triangle mesh: vertices (n, 3), faces (m, 3), values (n) in [min, max] (default [0, 1]) for color."""
    return _layer("mesh", vertices=vertices, faces=faces, values=values, **params)


def points(vertices, values=None, **params):
    """A point cloud, drawn as round dots `size` CSS pixels across."""
    return _layer("points", vertices=vertices, values=values, **params)


def lines(vertices, values=None, segments=None, **params):
    """A curve through the vertices in order (closed=True joins the ends), or the segments (s, 2) between them."""
    return _layer("lines", vertices=vertices, values=values, segments=segments, **params)


def vectors(origins, vectors, values=None, **params):
    """Arrows from origins (n, 3) along vectors (n, 3), scaled by `scale`."""
    return _layer("vectors", vertices=origins, vectors=vectors, values=values, **params)


def _layer(kind, **fields):
    return {"type": kind, **{k: v for k, v in fields.items() if v is not None}}
