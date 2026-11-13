// wgw: interactive meshes, points, curves and vector fields, drawn by a Rust core compiled to WebAssembly and
// driven by widgets: small WebAssembly modules (see widget.js), written in Rust with wgw-widget.
//
// Any <div class="wgw"> becomes a viewer. Its data attributes:
//   data-src="spot.wgw"        the figure: a widget (a .wgw file holding a WebAssembly module, with any data it
//                              needs inside), or a mesh (.obj, or a .wgw file of arrays) shown as the layer "mesh"
//   data-up="z"                the axis that points up: x, y or z
//   data-view="-60,20"         the starting camera: turn and tilt, in degrees
//   data-pick="click"          when a pick reaches the widget: click, hover or none
//   data-flat                  flat shading for the "mesh" layer
//   data-credit="false"        no "wgw" link in the corner
// CSS custom properties style it: --wgw-colors (the colors values run through), --wgw-accent (points,
// curves and arrows without values), --wgw-edge, --wgw-height.
//
// From JS: WGW.mount(element, options) returns the viewer: viewer.on("pick", fn), viewer.apply(patch),
// viewer.reset().

import init, { Viewer as Core } from "./core/wgw_core.js";
import { fetchBytes, parseMesh } from "./io.js";
import { animates, controlInputs } from "./patch.js";
import { isWidget, loadWidget } from "./widget.js";
import { readStyle, rgbaList } from "./style.js";

let wasm = null;
const loadCore = () => (wasm ||= init());  // only on pages with figures
const DPR_MAX = 2;
const REDUCED_MOTION = matchMedia("(prefers-reduced-motion: reduce)");

