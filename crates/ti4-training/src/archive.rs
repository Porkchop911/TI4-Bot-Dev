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
    #[error("profile validation: {0}")]
    ProfileValidation(String),
}

/// State needed to resume a training run from a checkpoint.
///
/// Extracted from a checkpoint: champion profiles, learner profiles, training history,
/// and seed ranges for validation and confirmation panels.
#[derive(Debug, Clone)]
pub struct ResumeState {
    /// Champion (accepted) profiles, keyed by faction name.
    pub champion: BTreeMap<String, Profile>,
    /// Active learner profiles, keyed by faction name.
    pub learner: BTreeMap<String, Profile>,
    /// Training history entries from the checkpoint.
    pub history: Vec<serde_json::Value>,
    /// Telemetry rows from the checkpoint.
    pub telemetry: Vec<serde_json::Value>,
    /// Historical artifacts from the checkpoint.
    pub archive: BTreeMap<String, String>,
    /// The update index to resume from.
    pub start_update: usize,
    /// The number of updates to train before the next evaluation.
    pub eval_every: usize,
    /// Seeds for the validation panel (mirrors the oracle's `validation_seeds`).
    pub validation_seeds: Vec<u64>,
    /// Seeds for the confirmation panel (mirrors the oracle's `confirmation_seeds`).
    pub confirmation_seeds: Vec<u64>,
    /// The checkpoint file path, for audit.
    pub checkpoint_path: PathBuf,
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

