//! A torus whose tube a slider sets, colored by height, its scenes made natively: `cargo run -p wgw-show --example torus`.
use wgw_show::{Figure, Save};
use wgw_widget::*;

struct Torus;

impl Widget for Torus {
    fn new(_: Option<MeshData>) -> Self {
        Torus
    }
    fn setup(&self) -> Setup {
        Setup::new().slider("r", Slider::new(0.1, 0.9, 0.4))
    }
    fn scene(&self, s: &State) -> Scene {
        const N: usize = 60;
        let r = s.value("r");
        let tau = std::f32::consts::TAU;
        let vertices: Vec<[f32; 3]> = (0..N * N)
            .map(|k| {
                let (u, v) = (tau * (k / N) as f32 / N as f32, tau * (k % N) as f32 / N as f32);
                [(1.0 + r * v.cos()) * u.cos(), (1.0 + r * v.cos()) * u.sin(), r * v.sin()]
            })
            .collect();
        let at = |i: usize, j: usize| ((i % N) * N + j % N) as u32;
        let faces: Vec<[u32; 3]> = (0..N)
            .flat_map(|i| (0..N).flat_map(move |j| [[at(i, j), at(i + 1, j), at(i + 1, j + 1)], [at(i, j), at(i + 1, j + 1), at(i, j + 1)]]))
            .collect();
        let heights: Vec<f32> = vertices.iter().map(|p| p[2]).collect();
        Scene::new().layer("mesh", Mesh::new(&vertices, &faces).values(heights).range(-0.5, 0.5))
    }
}

fn main() -> std::io::Result<()> {
    if std::env::var_os("WGW_SAVE").is_some() {
        // Headless: a still with a thinner tube, and a turning video.
        Figure::widget(Torus).save("torus-rust.png", &Save::default().control("r", 0.2))?;
        return Figure::widget(Torus).save("torus-rust.webm", &Save::default().size(400.0, 300.0).seconds(1.0));
    }
    Figure::widget(Torus).title("torus from Rust").show()
}
