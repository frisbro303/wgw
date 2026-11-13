//! wgw's core: a scene of named layers (meshes, points, curves, vector fields) drawn with WebGL2, an orbit
//! camera, GPU picking, and frame-by-frame animation. The page side (js/) handles the DOM, input events and
//! widgets, and talks to this through `Viewer`.

#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))] // natively, only the parsers are built and tested

// The scene itself (arrays, layers, mesh files, the camera, colors and lighting) is wgw-scene's, shared with the
// SVG stills; this crate draws it with WebGL, so it's built for WebAssembly only.
#[cfg(target_arch = "wasm32")]
use wgw_scene::{array, camera};

#[cfg(target_arch = "wasm32")]
mod io;
#[cfg(target_arch = "wasm32")]
mod gl;
#[cfg(target_arch = "wasm32")]
mod layer;
#[cfg(target_arch = "wasm32")]
mod lines;
#[cfg(target_arch = "wasm32")]
mod mesh;
#[cfg(target_arch = "wasm32")]
mod points;
#[cfg(target_arch = "wasm32")]
mod shaders;
#[cfg(target_arch = "wasm32")]
mod vectors;
#[cfg(target_arch = "wasm32")]
mod viewer;

