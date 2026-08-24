//! Durable artifact manifest and data-role enforcement (MLP plan §10).
//!
//! The five corpus artifacts this project measures against are identified by full sha256, not by
//! path: a file that parses but is not the expected artifact must fail closed. Pools carry a
//! logical role — `train`, `validation` (the seed-777 holdout, despite its filename), or
//! `final` (seed 20260822, sealed). Training and measurement commands may only consume
//! train/validation pools; final-role data is loaded exactly once by M10-038 after models and
//! analysis are frozen. Checkpoints are known identities for future teacher-checksum rejection.
//!
//! The human-readable record with recipes and provenance is `plans/evidence/MLP-ARTIFACTS.md`;
//! this table is the machine-readable form that code enforces at use time.

use std::fs;
use std::path::Path;

use sha2::{Digest, Sha256};

/// The logical role of a corpus artifact (MLP plan §10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactRole {
    /// Training data.
    Train,
    /// Validation data — the seed-777 holdout pool has already informed architecture and
    /// thresholds, so its logical role is validation despite the `holdout` filename.
    Validation,
    /// Sealed final data (seed 20260822). Only M10-038 may load it, once, after models and
    /// analysis are frozen.
    Final,
}

impl ArtifactRole {
    /// The stable name used in reports and errors.
    pub const fn label(self) -> &'static str {
        match self {
            ArtifactRole::Train => "train",
            ArtifactRole::Validation => "validation",
            ArtifactRole::Final => "final",
        }
    }
}

impl std::fmt::Display for ArtifactRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// One durable-manifest entry: display name, full sha256, and role (`None` = checkpoint).
const MANIFEST: &[(&str, &str, Option<ArtifactRole>)] = &[
    (
        "out/pools/full_np8_12_train.json",
        "106153d4384435b19bd27d7210140b4b46da84c72d7e5ce704ffc52083f2c6df",
        Some(ArtifactRole::Train),
    ),
    (
        "out/pools/full_np8_12_holdout.json (validation role)",
        "aba33c81aa04cefb15857b8ed1d40173f6f3de5e9b6e9633a6855c1d5a4c27e5",
        Some(ArtifactRole::Validation),
    ),
    (
        "out/pools/full_np8_12_final.json",
        "693253ecbcb33ac61c416110836286242be39271ecf49381a99c90acca653245",
        Some(ArtifactRole::Final),
    ),
    (
        "out/stage2_r6/final10000.json",
        "be792a2a207ced25d589162d875bae4fb1f320c8e5637045486db6a24ce5b55b",
        None,
    ),
    (
        "out/stage1_hacanclone/frozen5000.json",
        "0d0fa9e5d7a3f9ce848ef2c52a4a4144183af7ca5c15082850874a18c039ca4a",
        None,
    ),
];

/// The role of the pool with this sha256, if it is a known pool.
pub fn pool_role(sha256_hex: &str) -> Option<ArtifactRole> {
    MANIFEST
        .iter()
        .find(|entry| entry.1 == sha256_hex)
        .and_then(|entry| entry.2)
}

/// Whether this sha256 is a known baseline checkpoint (for teacher-checksum rejection).
pub fn is_known_checkpoint(sha256_hex: &str) -> bool {
    MANIFEST
        .iter()
        .any(|entry| entry.1 == sha256_hex && entry.2.is_none())
}

/// Why an artifact cannot be used at a given call site.
#[derive(Debug, thiserror::Error)]
pub enum ArtifactError {
    #[error("io error reading {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("artifact with sha256 {found} is not in the durable manifest — refusing to proceed")]
    UnknownArtifact { found: String },
    #[error("artifact role {found:?} is not allowed here (allowed roles: {allowed:?})")]
    RoleViolation {
        found: ArtifactRole,
        allowed: Vec<ArtifactRole>,
    },
}

