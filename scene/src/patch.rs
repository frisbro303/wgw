//! Patches: what a widget returns, as bytes. A patch changes a scene's layers by name: `None` removes a layer;
//! otherwise the layer becomes exactly what's given, its parameters replaced, the arrays in `remove` dropped and
//! those in `arrays` set (arrays not mentioned stay as they were, such as the figure's mesh's vertices when a
//! widget only recolors it).
//!
//! On the wire it's the container .wgw mesh files use: "WGW1", a little-endian u32 header length, a JSON header,
//! then the arrays' bytes, each at a 4-byte-aligned offset, so a page can view them as typed arrays in place:
//!
//! ```text
//! {"layers": [[name, null | {"type", "params": {...}, "remove": [...], "arrays": {key: {"dtype", "shape", "offset"}}}], ...]}
//! ```

use serde_json::{json, Map, Value};

use crate::array::{Data, NdArray};

#[derive(Clone, Debug, Default)]
pub struct LayerPatch {
    /// "mesh", "points", "lines" or "vectors".
    pub kind: String,
    /// Numbers, booleans, CSS colors (strings) and lists of them.
    pub params: Map<String, Value>,
    pub remove: Vec<String>,
    pub arrays: Vec<(String, NdArray)>,
}

#[derive(Clone, Debug, Default)]
pub struct Patch {
    pub layers: Vec<(String, Option<LayerPatch>)>,
}

/// The patch as bytes.
pub fn encode(patch: &Patch) -> Vec<u8> {
    let mut data: Vec<u8> = Vec::new();
    let mut layers = Vec::new();
    for (name, layer) in &patch.layers {
        let Some(l) = layer else {
            layers.push(json!([name, null]));
            continue;
        };
        let mut arrays = Map::new();
        for (key, a) in &l.arrays {
            let (dtype, bytes): (&str, &[u8]) = match &a.data {
                Data::F32(v) => ("f4", bytemuck::cast_slice(v)),
                Data::U32(v) => ("u4", bytemuck::cast_slice(v)),
            };
            arrays.insert(key.clone(), json!({ "dtype": dtype, "shape": a.shape, "offset": data.len() }));
            data.extend_from_slice(bytes);
        }
        layers.push(json!([name, { "type": l.kind, "params": l.params, "remove": l.remove, "arrays": arrays }]));
    }
    let mut header = json!({ "layers": layers }).to_string().into_bytes();
    while (8 + header.len()) % 4 != 0 {
        header.push(b' '); // the arrays start 4-byte aligned
    }
    let mut out = Vec::with_capacity(8 + header.len() + data.len());
    out.extend_from_slice(b"WGW1");
    out.extend_from_slice(&(header.len() as u32).to_le_bytes());
    out.extend_from_slice(&header);
    out.extend_from_slice(&data);
    out
}

/// A patch from its bytes.
pub fn decode(bytes: &[u8]) -> Result<Patch, String> {
    if bytes.len() < 8 || &bytes[..4] != b"WGW1" {
        return Err("not a wgw patch".into());
    }
    let length = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let start = 8 + length;
    let header: Value = serde_json::from_slice(bytes.get(8..start).ok_or("the patch's header is cut short")?)
        .map_err(|e| format!("the patch's header: {e}"))?;
    let data = &bytes[start..];
    let mut patch = Patch::default();
    for entry in header["layers"].as_array().ok_or("a patch needs layers")? {
        let name = entry[0].as_str().ok_or("a layer needs a name")?.to_string();
        let l = &entry[1];
        if l.is_null() {
            patch.layers.push((name, None));
            continue;
        }
        let mut arrays = Vec::new();
        for (key, a) in l["arrays"].as_object().into_iter().flatten() {
            let shape: Vec<usize> = a["shape"].as_array().into_iter().flatten().filter_map(|s| s.as_u64().map(|s| s as usize)).collect();
            let n: usize = shape.iter().product();
            let offset = a["offset"].as_u64().unwrap_or(0) as usize;
            let raw = data.get(offset..offset + 4 * n).ok_or_else(|| format!("{name}.{key}: the patch is cut short"))?;
            let words = raw.chunks_exact(4).map(|w| w.try_into().unwrap());
            let array = match a["dtype"].as_str() {
                Some("u4") => Data::U32(words.map(u32::from_le_bytes).collect()),
                Some("f4") => Data::F32(words.map(f32::from_le_bytes).collect()),
                other => return Err(format!("{name}.{key}: unsupported dtype {other:?}")),
            };
            arrays.push((key.clone(), NdArray::new(array, shape)?));
        }
        patch.layers.push((name, Some(LayerPatch {
            kind: l["type"].as_str().unwrap_or("").to_string(),
            params: l["params"].as_object().cloned().unwrap_or_default(),
            remove: l["remove"].as_array().into_iter().flatten().filter_map(|r| r.as_str().map(String::from)).collect(),
            arrays,
        })));
    }
    Ok(patch)
}

/// Frames' patches as one: each array gets a leading frames axis (faces and segments, the same in every frame, are
/// taken from the first), as the page does (stackFrames in js/widget.js). A layer only frame 0 has is the same in every frame.
pub fn stack(mut patches: Vec<Patch>) -> Patch {
    if patches.len() == 1 {
        return patches.pop().unwrap();
    }
    let mut out = Patch::default();
    for (name, layer) in &patches[0].layers {
        let Some(layer) = layer else {
            out.layers.push((name.clone(), None));
            continue;
        };
        let later = |p: &Patch| p.layers.iter().find(|(n, _)| n == name).and_then(|(_, l)| l.clone());
        if later(&patches[1]).is_none() {
            out.layers.push((name.clone(), Some(layer.clone()))); // unchanging: sent once
            continue;
        }
        let mut stacked = layer.clone();
        for (key, array) in &mut stacked.arrays {
            if key == "faces" || key == "segments" {
                continue;
            }
            let mut data = Vec::new();
            for p in &patches {
                if let Some(Data::F32(v)) = later(p).and_then(|l| l.arrays.into_iter().find(|(k, _)| k == key)).map(|(_, a)| a.data) {
                    data.extend(v);
                }
            }
            let shape = [vec![patches.len()], array.shape.clone()].concat();
            if let Ok(a) = NdArray::new(Data::F32(data), shape) {
                *array = a;
            }
        }
        out.layers.push((name.clone(), Some(stacked)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let layer = LayerPatch {
            kind: "lines".into(),
            params: json!({ "width": 2, "color": "var(--link)" }).as_object().unwrap().clone(),
            remove: vec!["values".into()],
            arrays: vec![
                ("vertices".into(), NdArray::new(Data::F32(vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0]), vec![2, 3]).unwrap()),
                ("segments".into(), NdArray::new(Data::U32(vec![0, 1]), vec![1, 2]).unwrap()),
            ],
        };
        let patch = Patch { layers: vec![("curve".into(), Some(layer)), ("gone".into(), None)] };
        let bytes = encode(&patch);
        let back = decode(&bytes).unwrap();
        assert_eq!(back.layers.len(), 2);
        let l = back.layers[0].1.as_ref().unwrap();
        assert_eq!(l.kind, "lines");
        let array = |k: &str| &l.arrays.iter().find(|(key, _)| key == k).unwrap().1;
        assert_eq!(array("vertices").shape, vec![2, 3]);
        assert!(matches!(&array("segments").data, Data::U32(v) if v == &vec![0, 1]));
        assert!(back.layers[1].1.is_none());
    }
}
