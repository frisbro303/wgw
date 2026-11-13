# wgw: web geometry widgets

Interactive 3D figures for web pages: meshes, point clouds, curves and vector fields, drawn by a Rust core compiled
to WebAssembly and driven by **widgets**, short Python or JavaScript programs that respond to sliders and clicks.
Python runs in the browser with Pyodide, off the page's thread.

<div class="wgw" data-widget="examples/fields_widget.py" data-view="-50,25" data-label="A sphere colored by a function, with its gradient as arrows and a level curve"></div>

Drag to turn it, click to place a point. It's this widget:

```python
--8<-- "examples/fields_widget.py"
```

and this HTML:

```html
<script type="module" src="wgw/wgw.js"></script>
<div class="wgw" data-widget="fields_widget.py"></div>
```

## Getting it

wgw isn't on a package registry yet: build it from [the repository](https://github.com/frisbro303/wgw), which needs
Rust with the `wasm32-unknown-unknown` target and [wasm-pack](https://github.com/wasm-bindgen/wasm-pack).

```sh
./build.sh                     # -> dist/
./build.sh ../site/theme/wgw   # ... and copy it into a site
```

Then serve `dist/` with your pages. Figures need WebGL2; Python widgets download Pyodide and NumPy (a few MB) on
a first visit.

## Next

- [Figures](figures.md): putting a figure on a page, its options and styling, mesh files
- [Writing widgets](widgets.md): a guide, by example
- [Python reference](reference.md): `Widget`, controls, layers, meshes, shapes, files
- [JavaScript](javascript.md): widgets in JavaScript, the page API, other languages
- [Internals](internals.md): how it works, extending it, building the docs
