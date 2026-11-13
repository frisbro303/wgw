#import "@local/tdg:0.1.0": *
#set document(title: "Overview", description: "Interactive 3D figures for the web, driven by Python and JavaScript widgets.")

wgw draws interactive 3D figures on web pages: meshes, point clouds, curves and vector fields, with a Rust core
compiled to WebAssembly. A figure can be driven by a *widget*, a short Python or JavaScript program that responds
to sliders and clicks. Python runs in the browser with Pyodide, off the page's thread.

#widget(widget: "/examples/fields_widget.py", view: (-50, 25),
        alt: "A sphere colored by a function, with its gradient as arrows and a level curve")

Drag to turn it, click to place a point. It's this widget:

#source-code("fields_widget.py", read("/examples/fields_widget.py"))

on a page with this HTML:

```html
<script type="module" src="wgw/wgw.js"></script>
<div class="wgw" data-widget="fields_widget.py"></div>
```

or, in a #link("https://github.com/frisbro303/tdg")[tdg] site, this Typst:

```typ
#widget(widget: "fields_widget.py")
```

= Getting it

wgw isn't on a package registry yet: build it from #link("https://github.com/frisbro303/wgw")[the repository],
which needs Rust with the `wasm32-unknown-unknown` target, and
#link("https://github.com/wasm-bindgen/wasm-pack")[wasm-pack].

```sh
./build.sh                     # -> dist/
./build.sh ../site/theme/wgw   # ... and copy it into a site
```

Then serve `dist/` with your pages. Figures need WebGL2; Python widgets download Pyodide and NumPy (a few MB) on
a first visit. tdg sites have wgw built in.

= Next

- #link("/figures/")[Figures]: putting a figure on a page, its options and styling, mesh files
- #link("/widgets/")[Writing widgets]: a guide, by example
- #link("/reference/")[Python reference]: `Widget`, controls, layers, meshes, shapes, files
- #link("/javascript/")[JavaScript]: widgets in JavaScript, the page API, other languages
- #link("/internals/")[Internals]: how it works, and extending it
