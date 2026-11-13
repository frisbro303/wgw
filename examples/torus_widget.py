"""A torus built in Python and colored by its Gaussian curvature; the sliders reshape it."""
import numpy as np
from wgw import rescale, torus

CONTROLS = {"R": (0.5, 2.0, 1.0), "r": (0.1, 0.9, 0.4)}


def update(R, r):
    vertices, faces, u, v = torus(R, r, n=96)
    curvature = np.cos(v) / (r * (R + r * np.cos(v)))  # K = cos v / (r (R + r cos v))
    bound = 1 / (r * (R - r))
    return {"vertices": vertices, "faces": faces, "values": rescale(curvature, -bound, bound)}
