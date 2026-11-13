"""A spot of color that follows the mouse over Spot (hover) or a finger (tap)."""
import numpy as np

CONTROLS = {"radius": (0.02, 0.5, 0.12)}


def setup(mesh):
    global VERTICES
    VERTICES = mesh["vertices"].astype(float)


def pick(vertex, radius):
    distance = np.linalg.norm(VERTICES - VERTICES[vertex], axis=1)
    return np.exp(-(distance / radius) ** 2)
