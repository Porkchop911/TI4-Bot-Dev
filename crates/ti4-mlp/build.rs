//! Stage the pinned libtorch DLLs beside the test and example binaries.
//!
//! Windows resolves a dependent DLL from the executable's own directory before it consults `PATH`.
//! Without this, every `cargo test` needs `%LIBTORCH%\lib` exported first, and the workspace gate
//! stops being runnable from a clean checkout — the first load attempt in M09-025 failed exactly
//! that way, with `STATUS_DLL_NOT_FOUND`.
//!
//! Hard links, not copies: `out/` and `target/` sit on one volume, so the 266 MB of libraries cost
//! no additional disk. A copy is the fallback when the link cannot be made (different volume, or a
//! filesystem without links).
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=LIBTORCH");
    let Ok(libtorch) = std::env::var("LIBTORCH") else {
        // Nothing to stage. `torch-sys` will produce the real diagnostic.
        return;
    };
    let lib = Path::new(&libtorch).join("lib");
    let Ok(entries) = std::fs::read_dir(&lib) else {
        return;
    };
    // OUT_DIR is <target>/<profile>/build/<pkg>-<hash>/out; the binaries land three levels up.
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    let Some(profile_dir) = out.ancestors().nth(3) else {
        return;
    };
    let deps = profile_dir.join("deps");
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "dll") {
            continue;
        }
        let Some(name) = path.file_name() else {
            continue;
        };
        for directory in [profile_dir, deps.as_path()] {
            let target = directory.join(name);
            if target.exists() {
                continue;
            }
            let _ = std::fs::create_dir_all(directory);
            if std::fs::hard_link(&path, &target).is_err() {
                let _ = std::fs::copy(&path, &target);
            }
        }
    }
}
