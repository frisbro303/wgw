#import "@local/tdg:0.1.0": *
#show: docs.doc.with("javascript", title: "The page API", order: 5, description: "wgw figures from JavaScript: mounting them, picks, patches and widgets.")

`wgw.js` makes every `<div class="wgw">` a figure on its own, and sets `window.WGW` for pages that want more:

#table(
  columns: 2,
  [*member*], [],
  [`WGW.mount(element, options)`], [make an element a figure (or get its figure); returns it],
  [`WGW.unmount(element)`], [take an element's figure away (its `destroy()`)],
  [`WGW.loadWidget(bytes)`], [a widget module, run on one of the page's widget workers: its `setup` (as given);
    `scene(state, frames, first, onFirst)`, a promise of the patch of every frame (`onFirst` gets frame `first` alone
    as soon as it's built; `null` if cancelled); `cancel()`; and `terminate()`],
)

`options` are the #link("/figures/#attributes")[attributes] without `data-`: `src`, `up`, `view`
(`[turn, tilt]` or `[turn, tilt, scale]`), `pick`, `flat`. A figure (also `element.wgw`) has `on("pick", fn)`,
which calls `fn({layer, index, point})` on each pick; `apply(patch, animate)`, to change the scene directly;
`reset()`, for the starting camera; `destroy()`, which stops its widget, frees what it holds on the GPU and removes
what wgw added to the element, for pages that remove figures; and `ready`, a promise, once the core has started.

A page can have any number of figures: they all draw with one WebGL context, so browsers' limit of about 16 live
contexts never applies.

A patch for `apply` is the #link("/reference/#files-and-patches")[patch] as JavaScript objects:

```js
{ layers: { name: null | { type, arrays: { key: { data, shape } }, params: { key: value }, remove: [key] } } }
```

Arrays are `Float32Array`s (`Uint32Array`s for `faces` and `segments`), row-major; one more axis than the key has
per frame (`values` 1, the rest 2) is a frames axis.
