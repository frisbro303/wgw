"""Check a head's patch against expected.json: python3 check.py <patch file>. Only the standard library."""
import json, struct, sys
from pathlib import Path

expected = json.loads((Path(__file__).parent / "expected.json").read_text())["patch"]
data = Path(sys.argv[1]).read_bytes()
assert data[:4] == b"WGW1", "not a patch"
n = struct.unpack("<I", data[4:8])[0]
assert (8 + n) % 4 == 0, "the arrays don't start 4-byte aligned"
header, body = json.loads(data[8:8 + n]), data[8 + n:]
got = {}
for name, layer in header["layers"]:
    arrays = {}
    for key, a in layer["arrays"].items():
        count = 1
        for s in a["shape"]:
            count *= s
        fmt = "<%d%s" % (count, "f" if a["dtype"] == "f4" else "I")
        arrays[key] = {"dtype": a["dtype"], "shape": a["shape"], "data": list(struct.unpack_from(fmt, body, a["offset"]))}
    got[name] = {"type": layer["type"], "params": layer["params"], "arrays": arrays}
if got != expected:
    sys.exit(f"{sys.argv[1]}: the patch differs\n got      {got}\n expected {expected}")
print(f"{sys.argv[1]}: ok")
