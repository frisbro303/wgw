//! Mesh files, parsed in the core: Wavefront .obj, and .wgw (named arrays: a JSON header, then the raw arrays;
//! see python/wgw.py). Either becomes a `MeshFile` of named arrays, which the page shows as a layer and hands to
//! its widget.

use std::collections::HashMap;

use serde::Deserialize;
use wasm_bindgen::prelude::*;

/// One array of a mesh file: its dtype ("f4", "u4", ...), shape, and raw little-endian bytes.
struct Entry {
    dtype: String,
    shape: Vec<usize>,
    bytes: Vec<u8>,
}

#[wasm_bindgen]
pub struct MeshFile {
    names: Vec<String>,
    arrays: HashMap<String, Entry>,
}

#[wasm_bindgen]
impl MeshFile {
    pub fn names(&self) -> Vec<String> {
        self.names.clone()
    }

    pub fn dtype(&self, name: &str) -> Option<String> {
        self.arrays.get(name).map(|e| e.dtype.clone())
    }

    pub fn shape(&self, name: &str) -> Option<Vec<u32>> {
        self.arrays.get(name).map(|e| e.shape.iter().map(|&s| s as u32).collect())
    }

    /// The array's raw bytes, for the page to view as a typed array of its dtype.
    pub fn bytes(&self, name: &str) -> Option<Vec<u8>> {
        self.arrays.get(name).map(|e| e.bytes.clone())
    }
}

impl MeshFile {
    fn new() -> Self {
        MeshFile { names: Vec::new(), arrays: HashMap::new() }
    }

    fn push(&mut self, name: &str, dtype: &str, shape: Vec<usize>, bytes: Vec<u8>) {
        self.names.push(name.into());
        self.arrays.insert(name.into(), Entry { dtype: dtype.into(), shape, bytes });
    }

    /// The vertices as float32 (n * 3), whatever their stored type.
    pub fn vertices(&self) -> Result<(Vec<f32>, Vec<usize>), String> {
        let e = self.arrays.get("vertices").ok_or("the mesh file has no vertices")?;
        let data: Vec<f32> = match e.dtype.as_str() {
            "f4" => read(&e.bytes, f32::from_le_bytes),
            "f8" => read(&e.bytes, f64::from_le_bytes).into_iter().map(|x| x as f32).collect(),
            other => return Err(format!("vertices must be float32 or float64, not {other}")),
        };
        Ok((data, e.shape.clone()))
    }

    /// The faces as uint32 (m * 3), whatever integer type they're stored as.
    pub fn faces(&self) -> Result<(Vec<u32>, Vec<usize>), String> {
        let e = self.arrays.get("faces").ok_or("the mesh file has no faces")?;
        let data: Vec<u32> = match e.dtype.as_str() {
            "u4" => read(&e.bytes, u32::from_le_bytes),
            "i4" => read(&e.bytes, i32::from_le_bytes).into_iter().map(|x| x as u32).collect(),
            "u2" => read(&e.bytes, u16::from_le_bytes).into_iter().map(u32::from).collect(),
            other => return Err(format!("faces must be integers, not {other}")),
        };
        Ok((data, e.shape.clone()))
    }
}

/// Little-endian numbers of N bytes each.
fn read<T, const N: usize>(bytes: &[u8], from: fn([u8; N]) -> T) -> Vec<T> {
    bytes.chunks_exact(N).map(|c| from(c.try_into().unwrap())).collect()
}

/// Parse a mesh file's bytes; `format` is its extension, "obj" or "wgw".
#[wasm_bindgen]
pub fn parse_mesh(bytes: &[u8], format: &str) -> Result<MeshFile, JsValue> {
    match format {
        "obj" => parse_obj(bytes),
        "wgw" => parse_wgw(bytes),
        _ => Err(format!("can't read .{format} meshes: use .obj or .wgw")),
    }
    .map_err(|e| JsValue::from_str(&e))
}

