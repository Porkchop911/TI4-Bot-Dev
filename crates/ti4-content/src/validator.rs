//! Referential validation across content categories.
//!
//! Records reference each other by id — a faction names its leaders, a unit names the
//! technology that unlocks it, a deck names its cards. Nothing upstream enforces that those
//! targets exist, and nothing catches a reference that falls out of scope when an expansion
//! is switched off. That second case is the one that actually bit the oracle: with Thunder's
//! Edge out of scope the Naalu silently had two leaders instead of three, because faction
//! records point at `naaluagent-te`. [`ContentStore::resolve_id`] is applied here for the
//! same reason, so validation asks the question the game will ask at setup.
//!
//! Two things this deliberately does not do. It does not check the eight categories nothing
//! reads (`colors`, `combat_modifiers`, `franken_errata`, `galactic_events`, `genericcards`,
//! `map_templates`, `sources`, and `tokens` as a source of references). And it does not
//! treat the known upstream gaps in [`KNOWN_GAPS`] as failures — they are recorded there
//! with a reason rather than suppressed by loosening a rule.

use std::fmt::Write as _;

use ti4_model::content_types::{ContentType, SourceSet};

use crate::error::ReferenceError;
use crate::loader::ContentStore;
use crate::record::Record;

/// Whether a reference field holds one id or a list of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arity {
    One,
    Many,
}

/// Whether a reference must resolve *within the active source set* or merely exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// The reference must be usable in the current scope. Anything a seated faction or a
    /// live board needs is checked this way — that is where the Naalu regression lived.
    InScope,
    /// The reference must exist somewhere in the corpus. Deck records declare their whole
    /// contents regardless of which expansions are switched on, and the game picks a deck
    /// rather than filtering one, so scope-checking them would report false gaps.
    Anywhere,
}

/// "Records in `from` reference records in one of `to` through `field`."
#[derive(Debug, Clone, Copy)]
struct Rule {
    from: ContentType,
    field: &'static str,
    arity: Arity,
    scope: Scope,
    /// Alternative targets, tried in order. More than one where the corpus overloads a
    /// field across categories.
    to: &'static [ContentType],
}

/// The cross-references a game depends on at setup and during play.
const RULES: &[Rule] = &[
    // A faction cannot be seated if any of these is missing in scope.
    Rule {
        from: ContentType::Factions,
        field: "homeSystem",
        arity: Arity::One,
        scope: Scope::InScope,
        to: &[ContentType::Systems],
    },
    Rule {
        from: ContentType::Factions,
        field: "homePlanets",
        arity: Arity::Many,
        scope: Scope::InScope,
        to: &[ContentType::Planets],
    },
    Rule {
        from: ContentType::Factions,
        field: "factionTech",
        arity: Arity::Many,
        scope: Scope::InScope,
        to: &[ContentType::Technologies],
    },
    Rule {
        from: ContentType::Factions,
        field: "startingTech",
        arity: Arity::Many,
        scope: Scope::InScope,
        to: &[ContentType::Technologies],
    },
    Rule {
        from: ContentType::Factions,
        field: "abilities",
        arity: Arity::Many,
        scope: Scope::InScope,
        to: &[ContentType::Abilities],
    },
    Rule {
        from: ContentType::Factions,
        field: "leaders",
        arity: Arity::Many,
        scope: Scope::InScope,
        to: &[ContentType::Leaders],
    },
    Rule {
        from: ContentType::Factions,
        field: "promissoryNotes",
        arity: Arity::Many,
        scope: Scope::InScope,
        to: &[ContentType::PromissoryNotes],
    },
    Rule {
        from: ContentType::Factions,
        field: "units",
        arity: Arity::Many,
        scope: Scope::InScope,
        to: &[ContentType::Units],
    },
    // The unit upgrade chain and its prerequisites.
    Rule {
        from: ContentType::Units,
        field: "requiredTechId",
        arity: Arity::One,
        scope: Scope::InScope,
        to: &[ContentType::Technologies],
    },
    Rule {
        from: ContentType::Units,
        field: "upgradesToUnitId",
        arity: Arity::One,
        scope: Scope::InScope,
        to: &[ContentType::Units],
    },
    Rule {
        from: ContentType::Units,
        field: "upgradesFromUnitId",
        arity: Arity::One,
        scope: Scope::InScope,
        to: &[ContentType::Units],
    },
    // `baseUpgrade` names the *generic* unit-upgrade technology a faction technology
    // replaces (`sol so2` -> `inf2`), not a unit. Every value is a technology alias.
    Rule {
        from: ContentType::Technologies,
        field: "baseUpgrade",
        arity: Arity::One,
        scope: Scope::InScope,
        to: &[ContentType::Technologies],
    },
    // The board.
    Rule {
        from: ContentType::Systems,
        field: "planets",
        arity: Arity::Many,
        scope: Scope::InScope,
        to: &[ContentType::Planets],
    },
    // An exploration outcome either attaches a card to the planet or places a token on it,
    // and the corpus uses one field for both: `mirage`, `gamma`, and `ionalpha` are tokens.
    Rule {
        from: ContentType::Explores,
        field: "attachmentId",
        arity: Arity::One,
        scope: Scope::InScope,
        to: &[ContentType::Attachments, ContentType::Tokens],
    },
    // Strategy card sets name the cards they deal.
    Rule {
        from: ContentType::StrategyCardSets,
        field: "scIDs",
        arity: Arity::Many,
        scope: Scope::InScope,
        to: &[ContentType::StrategyCards],
    },
];

