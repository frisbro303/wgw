// The page's colors, read from CSS custom properties on a viewer, as flat RGBA in [0, 1] for the core.

let probe = null;

// Any CSS color (hex, rgb(), oklch(), names, ...) as [r, g, b, a] in [0, 1], via a 1-pixel canvas.
export function rgba(css) {
  if (!probe) {
    const canvas = Object.assign(document.createElement("canvas"), { width: 1, height: 1 });
    probe = canvas.getContext("2d", { willReadFrequently: true });
  }
  probe.clearRect(0, 0, 1, 1);
  probe.fillStyle = "#000";
  probe.fillStyle = css;
  probe.fillRect(0, 0, 1, 1);
  return Array.from(probe.getImageData(0, 0, 1, 1).data, (c) => c / 255);
}

export function rgbaList(colors) {
  return Float32Array.from(colors.flatMap(rgba));
}

// Split a CSS list on the commas that aren't inside rgb(...) and the like.
function split(text) {
  const out = [];
  let depth = 0, current = "";
  for (const ch of text) {
    if (ch === "(") depth++;
    if (ch === ")") depth--;
    if (ch === "," && !depth) { out.push(current.trim()); current = ""; } else current += ch;
  }
  if (current.trim()) out.push(current.trim());
  return out;
}

// --wgw-colors: the colors values run through; --wgw-accent: points, curves and arrows without values
// (the last of the colors by default); --wgw-edge: mesh edges.
export function readStyle(el) {
  const style = getComputedStyle(el), get = (name) => style.getPropertyValue(name).trim();
  const colors = split(get("--wgw-colors") || "#d8d6cf, #b8402f");
  return {
    colormap: rgbaList(colors),
    accent: Float32Array.from(rgba(get("--wgw-accent") || colors[colors.length - 1])),
    edge: Float32Array.from(rgba(get("--wgw-edge") || "rgba(0, 0, 0, 0.3)")),
  };
}
