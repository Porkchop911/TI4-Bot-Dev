//! F-M09-026-11: the smoke's input gates, as automated regressions rather than recorded runs.
//!
//! Each case drives the real example binary and requires it to refuse *before* game setup.

use std::process::Command;

/// The example binary. Cargo exports `CARGO_BIN_EXE_*` for bins but not for examples, so it is
/// located next to the test binary, where `cargo test` has just built it.
fn smoke() -> Command {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for profile in ["debug", "release"] {
        for name in ["mlp_smoke.exe", "mlp_smoke"] {
            let exe = root
                .join("target")
                .join(profile)
                .join("examples")
                .join(name);
            if exe.exists() {
                return Command::new(exe);
            }
        }
    }
    panic!("mlp_smoke example is not built; run cargo build --example mlp_smoke");
}

#[test]
fn a_vocabulary_that_is_not_the_accepted_generation_is_refused() {
    let scratch = std::env::temp_dir().join(format!("ti4-smoke-slots-{}", std::process::id()));
    std::fs::write(&scratch, "{\"oov_registry_version\":2}").expect("write");
    let output = smoke()
        .args(["--slots", scratch.to_str().expect("path")])
        .output()
        .expect("the smoke runs");
    let _ = std::fs::remove_file(&scratch);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "stderr: {stderr}");
    assert!(
        stderr.contains("not the accepted vocabulary generation"),
        "stderr: {stderr}"
    );
}

#[test]
fn a_pool_outside_the_allowed_roles_is_refused() {
    let output = smoke()
        .args(["--map-pool", "out/pools/full_np8_12_final.json"])
        .current_dir(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .output()
        .expect("the smoke runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "stderr: {stderr}");
    assert!(stderr.contains("not an allowed pool"), "stderr: {stderr}");
}
