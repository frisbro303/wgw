//! A torus colored by its Gaussian curvature; the sliders reshape it. Build it with
//! `cargo wgw -p torus`, which writes examples/torus.wgw.

use std::f32::consts::TAU;
use wgw_widget::*;

const N: usize = 96; // the mesh's resolution, around each way

struct Torus;

impl Widget for Torus {
    fn new(_mesh: Option<MeshData>) -> Self {
        Torus
    }

    fn setup(&self) -> Setup {
        Setup::new()
            .slider("R", Slider::new(0.5, 2.0, 1.0))
            .slider("r", Slider::new(0.1, 0.9, 0.4))
            .grid_toggle(false) // a checkbox for readers
    }

    fn scene(&self, state: &State) -> Scene {
        let (big, small) = (state.value("R"), state.value("r"));
        let (mut vertices, mut values) = (Vec::new(), Vec::new());
        let bound = 1.0 / (small * (big - small)).max(1e-6); // the curvature's largest size
        for i in 0..N {
            for j in 0..N {
                let (u, v) = (TAU * i as f32 / N as f32, TAU * j as f32 / N as f32);
                let ring = big + small * v.cos();
                vertices.push([ring * u.cos(), ring * u.sin(), small * v.sin()]);
                let curvature = v.cos() / (small * ring); // K = cos v / (r (R + r cos v))
                values.push(0.5 + 0.5 * curvature / bound);
            }
        }
        let at = |i: usize, j: usize| ((i % N) * N + j % N) as u32;
        let faces = (0..N).flat_map(|i| (0..N).flat_map(move |j| {
            [[at(i, j), at(i + 1, j), at(i + 1, j + 1)], [at(i, j), at(i + 1, j + 1), at(i, j + 1)]]
        })).collect();
        Scene::new().layer("torus", Mesh::new(vertices, faces).values(values))
    }
}

export!(Torus);
