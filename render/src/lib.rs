//! wgw's renderer: a scene of layers (meshes, points, curves, vector fields) in panes, with orbit cameras, picking
//! and animation, drawn through the `Gl` trait with wgw's shaders (written in WGSL, ../shaders). The page draws
//! it with WebGL; the still draws it on the CPU, running the same shaders.

pub mod gl;
pub mod layer;
mod lines;
mod mesh;
mod points;
pub mod renderer;
pub mod shaders;
mod vectors;

pub use renderer::Renderer;
pub use wgw_scene as scene;
