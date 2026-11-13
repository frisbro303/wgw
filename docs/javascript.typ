#import "@local/tdg:0.1.0": *
#show: docs.doc.with("javascript", title: "The page API", order: 5, description: "wgw figures from JavaScript: mounting them, picks, patches and widgets.")

`wgw.js` makes every `<div class="wgw">` a figure on its own, and sets `window.WGW` for pages that want more:

#table(
  columns: 2,
  [*member*], [],
  [`WGW.mount(element, options)`], [make an element a figure (or get its figure); returns it],
  [`WGW.loadWidget(bytes)`], [a widget module, with its `setup(mesh)` and `scene(mesh, state)` as functions],
)

`options` are the #link("/figures/#attributes")[attributes] without `data-`: `src`, `up`, `view`
(`[turn, tilt]` or `[turn, tilt, scale]`), `pick`, `flat`. A figure (also `element.wgw`) has `on("pick", fn)`,
which calls `fn({layer, index, point})` on each pick; `apply(patch, animate)`, to change the scene directly;
`reset()`, for the starting camera; and `ready`, a promise, once the core has started.

A patch for `apply` is the #link("/reference/#files-and-patches")[patch] as JavaScript objects:

```js
{ layers: { name: null | { type, arrays: { key: { data, shape } }, params: { key: value }, remove: [key] } } }
```

Arrays are `Float32Array`s (`Uint32Array`s for `faces` and `segments`), row-major; one more axis than the key has
per frame (`values` 1, the rest 2) is a frames axis.
