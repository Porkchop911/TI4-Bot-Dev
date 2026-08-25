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
use std::path::Path;
use std::sync::Arc;

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
}

/// Source (a): every feature name the r6 champions already carry.
///
/// The union across the six per-faction profiles, read from the weight maps by name. §4.5 records
/// 41,113 for this checkpoint; the figure is reproduced rather than asserted.
///
/// # Errors
/// [`CorpusError::Io`] if the checkpoint cannot be read, [`CorpusError::Checkpoint`] if it does not
/// hold a `profiles` object of the expected shape.
pub fn champion_names(checkpoint: &Path) -> Result<Contribution, CorpusError> {
    let bytes = std::fs::read(checkpoint).map_err(|source| CorpusError::Io {
        path: checkpoint.display().to_string(),
        source,
    })?;
    let document: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| CorpusError::Checkpoint {
            path: checkpoint.display().to_string(),
            reason: format!("not JSON: {error}"),
        })?;
    let profiles: BTreeMap<String, Profile> = serde_json::from_value(document["profiles"].clone())
        .map_err(|error| CorpusError::Checkpoint {
            path: checkpoint.display().to_string(),
            reason: format!("profiles: {error}"),
        })?;
    if profiles.is_empty() {
        return Err(CorpusError::Checkpoint {
            path: checkpoint.display().to_string(),
            reason: "no profiles".to_owned(),
        });
    }
    let mut names = BTreeSet::new();
    for profile in profiles.values() {
        for head in profile.learned.heads.values() {
            names.extend(head.weights.keys().cloned());
        }
    }
    Ok(Contribution {
        source: "r6-champions",
        names,
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
        names,
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
            let mut names = self.names.borrow_mut();
            for vector in ti4_policy::features::explicit_choice_features(
                observed,
                choice,
                &choice.player,
                &held,
            ) {
                names.extend(ti4_policy::features::names_of(&vector));
            }
            for option in &choice.options {
                for (name, _) in ti4_policy::features::option_feature_names(
                    observed,
                    choice,
                    option,
                    &choice.player,
                ) {
                    names.insert(name);
                }
            }
        }
        self.inner.choose_seeing(choice, seen)
    }
}

/// Source (b): every name emitted while replaying the §6.1 teacher seed schedule.
///
/// One bounded pass, single-threaded so the collected set does not depend on thread interleaving —
/// it would not anyway, since the union is a set and assignment is by key, but a discovery pass
/// whose output could depend on scheduling is harder to argue about than one that cannot.
///
/// # Panics
/// If a seat has no profile in `champions`, which is a caller error rather than a game state.
#[must_use]
pub fn replay_names(
    content: &'static ContentStore,
    sources: SourceSet,
    pool: &Arc<ti4_sim::MapPool>,
    champions: &BTreeMap<String, Profile>,
    factions: &[&str],
    seeds: std::ops::Range<u64>,
    tile_seed_offset: u64,
    horizon: crate::rollout::Horizon,
) -> (Contribution, usize) {
    let names = std::rc::Rc::new(std::cell::RefCell::new(BTreeSet::new()));
    let mut games = 0usize;

    let players: Vec<PlayerId> = (0..factions.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();

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
            let _ = rollout;
            games += 1;
        }
    }

    (
        Contribution {
            source: "replay",
            names: std::rc::Rc::try_unwrap(names)
                .expect("every collector is dropped with its game")
                .into_inner(),
        },
        games,
    )
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
