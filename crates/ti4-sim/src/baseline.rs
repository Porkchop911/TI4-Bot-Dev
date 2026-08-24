//! Post-rules r6 validation re-baseline panel (M09-019a).
//!
//! The surviving r6 champion was trained and measured against the pre-rules engine. After the
//! M06–M08 rules rework, every number quoted from it needs a fresh baseline on the current tree —
//! this module is that measurement, and nothing else: no optimization, no policy change (the row
//! says "no optimization bundled").
//!
//! # Fail-closed inputs
//!
//! MLP plan §10: "Every corpus/panel command validates artifact role and checksum before
//! starting." [`run_panel`] therefore verifies the pool's sha256 against the manifest prefix and
//! loads + validates every champion profile **before** playing a single game. The r6 checkpoints
//! are read-only inputs; their checksums are recorded on the report so evidence can prove the
//! panel never overwrote pre-rules weights.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use sha2::{Digest, Sha256};
use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;
use ti4_model::id::PlayerId;
use ti4_policy::learned::Profile;

use crate::artifacts::{ArtifactError, ArtifactRole};
use crate::maps::{MapPool, MapPoolError};
use crate::result::GameResult;
use crate::run::{Horizon, play_learned};

/// The expected sha256 prefix of the validation-role pool (MLP plan §10 artifact manifest:
/// `full_np8_12_holdout.json`, logical role **validation** despite its filename).
pub const VALIDATION_POOL_SHA_PREFIX: &str = "aba33c81aa04cefb";

/// The expected sha256 prefix of the completed r6 checkpoint (MLP plan §10 artifact manifest).
pub const R6_CHECKPOINT_SHA_PREFIX: &str = "be792a2a207ced25";

