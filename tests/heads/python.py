# The Python head against expected.json: python3 python.py <out dir>
import json, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "python"))
import wgw

out, fixture = Path(sys.argv[1]), json.loads((Path(__file__).parent / "expected.json").read_text())
scene = {"m": wgw.Mesh([[0, 0, 0], [1, 0, 0], [0, 1, 0]], [[0, 1, 2]], values=[0, 0.5, 1], color="red"),
         "p": wgw.Points([[0, 0, 1]], size=4)}
(out / "python.patch").write_bytes(wgw.encode(scene))

seen = []
fig = wgw.Figure(lambda s: (seen.append(s), scene)[1], {"t": wgw.Slider(0, 5, 1), "my box": False})
fig.render(fixture["state"])
assert dict(seen[0]) == fixture["values"], seen[0]
assert seen[0].pick == fixture["pick"], seen[0].pick

fig.save(out / "python.png", width=100, height=80, scale=1)
assert (out / "python.png").read_bytes()[:4] == b"\x89PNG"
