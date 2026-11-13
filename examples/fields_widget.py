"""Every layer kind at once: a sphere colored by a function, its gradient as arrows, a level curve, and the clicked point."""
import numpy as np
from wgw import icosphere, lines, mesh, points, rescale, vectors

CONTROLS = {"frequency": (1, 6, 3), "arrows": True}
VERTICES, FACES = icosphere(3)


def field(p, k):
    """f(x, y, z) = sin(kx) cos(ky) + z, and its gradient in space."""
    x, y, z = p.T
    f = np.sin(k * x) * np.cos(k * y) + z
    grad = np.stack([k * np.cos(k * x) * np.cos(k * y), -k * np.sin(k * x) * np.sin(k * y), np.ones_like(z)], axis=1)
    return f, grad


def update(frequency, arrows):
    f, grad = field(VERTICES, frequency)
    tangent = grad - np.sum(grad * VERTICES, axis=1, keepdims=True) * VERTICES  # the gradient on the sphere
    t = np.linspace(0, 2 * np.pi, 200, endpoint=False)
    circle = 1.003 * np.stack([np.cos(t) * np.sqrt(0.75), np.sin(t) * np.sqrt(0.75), np.full_like(t, 0.5)], axis=1)
    return {
        "sphere": mesh(VERTICES, FACES, rescale(f)),
        "gradient": vectors(VERTICES, tangent, scale=0.04, width=0.008, color="#205ea6") if arrows else None,
        "latitude": lines(circle, closed=True, width=2.5, color="#100f0f"),
    }


def pick(vertex, frequency, arrows):
    return {"picked": points(VERTICES[[vertex]] * 1.01, size=14, color="#24837b")}