/// Deck `type` values and the category their `cardIDs` point at.
const DECK_TARGETS: &[(&str, ContentType)] = &[
    ("action_card", ContentType::ActionCards),
    ("agenda", ContentType::Agendas),
    ("explore", ContentType::Explores),
    ("public_stage_1_objective", ContentType::PublicObjectives),
    ("public_stage_2_objective", ContentType::PublicObjectives),
    ("relic", ContentType::Relics),
    ("secret_objective", ContentType::SecretObjectives),
    ("technology", ContentType::Technologies),
];

/// References that are known to dangle upstream, with the reason.
///
/// Extraction keeps only the seven official sources and drops 56 homebrew tags, but it does
/// not rewrite records that pointed into what it dropped. These are the survivors of that.
/// Listed one by one rather than by pattern: a new dangling reference is a corpus change,
/// and it should fail until someone looks at it.
const KNOWN_GAPS: &[Gap] = &[
    // A Discordant Stars unit that upstream tags `base`, whose unlocking technology
    // (`dsghemcv`) lives in the dropped `ds` source. The unit is unreachable in play
    // because no faction in the corpus lists it.
    Gap {
        from: ContentType::Units,
        record: "ghemina_carrier2",
        reference: "dsghemcv",
    },
    // `explores_cpti` is the Council-Preview variant explore deck; 18 of its 80 cards are
    // homebrew and were dropped. The deck is not used by the base or PoK setups.
    Gap {
        from: ContentType::Decks,
        record: "explores_cpti",
        reference: "fiveac1",
    },
    Gap {
        from: ContentType::Decks,
        record: "explores_cpti",
        reference: "fiveac2",
    },
    Gap {
        from: ContentType::Decks,
        record: "explores_cpti",
        reference: "fiveac3",
    },
    Gap {
        from: ContentType::Decks,
        record: "explores_cpti",
        reference: "fivetg1",
    },
    Gap {
        from: ContentType::Decks,
        record: "explores_cpti",
        reference: "fivetg2",
    },
    Gap {
        from: ContentType::Decks,
        record: "explores_cpti",
        reference: "fivetg3",
    },
    Gap {
        from: ContentType::Decks,
        record: "explores_cpti",
        reference: "freetech1",
    },
    Gap {
        from: ContentType::Decks,
        record: "explores_cpti",
        reference: "freetech2",
    },
    Gap {
        from: ContentType::Decks,
        record: "explores_cpti",
        reference: "freetech3",
    },
    Gap {
        from: ContentType::Decks,
        record: "explores_cpti",
        reference: "gainarborecagent",
    },
    Gap {
        from: ContentType::Decks,
        record: "explores_cpti",
        reference: "gainnaazagent",
    },
    Gap {
        from: ContentType::Decks,
        record: "explores_cpti",
        reference: "gainnekroagent",
    },
    Gap {
        from: ContentType::Decks,
        record: "explores_cpti",
        reference: "gainsardakkagent",
    },
    Gap {
        from: ContentType::Decks,
        record: "explores_cpti",
        reference: "gaintitansagent",
    },
    Gap {
        from: ContentType::Decks,
        record: "explores_cpti",
        reference: "gainwinnuagent",
    },
    Gap {
        from: ContentType::Decks,
        record: "explores_cpti",
        reference: "gainxxchaagent",
    },
    Gap {
        from: ContentType::Decks,
        record: "explores_cpti",
        reference: "gainyinagent",
    },
    Gap {
        from: ContentType::Decks,
        record: "explores_cpti",
        reference: "kel3",
    },
];

/// One allowlisted dangling reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Gap {
    from: ContentType,
    record: &'static str,
    reference: &'static str,
}

