"""Heat spreading over a sphere from the clicked vertex, summed from the Laplacian's eigenvectors."""
import numpy as np
from wgw import icosphere

CONTROLS = {"time": (0.01, 1.0, 0.15)}
STEPS = 40


def setup(mesh):
    """Build the sphere and diagonalize its graph Laplacian once; returning the geometry shows it."""
    global EVALS, EVECS
    vertices, faces = icosphere(3)
    n = len(vertices)
    adjacency = np.zeros((n, n))
    for a, b in ((0, 1), (1, 2), (2, 0)):
        adjacency[faces[:, a], faces[:, b]] = adjacency[faces[:, b], faces[:, a]] = 1
    laplacian = np.diag(adjacency.sum(1)) - adjacency
    EVALS, EVECS = np.linalg.eigh(laplacian)
    EVALS, EVECS = EVALS[:150], EVECS[:, :150]
    return {"vertices": vertices, "faces": faces}


def pick(vertex, time):
    times = np.geomspace(time / 20, time, STEPS) * 30
    heat = EVECS @ (np.exp(-np.outer(EVALS, times)) * EVECS[vertex][:, None])
    return (heat / heat.max(axis=0)).T  # frames by vertices, each scaled to [0, 1]
