//! The Rust head against tests/heads/expected.json: `cargo run -p wgw-show --example fixture -- <out dir>`.
use wgw_show::{Figure, Save};
use wgw_widget::*;

fn main() -> std::io::Result<()> {
    let out = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    let scene = || {
        Scene::new()
            .layer("m", Mesh::new([[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], [[0, 1, 2]]).values(vec![0.0, 0.5, 1.0]).color("red"))
            .layer("p", Points::new([[0.0, 0.0, 1.0]]).size(4.0))
    };
    std::fs::write(out.join("rust.patch"), scene().to_bytes())?;
    Figure::scene(scene()).save(out.join("rust.png"), &Save::default().size(100.0, 80.0).scale(1.0))?;
    assert!(std::fs::read(out.join("rust.png"))?.starts_with(b"\x89PNG"));
    Ok(())
}
