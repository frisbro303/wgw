"""Write spot.wgw from spot.obj (Keenan Crane's Spot, CC0). Run: uv run --with numpy examples/make_spot.py"""
import sys
from pathlib import Path

import numpy as np

HERE = Path(__file__).parent
sys.path.insert(0, str(HERE.parent / "python"))
import wgw

positions, faces = [], []
for line in (HERE / "spot.obj").read_text().splitlines():
    parts = line.split()
    if parts[:1] == ["v"]:
        positions.append([float(x) for x in parts[1:4]])
    elif parts[:1] == ["f"]:
        corners = [int(p.split("/")[0]) - 1 for p in parts[1:]]
        faces += [[corners[0], corners[k], corners[k + 1]] for k in range(1, len(corners) - 1)]

wgw.save(HERE / "spot.wgw", np.array(positions), np.array(faces))