/// Vertices and faces of a Wavefront .obj; polygons are split into triangle fans, and texture coordinates and
/// normals are ignored, so vertices are exactly the file's `v` lines, in order.
fn parse_obj(bytes: &[u8]) -> Result<MeshFile, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "the .obj file isn't text")?;
    let (mut vertices, mut faces) = (Vec::<f32>::new(), Vec::<u32>::new());
    for (number, line) in text.lines().enumerate() {
        let mut parts = line.split_ascii_whitespace();
        match parts.next() {
            Some("v") => {
                for _ in 0..3 {
                    let x = parts.next().and_then(|p| p.parse::<f32>().ok());
                    vertices.push(x.ok_or_else(|| format!("line {}: a vertex needs three numbers", number + 1))?);
                }
            }
            Some("f") => {
                let n = vertices.len() as i64 / 3;
                let corners = parts
                    .map(|p| {
                        let i: i64 = p.split('/').next().unwrap_or("").parse().map_err(|_| format!("line {}: bad face {p:?}", number + 1))?;
                        let i = if i < 0 { n + i } else { i - 1 }; // 1-based, or negative from the end
                        if i < 0 || i >= n { Err(format!("line {}: face refers to vertex {} of {n}", number + 1, i + 1)) } else { Ok(i as u32) }
                    })
                    .collect::<Result<Vec<u32>, String>>()?;
                for k in 1..corners.len().saturating_sub(1) {
                    faces.extend_from_slice(&[corners[0], corners[k], corners[k + 1]]);
                }
            }
            _ => {}
        }
    }
    let mut file = MeshFile::new();
    let (n, m) = (vertices.len() / 3, faces.len() / 3);
    file.push("vertices", "f4", vec![n, 3], bytemuck::cast_slice(&vertices).to_vec());
    file.push("faces", "u4", vec![m, 3], bytemuck::cast_slice(&faces).to_vec());
    Ok(file)
}

#[derive(Deserialize)]
struct Header {
    arrays: serde_json::Map<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct HeaderEntry {
    dtype: String,
    shape: Vec<usize>,
    offset: usize,
}

fn parse_wgw(bytes: &[u8]) -> Result<MeshFile, String> {
    if bytes.len() < 8 || &bytes[..4] != b"WGW1" {
        return Err("not a .wgw file".into());
    }
    let length = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let start = 8 + length;
    let header: Header = serde_json::from_slice(bytes.get(8..start).ok_or("the .wgw header is cut short")?)
        .map_err(|e| format!("the .wgw header: {e}"))?;
    let mut file = MeshFile::new();
    for (name, value) in header.arrays {
        let entry: HeaderEntry = serde_json::from_value(value).map_err(|e| format!("{name}: {e}"))?;
        let size = match entry.dtype.as_str() {
            "f8" => 8,
            "f4" | "u4" | "i4" => 4,
            "f2" | "u2" => 2,
            "u1" => 1,
            other => return Err(format!("{name}: unknown dtype {other}")),
        } * entry.shape.iter().product::<usize>();
        let from = start + entry.offset;
        let data = bytes.get(from..from + size).ok_or_else(|| format!("{name}: the file is cut short"))?;
        file.push(&name, &entry.dtype, entry.shape, data.to_vec());
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obj_and_wgw_agree() {
        let obj = parse_obj(&std::fs::read("examples/spot.obj").unwrap()).unwrap();
        let wgw = parse_wgw(&std::fs::read("examples/spot.wgw").unwrap()).unwrap();
        let (v1, s1) = obj.vertices().unwrap();
        let (v2, s2) = wgw.vertices().unwrap();
        assert_eq!(s1, vec![2930, 3]);
        assert_eq!(s1, s2);
        assert!(v1.iter().zip(&v2).all(|(a, b)| (a - b).abs() < 1e-6));
        let (f1, t1) = obj.faces().unwrap();
        assert_eq!(t1, vec![5856, 3]);
        assert_eq!(f1, wgw.faces().unwrap().0);
    }

    #[test]
    fn obj_polygons_and_negative_indices() {
        let quad = b"v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf -4 -3 -2 -1\n";
        let file = parse_obj(quad).unwrap();
        assert_eq!(file.faces().unwrap().0, vec![0, 1, 2, 0, 2, 3]);
        assert!(parse_obj(b"v 0 0 0\nf 1 2 3\n").is_err());
    }
}
