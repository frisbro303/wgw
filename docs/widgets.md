# Writing widgets

A widget is a Python file next to a figure, named in its `data-widget`. It defines one function, `widget`, from
the figure's mesh to a [`Widget`](reference.md#wgw.Widget): a value saying what the figure starts as, which
controls it has, and how it responds to them.

## A first widget

Move the mouse over Spot:

<div class="wgw" data-src="examples/spot.obj" data-widget="examples/hover_widget.py" data-pick="hover" data-up="y" data-view="-110,15"></div>

```python
--8<-- "examples/hover_widget.py"
```

```html
<div class="wgw" data-src="spot.obj" data-widget="hover_widget.py" data-pick="hover" data-up="y"></div>
```

- `widget` runs once. `spot` is the figure's `data-src`, already drawn by the time Python starts; for a figure
  without one, define `widget()` with no argument.
- `on_pick` runs when a vertex is clicked (here, hovered), and again whenever a control moves.
- Controls arrive as keyword arguments, named as in `controls`.
- It returns a **scene**: layers by name. The figure's mesh is the layer `"mesh"`, so this recolors it.

## Responding to controls

`on_change` runs at the start, and whenever a control changes; it suits figures that don't need a click. This
one builds its shape in Python, and gives readers a checkbox for the triangle grid:

<div class="wgw" data-widget="examples/torus_widget.py" data-view="-60,35" style="--wgw-colors: #4385be, #dad8ce, #af3029"></div>

```python
--8<-- "examples/torus_widget.py"
```

Controls are [`Slider`](reference.md#wgw.Slider)s, with `log=True` for one that moves in powers of ten, and
[`Checkbox`](reference.md#wgw.Checkbox)es. A widget can have both `on_change` and `on_pick`.

## Scenes and layers

A scene is a dict of layers by name. Each layer you return replaces the layer of that name **exactly as given**;
`None` removes one; layers you don't name stay as they are. So here a click adds a dot without redrawing the rest,
and unticking *arrows* removes them:

<div class="wgw" data-widget="examples/fields_widget.py" data-view="-50,25"></div>

```python
--8<-- "examples/fields_widget.py"
```

There are four kinds of layer, [`Mesh`](reference.md#wgw.Mesh), [`Points`](reference.md#wgw.Points),
[`Lines`](reference.md#wgw.Lines) and [`Vectors`](reference.md#wgw.Vectors). Any of them is colored by `values`
(numbers per vertex, 0 for the first of the figure's colors, 1 for the last) or by one `color`.

Layers are values: `.with_values(...)` and `.replace(...)` make new layers that share the arrays they don't
change. That matters for speed, since an array is sent to the page only when it's a new object: returning
`sphere.with_values(...)` sends only the values. Make new arrays rather than changing ones you've returned.

## Animation

Give `values` or `vertices` a leading frames axis, `(frames, n)` or `(frames, n, 3)`, and the frames play at the
widget's `fps`, once (or forever, with `loop=True`). When a slider moves, the last frame shows at once. Click the
sphere:

<div class="wgw" data-widget="examples/heat_widget.py"></div>

```python
--8<-- "examples/heat_widget.py"
```

## Work done once, and data files

Anything expensive goes in `widget` itself, like the eigendecomposition above; the functions it returns close over
it, so nothing lives in globals. A widget reads files with paths relative to itself:

```python
def widget(spot: Mesh) -> Widget:
    spectrum = np.load("spot-spectrum.npz")                             # beside the widget
    embedding = spectrum["evecs"] * np.exp(-spectrum["evals"] * 0.3)    # once

    def on_pick(vertex, epsilon):                                       # on every click
        d2 = np.sum((embedding - embedding[vertex]) ** 2, axis=1)
        return {"mesh": spot.with_values(np.exp(-d2 / epsilon))}
    ...
```

In the browser, each file is fetched from beside the widget the first time it's opened (by `open`, `np.load`,
`Mesh.load`, ...), so the same code runs in the browser and on your machine. Heavy precomputation, like the
spectrum of a big mesh, is best done offline and saved next to the widget. The figure's own mesh is best given as
`data-src`, which is drawn before Python starts.

Widgets can import NumPy and anything else
[Pyodide ships](https://pyodide.org/en/stable/usage/packages-in-pyodide.html), such as SciPy, which loads the
first time it's imported.

## Which layer was clicked

Meshes and points can be picked (pass `pickable=False` to let clicks through to what's behind, or `True` to pick
curves and arrows). Give `on_pick` a `layer` parameter to be told which layer the vertex is in:

```python
def on_pick(vertex, epsilon, layer):
    if layer != "mesh":
        return None
    ...
```

With `data-pick="hover"`, `on_pick` follows the mouse instead; touch screens still tap.

## When it runs, and errors

Widgets run in a worker, so a slow one never freezes the page. While a widget is busy, a newer call replaces the
waiting one, so dragging a slider never builds up a backlog. Python starts loading as soon as the page is idle.

If a widget raises an error, its last line shows in the figure's corner, and the traceback is in the browser's
console.
