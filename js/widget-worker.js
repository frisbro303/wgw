// A widget, run in a worker so building its scenes (every frame of an animation) never holds up the page. The page
// talks to it through widget.js; the patches come back with their arrays transferred, not copied.
import { decodePatch, instantiate, stackFrames } from "./widget.js";

let widget = null;

self.onmessage = async ({ data: m }) => {
  try {
    if (m.type === "load") {
      widget = await instantiate(m.bytes);
      self.postMessage({ id: m.id, result: widget.setup(m.mesh) });
    } else if (m.type === "scene") {
      const frames = Array.from({ length: m.frames }, (_, k) => decodePatch(widget.scene(k === 0 ? m.mesh : null, { ...m.state, frame: k })));
      const patch = stackFrames(frames);
      const buffers = [];
      for (const layer of Object.values(patch.layers)) {
        for (const a of Object.values(layer?.arrays || {})) {
          if (!buffers.includes(a.data.buffer)) buffers.push(a.data.buffer);
        }
      }
      self.postMessage({ id: m.id, result: patch }, buffers);
    }
  } catch (err) {
    self.postMessage({ id: m.id, error: String((err && err.message) || err) });
  }
};
