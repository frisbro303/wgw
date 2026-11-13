//! wgw's core for the page: wgw-render's renderer drawing with WebGL2 (through glow) on a figure's canvas, and
//! mesh files parsed for the page's widgets. The page side (js/) handles the DOM, input events and widgets, and
//! talks to this through `Viewer`.

#[cfg(target_arch = "wasm32")]
mod io;
#[cfg(target_arch = "wasm32")]
mod viewer;
#[cfg(target_arch = "wasm32")]
mod webgl;
