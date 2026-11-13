// Mesh files as named arrays, {name: {data, shape, dtype}}: .wgw (written by python/wgw.py) or JSON.

const TYPES = { f4: Float32Array, f8: Float64Array, f2: Uint16Array, u4: Uint32Array, i4: Int32Array, u2: Uint16Array, u1: Uint8Array };

export async function loadMesh(url) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  return /\.json($|\?)/.test(url) ? fromJSON(await res.json()) : fromWGW(await res.arrayBuffer());
}

// float16 arrays stay raw bits (a Uint16Array): they are only handed on to widgets.
export function fromWGW(buffer) {
  if (new TextDecoder().decode(new Uint8Array(buffer, 0, 4)) !== "WGW1") throw new Error("not a .wgw file");
  const length = new DataView(buffer).getUint32(4, true), start = 8 + length;
  const header = JSON.parse(new TextDecoder().decode(new Uint8Array(buffer, 8, length))).arrays;
  const arrays = {};
  for (const [name, a] of Object.entries(header)) {
    const size = a.shape.reduce((x, y) => x * y, 1);
    arrays[name] = { data: new TYPES[a.dtype](buffer, start + a.offset, size), shape: a.shape, dtype: a.dtype };
  }
  return arrays;
}

export function fromJSON(json) {
  const arrays = {};
  for (const [name, value] of Object.entries(json)) {
    if (!Array.isArray(value)) continue;
    const shape = [];
    for (let level = value; Array.isArray(level); level = level[0]) shape.push(level.length);
    const faces = name === "faces";
    arrays[name] = { data: (faces ? Uint32Array : Float32Array).from(value.flat(Infinity)), shape, dtype: faces ? "u4" : "f4" };
  }
  return arrays;
}