/// Why a baseline panel cannot run.
#[derive(Debug, thiserror::Error)]
pub enum BaselineError {
    #[error("read artifact: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse checkpoint envelope: {0}")]
    Json(#[from] serde_json::Error),
    #[error("pool checksum mismatch: expected prefix {expected}, found {found}")]
    ChecksumMismatch { expected: String, found: String },
    #[error("checkpoint checksum mismatch: expected prefix {expected}, found {found}")]
    CheckpointChecksumMismatch { expected: String, found: String },
    #[error("champion profile for faction {faction} failed validation: {reason}")]
    InvalidProfile { faction: String, reason: String },
    #[error("map pool: {0}")]
    Pool(#[from] MapPoolError),
    #[error("artifact role: {0}")]
    Artifact(#[from] ArtifactError),
    #[error("baseline panel requires at least one seed")]
    EmptyPanel,
    #[error("baseline panel games failed: {details}")]
    GameFailures { details: String },
}

/// The r6 champion profiles loaded from a stage-2 checkpoint envelope.
pub struct Champions {
    /// Champion profile per faction name (the envelope's `accepted` map).
    pub profiles: BTreeMap<String, Arc<Profile>>,
    /// sha256 of the source file, so evidence can prove non-overwrite after the panel runs.
    pub source_sha256: String,
}

/// A partial read of a stage-2 checkpoint envelope: only the champion map is needed here, and
/// serde ignores the rest (history, telemetry, arguments) rather than trusting it.
#[derive(serde::Deserialize)]
struct Envelope {
    accepted: BTreeMap<String, Profile>,
}

impl Champions {
    /// Load and validate the `accepted` champion map from a stage-2 checkpoint envelope.
    ///
    /// # Errors
    /// I/O, JSON, or any profile that fails [`Profile::validate`] for its own faction.
    pub fn load_checkpoint_accepted(
        path: &Path,
        expected_sha_prefix: &str,
    ) -> Result<Self, BaselineError> {
        let bytes = fs::read(path)?;
        let source_sha256 = hex(&Sha256::digest(&bytes));
        if !source_sha256.starts_with(expected_sha_prefix) {
            return Err(BaselineError::CheckpointChecksumMismatch {
                expected: expected_sha_prefix.to_owned(),
                found: source_sha256,
            });
        }
        let envelope: Envelope = serde_json::from_slice(&bytes)?;
        let mut profiles = BTreeMap::new();
        for (faction, profile) in envelope.accepted {
            if let Err(reason) = profile.validate(Some(faction.as_str())) {
                return Err(BaselineError::InvalidProfile {
                    faction,
                    reason: reason.to_string(),
                });
            }
            profiles.insert(faction.clone(), Arc::new(profile));
        }
        Ok(Self {
            profiles,
            source_sha256,
        })
    }
}

/// Collect per-game failures as `seed N: reason` details, if any game failed.
fn failed_games(games: &[GameResult]) -> Option<String> {
    let failures: Vec<String> = games
        .iter()
        .filter_map(|game| {
            game.error
                .as_ref()
                .map(|reason| format!("seed {}: {reason}", game.seed))
        })
        .collect();
    (!failures.is_empty()).then(|| failures.join("; "))
}

/// Verify that `path`'s sha256 starts with `expected_prefix`, returning the full digest.
///
/// Fail-closed: a mismatched corpus is a different measurement, and playing on it would quote
/// numbers against the wrong boards.
///
/// # Errors
/// I/O, or [`BaselineError::ChecksumMismatch`] when the prefix does not match.
pub fn verify_checksum(path: &Path, expected_prefix: &str) -> Result<String, BaselineError> {
    let bytes = fs::read(path)?;
    let found = hex(&Sha256::digest(&bytes));
    if !found.starts_with(expected_prefix) {
        return Err(BaselineError::ChecksumMismatch {
            expected: expected_prefix.to_owned(),
            found,
        });
    }
    Ok(found)
}

/// The measured panel and the checksums of everything it read.
pub struct PanelReport {
    /// Full sha256 of the pool file actually played on (verified against the manifest prefix).
    pub pool_sha256: String,
    /// Full sha256 of the checkpoint file the champions were loaded from.
    pub champion_sha256: String,
    /// One result per seed, in seed order.
    pub games: Vec<GameResult>,
}

/// Aggregate baseline numbers for evidence.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PanelSummary {
    pub games_played: usize,
    pub games_failed: usize,
    /// Games that reached a natural end rather than the horizon or an error.
    pub completed: usize,
    /// Mean final VP per seat label across the panel.
    pub mean_vp_per_seat: BTreeMap<String, f64>,
    pub total_decisions: usize,
}

impl PanelReport {
    /// The aggregate numbers a baseline is quoted from.
    #[must_use]
    pub fn summary(&self) -> PanelSummary {
        let mut per_seat: BTreeMap<String, (i64, usize)> = BTreeMap::new();
        for game in &self.games {
            if game.error.is_some() {
                continue; // a failed game contributes no score, only its failure count
            }
            for (seat, points) in &game.victory_points {
                let entry = per_seat.entry(seat.clone()).or_default();
                entry.0 += i64::from(*points);
                entry.1 += 1;
            }
        }
        PanelSummary {
            games_played: self.games.len(),
            games_failed: self
                .games
                .iter()
                .filter(|game| game.error.is_some())
                .count(),
            completed: self.games.iter().filter(|game| game.finished).count(),
            mean_vp_per_seat: per_seat
                .into_iter()
                .map(|(seat, (total, count))| {
                    #[expect(
                        clippy::cast_precision_loss,
                        reason = "display means over small counts"
                    )]
                    let mean = total as f64 / count as f64;
                    (seat, mean)
                })
                .collect(),
            total_decisions: self.games.iter().map(|game| game.decisions).sum(),
        }
    }
}

