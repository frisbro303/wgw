// wgw: interactive meshes, points, curves and vector fields, drawn by a Rust core compiled to WebAssembly and
// driven by widgets in Python, JavaScript, or anything a runner is registered for (see runners.js).
//
// Any <div class="wgw"> becomes a viewer. Its data attributes:
//   data-src="spot.obj"        a mesh file, .obj or .wgw (from python/wgw.py), parsed by the core and shown as
//                              the layer "mesh"; a widget gets it as widget(mesh)
//   data-widget="x_widget.py"  a widget that builds, colors or reshapes the scene: .py or .js
//   data-up="z"                the axis that points up: x, y or z
//   data-view="-60,20"         the starting camera: turn and tilt, in degrees
//   data-pick="click"          when the widget's pick runs: click, hover or none
//   data-flat                  flat shading for the "mesh" layer
// CSS custom properties style it: --wgw-colors (the colors values run through), --wgw-accent (points,
// curves and arrows without values), --wgw-edge, --wgw-height.
//
// From JS: WGW.mount(element, options) returns the viewer: viewer.on("pick", fn), viewer.apply(patch),
// viewer.reset(); WGW.runners to add a language.

import init, { Viewer as Core } from "./core/wgw_core.js";
import { loadMesh, meshArrays } from "./io.js";
import { animates, controlInputs, toPatch } from "./patch.js";
import { runnerFor, runners } from "./runners.js";
import { readStyle, rgbaList } from "./style.js";

const wasm = init();
const DPR_MAX = 2;
const REDUCED_MOTION = matchMedia("(prefers-reduced-motion: reduce)");

const STYLE = `
.wgw{position:relative}
.wgw canvas{display:block;width:100%;height:var(--wgw-height,420px);touch-action:none;cursor:grab;outline:none}
.wgw canvas.dragging{cursor:grabbing}
.wgw-status{position:absolute;left:8px;top:8px;font:12px ui-monospace,monospace;opacity:.6;pointer-events:none;white-space:pre-line}
.wgw-controls{display:flex;flex-wrap:wrap;justify-content:center;gap:6px 18px;font:13px ui-monospace,monospace}
.wgw-controls:empty{display:none}
.wgw-controls label{display:inline-flex;align-items:center;gap:8px;cursor:pointer;user-select:none}
.wgw-controls output{min-width:5ch;font-variant-numeric:tabular-nums}
.wgw-controls input{--c:var(--wgw-control,currentColor);--faint:color-mix(in srgb,var(--c) 22%,transparent);margin:0;cursor:pointer}
.wgw-controls input:focus-visible{outline:none}
.wgw-controls input[type=range]{-webkit-appearance:none;appearance:none;width:140px;height:18px;background:none}
.wgw-controls input[type=range]::-webkit-slider-runnable-track{height:2px;border-radius:1px;background:linear-gradient(to right,var(--c) var(--fill,50%),var(--faint) var(--fill,50%))}
.wgw-controls input[type=range]::-webkit-slider-thumb{-webkit-appearance:none;width:12px;height:12px;margin-top:-5px;border:0;border-radius:50%;background:var(--c);transition:transform .12s}
.wgw-controls input[type=range]::-moz-range-track{height:2px;border-radius:1px;background:var(--faint)}
.wgw-controls input[type=range]::-moz-range-progress{height:2px;border-radius:1px;background:var(--c)}
.wgw-controls input[type=range]::-moz-range-thumb{width:12px;height:12px;border:0;border-radius:50%;background:var(--c);transition:transform .12s}
.wgw-controls input[type=range]:hover::-webkit-slider-thumb,.wgw-controls input[type=range]:active::-webkit-slider-thumb{transform:scale(1.25)}
.wgw-controls input[type=range]:hover::-moz-range-thumb,.wgw-controls input[type=range]:active::-moz-range-thumb{transform:scale(1.25)}
.wgw-controls input[type=range]:focus-visible::-webkit-slider-thumb{box-shadow:0 0 0 4px var(--faint)}
.wgw-controls input[type=range]:focus-visible::-moz-range-thumb{box-shadow:0 0 0 4px var(--faint)}
.wgw-controls input[type=checkbox]{-webkit-appearance:none;appearance:none;width:12px;height:12px;border:1.5px solid var(--c);border-radius:3px;display:grid;place-content:center}
.wgw-controls input[type=checkbox]::after{content:"";width:6px;height:6px;border-radius:1px;background:var(--c);transform:scale(0);transition:transform .12s}
.wgw-controls input[type=checkbox]:checked::after{transform:scale(1)}
.wgw-controls input[type=checkbox]:focus-visible{box-shadow:0 0 0 4px var(--faint)}`;

