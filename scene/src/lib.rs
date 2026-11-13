//! wgw's scene, apart from any renderer: named arrays, layers and their parameters, mesh files, the orbit camera
//! and how a scene is framed. wgw-render draws it, with WebGL on a page or on the CPU for stills (wgw-still).

pub mod array;
pub mod camera;
pub mod geometry;
pub mod layer;
pub mod mesh_file;
pub mod patch;

/// A color: red, green, blue and alpha, each in [0, 1].
pub type Rgba = [f32; 4];
