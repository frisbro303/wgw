#import "@local/tdg:0.1.0": *
#set document(title: "Python reference", description: "wgw's Python module: widgets, layers, meshes, shapes and files.")

Everything is in the `wgw` module. Widgets import it in the browser, where wgw puts it on their path; offline
scripts can import `python/wgw.py`. It needs only NumPy.

= Widgets

#api("wgw.Widget")
#api("wgw.Slider")
#api("wgw.Checkbox")

= Layers

#api("wgw.Layer")
#api("wgw.Mesh")
#api("wgw.Points", members: false)
#api("wgw.Lines", members: false)
#api("wgw.Vectors", members: false)

= Shapes

#api("wgw.icosphere")
#api("wgw.torus")
#api("wgw.surface")
#api("wgw.grid_faces")
#api("wgw.rescale")

= Files

#api("wgw.save")
#api("wgw.load")

A `.wgw` file is little-endian: `b"WGW1"`, a uint32 header length, a JSON header
`{"arrays": {name: {"dtype", "shape", "offset"}}}`, then each array's raw bytes at its offset from the end of the
header, 8-byte aligned.
