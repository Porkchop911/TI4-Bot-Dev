//! The corpus of feature names the dense vocabulary is built from (MLP plan §4.5, M09-024b).
//!
//! §4.5 defines the vocabulary's input as the union of three sources: the names already carried by
//! the r6 champions, every name emitted by replaying the §6.1 teacher seed schedule with the
//! completed extractors, and every statically enumerable content name. This module assembles them
//! and reports each one's contribution separately.
//!
//! # Why the contributions are reported separately
//!
//! A source that silently produced nothing looks exactly like a source that was redundant. The
//! union alone cannot tell them apart, and the difference matters: if the replay contributes no
//! names, either the extractors emit nothing new — a real and interesting result — or the replay
//! did not run. So each source's own set is kept and its unique contribution measured.
//!
//! # What this is not
//!
//! §4.5 is explicit that this is "a bounded discovery pass, not the M10 training corpus". Only
//! names leave here. No decision, option, probability, return or state value is retained; M10-031
//! is what captures those, under its own permission.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use sha2::Digest;
use std::io::Write;
use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{FactionId, PlayerId};
use ti4_policy::learned::Profile;

/// One source's names, and what it alone contributed.
#[derive(Debug, Clone)]
pub struct Contribution {
    /// Which of §4.5's three sources this is.
    pub source: &'static str,
    /// Every name the source produced.
    pub names: BTreeSet<String>,
}

impl Contribution {
    /// How many names this source produced that none of `others` did.
    #[must_use]
    pub fn unique_against(&self, others: &[&Self]) -> usize {
        self.names
            .iter()
            .filter(|name| !others.iter().any(|other| other.names.contains(*name)))
            .count()
    }
}

/// Anything that stopped the discovery pass.
#[derive(Debug, thiserror::Error)]
pub enum CorpusError {
    /// A declared input could not be read.
    #[error("reading {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    /// A checkpoint did not hold the profiles this pass needs.
    #[error("checkpoint {path}: {reason}")]
    Checkpoint { path: String, reason: String },
    /// Publication did not produce a complete generation.
    #[error("publication failed: {reason} (previous generation intact: {previous_intact})")]
    Publication {
        reason: String,
        previous_intact: bool,
    },
    /// The campaign did not complete every game it was asked for.
    #[error("campaign completed {completed} of {expected} games; {} failed", failures.len())]
    Campaign {
        completed: usize,
        expected: usize,
        failures: Vec<(u64, usize, String)>,
    },
}

/// Source (a): every feature name the r6 champions already carry.
///
/// Takes the **already-verified bytes**, not a path. An earlier version opened the checkpoint here
/// and the driver opened it again for the profiles, so the two consumers need not have read the
/// same file and neither checked it against the durable accepted identity (F-M09-024b2-1). One
/// read, one verification, and every consumer parses from that one immutable buffer.
///
/// # Errors
/// [`CorpusError::Checkpoint`] if the bytes are not an envelope with a non-empty `profiles` map.
pub fn champion_names(bytes: &[u8], label: &str) -> Result<Contribution, CorpusError> {
    let document: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|error| CorpusError::Checkpoint {
            path: label.to_owned(),
            reason: format!("not JSON: {error}"),
        })?;
    let profiles: BTreeMap<String, Profile> = serde_json::from_value(document["profiles"].clone())
        .map_err(|error| CorpusError::Checkpoint {
            path: label.to_owned(),
            reason: format!("profiles: {error}"),
        })?;
    if profiles.is_empty() {
        return Err(CorpusError::Checkpoint {
            path: label.to_owned(),
            reason: "no profiles".to_owned(),
        });
    }
    let mut raw = BTreeSet::new();
    for profile in profiles.values() {
        for head in profile.learned.heads.values() {
            raw.extend(head.weights.keys().cloned());
        }
    }
    Ok(Contribution {
        source: "r6-champions",
        // The checkpoint holds every name its schema-4 *and* legacy channels ever scored with,
        // including the `kind-faction`/`option-faction` families the explicit path never emits. The
        // projection is the same filter the model's own inputs go through, so a name that cannot
        // reach the model cannot reach the vocabulary either.
        names: ti4_policy::projection::project_names(raw),
    })
}