const viewers = [];
let widgetCount = 0;

const flag = (v) => v !== undefined && v !== "false";
const format = (x) => String(+(+x).toPrecision(3));
// How far along a slider is, for the filled part of its track.
const fill = (input) => input.style.setProperty("--fill", `${((input.value - input.min) / (input.max - input.min)) * 100}%`);

export class Viewer {
  constructor(el, options = {}) {
    const d = el.dataset;
    this.el = el;
    this.options = {
      src: options.src ?? d.src,
      widget: options.widget ?? d.widget,
      up: options.up ?? d.up ?? "z",
      view: options.view || (d.view ? d.view.split(",").map(Number) : [-60, 20]),
      pick: options.pick ?? d.pick ?? "click",
      fps: +(options.fps ?? d.fps ?? 30),
      flat: options.flat ?? flag(d.flat),
    };
    el.wgw = this;
    el.classList.add("wgw");

    this.canvas = document.createElement("canvas");
    this.canvas.setAttribute("role", "img");
    this.canvas.setAttribute("aria-label", d.label || "Interactive 3D figure: drag to turn, pinch or ctrl+scroll to zoom");
    this.statusEl = Object.assign(document.createElement("div"), { className: "wgw-status" });
    this.controlsEl = Object.assign(document.createElement("div"), { className: "wgw-controls" });
    el.append(this.canvas, this.statusEl, this.controlsEl);

    this.controls = {};
    this.functions = [];
    this.pending = new Map();
    this.lastPick = null;
    this.visible = false;
    this.ready = this.boot();
  }

  async boot() {
    try {
      await wasm;
      this.core = new Core(this.canvas);
    } catch (err) {
      return this.fail(err);
    }
    this.core.set_options(this.options.up, this.options.view[0], this.options.view[1], REDUCED_MOTION.matches);
    this.restyle();
    this.listen();
    this.canvas.addEventListener("webglcontextlost", (e) => { e.preventDefault(); this.status("graphics reset: reload the page"); });
    new ResizeObserver(() => this.requestRender()).observe(this.canvas);
    // Load when the viewer nears the screen; draw only while it's on it.
    let started = false;
    new IntersectionObserver((entries) => {
      this.visible = entries[entries.length - 1].isIntersecting;
      if (this.visible && !started) { started = true; this.start(); }
      if (this.visible) this.requestRender();
    }, { rootMargin: "400px" }).observe(this.el);
    viewers.push(this);
  }

  async start() {
    const o = this.options;
    try {
      if (o.src) {  // parsed and shown by the core right away, then handed to the widget
        this.status("loading mesh…");
        const file = await loadMesh(o.src);
        try {
          this.core.set_mesh_file("mesh", file);
          if (o.flat) this.core.set_number("mesh", "flat", 1);
          if (o.widget) this.arrays = meshArrays(file);
        } finally {
          file.free();
        }
        this.status("");
        this.requestRender();
      }
      if (o.widget) await this.startWidget();
    } catch (err) {
      this.fail(err);
    }
  }

  on(type, fn) {
    this.el.addEventListener("wgw:" + type, (e) => fn(e.detail));
    return this;
  }

  status(text) { this.statusEl.textContent = text || ""; }

  fail(err) {
    console.error(err);
    const lines = String((err && err.message) || err).trim().split("\n");
    this.status("error: " + lines[lines.length - 1]);
  }

  restyle() {
    if (!this.core) return;
    const s = readStyle(this.el);
    this.core.set_style(s.colormap, s.accent, s.edge);
    this.requestRender();
  }

  // ---- the scene