/// The outcome of validating a corpus against one source scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationReport {
    /// References that could not be resolved and are not in [`KNOWN_GAPS`].
    pub broken: Vec<ReferenceError>,
    /// References that could not be resolved but are allowlisted.
    pub allowed: Vec<ReferenceError>,
    /// How many references were followed.
    pub checked: usize,
}

impl ValidationReport {
    /// Whether every reference resolved, ignoring the allowlisted gaps.
    #[must_use]
    pub const fn is_clean(&self) -> bool {
        self.broken.is_empty()
    }

    /// A short multi-line summary of what broke, for error messages.
    #[must_use]
    pub fn summary(&self) -> String {
        if self.is_clean() {
            return format!(
                "{} references checked, {} allowlisted gaps",
                self.checked,
                self.allowed.len()
            );
        }
        let mut out = format!("{} broken references:\n", self.broken.len());
        for error in self.broken.iter().take(20) {
            let _ = writeln!(out, "  {error}");
        }
        out
    }
}

/// Follow every modelled reference in the corpus within a source scope.
///
/// A reference resolves if the target id exists (in scope, for [`Scope::InScope`] rules)
/// directly or through the Thunder's Edge suffix fallback.
#[must_use]
pub fn validate(store: &ContentStore, sources: SourceSet) -> ValidationReport {
    let mut report = ValidationReport {
        broken: Vec::new(),
        allowed: Vec::new(),
        checked: 0,
    };

    for rule in RULES {
        for record in store.from_sources(rule.from, sources) {
            let references: Vec<&str> = match rule.arity {
                Arity::One => record.text(rule.field).into_iter().collect(),
                Arity::Many => record.strings(rule.field),
            };
            for reference in references {
                check(
                    store,
                    sources,
                    record,
                    rule.field,
                    rule.to,
                    rule.scope,
                    reference,
                    &mut report,
                );
            }
        }
    }

    for deck in store.from_sources(ContentType::Decks, sources) {
        let Some(target) = deck
            .text("type")
            .and_then(|t| DECK_TARGETS.iter().find(|(name, _)| *name == t))
            .map(|(_, category)| *category)
        else {
            continue;
        };
        for reference in deck.strings("cardIDs") {
            check(
                store,
                sources,
                deck,
                "cardIDs",
                std::slice::from_ref(&target),
                Scope::Anywhere,
                reference,
                &mut report,
            );
        }
    }

    report.broken.sort_by(order);
    report.allowed.sort_by(order);
    report
}

fn order(a: &ReferenceError, b: &ReferenceError) -> std::cmp::Ordering {
    (a.category.to_string(), &a.record_id, a.field, &a.reference).cmp(&(
        b.category.to_string(),
        &b.record_id,
        b.field,
        &b.reference,
    ))
}