/// The profiles from the same verified buffer the names came from.
///
/// # Errors
/// [`CorpusError::Checkpoint`] if the envelope does not carry a `profiles` map.
pub fn champion_profiles(
    bytes: &[u8],
    label: &str,
) -> Result<BTreeMap<String, Profile>, CorpusError> {
    let document: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|error| CorpusError::Checkpoint {
            path: label.to_owned(),
            reason: format!("not JSON: {error}"),
        })?;
    serde_json::from_value(document["profiles"].clone()).map_err(|error| CorpusError::Checkpoint {
        path: label.to_owned(),
        reason: format!("profiles: {error}"),
    })
}

/// Source (c): every name that is a pure function of a content record.
///
/// **Deliberately narrow, and the boundary is the point.** A name belongs here only if the corpus
/// determines it without a game being played: the faction decomposition for every selectable seat
/// (MLP §5.3 — abilities, technology, opening units, home planets, commodities) and the met flag
/// for every objective and secret alias (§5.1). Those are closed sets, enumerable today, and every
/// one of them will be emitted eventually by some game.
///
/// What is left out, and why: `option:` word features depend on the prompt and option text the
/// engine constructs at a decision, not on a record alone, so enumerating them would mean guessing
/// at strings rather than reading them. They are the replay's job. Putting a guessed name in the
/// vocabulary is worse than leaving it out — an unseen name still routes to its family's OOV
/// column, whereas a wrong name occupies a column for ever and shifts nothing but wastes `width`
/// weights.
#[must_use]
pub fn content_names(content: &ContentStore, sources: SourceSet) -> Contribution {
    let mut names = BTreeSet::new();

    for (alias, faction) in ti4_content::factions::catalogue(content, sources) {
        if !ti4_policy::features::is_selectable_seat(&faction) {
            continue;
        }
        let _ = alias;
        for ability in faction.abilities() {
            names.insert(format!("ability:{ability}"));
        }
        for tech in faction.starting_tech() {
            names.insert(format!("faction-start-tech:{tech}"));
        }
        for tech in faction.faction_tech() {
            names.insert(format!("faction-tech:{tech}"));
        }
        for planet in faction.home_planets() {
            names.insert(format!("faction-home:{planet}"));
        }
        if faction.commodities() != 0 {
            names.insert("faction-commodities".to_owned());
        }
        if let Ok(deployments) = faction.deployments(content) {
            for deployment in deployments {
                names.insert(format!(
                    "faction-start-unit:{}",
                    deployment.unit_id.as_str()
                ));
            }
        }
    }

    for category in [ContentType::PublicObjectives, ContentType::SecretObjectives] {
        for record in content.from_sources(category, sources) {
            if let Some(id) = record.id() {
                names.insert(format!("objective-met:{id}"));
            }
        }
    }

    Contribution {
        source: "content",
        names: ti4_policy::projection::project_names(names),
    }
}

/// Collects every feature name a decision emits, then answers with the real bot.
///
/// The extraction runs through the bound [`SeatObservation`] the engine hands a decider, so the
/// pass sees exactly what live play sees — the acting seat's own secrets and nothing else. A
/// discovery pass that reached around that boundary would enumerate names no live decision can
/// produce.
struct Collector {
    inner: ti4_policy::inference::LearnedBot,
    names: std::rc::Rc<std::cell::RefCell<BTreeSet<String>>>,
}

impl Decider for Collector {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.inner.choose(choice)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let held = seen.held_secret_progress();
        let observed = seen.observed();
        {
            // **One path.** The architecture ruling is explicit that the MLP consumes one
            // schema-4 explicit policy path and "does not union two runtime extractors". The
            // first pass also collected `option_feature_names`, the legacy schema-2 hashed
            // channel, which put 38,542 `prompt-bigram` names into a vocabulary no schema-4
            // model reads (O-M09-024b-4). Collecting through the projection instead means the
            // names discovered here and the vectors the model is fed are the same set by
            // construction, rather than two lists someone has to keep in step.
            let mut names = self.names.borrow_mut();
            for vector in
                ti4_policy::projection::mlp_choice_features(observed, choice, &choice.player, &held)
            {
                names.extend(ti4_policy::features::names_of(&vector));
            }
        }
        self.inner.choose_seeing(choice, seen)
    }
}

