#import "@local/tdg:0.1.0": *
#show: docs.doc.with("index", title: "Overview", order: 1, description: "Interactive 3D figures for web pages, PDFs and programs in Python, Julia, R, Lean and Rust, drawn by one renderer.")

wgw draws interactive 3D figures: meshes, point clouds, curves and vector fields, with sliders and clicks. One
renderer, written in Rust, draws them on web pages (compiled to WebAssembly, with WebGL), in PDFs (through Typst and
tdg, on the CPU), and from programs in Python, Julia, R, Lean and Rust (in a window, inline in Jupyter, or saved as
images and videos: see #link("/heads/")[From other languages]).

A figure can be driven by a *widget*: a small WebAssembly module, written in Rust, that responds to sliders and
clicks, and runs the same in all of these. A figure is one file, `.wgw`: a widget with any data it needs inside, or
just a mesh.

#callout(title: [Beta])[wgw is in beta: its API and file format still change, without deprecations.]

#widget(path("../examples/torus.wgw"), view: (-60, 35),
        alt: "A torus colored by its Gaussian curvature, reshaped by two sliders")

Drag to turn it; the sliders reshape it. It's this widget, in Rust:

#source-code(path("../examples/torus/src/lib.rs"))

on a page with this HTML:

```html
<script type="module" src="wgw/wgw.js"></script>
<div class="wgw" data-src="torus.wgw"></div>
```

or, in a #link("https://github.com/frisbro303/tdg")[tdg] site, this Typst, where a PDF shows the figure's first
frame, drawn by wgw's own renderer with the page's shaders:

```typ
#widget(path("torus.wgw"))
```

= Getting it

wgw isn't on a package registry yet: build it from #link("https://github.com/frisbro303/wgw")[the repository],
which needs Rust with the `wasm32-unknown-unknown` target, `wasm-bindgen-cli` at the version in `Cargo.lock`
(`build.sh` says which), and optionally `wasm-opt` from #link("https://github.com/WebAssembly/binaryen")[binaryen].

```sh
./build.sh                     # -> dist/
./build.sh ../site/theme/wgw   # ... and copy it into a site
```

Then serve `dist/` with your pages. Figures need WebGL2. tdg sites have wgw built in.

= Next

- #link("/figures/")[Figures]: putting a figure on a page, its options and styling, mesh files
- #link("/widgets/")[Writing widgets]: widgets in Rust, by example
- #link("/reference/")[Reference]: layers and their parameters, and a widget's contract, for other languages
- #link("/javascript/")[The page API]: figures from JavaScript
- #link("/internals/")[Internals]: how it works, and extending it
