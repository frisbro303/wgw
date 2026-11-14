// A worker hosting some of the page's widgets (widget.js shares a few workers among them all), so building their
// scenes (every frame of an animation) never holds up the page. The patches come back with their arrays
// transferred, not copied.
//
// An animation's frames are built one at a time, the one asked for first: it's sent on its own at once, to show
// while the rest are built. Between frames the worker pauses for its other messages: a cancel (the state moved on),
// or another widget's request, which then needn't wait for a long animation to finish.
import { decodePatch, instantiate, stackFrames } from "./widget.js";

const widgets = new Map();  // by the page's id for each
const jobs = new Map();     // a widget's scene being built: its token, until it's done or cancelled

// A pause that lets the worker's other messages in, without setTimeout's clamp to 4 ms.
const channel = new MessageChannel(), resumes = [];
channel.port1.onmessage = () => resumes.shift()();
const pause = () => new Promise((resume) => { resumes.push(resume); channel.port2.postMessage(null); });

async function scene(m) {
  const widget = widgets.get(m.widget);
  const token = {};
  jobs.set(m.widget, token);
  try {
    const frames = Math.max(1, m.frames | 0), first = Math.min(Math.max(0, m.first | 0), frames - 1);
    const order = [first, ...Array.from({ length: frames }, (_, k) => k).filter((k) => k !== first)];
    const patches = [];
    let since = performance.now();
    for (const k of order) {
      if (jobs.get(m.widget) !== token) return self.postMessage({ id: m.id, result: null });  // cancelled
      patches[k] = decodePatch(widget.scene(k === 0 ? m.mesh : null, { ...m.state, frame: k }));
      if (k === first && frames > 1) self.postMessage({ id: m.id, partial: patches[k] });  // copied: it's stacked too
      if (performance.now() - since > 8) {
        await pause();
        since = performance.now();
      }
    }
    const patch = stackFrames(patches);
    const buffers = [];
    for (const layer of Object.values(patch.layers)) {
      for (const a of Object.values(layer?.arrays || {})) {
        if (!buffers.includes(a.data.buffer)) buffers.push(a.data.buffer);
      }
    }
    self.postMessage({ id: m.id, result: patch }, buffers);
  } finally {
    if (jobs.get(m.widget) === token) jobs.delete(m.widget);
  }
}

self.onmessage = async ({ data: m }) => {
  try {
    if (m.type === "load") {
      const widget = await instantiate(m.bytes);
      widgets.set(m.widget, widget);
      self.postMessage({ id: m.id, result: widget.setup(m.mesh) });
    } else if (m.type === "scene") {
      await scene(m);
    } else if (m.type === "cancel") {
      jobs.delete(m.widget);
    } else if (m.type === "drop") {
      jobs.delete(m.widget);
      widgets.delete(m.widget);
    }
  } catch (err) {
    if (m.id !== undefined) self.postMessage({ id: m.id, error: String((err && err.message) || err) });
  }
};