/// Run the bounded validation panel: one game per seed, each on a pool-drawn board with every
/// seat answered by its faction's champion profile.
///
/// Validates artifact role and checksum before starting (MLP plan §10): the pool is verified
/// against `pool_sha_prefix` and every champion profile is validated for its own faction before
/// any game is played. The checkpoint checksum is verified against `checkpoint_sha_prefix` from
/// the same byte buffer that is deserialized.
///
/// # Errors
/// Checksum mismatch, unreadable/unparseable artifacts, invalid profiles, or a map-pool error.
pub fn run_panel(
    content: &ContentStore,
    players: &[PlayerId],
    sources: SourceSet,
    seeds: &[u64],
    pool_path: &Path,
    checkpoint_path: &Path,
    horizon: Horizon,
    pool_sha_prefix: &str,
    checkpoint_sha_prefix: &str,
) -> Result<PanelReport, BaselineError> {
    if seeds.is_empty() {
        return Err(BaselineError::EmptyPanel);
    }
    let pool_sha256 = verify_checksum(pool_path, pool_sha_prefix)?;
    // Data-role gate (MLP plan §10): a measurement panel may never consume final-role data,
    // and an unknown pool identity fails closed before any game runs.
    crate::artifacts::verify_pool_role(
        pool_path,
        &[ArtifactRole::Train, ArtifactRole::Validation],
    )?;
    let champions = Champions::load_checkpoint_accepted(checkpoint_path, checkpoint_sha_prefix)?;
    let pool = MapPool::load(pool_path)?;

    let mut games = Vec::with_capacity(seeds.len());
    for &seed in seeds {
        games.push(play_learned(
            content,
            players,
            sources,
            seed,
            horizon,
            &pool,
            &champions.profiles,
        ));
    }
    if let Some(details) = failed_games(&games) {
        return Err(BaselineError::GameFailures { details });
    }
    Ok(PanelReport {
        pool_sha256,
        champion_sha256: champions.source_sha256,
        games,
    })
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
    use std::io::Cursor;
    use ti4_content::galaxy::all_systems;
    use ti4_engine::seating::IN_SCOPE_FACTIONS;
    use ti4_model::content_types::DEFAULT;

    fn players() -> Vec<PlayerId> {
        (1..=6)
            .map(|index| PlayerId::new(format!("p{index}")))
            .collect()
    }

    /// A schema-4 profile with one weight per head, built through the same JSON path real
    /// checkpoints use: valid for its faction, distinct per faction.
    fn test_profile(faction: &str) -> Profile {
        let heads = ti4_policy::learned::STAGE1_DECISION_HEADS
            .iter()
            .map(|head| {
                (
                    (*head).to_owned(),
                    serde_json::json!({ "weights": { "bucket": 0.5 * f64::from(u32::try_from(faction.len()).unwrap()) }, "temperature": 1.0 }),
                )
            })
            .collect::<serde_json::Map<String, serde_json::Value>>();
        let value = serde_json::json!({
            "schema": 4,
            "mode": "fully_learned",
            "name": format!("test-{faction}"),
            "faction": faction,
            "learned": { "heads": heads },
        });
        serde_json::from_value(value).expect("test profile deserializes")
    }

    /// A pool with six home slots and four neutral positions, drawn from real content systems.
    fn synthetic_pool_json(content: &ContentStore) -> String {
        let homes: Vec<&str> = IN_SCOPE_FACTIONS
            .iter()
            .map(|faction| {
                ti4_content::factions::get(content, faction)
                    .and_then(|record| record.home_system())
                    .expect("in-scope factions have home systems")
            })
            .collect();
        let neutrals: Vec<&str> = all_systems(content, DEFAULT)
            .keys()
            .filter(|system| !homes.contains(system))
            .take(4)
            .copied()
            .collect();

        // Six slot coordinates plus four neutral coordinates; the arrangement lists real systems
        // at every position (slot positions are replaced by homes during placement).
        let coords: Vec<(i32, i32)> = vec![
            (-3, 0),
            (0, -3),
            (3, -3),
            (3, 0),
            (0, 3),
            (-3, 3),
            (10, 0),
            (11, 0),
            (12, 0),
            (13, 0),
        ];
        let systems: Vec<&str> = homes.iter().chain(neutrals.iter()).copied().collect();

        let coords_json = coords
            .iter()
            .map(|(q, r)| format!("[{q},{r}]"))
            .collect::<Vec<_>>()
            .join(",");
        let slots_json = coords[..6]
            .iter()
            .map(|(q, r)| format!("[{q},{r}]"))
            .collect::<Vec<_>>()
            .join(",");
        let systems_json = systems
            .iter()
            .map(|system| format!("\"{system}\""))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            r#"{{"schema":"ti4-map-pool-v1","effort":1,"coords":[{coords_json}],"slots":[{slots_json}],"arrangements":[[{systems_json}]]}}"#
        )
    }

    #[test]
    fn checksum_verification_fails_closed_on_tampered_bytes() {
        let dir = std::env::temp_dir().join("ti4-sim-baseline-test");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pool.json");
        fs::write(&path, b"some pool bytes").unwrap();

        let found = hex(&Sha256::digest(b"some pool bytes"));
        // A matching prefix passes and returns the full digest.
        assert_eq!(verify_checksum(&path, &found[..8]).unwrap(), found);
        // A mismatched prefix fails closed with the mismatch named.
        let err = verify_checksum(&path, "ffffffffffffffff").unwrap_err();
        assert!(matches!(err, BaselineError::ChecksumMismatch { .. }));
    }

    #[test]
    fn champions_load_validates_and_records_checksum() {
        let dir = std::env::temp_dir().join("ti4-sim-baseline-test");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("checkpoint.json");

        let envelope = serde_json::json!({
            "schema": 1,
            "trainer": "test",
            "stage": "stage2",
            "accepted": {
                "sol": test_profile("sol"),
                "hacan": test_profile("hacan"),
            },
        });
        let bytes = serde_json::to_vec(&envelope).unwrap();
        fs::write(&path, &bytes).unwrap();

        let digest = hex(&Sha256::digest(&bytes));
        let champions = Champions::load_checkpoint_accepted(&path, &digest).unwrap();
        assert_eq!(champions.profiles.len(), 2);
        assert!(champions.profiles.contains_key("sol"));
        assert!(champions.profiles.contains_key("hacan"));
        assert_eq!(champions.source_sha256, hex(&Sha256::digest(&bytes)));

        // A profile that fails validation for its faction is refused, not repaired.
        let bad = serde_json::json!({
            "accepted": {
                "sol": test_profile("hacan"), // faction field says hacan; key says sol
            },
        });
        fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(matches!(
            Champions::load_checkpoint_accepted(
                &path,
                &hex(&Sha256::digest(serde_json::to_vec(&bad).unwrap()))
            ),
            Err(BaselineError::InvalidProfile { .. })
        ));

        // A structurally valid checkpoint with the wrong manifest identity is refused before
        // deserialization can make it look like the requested champion.
        let valid = serde_json::json!({ "accepted": { "sol": test_profile("sol") } });
        fs::write(&path, serde_json::to_vec(&valid).unwrap()).unwrap();
        assert!(matches!(
            Champions::load_checkpoint_accepted(&path, "ffffffffffffffff"),
            Err(BaselineError::CheckpointChecksumMismatch { .. })
        ));
    }

    #[test]
    fn learned_play_on_a_pooled_board_is_deterministic() {
        let content = ContentStore::embedded();
        let pool_json = synthetic_pool_json(content);
        let pool = MapPool::from_reader(Cursor::new(pool_json.as_bytes())).unwrap();

        let champions: BTreeMap<String, Arc<Profile>> = IN_SCOPE_FACTIONS
            .iter()
            .map(|faction| (faction.to_string(), Arc::new(test_profile(faction))))
            .collect();

        let horizon = Horizon {
            rounds: 1,
            steps: 20_000,
        };
        let first = play_learned(content, &players(), DEFAULT, 42, horizon, &pool, &champions);
        let second = play_learned(content, &players(), DEFAULT, 42, horizon, &pool, &champions);

        assert!(first.error.is_none(), "game failed: {:?}", first.error);
        assert_eq!(first.seed, second.seed);
        assert_eq!(first.finished, second.finished);
        assert_eq!(first.winner, second.winner);
        assert_eq!(first.rounds, second.rounds);
        assert_eq!(first.victory_points, second.victory_points);
        assert_eq!(first.events, second.events);
        assert_eq!(first.decisions, second.decisions);
        assert_eq!(first.ended_because, second.ended_because);

        // Every seat scored through its own profile: six seats, all present in the result.
        assert_eq!(first.victory_points.len(), 6);
    }

    #[test]
    fn panel_rejects_empty_and_failed_runs() {
        let missing = Path::new("does-not-need-to-exist-for-an-empty-panel");
        assert!(matches!(
            run_panel(
                ContentStore::embedded(),
                &players(),
                DEFAULT,
                &[],
                missing,
                missing,
                Horizon {
                    rounds: 1,
                    steps: 1
                },
                "unused",
                "unused",
            ),
            Err(BaselineError::EmptyPanel)
        ));

        // M09-020 strengthened the contract: a panel given a pool that is not in the durable
        // manifest fails closed at the role gate, before any game runs. The synthetic test pool
        // has no manifest identity, so it must be refused as unknown.
        let dir = std::env::temp_dir().join("ti4-sim-baseline-panel-failure-test");
        fs::create_dir_all(&dir).unwrap();
        let pool_path = dir.join("pool.json");
        let checkpoint_path = dir.join("checkpoint.json");
        let pool_bytes = synthetic_pool_json(ContentStore::embedded()).into_bytes();
        fs::write(&pool_path, &pool_bytes).unwrap();
        fs::write(
            &checkpoint_path,
            serde_json::to_vec(&serde_json::json!({ "accepted": {} })).unwrap(),
        )
        .unwrap();

        let result = run_panel(
            ContentStore::embedded(),
            &players(),
            DEFAULT,
            &[919_001],
            &pool_path,
            &checkpoint_path,
            Horizon {
                rounds: 1,
                steps: 20_000,
            },
            &hex(&Sha256::digest(&pool_bytes)),
            "unused-for-an-unknown-pool",
        );
        let Err(error) = result else {
            panic!("a panel on an unmanifested pool must be refused");
        };
        assert!(
            matches!(
                error,
                BaselineError::Artifact(crate::artifacts::ArtifactError::UnknownArtifact { .. })
            ),
            "{error}"
        );
    }

    /// The per-game failure details that `run_panel` turns into `GameFailures`: every failing
    /// seed and reason is preserved, in game order.
    #[test]
    fn failed_games_preserves_seed_and_reason() {
        let games = vec![
            game_with_error(919_001, "no champion profile for l1z1x"),
            game_with_error(919_002, "deployment fault"),
        ];
        let details = super::failed_games(&games).expect("two failed games must produce details");
        assert!(
            details.contains("seed 919001: no champion profile for l1z1x"),
            "{details}"
        );
        assert!(
            details.contains("seed 919002: deployment fault"),
            "{details}"
        );

        // No failures, no details.
        let mut clean = game_with_error(0, "");
        clean.error = None;
        assert!(super::failed_games(&[clean]).is_none());
    }

    fn game_with_error(seed: u64, reason: &str) -> GameResult {
        GameResult {
            seed,
            finished: false,
            winner: None,
            rounds: 0,
            victory_points: BTreeMap::new(),
            events: BTreeMap::new(),
            decisions: 0,
            seconds: 0.0,
            ended_because: crate::result::Ending::Error,
            error: Some(reason.to_owned()),
        }
    }
}