const STYLE = `
.wgw{position:relative}
.wgw canvas{display:block;width:100%;height:var(--wgw-height,420px);touch-action:none;cursor:grab;outline:none}
.wgw canvas.dragging{cursor:grabbing}
.wgw-status{position:absolute;left:0;right:0;top:0;height:var(--wgw-height,420px);display:flex;align-items:center;justify-content:center;padding:0 24px;text-align:center;font:12px ui-monospace,monospace;opacity:.6;pointer-events:none;white-space:pre-line}
.wgw-status:empty{display:none}
.wgw-controls{display:flex;flex-wrap:wrap;justify-content:center;gap:6px 18px;font:13px ui-monospace,monospace}
.wgw-controls:empty{display:none}
.wgw canvas,.wgw-controls{transition:opacity .3s}
.wgw.wgw-loading canvas,.wgw.wgw-loading .wgw-controls{opacity:0;pointer-events:none}
.wgw-credit{position:absolute;top:8px;right:10px;display:inline-flex;align-items:center;gap:4px;font:11px ui-monospace,monospace;color:inherit;opacity:.45;text-decoration:none}
.wgw .wgw-credit img{width:12px;height:12px;background:none;padding:0;border:0;box-shadow:none;transform:translateY(.15em)}
.wgw-credit:hover{opacity:.85;text-decoration:underline}
.wgw-controls label{display:inline-flex;align-items:center;gap:8px;cursor:pointer;user-select:none}
.wgw-controls output{font-variant-numeric:tabular-nums;text-align:right}
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
// A slider's value with a fixed number of decimals, so it keeps its length: on a log slider, as many as its minimum
// needs (0.001 ... 0.020 ... 1.000); on a linear one, as many as its step.
const formatter = (c) => {
  const smallest = c.log ? 10 ** c.min : c.step || 1;
  const decimals = Math.min(6, Math.max(0, -Math.floor(Math.log10(smallest) + 1e-9)));
  return (x) => (+x).toFixed(decimals);
};
// "turn,tilt" in degrees, then optionally a scale (2: twice as large); a typographic minus (as Typst writes) reads as
// one, and anything unreadable as the default.
const parseView = (text) => {
  const view = (text || "").split(",").map((x) => Number(x.trim().replace("\u2212", "-")));
  return (view.length === 2 || view.length === 3) && view.every(Number.isFinite) ? view : [-60, 20];
};
// How far along a slider is, for the filled part of its track.
const fill = (input) => input.style.setProperty("--fill", `${((input.value - input.min) / (input.max - input.min)) * 100}%`);

export class Viewer {
  constructor(el, options = {}) {
    const d = el.dataset;
    this.el = el;
    this.options = {
      src: options.src ?? d.src,
      up: options.up ?? d.up ?? "z",
      view: options.view || parseView(d.view),
      pick: options.pick ?? d.pick ?? "click",
      fps: +(options.fps ?? d.fps ?? 30),
      flat: options.flat ?? flag(d.flat),
      credit: options.credit ?? d.credit !== "false",
    };
    el.wgw = this;
    el.classList.add("wgw", "wgw-loading");  // nothing shows until the mesh, the widget and its first results are in

    this.canvas = document.createElement("canvas");
    this.canvas.setAttribute("role", "img");
    this.canvas.setAttribute("aria-label", d.label || "Interactive 3D figure: drag to turn, pinch or ctrl+scroll to zoom");
    this.statusEl = Object.assign(document.createElement("div"), { className: "wgw-status", textContent: "loading…" });
    this.controlsEl = Object.assign(document.createElement("div"), { className: "wgw-controls" });
    el.append(this.canvas, this.statusEl, this.controlsEl);
    if (this.options.credit) {  // a small link to wgw; data-credit="false" (or CSS) hides it
      const credit = Object.assign(document.createElement("a"), {
        className: "wgw-credit", href: "https://github.com/frisbro303/wgw", target: "_blank", rel: "noopener",
      });
      const logo = Object.assign(document.createElement("img"), { src: new URL("logo.svg", import.meta.url).href, alt: "", width: 12, height: 12 });
      credit.append(logo, "wgw");
      el.append(credit);
    }

    this.controls = {};
    this.widget = null;
    this.picks = [];  // the widget's picks so far (only the latest, unless it keeps them)
    this.lastPick = null;
    this.themed = {};  // layer colors that use CSS variables, by layer and parameter, re-read when the theme changes
    this.visible = false;
    this.ready = this.boot();
  }

  async boot() {
    try {
      await loadCore();
      this.core = new Core(this.canvas);
    } catch (err) {
      return this.fail(err);
    }
    this.core.set_options(this.options.up, this.options.view[0], this.options.view[1], this.options.view[2] ?? 1, REDUCED_MOTION.matches);
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
      if (o.src) {
        const bytes = await fetchBytes(o.src);
        if (isWidget(bytes)) {
          await this.startWidget(bytes);
          return;
        }
        // A mesh: parsed and shown by the core.
        const file = parseMesh(bytes, o.src);
        try {
          this.core.set_mesh_file("mesh", file);
          if (o.flat) this.core.set_number("mesh", "flat", 1);
        } finally {
          file.free();
        }
      }
      this.reveal();
    } catch (err) {
      this.fail(err);
    }
  }

  on(type, fn) {
    this.el.addEventListener("wgw:" + type, (e) => fn(e.detail));
    return this;
  }

  status(text) { this.statusEl.textContent = text || ""; }

  // Show the figure: once everything has loaded, or on an error, so it's seen.
  reveal() {
    if (!this.el.classList.contains("wgw-loading")) return;
    this.requestRender();
    requestAnimationFrame(() => requestAnimationFrame(() => {  // after the first frame is drawn
      this.el.classList.remove("wgw-loading");
      if (!this.statusEl.textContent.startsWith("error")) this.status("");
    }));
  }

  fail(err) {
    this.reveal();
    console.error(err);
    const lines = String((err && err.message) || err).trim().split("\n");
    this.status("error: " + lines[lines.length - 1]);
  }

  restyle() {
    if (!this.core) return;
    const s = readStyle(this.el);
    this.core.set_style(s.colormap, s.accent, s.edge);
    // Layer colors naming the page's CSS variables, such as "var(--link)", follow its theme.
    for (const [name, keys] of Object.entries(this.themed)) {
      for (const [key, value] of Object.entries(keys)) this.core.set_colors(name, key, rgbaList(this.resolve(value)));
    }
    this.requestRender();
  }

  // Colors as given, with var(--name) read from the figure's CSS.
  resolve(value) {
    const style = getComputedStyle(this.el);
    return [].concat(value).map((c) => c.replace(/var\((--[\w-]+)\)/g, (_, name) => style.getPropertyValue(name).trim()));
  }

  // ---- the scene

  // Apply a patch (see patch.js): add, change or remove layers, then play any frames it brought.
  apply(patch, animate) {
    if (!patch) return;
    const core = this.core;
    for (const [name, layer] of Object.entries(patch.layers || {})) {
      delete this.themed[name];
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
        else if (typeof value === "string" || Array.isArray(value)) {
          if ([].concat(value).some((c) => c.includes("var("))) (this.themed[name] ??= {})[key] = value;
          core.set_colors(name, key, rgbaList(this.resolve(value)));
        }
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
    this.lastPick = { layer, index, point: [x, y, z] };
    this.el.dispatchEvent(new CustomEvent("wgw:pick", { detail: { layer, index, point: [x, y, z] } }));
    if (!this.widget) return;
    const pick = { vertex: index, layer, point: [x, y, z], frame: this.core.current_frame() };  // where, and when
    this.picks = this.keepPicks ? [...this.picks, pick] : [pick];
    this.update(!hovering);
  }

  // ---- widgets

  async startWidget(bytes) {
    this.widget = await loadWidget(bytes);
    const info = this.widget.setup;
    this.frames = Math.max(1, info.frames || 1);
    this.fps = info.fps;
    this.loop = info.loop;
    this.keepPicks = !!info.keepPicks;
    const toggle = info.grid && typeof info.grid === "object";
    this.core.set_grid(toggle ? !!info.grid.toggle : !!info.grid);
    if (info.panes) this.core.set_pane_widths(Float32Array.from(info.panes));
    if (info.picked != null) this.picks = [{ vertex: info.picked, layer: "mesh", point: null, frame: 0 }];  // its starting pick
    this.buildControls(controlInputs({ ...info.controls, ...(toggle ? { grid: !!info.grid.toggle } : {}) }));
    this.revealWhenIdle = true;  // once its first scene is drawn
    this.update(true);
  }

  buildControls(spec) {
    for (const [name, c] of Object.entries(spec)) {
      const label = document.createElement("label"), input = document.createElement("input"), output = document.createElement("output");
      const actual = (x) => (c.log ? 10 ** x : x);  // what the widget gets
      const show = formatter(c);
      // "grid" is the viewer's: a checkbox for the triangle grid, not passed to the widget.
      const viewer = name === "grid" && c.type === "checkbox";
      if (viewer) this.core.set_grid(c.value);
      else this.controls[name] = actual(c.value);
      if (c.type === "checkbox") {
        Object.assign(input, { type: "checkbox", checked: c.value });
        label.append(input, name);
      } else {
        Object.assign(input, { type: "range", min: c.min, max: c.max, step: c.step, value: c.value });
        output.textContent = show(actual(c.value));
        output.style.minWidth = `${Math.max(show(actual(c.min)).length, show(actual(c.max)).length)}ch`;
        fill(input);
        label.append(name, input, output);
      }
      input.addEventListener("input", () => {
        const value = input.type === "checkbox" ? input.checked : actual(+input.value);
        if (viewer) { this.core.set_grid(value); this.requestRender(); return; }
        this.controls[name] = value;
        if (input.type !== "checkbox") { output.textContent = show(value); fill(input); }
        this.update(false);
      });
      this.controlsEl.appendChild(label);
    }
  }

  // Ask the widget for its scene in the current state (every frame of an animation, stacked), at most one at a
  // time: while it's busy, newer requests collapse into one, so dragging a slider never builds up a backlog.
  update(animate) {
    this.wanted = { animate: animate || this.wanted?.animate };
    if (!this.busy) this.flush();
  }

  async flush() {
    if (!this.wanted || !this.widget) return;
    const { animate } = this.wanted;
    this.wanted = null;
    this.busy = true;
    try {
      const patch = await this.widget.scene({ controls: { ...this.controls }, picks: this.picks }, this.frames);
      this.apply(patch, animate);
    } catch (err) {
      this.fail(err);
    }
    this.busy = false;
    if (this.revealWhenIdle && !this.wanted) this.reveal();
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
  // Follow theme switches: an attribute on <html> or <body> (data-md-color-scheme is MkDocs Material's), or the
  // system setting.
  const restyle = () => viewers.forEach((v) => v.restyle());
  const watched = { attributes: true, attributeFilter: ["class", "data-theme", "style", "data-md-color-scheme"] };
  for (const el of [document.documentElement, document.body]) new MutationObserver(restyle).observe(el, watched);
  matchMedia("(prefers-color-scheme: dark)").addEventListener("change", restyle);
}

if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", autoload); else autoload();

window.WGW = { mount, Viewer, loadWidget, version: "0.1.0" };
