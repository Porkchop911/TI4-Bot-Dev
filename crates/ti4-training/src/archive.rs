//! Checkpoint persistence: save and resume training runs (M10-020).
//!
//! Ported from the oracle's `atomic_write_json` and checkpoint format in
//! `tools/train_stage1_policy_gradient.py`.
//!
//! # Design
//!
//! A checkpoint captures the full state of a training run so it can be resumed later — whether
//! because the process crashed, because the operator wants to inspect intermediate results, or
//! because the run needs to span multiple invocations.
//!
//! **Atomic writes.** Every save writes to a `.tmp` file first, then renames it to the target.
//! If the process dies mid-write, the old checkpoint (or nothing) remains on disk; no partial
//! file is ever left at the final path. Recovery from an interrupted temp file is handled by
//! `load`, which ignores stale `.tmp` files.
//!
//! **Schema version.** The checkpoint carries a `schema` field that is checked on load. A
//! mismatched schema is a hard error, not a silent misinterpretation.
//!
//! # Checkpoint format (schema 1)
//!
//! ```json
//! {
//!   "schema": 1,
//!   "trainer": "teacher_free_stage2_policy_gradient",
//!   "stage": 2,
//!   "horizon": {"rounds": 4, "steps": 500000},
//!   "arguments": {"seed": "0", "games": "100", ...},
//!   "resumed_from": null,
//!   "final_update": 42,
//!   "run_complete": false,
//!   "profiles": {"sol": <Profile>, "ath": <Profile>, ...},
//!   "accepted": {"sol": <Profile>, "ath": <Profile>, ...},
//!   "history": [...],
//!   "training_telemetry": [...],
//!   "checkpoint_archive": {}
//! }
//! ```
//!
//! `profiles` are the active learner profiles (updated every generation). `accepted` are the
//! champion profiles (updated only on promotion). `history` and `training_telemetry` are retained
//! for inspection; `checkpoint_archive` holds historical artifacts.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ti4_policy::learned::Profile;

use crate::reward::Stage;
use crate::rollout::Horizon;

/// The schema version this module reads and writes.
pub const SCHEMA: u32 = 1;

/// A training checkpoint: the full state needed to resume a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Schema version for forward/backward compatibility.
    pub schema: u32,
    /// Identifies the trainer (e.g. `"teacher_free_stage2_policy_gradient"`).
    pub trainer: String,
    /// Which stage's returns are being optimised.
    pub stage: Stage,
    /// How far rollouts run for this stage.
    pub horizon: Horizon,
    /// Command-line / configuration arguments, serialized as strings.
    pub arguments: BTreeMap<String, String>,
    /// The checkpoint this run was resumed from, if any.
    pub resumed_from: Option<String>,
    /// The final update (generation) index recorded.
    pub final_update: usize,
    /// Whether the run completed its full schedule.
    pub run_complete: bool,
    /// Active learner profiles, keyed by faction name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub profiles: BTreeMap<String, Profile>,
    /// Champion (accepted) profiles, keyed by faction name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub accepted: BTreeMap<String, Profile>,
    /// Training history entries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<serde_json::Value>,
    /// Recent telemetry rows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub training_telemetry: Vec<serde_json::Value>,
    /// Historical artifacts (old checkpoints, snapshots, etc.).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub checkpoint_archive: BTreeMap<String, String>,
    /// Audit metrics for the accepted profiles.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audit_metrics: Option<serde_json::Value>,
    /// Audit metrics for the learner profiles.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub learner_audit_metrics: Option<serde_json::Value>,
}

impl Checkpoint {
    /// Create a fresh checkpoint for a new run.
    #[must_use]
    pub fn new(
        trainer: String,
        stage: Stage,
        horizon: Horizon,
        arguments: BTreeMap<String, String>,
    ) -> Self {
        Self {
            schema: SCHEMA,
            trainer,
            stage,
            horizon,
            arguments,
            resumed_from: None,
            final_update: 0,
            run_complete: false,
            profiles: BTreeMap::new(),
            accepted: BTreeMap::new(),
            history: Vec::new(),
            training_telemetry: Vec::new(),
            checkpoint_archive: BTreeMap::new(),
            audit_metrics: None,
            learner_audit_metrics: None,
        }
    }

    /// Create a checkpoint resumed from another checkpoint.
    #[must_use]
    pub fn resumed(from: &Checkpoint) -> Self {
        let mut args = from.arguments.clone();
        args.insert("resume".to_string(), from.trainer.clone());
        Self {
            schema: SCHEMA,
            trainer: from.trainer.clone(),
            stage: from.stage,
            horizon: from.horizon,
            arguments: args,
            resumed_from: Some(from.trainer.clone()),
            final_update: from.final_update,
            run_complete: false,
            profiles: BTreeMap::new(),
            accepted: BTreeMap::new(),
            history: from.history.clone(),
            training_telemetry: Vec::new(),
            checkpoint_archive: from.checkpoint_archive.clone(),
            audit_metrics: None,
            learner_audit_metrics: None,
        }
    }

