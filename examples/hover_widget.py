"""A spot of color that follows the mouse over Spot (with pick: "hover") or a finger (tap)."""
import numpy as np
from wgw import Mesh, Slider, Widget


def widget(spot: Mesh) -> Widget:
    def on_pick(vertex, radius):
        distance = np.linalg.norm(spot.vertices - spot.vertices[vertex], axis=1)
        return {"mesh": spot.with_values(np.exp(-(distance / radius) ** 2))}

    return Widget(controls={"radius": Slider(0.02, 0.5, 0.12)}, on_pick=on_pick)
