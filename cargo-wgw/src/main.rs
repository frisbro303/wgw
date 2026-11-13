//! `cargo wgw`: build the widget crates in a package or workspace (a `cdylib` depending on `wgw-widget`) for
//! `wasm32-unknown-unknown`, in release, and write each one's `.wgw` file. Where it goes is `out`, relative to the
//! crate, from the crate's `[package.metadata.wgw]` or else the workspace's `[workspace.metadata.wgw]`, with `{crate}`
//! replaced by the crate's name and `{dir}` by its folder's; without either, `target/wgw/<crate>.wgw`.
//!
//!     cargo wgw [build] [-p <crate>]... [--manifest-path <Cargo.toml>]

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{exit, Command};

const TARGET: &str = "wasm32-unknown-unknown";
const USAGE: &str = "usage: cargo wgw [build] [-p <crate>]... [--manifest-path <Cargo.toml>]";

fn fail(message: &str) -> ! {
    eprintln!("cargo-wgw: {message}");
    exit(1)
}

fn main() {
    // cargo runs `cargo-wgw wgw <args>`; run directly, it's `cargo-wgw <args>`.
    let mut args = std::env::args().skip(1).peekable();
    if args.peek().map(String::as_str) == Some("wgw") {
        args.next();
    }
    if args.peek().map(String::as_str) == Some("build") {
        args.next();
    }
    let (mut packages, mut manifest) = (Vec::new(), None);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-p" | "--package" => packages.push(args.next().unwrap_or_else(|| fail(USAGE))),
            "--manifest-path" => manifest = Some(args.next().unwrap_or_else(|| fail(USAGE))),
            "-h" | "--help" => {
                println!("{USAGE}");
                return;
            }
            _ => fail(&format!("unexpected argument '{arg}'\n{USAGE}")),
        }
    }
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let manifest_args: Vec<String> = manifest.into_iter().flat_map(|m| ["--manifest-path".into(), m]).collect();

    let output = Command::new(&cargo)
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .args(&manifest_args)
        .output()
        .unwrap_or_else(|e| fail(&format!("running cargo metadata: {e}")));
    if !output.status.success() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        exit(1);
    }
    let metadata: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| fail(&e.to_string()));
    let members: Vec<&Value> = metadata["workspace_members"].as_array().unwrap().iter().collect();
    let target_dir = PathBuf::from(metadata["target_directory"].as_str().unwrap());
    let workspace_out = metadata["metadata"]["wgw"]["out"].as_str();

    // The widgets: (package, its library's name, where its .wgw goes).
    let mut widgets = Vec::new();
    for package in metadata["packages"].as_array().unwrap() {
        let name = package["name"].as_str().unwrap();
        if !members.contains(&&package["id"]) || (!packages.is_empty() && !packages.iter().any(|p| p == name)) {
            continue;
        }
        let lib = package["targets"].as_array().unwrap().iter().find(|t| {
            t["crate_types"].as_array().unwrap().iter().any(|k| k == "cdylib")
        });
        let uses_wgw = package["dependencies"].as_array().unwrap().iter().any(|d| d["name"] == "wgw-widget");
        let (Some(lib), true) = (lib, uses_wgw) else {
            if packages.iter().any(|p| p == name) {
                fail(&format!("{name} isn't a widget: it needs crate-type = [\"cdylib\"] and wgw-widget"));
            }
            continue;
        };
        let dir = Path::new(package["manifest_path"].as_str().unwrap()).parent().unwrap();
        let out = match package["metadata"]["wgw"]["out"].as_str().or(workspace_out) {
            Some(out) => {
                let folder = dir.file_name().unwrap().to_string_lossy();
                dir.join(out.replace("{crate}", name).replace("{dir}", &folder))
            }
            None => target_dir.join("wgw").join(format!("{name}.wgw")),
        };
        widgets.push((name.to_string(), lib["name"].as_str().unwrap().replace('-', "_"), out));
    }
    if widgets.is_empty() {
        fail("no widget crates here: a widget is a cdylib depending on wgw-widget");
    }

    let status = Command::new(&cargo)
        .args(["build", "--release", "--target", TARGET])
        .args(&manifest_args)
        .args(widgets.iter().flat_map(|(name, ..)| ["-p", name.as_str()]))
        .status()
        .unwrap_or_else(|e| fail(&format!("running cargo build: {e}")));
    if !status.success() {
        exit(status.code().unwrap_or(1));
    }

    let cwd = std::env::current_dir().unwrap();
    for (_, lib, out) in &widgets {
        let built = target_dir.join(TARGET).join("release").join(format!("{lib}.wasm"));
        let out = normalize(out);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).unwrap_or_else(|e| fail(&format!("{}: {e}", parent.display())));
        }
        std::fs::copy(&built, &out).unwrap_or_else(|e| fail(&format!("{} -> {}: {e}", built.display(), out.display())));
        let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
        println!("{}: {} KB", out.strip_prefix(&cwd).unwrap_or(&out).display(), size / 1024);
    }
}

/// `a/b/../c` as `a/c`, so the paths printed are short.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::ParentDir if out.file_name().is_some() => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            _ => out.push(part),
        }
    }
    out
}
