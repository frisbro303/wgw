// Mesh files: fetched here, parsed by the core (src/io.rs), which reads .obj and .wgw.

import { parse_mesh } from "./core/wgw_core.js";

const TYPES = { f4: Float32Array, f8: Float64Array, f2: Uint16Array, u4: Uint32Array, i4: Int32Array, u2: Uint16Array, u1: Uint8Array };

// A parsed mesh file (the core's MeshFile); free() it when done.
export async function loadMesh(url) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  const format = url.split(/[?#]/)[0].split(".").pop().toLowerCase();
  return parse_mesh(new Uint8Array(await res.arrayBuffer()), format);
}

// A parsed file's arrays, {name: {data, shape, dtype}}, for a widget. float16 arrays stay raw bits (a Uint16Array):
// they are only handed on to widgets.
export function meshArrays(file) {
  const arrays = {};
  for (const name of file.names()) {
    const bytes = file.bytes(name), dtype = file.dtype(name), Type = TYPES[dtype];
    arrays[name] = { data: new Type(bytes.buffer, bytes.byteOffset, bytes.byteLength / Type.BYTES_PER_ELEMENT), shape: Array.from(file.shape(name)), dtype };
  }
  return arrays;
}