  // Apply a patch (see patch.js): add, change or remove layers, then play any frames it brought.
  apply(patch, animate) {
    if (!patch) return;
    const core = this.core;
    for (const [name, layer] of Object.entries(patch.layers || {})) {
      if (layer === null) { core.remove_layer(name); continue; }
      const arrays = layer.arrays || {};
      core.set_layer(name, layer.type);
      core.clear_params(name);  // the layer is exactly what's given
      if (name === "mesh" && this.options.flat) core.set_number(name, "flat", 1);  // data-flat, the figure's own
      for (const key of layer.remove || []) core.remove_array(name, key);
      for (const [key, a] of Object.entries(arrays)) {
        const shape = Uint32Array.from(a.shape);
        if (a.data instanceof Uint32Array) core.set_u32(name, key, a.data, shape);
        else core.set_f32(name, key, a.data instanceof Float32Array ? a.data : Float32Array.from(a.data), shape);
      }
      for (const [key, value] of Object.entries(layer.params || {})) {
        if (typeof value === "number" || typeof value === "boolean") core.set_number(name, key, +value);
        else if (typeof value === "string" || Array.isArray(value)) core.set_colors(name, key, rgbaList([].concat(value)));
      }
    }
    if (animates(patch)) core.play(performance.now(), this.fps || this.options.fps, !!this.loop, animate);
    this.requestRender();
  }

  reset() {
    this.core?.reset();
    this.requestRender();
  }

  // ---- drawing: only when something changed, and only while on screen

  requestRender() {
    if (this.frameRequested || !this.core) return;
    this.frameRequested = true;
    requestAnimationFrame((t) => this.frame(t));
  }

  frame(t) {
    this.frameRequested = false;
    if (!this.visible) return;
    const dpr = Math.min(window.devicePixelRatio || 1, DPR_MAX);
    const w = Math.max(1, Math.round(this.canvas.clientWidth * dpr)), h = Math.max(1, Math.round(this.canvas.clientHeight * dpr));
    if (this.canvas.width !== w || this.canvas.height !== h) { this.canvas.width = w; this.canvas.height = h; }
    const moving = this.core.frame(t, w, h, dpr);
    const errors = this.core.take_errors();
    if (errors) this.fail(errors);
    if (this.hoverAt) {
      const [x, y] = this.hoverAt;
      this.hoverAt = null;
      this.picked(this.core.pick(x, y), true);
    }
    if (moving) this.requestRender();
  }

  // ---- input

  listen() {
    const canvas = this.canvas, core = this.core;
    const at = (e) => {
      const r = canvas.getBoundingClientRect();
      return [e.clientX - r.left, e.clientY - r.top];
    };
    canvas.addEventListener("pointerdown", (e) => {
      canvas.setPointerCapture(e.pointerId);
      core.pointer_down(e.pointerId, ...at(e), performance.now());
      canvas.classList.add("dragging");
    });
    canvas.addEventListener("pointermove", (e) => {
      if (core.pointer_move(e.pointerId, ...at(e), performance.now())) this.requestRender();
      else if (this.options.pick === "hover" && e.pointerType === "mouse") { this.hoverAt = at(e); this.requestRender(); }
    });
    const release = (e) => {
      const click = core.pointer_up(e.pointerId, performance.now(), e.type === "pointercancel");
      if (click && this.options.pick !== "none") this.picked(core.pick(...at(e)), false);
      canvas.classList.remove("dragging");
      this.requestRender();
    };
    canvas.addEventListener("pointerup", release);
    canvas.addEventListener("pointercancel", release);
    // Plain scrolling scrolls the page; a trackpad pinch (ctrl+wheel to the browser) or ctrl/cmd+scroll zooms.
    canvas.addEventListener("wheel", (e) => {
      if (!e.ctrlKey && !e.metaKey) return;
      e.preventDefault();
      core.zoom_by(Math.exp(e.deltaY * (e.deltaMode ? 0.05 : 0.01)));
      this.requestRender();
    }, { passive: false });
    canvas.addEventListener("dblclick", () => this.reset());
  }

  picked(hit, hovering) {
    if (!hit) return;
    const [layerIndex, index, x, y, z] = hit;
    const layer = this.core.layer_name(layerIndex);
    if (hovering && this.lastPick && this.lastPick.layer === layer && this.lastPick.index === index) return;
    this.lastPick = { layer, index };
    this.el.dispatchEvent(new CustomEvent("wgw:pick", { detail: { layer, index, point: [x, y, z] } }));
    if (this.functions.includes("pick")) this.call("pick", !hovering);
  }

