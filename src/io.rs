//! Mesh files for the page: parsed by wgw-scene, handed to JavaScript as named arrays of raw bytes.

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct MeshFile(pub(crate) wgw_scene::mesh_file::MeshFile);

#[wasm_bindgen]
impl MeshFile {
    pub fn names(&self) -> Vec<String> {
        self.0.names()
    }

    pub fn dtype(&self, name: &str) -> Option<String> {
        self.0.dtype(name)
    }

    pub fn shape(&self, name: &str) -> Option<Vec<u32>> {
        self.0.shape(name)
    }

    /// The array's raw bytes, for the page to view as a typed array of its dtype.
    pub fn bytes(&self, name: &str) -> Option<Vec<u8>> {
        self.0.bytes(name).map(|b| b.to_vec())
    }
}

/// Parse a mesh file's bytes; `format` is its extension, "obj" or "wgw".
#[wasm_bindgen]
pub fn parse_mesh(bytes: &[u8], format: &str) -> Result<MeshFile, JsValue> {
    wgw_scene::mesh_file::parse(bytes, format).map(MeshFile).map_err(|e| JsValue::from_str(&e))
}
