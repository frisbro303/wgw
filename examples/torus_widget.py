"""A torus built in Python and colored by its Gaussian curvature; the sliders reshape it."""
import numpy as np
from wgw import Checkbox, Mesh, Slider, Widget, rescale, torus


def widget() -> Widget:
    def on_change(R, r):
        vertices, faces, u, v = torus(R, r, n=96)
        curvature = np.cos(v) / (r * (R + r * np.cos(v)))  # K = cos v / (r (R + r cos v))
        bound = 1 / (r * (R - r))
        return {"torus": Mesh(vertices, faces, rescale(curvature, -bound, bound))}

    return Widget(
        controls={"R": Slider(0.5, 2.0, 1.0), "r": Slider(0.1, 0.9, 0.4)},
        on_change=on_change,
        grid=Checkbox(False),  # a checkbox for readers
    )