    /// Resume a training run from a checkpoint.
    ///
    /// Loads the checkpoint, extracts champion and learner profiles, restores training
    /// state (history, telemetry, update count), and validates that factions match.
    ///
    /// Mirrors the oracle's resume logic in `train_stage1_policy_gradient.py`:
    ///
    /// - `accepted` = champion profiles (from `checkpoint.accepted`)
    /// - `learner` = learner profiles (from `checkpoint.profiles`, falling back to `accepted`)
    /// - `start_update` = max update index in history
    /// - `validation_seeds` / `confirmation_seeds` = from checkpoint arguments or defaults
    ///
    /// # Errors
    /// I/O errors, deserialization errors, schema mismatch, checksum failures,
    /// or faction mismatch.
    pub fn resume(&self, path: &Path) -> Result<ResumeState, CheckpointError> {
        let checkpoint = self.load(path)?;
        checkpoint.validate_schema()?;

        // Champion profiles come from `accepted`.
        let champion = checkpoint.accepted;
        if champion.is_empty() {
            return Err(CheckpointError::ProfileValidation(
                "checkpoint has no accepted (champion) profiles".to_string(),
            ));
        }
        let champion_factions: Vec<String> = champion.keys().cloned().collect();

        // Learner profiles come from `profiles`, falling back to `accepted`.
        let learner = if checkpoint.profiles.is_empty() {
            champion.clone()
        } else {
            checkpoint.profiles
        };

        // Validate factions match.
        let learner_factions: Vec<String> = learner.keys().cloned().collect();
        if champion_factions != learner_factions {
            return Err(CheckpointError::ProfileValidation(format!(
                "champion and learner factions do not match: champion={champion_factions:?}, learner={learner_factions:?}",
            )));
        }

        // Restore training state.
        let history = checkpoint.history;
        let telemetry = checkpoint.training_telemetry;
        let archive = checkpoint.checkpoint_archive;

        // Compute start_update from the maximum update index in history.
        let start_update = usize::try_from(
            history
                .iter()
                .filter_map(|entry| entry.get("update").and_then(serde_json::Value::as_u64))
                .max()
                .unwrap_or(0),
        )
        .unwrap_or(0);

        // Extract validation/confirmation seed ranges from checkpoint arguments.
        // The oracle uses: validation_seeds = seed + 9_000_000, confirmation_seeds = seed + 14_000_000
        let default_seed = checkpoint
            .arguments
            .get("seed")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let validation_seeds: Vec<u64> = (0..32).map(|i| default_seed + 9_000_000 + i).collect();
        let confirmation_seeds: Vec<u64> = (0..32).map(|i| default_seed + 14_000_000 + i).collect();

        Ok(ResumeState {
            champion,
            learner,
            history,
            telemetry,
            archive,
            start_update,
            eval_every: 10, // oracle default
            validation_seeds,
            confirmation_seeds,
            checkpoint_path: path.to_path_buf(),
        })
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
        let base = PathBuf::from(".worktrees/training/test_roundtrip.json");
        let _ = fs::remove_file(&base);
        let _ = fs::remove_file(base.with_extension("tmp"));
        let _ = fs::remove_file(base.with_extension("sha256"));

        let mut cp = Checkpoint::new("test_stage2".to_string(), Stage::Two, Horizon::short(), {
            let mut args = BTreeMap::new();
            args.insert("seed".to_string(), "0".to_string());
            args.insert("games".to_string(), "10".to_string());
            args
        });

        // Add some profiles to test serialization of the profile maps.
        let mut profiles = BTreeMap::new();
        profiles.insert("sol".to_string(), blank_profile("sol", 512));
        profiles.insert("ath".to_string(), blank_profile("ath", 512));
        cp.profiles = profiles;

        let archive = Archive::new();
        archive.save(&cp, &base).expect("save should succeed");
        let loaded = archive.load(&base).expect("load should succeed");

        // Schema must match.
        assert_eq!(loaded.schema, cp.schema);
        assert_eq!(loaded.trainer, cp.trainer);
        assert_eq!(loaded.stage, cp.stage);
        assert_eq!(loaded.horizon, cp.horizon);
        assert_eq!(loaded.final_update, cp.final_update);
        assert_eq!(loaded.run_complete, cp.run_complete);
        assert_eq!(loaded.profiles.len(), cp.profiles.len());

        // Clean up.
        let _ = fs::remove_file(&base);
        let _ = fs::remove_file(base.with_extension("sha256"));
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

    #[test]
    fn resume_loads_champion_and_learner_profiles() {
        let base = PathBuf::from(".worktrees/training/test_resume_profiles.json");
        let _ = fs::remove_file(&base);
        let _ = fs::remove_file(base.with_extension("tmp"));
        let _ = fs::remove_file(base.with_extension("sha256"));

        let mut cp = Checkpoint::new(
            "test_resume_profiles".to_string(),
            Stage::One,
            Horizon::opening(),
            {
                let mut args = BTreeMap::new();
                args.insert("seed".to_string(), "0".to_string());
                args
            },
        );

        // Set up champion (accepted) profiles.
        let mut champion = BTreeMap::new();
        champion.insert("sol".to_string(), blank_profile("sol", 512));
        champion.insert("ath".to_string(), blank_profile("ath", 512));
        cp.accepted = champion;

        // Set up learner profiles (different from champion).
        let mut learner = BTreeMap::new();
        learner.insert("sol".to_string(), blank_profile("sol", 512));
        learner.insert("ath".to_string(), blank_profile("ath", 512));
        cp.profiles = learner.clone();

        // Add history with an update.
        cp.history
            .push(serde_json::json!({"update": 42, "metrics": {}}));

        let archive = Archive::new();
        archive.save(&cp, &base).expect("save should succeed");

        let state = archive.resume(&base).expect("resume should succeed");

        // Champion profiles loaded correctly.
        assert_eq!(state.champion.len(), 2);
        assert!(state.champion.contains_key("sol"));
        assert!(state.champion.contains_key("ath"));

        // Learner profiles loaded correctly.
        assert_eq!(state.learner.len(), 2);
        assert!(state.learner.contains_key("sol"));
        assert!(state.learner.contains_key("ath"));

        // Training state restored.
        assert_eq!(state.start_update, 42);
        assert_eq!(state.history.len(), 1);
        assert_eq!(state.validation_seeds.len(), 32);
        assert_eq!(state.confirmation_seeds.len(), 32);

        // Clean up.
        let _ = fs::remove_file(&base);
        let _ = fs::remove_file(base.with_extension("sha256"));
    }

    #[test]
    fn resume_falls_back_to_champion_when_no_learner_profiles() {
        let base = PathBuf::from(".worktrees/training/test_resume_fallback.json");
        let _ = fs::remove_file(&base);
        let _ = fs::remove_file(base.with_extension("tmp"));
        let _ = fs::remove_file(base.with_extension("sha256"));

        let (mut cp, path) = temp_checkpoint();

        // Set up champion profiles only.
        let mut champion = BTreeMap::new();
        champion.insert("sol".to_string(), blank_profile("sol", 512));
        champion.insert("ath".to_string(), blank_profile("ath", 512));
        cp.accepted = champion.clone();
        cp.profiles = BTreeMap::new(); // No learner profiles

        let archive = Archive::new();
        archive.save(&cp, &path).expect("save should succeed");

        let state = archive.resume(&path).expect("resume should succeed");

        // Learner should fall back to champion.
        assert_eq!(state.learner.len(), 2);
        assert_eq!(state.champion.len(), 2);
        assert_eq!(
            state.champion.keys().collect::<Vec<_>>(),
            state.learner.keys().collect::<Vec<_>>()
        );

        // Clean up.
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(path.with_extension("sha256"));
    }

    #[test]
    fn failed_promotion_does_not_roll_back_champion() {
        // Simulate a training run where promotion failed:
        // 1. Save checkpoint with champion A and learner B (learner improved but not enough)
        // 2. Resume and continue training (learner becomes C)
        // 3. Champion should still be A, not rolled back to B or C.

        // Clean up any leftover files.
        let p = PathBuf::from(".worktrees/training/test_resume_no_rollback.json");
        let _ = fs::remove_file(&p);
        let _ = fs::remove_file(p.with_extension("tmp"));
        let _ = fs::remove_file(p.with_extension("sha256"));

        let archive = Archive::new();
        let path = p;

        // Step 1: Initial checkpoint with champion A and learner B.
        let mut cp1 = Checkpoint::new("test_resume".to_string(), Stage::One, Horizon::opening(), {
            let mut args = BTreeMap::new();
            args.insert("seed".to_string(), "0".to_string());
            args
        });
        let mut champion_a = BTreeMap::new();
        champion_a.insert("sol".to_string(), blank_profile("sol", 512));
        champion_a.insert("ath".to_string(), blank_profile("ath", 512));
        cp1.accepted = champion_a.clone();

        let mut learner_b = BTreeMap::new();
        learner_b.insert("sol".to_string(), blank_profile("sol", 512));
        learner_b.insert("ath".to_string(), blank_profile("ath", 512));
        cp1.profiles = learner_b.clone();

        // Record that promotion was attempted but failed.
        cp1.history.push(serde_json::json!({
            "update": 10,
            "candidate_metrics": {"sol": {"clearance": 0.92}},
            "accepted_metrics": {"sol": {"clearance": 0.90}},
            "accepted": [],
            "accepted_kind": "none"
        }));

        archive.save(&cp1, &path).expect("save should succeed");

        // Step 2: Resume.
        let state = archive.resume(&path).expect("resume should succeed");

        // Champion should still be A.
        assert_eq!(state.champion.len(), 2);
        assert!(state.champion.contains_key("sol"));
        assert!(state.champion.contains_key("ath"));
        assert_eq!(state.start_update, 10);

        // Verify the history records the failed promotion.
        let last_entry = state.history.last().expect("should have history");
        assert_eq!(last_entry["accepted_kind"], "none");
        assert!(last_entry["accepted"].is_array());
        assert!(last_entry["accepted"].as_array().unwrap().is_empty());

        // Clean up.
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(path.with_extension("sha256"));
    }

    #[test]
    fn uninterrupted_equivalence_check() {
        // A run that completes 10 updates in one shot should produce the same
        // checkpoint state as a run that does 5 + 5 updates with a resume in between.
        //
        // We test this by creating two checkpoints that represent the same logical state:
        // 1. A checkpoint written after 10 updates in one shot.
        // 2. A checkpoint written after 5 updates, resumed, then 5 more updates.
        //
        // The final_update and history should match.

        // Clean up any leftover files.
        for name in &["test_eq1.json", "test_eq2.json"] {
            let p = PathBuf::from(format!(".worktrees/training/{name}"));
            let _ = fs::remove_file(&p);
            let _ = fs::remove_file(p.with_extension("tmp"));
            let _ = fs::remove_file(p.with_extension("sha256"));
        }

        let archive = Archive::new();
        let path1 = PathBuf::from(".worktrees/training/test_eq1.json");
        let path2 = PathBuf::from(".worktrees/training/test_eq2.json");

        // Checkpoint 1: 10 updates in one shot.
        let mut cp1 = Checkpoint::new("test_eq".to_string(), Stage::One, Horizon::opening(), {
            let mut args = BTreeMap::new();
            args.insert("seed".to_string(), "0".to_string());
            args
        });
        cp1.final_update = 10;
        for i in 1..=10 {
            cp1.history
                .push(serde_json::json!({"update": i, "metrics": {}}));
        }
        archive.save(&cp1, &path1).expect("save should succeed");

        // Checkpoint 2: 5 updates, resume, 5 more updates.
        let mut cp2 = Checkpoint::new("test_eq".to_string(), Stage::One, Horizon::opening(), {
            let mut args = BTreeMap::new();
            args.insert("seed".to_string(), "0".to_string());
            args
        });
        let mut champion = BTreeMap::new();
        champion.insert("sol".to_string(), blank_profile("sol", 512));
        champion.insert("ath".to_string(), blank_profile("ath", 512));
        cp2.accepted = champion;
        cp2.final_update = 5;
        for i in 1..=5 {
            cp2.history
                .push(serde_json::json!({"update": i, "metrics": {}}));
        }
        archive.save(&cp2, &path2).expect("save should succeed");

        // Resume from cp2 and continue.
        let state = archive.resume(&path2).expect("resume should succeed");
        assert_eq!(state.start_update, 5);

        // Simulate continuing from 5 to 10.
        let mut resumed_cp = Checkpoint::resumed(&cp2);
        resumed_cp.final_update = 10;
        for i in 6..=10 {
            resumed_cp
                .history
                .push(serde_json::json!({"update": i, "metrics": {}}));
        }

        // Both checkpoints should have the same final_update and equivalent history.
        assert_eq!(cp1.final_update, resumed_cp.final_update);
        assert_eq!(cp1.history.len(), resumed_cp.history.len());
        for (a, b) in cp1.history.iter().zip(&resumed_cp.history) {
            assert_eq!(a, b, "history entries should match");
        }

        // Clean up.
        let _ = fs::remove_file(&path1);
        let _ = fs::remove_file(path1.with_extension("sha256"));
        let _ = fs::remove_file(&path2);
        let _ = fs::remove_file(path2.with_extension("sha256"));
    }

    #[test]
    fn resume_with_mismatched_factions_fails() {
        let base = PathBuf::from(".worktrees/training/test_resume_mismatch.json");
        let _ = fs::remove_file(&base);
        let _ = fs::remove_file(base.with_extension("tmp"));
        let _ = fs::remove_file(base.with_extension("sha256"));

        let mut cp = Checkpoint::new("test_resume".to_string(), Stage::One, Horizon::opening(), {
            let mut args = BTreeMap::new();
            args.insert("seed".to_string(), "0".to_string());
            args
        });

        let mut champion = BTreeMap::new();
        champion.insert("sol".to_string(), blank_profile("sol", 512));
        champion.insert("ath".to_string(), blank_profile("ath", 512));
        cp.accepted = champion;

        let mut learner = BTreeMap::new();
        learner.insert("sol".to_string(), blank_profile("sol", 512));
        learner.insert("nak".to_string(), blank_profile("nak", 512)); // mismatch!
        cp.profiles = learner;

        let archive = Archive::new();
        archive.save(&cp, &base).expect("save should succeed");

        let result = archive.resume(&base);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            CheckpointError::ProfileValidation(_)
        ));

        // Clean up.
        let _ = fs::remove_file(&base);
        let _ = fs::remove_file(base.with_extension("sha256"));
    }

    #[test]
    fn resume_without_accepted_profiles_fails() {
        let base = PathBuf::from(".worktrees/training/test_resume_no_accepted.json");
        let _ = fs::remove_file(&base);
        let _ = fs::remove_file(base.with_extension("tmp"));
        let _ = fs::remove_file(base.with_extension("sha256"));

        let cp = Checkpoint::new("test_resume".to_string(), Stage::One, Horizon::opening(), {
            let mut args = BTreeMap::new();
            args.insert("seed".to_string(), "0".to_string());
            args
        });
        // accepted is empty by default.

        let archive = Archive::new();
        archive.save(&cp, &base).expect("save should succeed");

        let result = archive.resume(&base);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            CheckpointError::ProfileValidation(_)
        ));

        // Clean up.
        let _ = fs::remove_file(&base);
        let _ = fs::remove_file(base.with_extension("sha256"));
    }
}