  // ---- widgets

  async startWidget() {
    const url = this.options.widget;
    this.runner = runnerFor(url);
    this.status("loading widget…");
    this.key = `${url.split("/").pop()}#${++widgetCount}`;
    const info = await this.runner.load(this.key, url, this.arrays || null);
    this.status("");
    this.functions = info.functions;
    this.fps = info.fps;
    this.loop = info.loop;
    this.core.set_grid(!!info.grid);
    this.buildControls(controlInputs(info.controls));
    this.apply(info.result, true);
    this.widgetReady = true;
    if (this.functions.includes("change")) this.call("change", false);
    this.requestRender();
    if (this.lastPick && this.functions.includes("pick")) this.call("pick", true);
  }

  buildControls(spec) {
    for (const [name, c] of Object.entries(spec)) {
      const label = document.createElement("label"), input = document.createElement("input"), output = document.createElement("output");
      const actual = (x) => (c.log ? 10 ** x : x);  // what the widget gets
      // "grid" is the viewer's: a checkbox for the triangle grid, not passed to the widget.
      const viewer = name === "grid" && c.type === "checkbox";
      if (viewer) this.core.set_grid(c.value);
      else this.controls[name] = actual(c.value);
      if (c.type === "checkbox") {
        Object.assign(input, { type: "checkbox", checked: c.value });
        label.append(input, name);
      } else {
        Object.assign(input, { type: "range", min: c.min, max: c.max, step: c.step, value: c.value });
        output.textContent = format(actual(c.value));
        fill(input);
        label.append(name, input, output);
      }
      input.addEventListener("input", () => {
        const value = input.type === "checkbox" ? input.checked : actual(+input.value);
        if (viewer) { this.core.set_grid(value); this.requestRender(); return; }
        this.controls[name] = value;
        if (input.type !== "checkbox") { output.textContent = format(value); fill(input); }
        if (this.functions.includes("change")) this.call("change", false);
        if (this.functions.includes("pick") && this.lastPick) this.call("pick", false);
      });
      this.controlsEl.appendChild(label);
    }
  }

  // Run a widget function, with at most one call in flight: while it's busy, a newer call to the same function
  // replaces the waiting one, so dragging a slider never builds up a backlog.
  call(name, animate) {
    this.pending.set(name, { name, animate });
    if (this.widgetReady && !this.busy) this.flush();
  }

  async flush() {
    const next = this.pending.values().next();
    if (next.done) return;
    const job = next.value;
    this.pending.delete(job.name);
    this.busy = true;
    try {
      const pick = this.lastPick || { index: null, layer: null };
      this.apply(await this.runner.call(this.key, job.name, pick.index, pick.layer, { ...this.controls }), job.animate);
    } catch (err) {
      this.fail(err);
    }
    this.busy = false;
    this.flush();
  }
}

export function mount(el, options) {
  return el.wgw || new Viewer(el, options);
}

function autoload() {
  const style = document.createElement("style");
  style.textContent = STYLE;
  document.head.prepend(style);  // first, so the page's own CSS wins
  const els = document.querySelectorAll(".wgw");
  els.forEach((el) => mount(el));
  // Runners that take a while to start (Python takes seconds) begin as soon as the page is idle.
  for (const el of els) {
    const runner = el.dataset.widget && runners[el.dataset.widget.split(/[?#]/)[0].split(".").pop()];
    if (runner && runner.preload) (window.requestIdleCallback || setTimeout)(runner.preload);
  }
  // Follow theme switches, by an attribute on <html> or the system setting.
  const restyle = () => viewers.forEach((v) => v.restyle());
  new MutationObserver(restyle).observe(document.documentElement, { attributes: true, attributeFilter: ["class", "data-theme", "style"] });
  matchMedia("(prefers-color-scheme: dark)").addEventListener("change", restyle);
}

if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", autoload); else autoload();

window.WGW = { mount, Viewer, loadMesh, runners, toPatch, version: "0.2.0" };
