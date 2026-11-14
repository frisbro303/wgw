#import "@local/tdg:0.1.0": *
#show: docs.doc.with("heads", title: "From other languages", order: 6, description: "wgw figures from Python, Julia, R, Lean and Rust: windows, saved images and videos, and the protocol behind them.")

The same figures as on a page, from a program: a head in each language makes them, and `wgw-window`, one small
Rust program they all start, shows each in a window with a webview running wgw's page code and core. A figure is a
`.wgw` file (a widget or a mesh), arrays as layers, or a function of the controls, which `wgw-window` calls back
whenever a slider moves or a point is picked. A window closes when its program ends, so a script waits on showing a
figure until its window is closed.

#table(
  columns: 3,
  [*head*], [*where*], [*shows / saves*],
  [Python], [`heads/python`], [`Figure(...).show()`, inline in Jupyter / `save`],
  [Julia], [`heads/julia`], [`display(Figure(...))`, inline in Jupyter and Pluto / `savefig`],
  [R], [`heads/r`], [printing a `wgw_figure(...)`, or `wgw_show` / `wgw_save`],
  [Lean 4], [`heads/lean`], [`Figure.display` / `Figure.save`],
  [Rust], [`heads/rust` (wgw-show)], [`Figure::show` / `Figure::save`],
)

```python
import wgw
wgw.Figure("torus.wgw").show()                                   # a widget, or a mesh file
wgw.Figure(wgw.Mesh(vertices, faces, values=k)).show()           # arrays
wgw.Figure(lambda s: {"mesh": wgw.Mesh(vertices * s["t"], faces)},
           {"t": wgw.Slider(0.5, 2, 1)}).save("torus.mp4", seconds=4)
```

```julia
using Wgw
display(Figure(s -> Dict("mesh" => Mesh(vertices .* s["t"], faces)), "t" => Slider(0.5, 2, 1)))
```

```r
wgw_figure(function(s) list(mesh = wgw_mesh(vertices * s$t, faces)), list(t = wgw_slider(0.5, 2, 1)))
```

In Rust, `wgw_show::Figure::widget(w)` runs a `Widget` natively: the same code `cargo wgw` compiles to a `.wgw`
file. Julia and R count faces and picks from 1, as those languages do; the others from 0. Arrays with one more
leading axis than usual (frames × n × 3 vertices) are an animation.

= Saving images and videos

Every head saves a figure headless, with no window or GPU. wgw-headless does all of it, for `wgw-window`: it draws the
figure on the CPU with the page's own renderer and shaders, runs a widget in a WebAssembly interpreter, and writes
the file. `.png` is a still; `.gif` and `.apng` are videos it writes itself; `.mp4`, `.webm` and `.mov` are made by
`ffmpeg`, if it's installed. A video plays an animation through, or turns a figure that doesn't animate once around
(in 4 seconds, by default). A function's figure, or a widget's, is drawn with its controls as set up, or as given.
Videos and widgets are wgw-headless's optional features (`video`, `widgets`), so tdg's plugin, which only draws
stills, doesn't carry them.

#table(
  columns: 2,
  [*option*], [],
  [width, height], [the figure's size in CSS pixels (800 by its height)],
  [scale], [image pixels to each CSS pixel (2)],
  [background], [a CSS color behind it (transparent for PNG, white for videos)],
  [controls], [the controls' values],
  [fps, frames, seconds], [a video's rate and length],
  [spin], [degrees it turns about its up axis over the video],
)

= Typst

In Typst, tdg's `#widget` is the head: a figure is live in HTML export and, in a PDF, a still, drawn by wgw-headless,
which tdg's plugin uses as a library.

= Installing

The heads find `wgw-window` as `WGW_WINDOW`, or on the PATH. Each release has it built for Linux, macOS and
Windows; from the repository, `./build.sh` then `cargo install --path window`. Without the `window` feature
(`--no-default-features`) it has no windows, and so needs no webview (GTK's WebKit, on Linux): it saves, and
serves, on machines with no display.

= The protocol

A head starts `wgw-window --protocol 1` (with `--serve` to serve only; with `--connect <port>` to talk over a TCP
connection to the head, for R), and a `wgw-window` speaking another protocol stops at once, saying so. They talk in
frames: a little-endian u32 length and a header, then a u32 length and a body. A head's headers are JSON;
`wgw-window`'s are a line of words, so a head needs no JSON reader.

#table(
  columns: 2,
  [*from the head*], [],
  [`{"type": "figure", "id", "title", "options", "setup", "height", "body"}`], [show a figure: body `"src"` (a
    `.wgw` file), `"scene"` (a patch) or `"callback"` (its scenes asked for)],
  [`{"type": "scene", "request"}`], [a scene asked for, as the body: a patch],
  [`{"type": "error", "request", "message"}`], [it couldn't be made],
  [`{"type": "save", "request", "path", "figure", "video", "controls", "setup", "body"}`], [save it (as above)],
)

#table(
  columns: 2,
  [*to the head*], [],
  [`url <id> <url>`], [where the figure is served],
  [`scene <request> <id>`], [its scene, for the state in the body: a line `control <value> <name>` for each
    control, and `pick <vertex> <frame> <x> <y> <z> <layer>` for each pick, the latest last],
  [`closed <id>`], [its window was closed],
  [`saved <request>`, `failed <request> <message>`], [how a save went],
)

`tests/heads/` holds what every head must do with the same input: encode the same scene, read the same state and
save through `wgw-window`. CI runs them all.