    /// Validate schema version.
    ///
    /// # Errors
    /// Returns an error if the schema does not match `SCHEMA`.
    pub fn validate_schema(&self) -> Result<(), CheckpointError> {
        if self.schema != SCHEMA {
            return Err(CheckpointError::SchemaMismatch {
                found: self.schema,
                expected: SCHEMA,
            });
        }
        Ok(())
    }

    /// Compute a checksum over the serialized checkpoint bytes.
    ///
    /// # Errors
    /// Never — serialisation is infallible for this struct.
    pub fn checksum(&self) -> Result<String, CheckpointError> {
        let bytes = serde_json::to_vec(self).map_err(CheckpointError::Serialize)?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        Ok(format!("{:x}", hasher.finalize()))
    }

    /// Mark this checkpoint as the completed final output.
    #[must_use]
    pub fn mark_complete(mut self) -> Self {
        self.run_complete = true;
        self
    }
}

/// Checkpoint I/O errors.
#[derive(Debug, thiserror::Error)]
pub enum CheckpointError {
    #[error("schema mismatch: found {found}, expected {expected}")]
    SchemaMismatch { found: u32, expected: u32 },
    #[error("serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("deserialization failed: {0}")]
    Deserialize(serde_json::Error),
    #[error("file I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("checksum mismatch: expected {expected}, got {found}")]
    ChecksumMismatch { expected: String, found: String },
    #[error("interrupted temp file at {path}")]
    InterruptedTemp { path: PathBuf },
}

/// Persistent archive: save and load checkpoints.
///
/// # Thread safety
/// `Archive` itself is not `Sync` because `save` and `load` take `&self` and perform file I/O.
/// Callers should serialize access (e.g. one writer at a time). This is intentional: checkpoint
/// writes are infrequent and the cost of a mutex here is negligible compared to the I/O.
pub struct Archive {
    /// The base path where checkpoints are stored.
    path: PathBuf,
}

impl Default for Archive {
    fn default() -> Self {
        Self::new()
    }
}

impl Archive {
    /// Create an archive at the default path (`.worktrees/training/checkpoints/`).
    #[must_use]
    pub fn new() -> Self {
        Self {
            path: PathBuf::from(".worktrees/training/checkpoints/"),
        }
    }

    /// Create an archive at a specific directory.
    #[must_use]
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    /// Save a checkpoint atomically to `path`.
    ///
    /// Writes to `<path>.tmp` first, then renames. If the process dies mid-write, the old
    /// checkpoint (or nothing) remains; no partial file is left at the final path.
    ///
    /// # Errors
    /// I/O errors, serialization errors, or checksum verification failures.
    pub fn save(&self, checkpoint: &Checkpoint, path: &Path) -> Result<(), CheckpointError> {
        // Validate schema before writing.
        checkpoint.validate_schema()?;

        // Ensure the target directory exists.
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Serialize the checkpoint.
        let json = serde_json::to_string_pretty(checkpoint)?;

        // Compute checksum before writing.
        let checksum = checkpoint.checksum()?;

        // Write to a temp file.
        let tmp_path = path.with_extension("tmp");
        fs::write(&tmp_path, &json)?;

        // Write the checksum to a companion file (so it survives if the temp file is moved).
        let cksum_path = path.with_extension("sha256");
        fs::write(&cksum_path, &checksum)?;

        // Atomic rename.
        fs::rename(&tmp_path, path)?;

        Ok(())
    }

