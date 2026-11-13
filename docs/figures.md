# Figures

Load `wgw.js` as a module (everything else in `dist/` must sit beside it), and any `<div class="wgw">` becomes a
figure:

```html
<script type="module" src="wgw/wgw.js"></script>
<div class="wgw" data-src="meshes/spot.obj" data-widget="distance_widget.py" data-up="y" data-view="90,10"></div>
```

A figure needs a browser with WebGL2. It loads when it nears the screen, and draws only while on it.

A figure doesn't need a widget. This is just `data-src`, `data-up` and `data-view`:

<div class="wgw" data-src="examples/spot.obj" data-up="y" data-view="-110,15"></div>

## Attributes

| attribute     | default  |                                                                         |
|---------------|----------|-------------------------------------------------------------------------|
| `data-src`    |          | a mesh file, `.obj` or `.wgw`; shown as the layer `mesh`                |
| `data-widget` |          | a widget, `.py` or `.js` ([writing widgets](widgets.md))                |
| `data-up`     | `z`      | the axis that points up: `x`, `y` or `z`                                |
| `data-view`   | `-60,20` | the starting camera: turn about the up axis, then tilt, in degrees      |
| `data-pick`   | `click`  | when the widget's `on_pick` runs: `click`, `hover` or `none`            |
| `data-flat`   | off      | flat shading for the `mesh` layer                                       |
| `data-label`  |          | the figure's accessible description                                     |

## Interaction

Drag to turn (it glides on), pinch or ctrl/cmd+scroll to zoom, double-click to reset. Plain scrolling scrolls the
page, so figures never trap it.

## Mesh files

The page fetches `data-src` and the core parses it, so the mesh is drawn at once, before any widget has
started; then the widget gets it as `widget(mesh)`.

- **`.obj`**: Wavefront OBJ. Triangles and polygons (split into fans), 1-based or negative indices. Texture
  coordinates and normals are ignored, so the vertices are exactly the file's `v` lines, in order.
- **`.wgw`**: a mesh plus any arrays its widget needs (a spectrum, say), which the widget gets as attributes
  (`mesh.evecs`). Write one with `Mesh(vertices, faces).save("spot.wgw", evecs=...)`; see the
  [Python reference](reference.md#files).

## Styling

CSS custom properties on the figure or any ancestor:

| property        | default                     |                                                         |
|-----------------|-----------------------------|---------------------------------------------------------|
| `--wgw-colors`  | `#d8d6cf, #b8402f`          | the colors `values` run through, from low to high       |
| `--wgw-accent`  | the last of `--wgw-colors`  | points, curves and arrows without values                |
| `--wgw-edge`    | `rgba(0, 0, 0, 0.3)`        | the triangle grid                                       |
| `--wgw-control` | the text color              | sliders and checkboxes                                  |
| `--wgw-height`  | `420px`                     | the canvas's height                                     |

Any CSS color works, `var(...)` included, so figures can follow a site's theme:

```css
.wgw { --wgw-colors: var(--surface), var(--red); --wgw-accent: var(--text); --wgw-height: 420px; }
```

When the theme switches (a `data-theme` or `class` on `<html>`, or the system's setting), every figure recolors.
The controls row (`.wgw-controls`) and the status line (`.wgw-status`) can be restyled too; wgw's own styles
come first in the page, so a site's win.