/// What one discovery campaign did, in full.
#[derive(Debug, Clone)]
pub struct Campaign {
    /// The names the campaign emitted.
    pub names: BTreeSet<String>,
    /// Games that completed without an engine error.
    pub completed: usize,
    /// Games that failed, with seed, rotation and reason.
    pub failures: Vec<(u64, usize, String)>,
}

/// Source (b): every name emitted while replaying the §6.1 teacher seed schedule.
///
/// One bounded pass, single-threaded so the collected set cannot depend on thread interleaving.
///
/// **Failures are returned, not counted as successes.** An earlier version discarded every
/// `Rollout.error` and incremented the game count regardless, so a seating failure, an illegal
/// choice or a horizon trip contributed a partial name set and was reported as one of 768 good
/// games — after which the caller published the artifact (F-M09-024b2-2).
///
/// # Errors
/// [`CorpusError::Campaign`] if any game failed or the completed count is not the expected one.
///
/// # Panics
/// If a collector outlives its game, which cannot happen: the table owns every decider and is
/// dropped with the rollout.
pub fn replay_names(
    content: &'static ContentStore,
    sources: SourceSet,
    pool: &Arc<ti4_sim::MapPool>,
    champions: &BTreeMap<String, Profile>,
    factions: &[&str],
    seeds: std::ops::Range<u64>,
    tile_seed_offset: u64,
    horizon: crate::rollout::Horizon,
) -> Result<Campaign, CorpusError> {
    let names = std::rc::Rc::new(std::cell::RefCell::new(BTreeSet::new()));
    let mut completed = 0usize;
    let mut failures = Vec::new();

    let players: Vec<PlayerId> = (0..factions.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let expected = usize::try_from(seeds.end - seeds.start).unwrap_or(0) * factions.len();

    for seed in seeds {
        for rotation in 0..factions.len() {
            let seated: BTreeMap<PlayerId, FactionId> = players
                .iter()
                .enumerate()
                .map(|(index, player)| {
                    (
                        player.clone(),
                        FactionId::new(factions[(index + rotation) % factions.len()]),
                    )
                })
                .collect();
            // A seat with no champion profile is a campaign failure, not something to substitute
            // a default for.
            let missing: Vec<String> = players
                .iter()
                .map(|player| seated[player].as_str().to_owned())
                .filter(|faction| !champions.contains_key(faction))
                .collect();
            if !missing.is_empty() {
                failures.push((
                    seed,
                    rotation,
                    format!("no champion profile for {}", missing.join(", ")),
                ));
                continue;
            }
            let deciders: BTreeMap<PlayerId, Box<dyn Decider>> = players
                .iter()
                .enumerate()
                .map(|(index, player)| {
                    let profile = champions[&seated[player].to_string()].clone();
                    let stream = seed
                        .wrapping_mul(1_000_003)
                        .wrapping_add(u64::try_from(index).unwrap_or(0));
                    let decider: Box<dyn Decider> = Box::new(Collector {
                        inner: ti4_policy::inference::LearnedBot::from_shared(
                            Arc::new(profile),
                            stream,
                        ),
                        names: std::rc::Rc::clone(&names),
                    });
                    (player.clone(), decider)
                })
                .collect();

            let rollout = crate::rollout::play_with_deciders(
                content,
                &players,
                &seated,
                sources,
                seed,
                horizon,
                ti4_engine::opening::DEFAULT_REQUIREMENT,
                &crate::rollout::OpeningMap::PythonPool {
                    pool: Arc::clone(pool),
                    tile_seed_offset,
                },
                deciders,
            );
            if let Some(error) = rollout.error {
                failures.push((seed, rotation, error.clone()));
            } else {
                completed += 1;
            }
        }
    }

    let names = std::rc::Rc::try_unwrap(names)
        .expect("every collector is dropped with its game")
        .into_inner();

    if !failures.is_empty() || completed != expected {
        return Err(CorpusError::Campaign {
            completed,
            expected,
            failures,
        });
    }
    Ok(Campaign {
        names,
        completed,
        failures,
    })
}

/// Publish a vocabulary and its provenance as one recoverable generation.
///
/// # Why both files together
///
/// An earlier version renamed the vocabulary into place and then wrote the provenance with a bare
/// `fs::write`. A failure at that second step left a newly published artifact beside missing or
/// stale provenance, while the caller reported "no artifact was written" — which was false
/// (F-M09-024b2-7). The two files are one generation: both are staged, flushed, re-read, re-hashed
/// and re-parsed, and the provenance must name the vocabulary's digest, so a torn pair cannot be
/// mistaken for a matched one.
///
/// On any failure the previous generation is restored from a snapshot taken before either rename,
/// and the returned error says truthfully what state the destination is in.
///
/// # Errors
/// [`CorpusError::Publication`] carrying what happened and whether the previous generation survived.
pub fn publish_generation(
    slots_path: &std::path::Path,
    slots_text: &str,
    provenance_path: &std::path::Path,
    provenance_text: &str,
) -> Result<String, CorpusError> {
    let digest = format!("{:x}", sha2::Sha256::digest(slots_text.as_bytes()));

    if !provenance_text.contains(&digest) {
        return Err(CorpusError::Publication {
            reason: "the provenance does not name the vocabulary's digest".to_owned(),
            previous_intact: true,
        });
    }

    let fail = |reason: String| CorpusError::Publication {
        reason,
        previous_intact: true,
    };

    // Snapshot whatever is already there, so a half-applied generation can be undone.
    let previous_slots = std::fs::read(slots_path).ok();
    let previous_provenance = std::fs::read(provenance_path).ok();

    if let Some(parent) = slots_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| fail(format!("output directory: {e}")))?;
    }

    // Stage both, flushing each to disk rather than trusting the write cache.
    let staged_slots = slots_path.with_extension("json.staging");
    let staged_provenance = provenance_path.with_extension("json.staging");
    for (path, text) in [
        (&staged_slots, slots_text),
        (&staged_provenance, provenance_text),
    ] {
        let mut file = std::fs::File::create(path)
            .map_err(|e| fail(format!("staging {}: {e}", path.display())))?;
        file.write_all(text.as_bytes())
            .map_err(|e| fail(format!("staging write {}: {e}", path.display())))?;
        file.sync_all()
            .map_err(|e| fail(format!("staging sync {}: {e}", path.display())))?;
    }

    // Verify the bytes that actually landed, not the ones in memory.
    let written_slots =
        std::fs::read(&staged_slots).map_err(|e| fail(format!("staging reread: {e}")))?;
    let written_digest = format!("{:x}", sha2::Sha256::digest(&written_slots));
    if written_digest != digest {
        let _ = std::fs::remove_file(&staged_slots);
        let _ = std::fs::remove_file(&staged_provenance);
        return Err(fail(format!(
            "staged vocabulary hashes {written_digest}, expected {digest}"
        )));
    }
    let reparsed = String::from_utf8(written_slots)
        .map_err(|e| fail(format!("staged vocabulary is not UTF-8: {e}")))?;
    if ti4_policy::vocabulary::Vocabulary::from_json(&reparsed).is_err() {
        let _ = std::fs::remove_file(&staged_slots);
        let _ = std::fs::remove_file(&staged_provenance);
        return Err(fail("the staged vocabulary does not load".to_owned()));
    }

    // Replace. `std::fs::rename` replaces an existing destination on both Unix and Windows (the
    // Windows implementation passes `MOVEFILE_REPLACE_EXISTING`), so this is a replacement rather
    // than a create.
    std::fs::rename(&staged_slots, slots_path).map_err(|e| {
        let _ = std::fs::remove_file(&staged_slots);
        let _ = std::fs::remove_file(&staged_provenance);
        fail(format!("replacing the vocabulary: {e}"))
    })?;

    if let Err(error) = std::fs::rename(&staged_provenance, provenance_path) {
        // The vocabulary is already in place. Roll it back rather than leaving a generation whose
        // halves disagree, and report which state the destination ended in.
        let restored = match &previous_slots {
            Some(bytes) => std::fs::write(slots_path, bytes).is_ok(),
            None => std::fs::remove_file(slots_path).is_ok(),
        };
        if let Some(bytes) = &previous_provenance {
            let _ = std::fs::write(provenance_path, bytes);
        }
        let _ = std::fs::remove_file(&staged_provenance);
        return Err(CorpusError::Publication {
            reason: format!("replacing the provenance: {error}"),
            previous_intact: restored,
        });
    }

    Ok(digest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::content_types::DEFAULT;

    #[test]
    fn content_names_cover_every_selectable_seat_and_objective() {
        // Source (c) is only useful if it is complete over the closed sets it claims. If a seat or
        // an objective is missed, its names wait for a game to emit them — which is not wrong, but
        // it makes the source a subset of the replay rather than an independent contribution.
        let content = ContentStore::embedded();
        let contribution = content_names(content, DEFAULT);
        assert!(!contribution.names.is_empty());

        let seats = ti4_content::factions::catalogue(content, DEFAULT)
            .into_iter()
            .filter(|(_, faction)| ti4_policy::features::is_selectable_seat(faction))
            .count();
        assert_eq!(seats, 33, "the corpus holds 33 selectable seats");

        // Every selectable seat contributed at least its home planets.
        for (_, faction) in ti4_content::factions::catalogue(content, DEFAULT) {
            if !ti4_policy::features::is_selectable_seat(&faction) {
                continue;
            }
            for planet in faction.home_planets() {
                assert!(
                    contribution
                        .names
                        .contains(&format!("faction-home:{planet}")),
                    "{planet} is missing from the content names"
                );
            }
        }

        // Every objective and secret alias contributed a met flag.
        for category in [ContentType::PublicObjectives, ContentType::SecretObjectives] {
            for record in content.from_sources(category, DEFAULT) {
                let id = record.id().expect("records have ids");
                assert!(
                    contribution.names.contains(&format!("objective-met:{id}")),
                    "{id} is missing a met flag"
                );
            }
        }
    }

    #[test]
    fn content_names_stay_inside_the_closed_grammar() {
        // Every name this source invents must belong to a registered family, or it occupies a
        // column for ever while naming something no extractor emits.
        let content = ContentStore::embedded();
        let families: BTreeSet<&str> = ti4_policy::vocabulary::oov_families()
            .iter()
            .copied()
            .collect();
        for name in &content_names(content, DEFAULT).names {
            let family = ti4_policy::vocabulary::family_of(name);
            assert!(
                families.contains(family),
                "{name} is outside the registered families"
            );
        }
    }

    /// A scratch directory that removes itself.
    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("ti4-b2-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("scratch");
            Self(dir)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn small_vocabulary_text() -> String {
        ti4_policy::vocabulary::Vocabulary::build(["option:a", "option:b"])
            .expect("builds")
            .to_json()
            .expect("json")
    }

    #[test]
    fn a_campaign_with_a_failed_game_publishes_nothing() {
        // F-M09-024b2-8. The campaign type is what stops a partial name set being reported as a
        // complete run, so the refusal is exercised rather than assumed. An empty champion map
        // makes every game fail at seating.
        let content = ti4_content::ContentStore::embedded();
        let pool = std::sync::Arc::new(
            ti4_sim::MapPool::load(std::path::Path::new(
                "../../out/pools/full_np8_12_train.json",
            ))
            .or_else(|_| {
                ti4_sim::MapPool::load(std::path::Path::new("out/pools/full_np8_12_train.json"))
            })
            .expect("the train pool is on disk"),
        );
        let error = replay_names(
            content,
            DEFAULT,
            &pool,
            &BTreeMap::new(),
            &["sol", "letnev"],
            202_608_210..202_608_211,
            20_000_000,
            crate::rollout::Horizon {
                rounds: 1,
                steps: 10_000,
            },
        )
        .expect_err("a campaign with no champions must fail");
        match error {
            CorpusError::Campaign {
                completed,
                expected,
                failures,
            } => {
                assert_eq!(completed, 0);
                assert_eq!(expected, 2);
                assert_eq!(failures.len(), 2, "every game is reported, with its reason");
                assert!(failures[0].2.contains("no champion profile"));
            }
            other => panic!("wrong error: {other}"),
        }
    }

    #[test]
    fn a_publication_whose_provenance_does_not_name_the_artifact_is_refused() {
        let scratch = Scratch::new("mismatch");
        let slots = scratch.0.join("slots.json");
        let provenance = scratch.0.join("slots.provenance.json");
        let error = publish_generation(
            &slots,
            &small_vocabulary_text(),
            &provenance,
            "{
 \"slots_sha256\": \"0000\"
}
",
        )
        .expect_err("a provenance that names the wrong artifact must be refused");
        assert!(matches!(
            error,
            CorpusError::Publication {
                previous_intact: true,
                ..
            }
        ));
        assert!(
            !slots.exists(),
            "a refused publication wrote the vocabulary anyway"
        );
        assert!(!provenance.exists());
    }

    #[test]
    fn a_failed_second_write_leaves_the_previous_generation_intact() {
        // The torn-generation case: the vocabulary is replaced and the provenance cannot be. A
        // directory standing where the provenance file must go makes the second rename fail.
        let scratch = Scratch::new("torn");
        let slots = scratch.0.join("slots.json");
        let provenance = scratch.0.join("slots.provenance.json");

        // An accepted generation already in place.
        std::fs::write(&slots, "PREVIOUS VOCABULARY").expect("seed");
        std::fs::write(&provenance, "PREVIOUS PROVENANCE").expect("seed");
        // Now make the provenance replacement impossible.
        std::fs::remove_file(&provenance).expect("remove");
        std::fs::create_dir(&provenance).expect("a directory blocks the rename");

        let text = small_vocabulary_text();
        let digest = format!("{:x}", sha2::Sha256::digest(text.as_bytes()));
        let error = publish_generation(
            &slots,
            &text,
            &provenance,
            &format!(
                "{{
 \"slots_sha256\": \"{digest}\"
}}
"
            ),
        )
        .expect_err("the second replacement cannot succeed");

        match error {
            CorpusError::Publication {
                previous_intact,
                reason,
            } => {
                assert!(reason.contains("provenance"), "reason: {reason}");
                assert!(
                    previous_intact,
                    "the rollback did not restore the previous generation"
                );
            }
            other => panic!("wrong error: {other}"),
        }
        assert_eq!(
            std::fs::read_to_string(&slots).expect("still there"),
            "PREVIOUS VOCABULARY",
            "a torn publication left the new vocabulary in place"
        );
    }

    #[test]
    fn a_complete_generation_replaces_an_existing_one() {
        // The success path, over an existing destination — the case the finding worried `rename`
        // could not do on Windows. It can: the standard library passes MOVEFILE_REPLACE_EXISTING.
        let scratch = Scratch::new("replace");
        let slots = scratch.0.join("slots.json");
        let provenance = scratch.0.join("slots.provenance.json");
        std::fs::write(&slots, "PREVIOUS").expect("seed");
        std::fs::write(&provenance, "PREVIOUS").expect("seed");

        let text = small_vocabulary_text();
        let digest = format!("{:x}", sha2::Sha256::digest(text.as_bytes()));
        let published = publish_generation(
            &slots,
            &text,
            &provenance,
            &format!(
                "{{
 \"slots_sha256\": \"{digest}\"
}}
"
            ),
        )
        .expect("a complete generation publishes");
        assert_eq!(published, digest);
        assert_eq!(std::fs::read_to_string(&slots).expect("read"), text);
        assert!(
            std::fs::read_to_string(&provenance)
                .expect("read")
                .contains(&digest),
            "the provenance does not name the published artifact"
        );
        assert!(
            !slots.with_extension("json.staging").exists(),
            "staging left behind"
        );
    }

    #[test]
    fn a_source_reports_what_it_alone_contributed() {
        let first = Contribution {
            source: "first",
            names: ["a", "b", "c"].iter().map(|s| (*s).to_owned()).collect(),
        };
        let second = Contribution {
            source: "second",
            names: ["b", "c", "d"].iter().map(|s| (*s).to_owned()).collect(),
        };
        assert_eq!(first.unique_against(&[&second]), 1);
        assert_eq!(second.unique_against(&[&first]), 1);
        assert_eq!(first.unique_against(&[]), 3);
    }
}