/// Verify that `path` is a known pool whose role is in `allowed`, hashing the exact bytes read.
///
/// Fails closed on unknown artifacts and disallowed roles (MLP plan §10): these are fail-closed
/// tests, not operator conventions.
///
/// # Errors
/// I/O errors reading the file, [`ArtifactError::UnknownArtifact`] when the bytes are not in the
/// durable manifest, or [`ArtifactError::RoleViolation`] when the pool's role is not allowed.
pub fn verify_pool_role(path: &Path, allowed: &[ArtifactRole]) -> Result<(), ArtifactError> {
    let bytes = fs::read(path).map_err(|source| ArtifactError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let digest = hex(&Sha256::digest(&bytes));
    match pool_role(&digest) {
        None => Err(ArtifactError::UnknownArtifact { found: digest }),
        Some(role) if allowed.contains(&role) => Ok(()),
        Some(role) => Err(ArtifactError::RoleViolation {
            found: role,
            allowed: allowed.to_vec(),
        }),
    }
}

fn hex(digest: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRAIN_SHA: &str = "106153d4384435b19bd27d7210140b4b46da84c72d7e5ce704ffc52083f2c6df";
    const VALIDATION_SHA: &str = "aba33c81aa04cefb15857b8ed1d40173f6f3de5e9b6e9633a6855c1d5a4c27e5";
    const FINAL_SHA: &str = "693253ecbcb33ac61c416110836286242be39271ecf49381a99c90acca653245";
    const R6_CHECKPOINT_SHA: &str =
        "be792a2a207ced25d589162d875bae4fb1f320c8e5637045486db6a24ce5b55b";
    const STAGE1_CHECKPOINT_SHA: &str =
        "0d0fa9e5d7a3f9ce848ef2c52a4a4144183af7ca5c15082850874a18c039ca4a";

    #[test]
    fn manifest_roles_are_as_recorded() {
        assert_eq!(pool_role(TRAIN_SHA), Some(ArtifactRole::Train));
        // The seed-777 holdout's logical role is validation despite its filename.
        assert_eq!(pool_role(VALIDATION_SHA), Some(ArtifactRole::Validation));
        assert_eq!(pool_role(FINAL_SHA), Some(ArtifactRole::Final));
    }

    #[test]
    fn final_role_is_refused_by_train_and_validation_checks() {
        let allowed = [ArtifactRole::Train, ArtifactRole::Validation];
        let role = pool_role(FINAL_SHA).expect("final pool is in the manifest");
        assert!(
            !allowed.contains(&role),
            "final data must never pass a train/validation gate"
        );
    }

    #[test]
    fn checkpoints_are_known_identities_but_not_pools() {
        assert!(is_known_checkpoint(R6_CHECKPOINT_SHA));
        assert!(is_known_checkpoint(STAGE1_CHECKPOINT_SHA));
        // A checkpoint is not a pool: role lookups must not match it.
        assert_eq!(pool_role(R6_CHECKPOINT_SHA), None);
        assert_eq!(pool_role(STAGE1_CHECKPOINT_SHA), None);
    }

    #[test]
    fn unknown_bytes_fail_closed() {
        let dir = std::env::temp_dir().join("ti4-sim-artifacts-unknown-test");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("not-a-known-pool.json");
        // Arbitrary bytes: parseable or not is irrelevant — the identity is unknown.
        fs::write(&path, b"not a known pool").unwrap();
        let error = verify_pool_role(&path, &[ArtifactRole::Train, ArtifactRole::Validation])
            .expect_err("unknown artifact bytes must be refused");
        assert!(
            matches!(error, ArtifactError::UnknownArtifact { .. }),
            "{error}"
        );
    }

    /// The sealed fixtures (M09-020) must extract back to the exact raw checkpoint bytes:
    /// compressed sha256 matches the committed fixture manifest, and extracted bytes match
    /// the durable-manifest checkpoint hashes. Checksums verified at use time (§10).
    #[test]
    fn sealed_fixtures_extract_to_the_recorded_checkpoints() {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixtures = repo_root.join("fixtures/mlp-baselines");
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(fixtures.join("manifest.json")).unwrap_or_else(
                |error| panic!("fixture manifest is missing from the repository: {error}"),
            ))
            .expect("fixture manifest parses");
        let entries = manifest["entries"]
            .as_array()
            .expect("the fixture manifest has entries");
        assert_eq!(
            entries.len(),
            2,
            "exactly the two baseline checkpoints are sealed"
        );

        for entry in entries {
            let name = entry["fixture"]
                .as_str()
                .expect("a fixture path")
                .to_owned();
            let expected_raw_sha = entry["raw_sha256"]
                .as_str()
                .expect("a raw sha256")
                .to_owned();
            let expected_compressed_sha = entry["compressed_sha256"]
                .as_str()
                .expect("a compressed sha256")
                .to_owned();

            // The sealed bytes are what the manifest says they are.
            // The manifest's `fixture` field is a path relative to the repository root.
            let fixture_path = repo_root.join(&name);
            let sealed = fs::read(&fixture_path).unwrap_or_else(|error| {
                panic!("sealed fixture {name} is missing from the repository: {error}")
            });
            assert_eq!(
                hex(&Sha256::digest(&sealed)),
                expected_compressed_sha,
                "{name}: sealed bytes do not match the recorded compressed hash"
            );

            // Extraction yields exactly the raw checkpoint the durable manifest identifies.
            let raw = zstd::decode_all(sealed.as_slice())
                .unwrap_or_else(|error| panic!("{name}: extraction failed: {error}"));
            assert_eq!(
                hex(&Sha256::digest(&raw)),
                expected_raw_sha,
                "{name}: extracted bytes are not the recorded checkpoint"
            );
            // And that raw identity is a known durable-manifest checkpoint.
            assert!(
                is_known_checkpoint(&expected_raw_sha),
                "{name}: the manifest names an unknown checkpoint identity"
            );
        }
    }

    #[test]
    fn role_violation_error_names_roles() {
        let error = ArtifactError::RoleViolation {
            found: ArtifactRole::Final,
            allowed: vec![ArtifactRole::Train, ArtifactRole::Validation],
        };
        let message = error.to_string();
        assert!(message.contains("Final"), "{message}");
        assert!(message.contains("Train"), "{message}");
        assert!(message.contains("Validation"), "{message}");
    }
}
