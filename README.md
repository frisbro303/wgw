# wgw: web geometry widgets

Interactive 3D figures for web pages: meshes, point clouds, curves and vector fields, drawn by a Rust core
compiled to WebAssembly and driven by **widgets**, short Python or JavaScript programs that respond to sliders
and clicks. Python runs in the browser with Pyodide, off the page's thread.

```html
<script type="module" src="wgw/wgw.js"></script>
<div class="wgw" data-src="spot.obj" data-widget="distance_widget.py" data-up="y"></div>
```

```python
# distance_widget.py: click Spot to color it by diffusion distance from the clicked vertex
import numpy as np
from wgw import Mesh, Points, Slider, Widget

def widget(spot: Mesh) -> Widget:
    spectrum = np.load("spot-spectrum.npz")
    embedding = spectrum["evecs"] * np.exp(-spectrum["evals"] * 0.3)

    def on_pick(vertex, epsilon):
        d2 = np.sum((embedding - embedding[vertex]) ** 2, axis=1)
        return {"mesh": spot.with_values(np.exp(-d2 / epsilon)), "dot": Points(spot.vertices[vertex])}

    return Widget(controls={"epsilon": Slider(0.001, 1.0, 0.02, log=True)}, on_pick=on_pick, grid=True)
```

## Build

Needs Rust with the `wasm32-unknown-unknown` target and [wasm-pack](https://github.com/wasm-bindgen/wasm-pack).

```sh
./build.sh                     # -> dist/
./build.sh ../site/theme/wgw   # ... and copy it into a site
python3 -m http.server         # examples at http://localhost:8000/examples/
```

## Docs

**https://frisbro303.github.io/wgw/**, with live figures: [figures](https://frisbro303.github.io/wgw/figures/),
[writing widgets](https://frisbro303.github.io/wgw/widgets/), the
[Python reference](https://frisbro303.github.io/wgw/reference/),
[JavaScript](https://frisbro303.github.io/wgw/javascript/) and
[internals](https://frisbro303.github.io/wgw/internals/). They're Typst pages in [`docs/`](docs), built by
[tdg](https://github.com/frisbro303/tdg): `./build.sh`, then `tdg serve docs`.
