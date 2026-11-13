//! wgw's scene, apart from any renderer: named arrays, layers and their parameters, mesh files, the orbit camera,
//! how a scene is framed, and how it's colored and lit. The WebGL viewer (the `wgw` crate) draws it on a page; the
//! still (`wgw-still`) draws it as SVG for a PDF. Both use this, so a figure looks the same in either.

pub mod array;
pub mod camera;
pub mod geometry;
pub mod layer;
pub mod light;
pub mod mesh_file;

/// A color: red, green, blue and alpha, each in [0, 1].
pub type Rgba = [f32; 4];
