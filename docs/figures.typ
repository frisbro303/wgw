#import "@local/tdg:0.1.0": *
#set document(title: "Figures", description: "Putting a wgw figure on a page: its attributes, styling and mesh files.")

Load `wgw.js` as a module (everything else in `dist/` must sit beside it), and any `<div class="wgw">` becomes a
figure:

```html
<script type="module" src="wgw/wgw.js"></script>
<div class="wgw" data-src="meshes/spot.obj" data-widget="distance_widget.py" data-up="y" data-view="90,10"></div>
```

A figure needs a browser with WebGL2. It loads when it nears the screen, and draws only while on it. It doesn't
need a widget: this is just `data-src`, `data-up` and `data-view`.

#widget(src: "/examples/spot.obj", up: "y", view: (-110, 15), alt: "Keenan Crane's Spot, a cow")

= In Typst

In a #link("https://github.com/frisbro303/tdg")[tdg] site, `widget(...)` makes the same figure, numbered and
captioned like any other, and shows a still image in PDFs:

```typ
#widget(widget: "distance_widget.py", src: "meshes/spot.obj", up: "y", view: (90, 10),
        still: image("distance_widget.png"), caption: [Diffusion distance on Spot.])
```

= Attributes

#table(
  columns: 3,
  [*attribute*], [*default*], [],
  [`data-src`], [], [a mesh file, `.obj` or `.wgw`; shown as the layer `mesh`],
  [`data-widget`], [], [a widget, `.py` or `.js` (#link("/widgets/")[writing widgets])],
  [`data-up`], [`z`], [the axis that points up: `x`, `y` or `z`],
  [`data-view`], [`-60,20`], [the starting camera: turn about the up axis, then tilt, in degrees],
  [`data-pick`], [`click`], [when the widget's `on_pick` runs: `click`, `hover` or `none`],
  [`data-flat`], [off], [flat shading for the `mesh` layer],
  [`data-label`], [], [the figure's accessible description],
  [`data-credit`], [on], [a small "wgw" link in the corner; `"false"` hides it (or `.wgw-credit { display: none }`)],
)

= Interaction

Drag to turn (it glides on), pinch or ctrl/cmd+scroll to zoom, double-click to reset. Plain scrolling scrolls
the page, so figures never trap it.

= Mesh files

The page fetches `data-src` and the core parses it, so the mesh is drawn at once, before any widget has started;
then the widget gets it as `widget(mesh)`.

- *`.obj`*: Wavefront OBJ. Triangles and polygons (split into fans), 1-based or negative indices. Texture
  coordinates and normals are ignored, so the vertices are exactly the file's `v` lines, in order.
- *`.wgw`*: a mesh plus any arrays its widget needs (a spectrum, say), which the widget gets as attributes
  (`mesh.evecs`). Write one with `Mesh(vertices, faces).save("spot.wgw", evecs=...)`; see the
  #link("/reference/#wgw.save")[Python reference].

= Styling

CSS custom properties on the figure or any ancestor:

#table(
  columns: 3,
  [*property*], [*default*], [],
  [`--wgw-colors`], [`#d8d6cf, #b8402f`], [the colors `values` run through, from low to high],
  [`--wgw-accent`], [the last of `--wgw-colors`], [points, curves and arrows without values],
  [`--wgw-edge`], [`rgba(0, 0, 0, 0.3)`], [the triangle grid],
  [`--wgw-control`], [the text color], [sliders and checkboxes],
  [`--wgw-height`], [`420px`], [the canvas's height],
)

Any CSS color works, `var(...)` included, so figures can follow a site's theme:

```css
.wgw { --wgw-colors: var(--surface), var(--red); --wgw-accent: var(--text); --wgw-height: 420px; }
```

When the theme switches (a `data-theme` or `class` on `<html>` or `<body>`, or the system's setting), every
figure recolors. The controls row (`.wgw-controls`) and the status line (`.wgw-status`) can be restyled too;
wgw's own styles come first in the page, so a site's win.
