#import "@local/tdg:0.2.0": *
#show: doc.with("reference", title: "Python reference", order: 4, description: "wgw's Python module: widgets, layers, meshes, shapes and files.")

#let wgw-api = python-api(read("../python/wgw.py"), module: "wgw", page: "reference")

Everything is in the `wgw` module. Widgets import it in the browser, where wgw puts it on their path; offline
scripts can import `python/wgw.py`. It needs only NumPy.

= Widgets

#api(wgw-api, "wgw.Widget")
#api(wgw-api, "wgw.Slider")
#api(wgw-api, "wgw.Checkbox")

= Layers

#api(wgw-api, "wgw.Layer")
#api(wgw-api, "wgw.Mesh")
#api(wgw-api, "wgw.Points", members: false)
#api(wgw-api, "wgw.Lines", members: false)
#api(wgw-api, "wgw.Vectors", members: false)

= Shapes

#api(wgw-api, "wgw.icosphere")
#api(wgw-api, "wgw.torus")
#api(wgw-api, "wgw.surface")
#api(wgw-api, "wgw.grid_faces")
#api(wgw-api, "wgw.rescale")

= Files

#api(wgw-api, "wgw.save")
#api(wgw-api, "wgw.load")

A `.wgw` file is little-endian: `b"WGW1"`, a uint32 header length, a JSON header
`{"arrays": {name: {"dtype", "shape", "offset"}}}`, then each array's raw bytes at its offset from the end of the
header, 8-byte aligned.