    /// Load a checkpoint from `path`.
    ///
    /// If the file exists but is a temp file (left by a crashed writer), returns
    /// `CheckpointError::InterruptedTemp`.
    ///
    /// # Errors
    /// I/O errors, deserialization errors, schema mismatch, or checksum verification failures.
    pub fn load(&self, path: &Path) -> Result<Checkpoint, CheckpointError> {
        if !path.exists() {
            return Err(CheckpointError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("checkpoint not found: {}", path.display()),
            )));
        }

        // Check for an interrupted temp file.
        let tmp_path = path.with_extension("tmp");
        if tmp_path.exists() {
            return Err(CheckpointError::InterruptedTemp { path: tmp_path });
        }

        // Read the JSON.
        let json = fs::read_to_string(path)?;
        let checkpoint: Checkpoint =
            serde_json::from_str(&json).map_err(CheckpointError::Deserialize)?;

        // Validate schema.
        checkpoint.validate_schema()?;

        // Verify checksum if available.
        let cksum_path = path.with_extension("sha256");
        if cksum_path.exists() {
            let expected = fs::read_to_string(&cksum_path)?.trim().to_string();
            let found = checkpoint.checksum()?;
            if expected != found {
                return Err(CheckpointError::ChecksumMismatch { expected, found });
            }
        }

        Ok(checkpoint)
    }

    /// Check whether a checkpoint exists at `path`.
    pub fn exists(&self, path: &Path) -> bool {
        path.exists() && !path.with_extension("tmp").exists()
    }

    /// List all checkpoint files in the archive directory.
    ///
    /// # Errors
    /// I/O errors reading the archive directory.
    pub fn list(&self) -> Result<Vec<PathBuf>, CheckpointError> {
        let mut result = Vec::new();
        if self.path.exists() {
            for entry in fs::read_dir(&self.path)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file()
                    && path.extension().is_some_and(|ext| ext == "json")
                    && !path.with_extension("tmp").exists()
                {
                    result.push(path);
                }
            }
        }
        result.sort();
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use ti4_policy::learned::blank_profile;

    fn temp_checkpoint() -> (Checkpoint, PathBuf) {
        let mut args = BTreeMap::new();
        args.insert("seed".to_string(), "0".to_string());
        args.insert("games".to_string(), "10".to_string());

        let checkpoint = Checkpoint::new(
            "test_stage2".to_string(),
            Stage::Two,
            Horizon::short(),
            args,
        );
        let path = PathBuf::from(".worktrees/training/test_checkpoint.json");
        (checkpoint, path)
    }

    #[test]
    fn a_checkpoint_has_schema_one() {
        let (cp, _) = temp_checkpoint();
        assert_eq!(cp.schema, 1);
    }

    #[test]
    fn a_checkpoint_validates_its_own_schema() {
        let (cp, _) = temp_checkpoint();
        assert!(cp.validate_schema().is_ok());
    }

    #[test]
    fn a_checkpoint_with_wrong_schema_fails_validation() {
        let mut cp = temp_checkpoint().0;
        cp.schema = 99;
        assert!(cp.validate_schema().is_err());
    }

    #[test]
    fn a_checkpoint_round_trips_through_json() {
        let (mut cp, path) = temp_checkpoint();

        // Add some profiles to test serialization of the profile maps.
        let mut profiles = BTreeMap::new();
        profiles.insert("sol".to_string(), blank_profile("sol", 512));
        profiles.insert("ath".to_string(), blank_profile("ath", 512));
        cp.profiles = profiles;

        let archive = Archive::new();
        archive.save(&cp, &path).expect("save should succeed");
        let loaded = archive.load(&path).expect("load should succeed");

        // Schema must match.
        assert_eq!(loaded.schema, cp.schema);
        assert_eq!(loaded.trainer, cp.trainer);
        assert_eq!(loaded.stage, cp.stage);
        assert_eq!(loaded.horizon, cp.horizon);
        assert_eq!(loaded.final_update, cp.final_update);
        assert_eq!(loaded.run_complete, cp.run_complete);
        assert_eq!(loaded.profiles.len(), cp.profiles.len());

        // Clean up.
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(path.with_extension("sha256"));
    }

    #[test]
    fn a_resume_checkpoint_carrys_the_source_trainer() {
        let (cp, _path) = temp_checkpoint();
        let resumed = Checkpoint::resumed(&cp);
        assert_eq!(resumed.resumed_from, Some("test_stage2".to_string()));
        assert_eq!(resumed.final_update, 0);
        assert!(!resumed.run_complete);
    }

    #[test]
    fn mark_complete_sets_run_complete() {
        let (cp, _path) = temp_checkpoint();
        let completed = cp.mark_complete();
        assert!(completed.run_complete);
    }

    #[test]
    fn loading_a_nonexistent_file_returns_not_found() {
        let archive = Archive::new();
        let result = archive.load(Path::new(".worktrees/training/nonexistent.json"));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CheckpointError::Io(_)));
    }

    #[test]
    fn an_interrupted_temp_file_prevents_load() {
        let (cp, path) = temp_checkpoint();
        let archive = Archive::new();
        archive.save(&cp, &path).expect("save should succeed");

        // Create a temp file alongside the checkpoint.
        let tmp_path = path.with_extension("tmp");
        fs::write(&tmp_path, "interrupted").expect("write temp");

        let result = archive.load(&path);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            CheckpointError::InterruptedTemp { .. }
        ));

        // Clean up.
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&tmp_path);
        let _ = fs::remove_file(path.with_extension("sha256"));
    }

    #[test]
    fn checksum_is_deterministic() {
        let (mut cp1, _) = temp_checkpoint();
        let (mut cp2, _) = temp_checkpoint();
        cp1.final_update = 42;
        cp2.final_update = 42;

        let cs1 = cp1.checksum().expect("checksum should succeed");
        let cs2 = cp2.checksum().expect("checksum should succeed");
        assert_eq!(
            cs1, cs2,
            "identical checkpoints should have identical checksums"
        );
    }

    #[test]
    fn checksum_changes_when_checkpoint_changes() {
        let (mut cp1, _) = temp_checkpoint();
        let (mut cp2, _) = temp_checkpoint();
        cp1.final_update = 42;
        cp2.final_update = 43;

        let cs1 = cp1.checksum().expect("checksum should succeed");
        let cs2 = cp2.checksum().expect("checksum should succeed");
        assert_ne!(
            cs1, cs2,
            "different checkpoints should have different checksums"
        );
    }
}
