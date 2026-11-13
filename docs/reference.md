# Python reference

Everything is in the `wgw` module. Widgets import it in the browser, where wgw puts it on their path; offline
scripts can import `python/wgw.py`. It needs only NumPy.

## Widgets

::: wgw.Widget

::: wgw.Slider

::: wgw.Checkbox

## Layers

::: wgw.Layer
    options:
      members: [replace, with_values]

::: wgw.Mesh
    options:
      members: [load, save, normals, face_normals, face_areas, areas, area, edges]

::: wgw.Points
    options:
      members: false

::: wgw.Lines
    options:
      members: false

::: wgw.Vectors
    options:
      members: false

## Shapes

::: wgw.icosphere

::: wgw.torus

::: wgw.surface

::: wgw.grid_faces

::: wgw.rescale

## Files

::: wgw.save

::: wgw.load

A `.wgw` file is little-endian: `b"WGW1"`, a uint32 header length, a JSON header
`{"arrays": {name: {"dtype", "shape", "offset"}}}`, then each array's raw bytes at its offset from the end of the
header, 8-byte aligned.
