//! What's derived from a layer's arrays before drawing: a mesh's normals and edges, a curve's segments.

use glam::Vec3;

use crate::layer::LayerData;

/// Area-weighted unit normals at each vertex (n * 3), from the faces' winding.
pub fn vertex_normals(p: &[f32], faces: &[u32]) -> Vec<f32> {
    let mut normals = vec![0.0f32; p.len()];
    let at = |v: u32| Vec3::new(p[3 * v as usize], p[3 * v as usize + 1], p[3 * v as usize + 2]);
    for f in faces.chunks_exact(3) {
        let n = (at(f[1]) - at(f[0])).cross(at(f[2]) - at(f[0]));
        for &v in f {
            let v = 3 * v as usize;
            normals[v] += n.x;
            normals[v + 1] += n.y;
            normals[v + 2] += n.z;
        }
    }
    for n in normals.chunks_exact_mut(3) {
        let v = Vec3::new(n[0], n[1], n[2]).normalize_or_zero();
        n.copy_from_slice(&v.to_array());
    }
    normals
}

/// Each edge of the faces once, as vertex pairs (the smaller index first), flattened.
pub fn unique_edges(faces: &[u32]) -> Vec<u32> {
    let mut edges: Vec<(u32, u32)> = faces
        .chunks_exact(3)
        .flat_map(|f| [(f[0], f[1]), (f[1], f[2]), (f[2], f[0])])
        .map(|(a, b)| (a.min(b), a.max(b)))
        .collect();
    edges.sort_unstable();
    edges.dedup();
    edges.into_iter().flat_map(|(a, b)| [a, b]).collect()
}

/// A lines layer's segments over its n vertices: its `segments`, or each vertex to the next (and the last back to
/// the first, when `closed`).
pub fn segments(data: &LayerData, n: usize) -> Result<Vec<[u32; 2]>, String> {
    let segments: Vec<[u32; 2]> = match data.arrays.get("segments").and_then(|a| a.u32s()) {
        Some(s) => s.chunks_exact(2).map(|s| [s[0], s[1]]).collect(),
        None => {
            let mut s: Vec<[u32; 2]> = (1..n as u32).map(|i| [i - 1, i]).collect();
            if data.flag("closed", false) && n > 2 {
                s.push([n as u32 - 1, 0]);
            }
            s
        }
    };
    if segments.iter().flatten().any(|&i| i as usize >= n) {
        return Err(format!("segments must be indices below {n}"));
    }
    Ok(segments)
}
