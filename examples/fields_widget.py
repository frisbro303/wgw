"""Every layer kind at once: a sphere colored by a function, its gradient as arrows, a level curve, and the clicked point."""
import numpy as np
from wgw import Checkbox, Lines, Mesh, Points, Slider, Vectors, Widget, icosphere, rescale


def field(p, k):
    """f(x, y, z) = sin(kx) cos(ky) + z, and its gradient in space."""
    x, y, z = p.T
    f = np.sin(k * x) * np.cos(k * y) + z
    grad = np.stack([k * np.cos(k * x) * np.cos(k * y), -k * np.sin(k * x) * np.sin(k * y), np.ones_like(z)], axis=1)
    return f, grad


def widget() -> Widget:
    sphere = Mesh(*icosphere(3))
    V = sphere.vertices
    t = np.linspace(0, 2 * np.pi, 200, endpoint=False)
    latitude = Lines(1.003 * np.stack([np.cos(t) * np.sqrt(0.75), np.sin(t) * np.sqrt(0.75), np.full_like(t, 0.5)], 1),
                     closed=True, width=2.5, color="#100f0f")

    def on_change(frequency, arrows):
        f, grad = field(V, frequency)
        tangent = grad - np.sum(grad * V, axis=1, keepdims=True) * V  # the gradient along the sphere
        return {
            "sphere": sphere.with_values(rescale(f)),
            "gradient": Vectors(V, tangent, scale=0.04, width=0.008, color="#205ea6") if arrows else None,
            "latitude": latitude,
        }

    def on_pick(vertex, frequency, arrows):
        return {"picked": Points(V[vertex] * 1.01, size=14, color="#24837b")}

    return Widget(
        controls={"frequency": Slider(1, 6, 3), "arrows": Checkbox(True)},
        on_change=on_change,
        on_pick=on_pick,
    )
