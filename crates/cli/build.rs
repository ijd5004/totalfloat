//! Compiles the web app into the binary when `ui/dist/` has been built
//! (`cargo xtask ui`), so a release is one file (ARCHITECTURE.md, Local app). Without it, the
//! build still works and `serve` shows a page saying how to build the app.
//!
//! Each file becomes an `include_bytes!` in `$OUT_DIR/ui_assets.rs`, sorted by
//! path, so the build is deterministic and needs no dependency.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn main() {
    // Windows gives the main thread 1 MiB of stack, and a debug build of
    // `main` needs more, so give the binary the 8 MiB Linux has.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        match std::env::var("CARGO_CFG_TARGET_ENV").as_deref() {
            Ok("msvc") => println!("cargo::rustc-link-arg-bins=/STACK:8388608"),
            Ok("gnu") => println!("cargo::rustc-link-arg-bins=-Wl,--stack,8388608"),
            _ => {}
        }
    }
    println!("cargo::rustc-check-cfg=cfg(totalfloat_ui)");
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default());
    let ui = manifest.join("../../ui");
    let dist = ui.join("dist");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_default());
    if dist.join("index.html").is_file() {
        // Rebuild when the built app changes.
        println!("cargo::rerun-if-changed={}", dist.display());
        let mut files = Vec::new();
        walk(&dist, &dist, &mut files);
        files.sort();
        let mut code = String::from(
            "/// The built app's files: path, bytes and content type.\n\
             pub static FILES: &[(&str, &[u8], &str)] = &[\n",
        );
        for (path, absolute) in &files {
            let _ = writeln!(
                code,
                "    ({path:?}, include_bytes!({absolute:?}), {:?}),",
                content_type(path)
            );
        }
        code.push_str("];\n");
        write(&out.join("ui_assets.rs"), &code);
        println!("cargo::rustc-cfg=totalfloat_ui");
    } else {
        // A missing path would rerun this, and rebuild the crate, every
        // build, and watching all of ui/ would scan node_modules. So watch
        // one tracked file, which `cargo xtask ui` touches after a build.
        println!(
            "cargo::rerun-if-changed={}",
            ui.join("package.json").display()
        );
    }
}

fn write(path: &Path, code: &str) {
    if std::fs::read_to_string(path).ok().as_deref() != Some(code) {
        std::fs::write(path, code)
            .unwrap_or_else(|e| panic!("can't write {}: {e}", path.display()));
    }
}

/// Every file under `dir`, as its path from `root` with `/` separators and
/// its absolute path.
fn walk(root: &Path, dir: &Path, files: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() {
            // A link could reach a file outside the build, which would then be
            // served to anyone; the build makes none, so leave them out.
            println!(
                "cargo::warning=left out {}: a symbolic link",
                path.display()
            );
        } else if kind.is_dir() {
            walk(root, &path, files);
        } else if let Ok(relative) = path.strip_prefix(root) {
            let name = relative
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            let absolute = path.canonicalize().unwrap_or(path);
            files.push((name, absolute.to_string_lossy().into_owned()));
        }
    }
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, ext)| ext) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json" | "map") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("wasm") => "application/wasm",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}
