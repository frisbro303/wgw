#import "@local/tdg:0.1.0": *
#show: docs.doc.with("figures", title: "Figures", order: 2, description: "Putting a wgw figure on a page: its attributes, styling and mesh files.")

Load `wgw.js` as a module (everything else in `dist/` must sit beside it), and any `<div class="wgw">` becomes a
figure:

```html
<script type="module" src="wgw/wgw.js"></script>
<div class="wgw" data-src="distance.wgw" data-up="y" data-view="90,10"></div>
```

A figure needs a browser with WebGL2. It loads when it nears the screen, and draws only while on it. Its file is a
widget or a mesh: this one is just a mesh (`data-src`, `data-up` and `data-view`).

#widget(path("../examples/spot.obj"), up: "y", view: (-110, 15), alt: "Keenan Crane's Spot, a cow")

= In Typst

In a #link("https://github.com/frisbro303/tdg")[tdg] site, `widget(...)` makes the same figure, numbered and
captioned like any other; tdg publishes its file (given as a path) with the site, and a PDF shows its first
frame: tdg runs the widget (as a Typst plugin) and wgw's own renderer draws it on the CPU (wgw-still), with the
page's shaders:

```typ
#widget(path("distance.wgw"), up: "y", view: (90, 10), caption: [Diffusion distance on Spot.])
```

= Attributes

#table(
  columns: 3,
  [*attribute*], [*default*], [],
  [`data-src`], [], [the figure: a widget (a `.wgw` file holding WebAssembly; #link("/widgets/")[writing widgets]), or a mesh (`.obj`, or a `.wgw` file of arrays), shown as the layer `mesh`],
  [`data-up`], [`z`], [the axis that points up: `x`, `y` or `z`],
  [`data-view`], [`-60,20`], [the starting camera: turn about the up axis, then tilt, in degrees; then, optionally, a scale (`-60,20,1.5` shows the scene half again as large, for a wide scene the fit leaves small)],
  [`data-pick`], [`click`], [when a pick reaches the widget: `click`, `hover` or `none`],
  [`data-flat`], [off], [flat shading for the `mesh` layer],
  [`data-label`], [], [the figure's accessible description],
  [`data-credit`], [on], [a small "wgw" link in the corner; `"false"` hides it (or `.wgw-credit { display: none }`)],
)

= Interaction

Drag to turn (it glides on), pinch or ctrl/cmd+scroll to zoom, double-click to reset. Plain scrolling scrolls
the page, so figures never trap it.

From the keyboard, Tab to a figure: the arrow keys turn it (with shift, in small steps), + and - zoom, 0 resets, and
Enter or space picks the point at the center of its first view, which a small ring marks while it's focused.

= Files

A figure's file is told apart by its first bytes, whatever its name:

- *A widget*: a WebAssembly module (#link("/widgets/")[writing widgets]), with any data it needs inside. wgw runs
  it; it builds the scene.
- *`.wgw` arrays*: a mesh (vertices and faces) plus any other arrays, in wgw's container: `WGW1`, the header's
  length (a little-endian u32), a JSON header `{"arrays": {name: {dtype, shape, offset}}}`, then the arrays. Shown
  as the layer `mesh`. A widget can embed one (`MeshData::parse(include_bytes!("spot.wgw"))`).
- *`.obj`*: Wavefront OBJ. Triangles and polygons (split into fans), 1-based or negative indices. Texture
  coordinates and normals are ignored, so the vertices are exactly the file's `v` lines, in order.

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
figure recolors, along with layer colors a widget gives as the page's variables (`color="var(--link)"`). The controls row (`.wgw-controls`) and the status line (`.wgw-status`) can be restyled too;
wgw's own styles come first in the page, so a site's win.
