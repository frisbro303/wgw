//! wgw's shaders: written in WGSL (shaders/), translated to GLSL for WebGL by build.rs. Each program is a vertex
//! and a fragment shader, in pairs: a lit color pass and an id pass that writes (layer, element) for picking.

use crate::gl::Program;

/// A program as build.rs generates it: its GLSL, and how to feed it.
pub struct ShaderDef {
    pub vertex: &'static str,
    pub fragment: &'static str,
    /// Its uniform block's name in each stage that uses it.
    pub blocks: &'static [&'static str],
    /// Each uniform's name, byte offset and size in the block.
    pub uniforms: &'static [(&'static str, u32, u32)],
    /// Each texture's name in the WGSL and in the GLSL.
    pub textures: &'static [(&'static str, &'static str)],
    /// The block's size in bytes.
    pub size: u32,
}

include!(concat!(env!("OUT_DIR"), "/shaders.rs"));

pub struct Shaders {
    pub mesh: Program,
    pub mesh_ids: Program,
    pub edges: Program,
    pub points: Program,
    pub points_ids: Program,
    pub lines: Program,
    pub lines_ids: Program,
    pub vectors: Program,
    pub vectors_ids: Program,
}

impl Shaders {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        let p = |def: &'static ShaderDef| Program::new(gl, def);
        Ok(Shaders {
            mesh: p(&MESH)?,
            mesh_ids: p(&MESH_IDS)?,
            edges: p(&EDGES)?,
            points: p(&POINTS)?,
            points_ids: p(&POINTS_IDS)?,
            lines: p(&LINES)?,
            lines_ids: p(&LINES_IDS)?,
            vectors: p(&VECTORS)?,
            vectors_ids: p(&VECTORS_IDS)?,
        })
    }
}
