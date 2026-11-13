"""Heat spreading over a sphere from the clicked vertex, summed from its graph Laplacian's eigenvectors."""
import numpy as np
from wgw import Mesh, Points, Slider, Widget, icosphere

STEPS = 40


def widget() -> Widget:
    sphere = Mesh(*icosphere(3))
    n = len(sphere.vertices)
    adjacency = np.zeros((n, n))
    a, b = sphere.edges.T
    adjacency[a, b] = adjacency[b, a] = 1
    evals, evecs = np.linalg.eigh(np.diag(adjacency.sum(1)) - adjacency)
    evals, evecs = evals[:150], evecs[:, :150]

    def on_pick(vertex, time):
        times = np.geomspace(time / 20, time, STEPS) * 30
        heat = evecs @ (np.exp(-np.outer(evals, times)) * evecs[vertex][:, None])
        return {
            "sphere": sphere.with_values((heat / heat.max(axis=0)).T),  # frames by vertices: animates
            "source": Points(sphere.vertices[vertex], size=8, pickable=False),
        }

    return Widget(scene={"sphere": sphere}, controls={"time": Slider(0.01, 1.0, 0.15)}, on_pick=on_pick, grid=True)