#[allow(clippy::too_many_arguments)]
fn check(
    store: &ContentStore,
    sources: SourceSet,
    record: &Record,
    field: &'static str,
    targets: &[ContentType],
    scope: Scope,
    reference: &str,
    report: &mut ValidationReport,
) {
    // A present-but-empty field is unset, not a dangling reference. The `neutral`
    // placeholder faction carries `homeSystem: ""` because it is never seated.
    if reference.is_empty() {
        return;
    }
    report.checked += 1;

    let resolved = targets.iter().any(|&target| match scope {
        Scope::InScope => store.resolve_id(target, reference, sources).is_some(),
        Scope::Anywhere => store.get(target, reference).is_some(),
    });
    if resolved {
        return;
    }

    let record_id = record.id().unwrap_or("<composite>");
    let error = ReferenceError {
        category: record.category(),
        record_id: record_id.to_owned(),
        field,
        target: targets[0],
        reference: reference.to_owned(),
    };

    let known = KNOWN_GAPS.iter().any(|gap| {
        gap.from == record.category() && gap.record == record_id && gap.reference == reference
    });
    if known {
        report.allowed.push(error);
    } else {
        report.broken.push(error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::content_types::{BASE, FULL, POK};

    fn store() -> &'static ContentStore {
        ContentStore::embedded()
    }

    #[test]
    fn the_full_corpus_has_no_unexpected_broken_references() {
        let report = validate(store(), FULL);
        assert!(report.is_clean(), "{}", report.summary());
        assert!(
            report.checked > 1000,
            "only {} refs checked",
            report.checked
        );
    }

    #[test]
    fn the_pok_corpus_has_no_unexpected_broken_references() {
        // The TE suffix fallback is what makes this pass: faction records point at `-te`
        // ids that do not exist when Thunder's Edge is out of scope.
        let report = validate(store(), POK);
        assert!(report.is_clean(), "{}", report.summary());
    }

    /// The base game has no leaders and no mechs, so under a base-only scope every faction
    /// record points at content that does not exist. That is the corpus describing the real
    /// game, not a defect: `leaders.json` is entirely `pok`/`thunders_edge`, mechs are
    /// `codex3` and later, and Arborec's `md` is the codex 4 printing of Magen Defense Grid
    /// (`md_base` is the base one). BASE is therefore a scope for reading strategy cards,
    /// not for seating a faction — which is why the oracle defaults `factions()` to FULL and
    /// only `strategy_cards()` to BASE.
    ///
    /// Pinned as a characterisation test so that a *different* breakage under BASE still
    /// fails, rather than being lost in an expected pile of noise.
    #[test]
    fn the_base_scope_lacks_leaders_and_mechs_by_design() {
        let report = validate(store(), BASE);
        let unresolved: Vec<&ReferenceError> = report.broken.iter().collect();

        let leaders = unresolved.iter().filter(|e| e.field == "leaders").count();
        // `contains` rather than `ends_with`: the Naalu mech is `naalu_mech_te`, the same
        // TE-suffixed id that made their leader vanish in the oracle.
        let mechs = unresolved
            .iter()
            .filter(|e| e.field == "units" && e.reference.contains("_mech"))
            .count();
        let tech = unresolved
            .iter()
            .filter(|e| e.field == "startingTech")
            .count();

        assert_eq!(leaders, 51, "17 base factions with 3 leaders each");
        assert_eq!(mechs, 17, "one mech per base faction");
        assert_eq!(tech, 1, "only Arborec's codex-4 Magen Defense Grid");
        assert_eq!(
            leaders + mechs + tech,
            unresolved.len(),
            "an unexpected kind of gap appeared under BASE:\n{}",
            report.summary()
        );
    }

    #[test]
    fn an_empty_reference_is_unset_not_broken() {
        // The `neutral` placeholder faction is never seated and has no home system.
        let neutral = store().get(ContentType::Factions, "neutral").unwrap();
        assert_eq!(neutral.text("homeSystem"), Some(""));
        assert!(validate(store(), FULL).is_clean());
    }

    #[test]
    fn every_allowlisted_gap_is_still_a_real_gap() {
        // An allowlist that outlives the problem it documents is worse than no allowlist.
        let report = validate(store(), FULL);
        assert_eq!(
            report.allowed.len(),
            KNOWN_GAPS.len(),
            "allowlist and corpus disagree: {} entries, {} hit",
            KNOWN_GAPS.len(),
            report.allowed.len()
        );
    }

    #[test]
    fn every_faction_keeps_its_full_complement_when_te_is_out_of_scope() {
        // The Naalu regression: three leaders under FULL must still be three under POK.
        for faction in store().factions(POK) {
            let leaders = faction.strings("leaders");
            let resolved = leaders
                .iter()
                .filter(|id| store().resolve_id(ContentType::Leaders, id, POK).is_some())
                .count();
            assert_eq!(
                resolved,
                leaders.len(),
                "{:?} lost a leader under POK",
                faction.id()
            );
        }
    }

    #[test]
    fn an_exploration_outcome_may_attach_a_card_or_place_a_token() {
        // `mirage` is a token, `biotic` is an attachment; one field carries both.
        assert!(store().get(ContentType::Tokens, "mirage").is_some());
        assert!(store().get(ContentType::Attachments, "mirage").is_none());
        assert!(store().get(ContentType::Attachments, "biotic").is_some());
    }

    #[test]
    fn a_faction_technology_names_the_generic_upgrade_it_replaces() {
        // Sol's Advanced Carrier II replaces the generic Carrier II technology.
        let so2 = store().get(ContentType::Technologies, "so2").unwrap();
        assert_eq!(so2.text("baseUpgrade"), Some("inf2"));
        assert!(store().get(ContentType::Technologies, "inf2").is_some());
    }

    #[test]
    fn a_reference_into_a_dropped_homebrew_source_is_reported_not_hidden() {
        let report = validate(store(), FULL);
        let ghemina = report
            .allowed
            .iter()
            .find(|e| e.record_id == "ghemina_carrier2")
            .expect("the known ghemina gap must still be detected");
        assert_eq!(ghemina.reference, "dsghemcv");
        assert_eq!(ghemina.field, "requiredTechId");
    }

    #[test]
    fn validation_is_deterministic() {
        assert_eq!(validate(store(), FULL), validate(store(), FULL));
    }
}
