// wgw-window carries wgw's page code and core (dist/, built by build.sh), so they must exist first.
fn main() {
    let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../dist");
    println!("cargo:rerun-if-changed={}", dist.display());
    if !dist.join("wgw.js").exists() || !dist.join("core/wgw_core_bg.wasm").exists() {
        panic!("\n\nwgw-window carries wgw's page code, from dist/: run ./build.sh in wgw's repository first\n\n");
    }
}
