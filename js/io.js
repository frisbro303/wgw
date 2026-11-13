// Fetching a figure's file, and parsing a mesh file (.obj, or a .wgw file of arrays) in the core.

import { parse_mesh } from "./core/wgw_core.js";

export async function fetchBytes(url) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  return new Uint8Array(await res.arrayBuffer());
}

// A mesh file's bytes, parsed (the core's MeshFile; free() it when done): .wgw files by their first bytes, anything
// else as .obj.
export function parseMesh(bytes, url) {
  const wgw = bytes.length >= 4 && String.fromCharCode(...bytes.subarray(0, 4)) === "WGW1";
  return parse_mesh(bytes, wgw ? "wgw" : "obj");
}
