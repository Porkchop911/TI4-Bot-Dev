//! Reading the content corpus.
//!
//! This mirrors the oracle's `engine/content.py`. Two properties of that module are
//! load-bearing and are preserved exactly here:
//!
//! * **File order is the iteration order.** Decks are built from these sequences and then
//!   shuffled with a seeded RNG, so reordering a category changes every seeded game.
//!   `strategy_cards` is the single exception: it re-sorts by initiative.
//! * **A source filter compares against the record's own `source` tag.** The three
//!   untagged categories therefore yield nothing under any filter and must be read
//!   unfiltered.
//!
//! The corpus is compiled into the binary. A simulation harness that reads content
//! relative to the current directory is one `cd` away from loading nothing, and the data
//! is ~1 MB of immutable text that is versioned alongside the code.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

use serde_json::Value;
use ti4_model::content_types::{ALL_CONTENT_TYPES, ContentType, SourceSet};

use crate::error::ContentError;
use crate::manifest::Manifest;
use crate::record::{Record, type_name};

/// Suffixes that mark a Thunder's Edge replacement of an existing record.
///
/// Faction records point at the newer id — `naaluagent-te`, `naalu_mech_te` — so a
/// suffixed id falls back to its base record and a faction is never quietly left short of
/// a leader or a mech when TE content is out of scope.
const SOURCE_SUFFIXES: [&str; 2] = ["-te", "_te"];

/// Suffixes marking a newer printing of a component, newest first.
///
/// A component reprinted by a later release keeps its original id and gains a suffixed sibling:
/// `xxchahero` and `xxchahero-te`, `naalu_mech` and `naalu_mech_omega`. When both are in scope the
/// newer one is the card actually in the box, so that is the one a lookup should find.
///
/// Ordered newest-first, and Thunder's Edge is newer than the Codex Omega printings.
const NEWER_PRINTINGS: [&str; 4] = ["-te", "_te", "_omega", "-omega"];

/// The corpus, compiled in. Order matches [`ALL_CONTENT_TYPES`].
const EMBEDDED: [(ContentType, &str); 28] = [
    (
        ContentType::Abilities,
        include_str!("../content/abilities.json"),
    ),
    (
        ContentType::ActionCards,
        include_str!("../content/action_cards.json"),
    ),
    (
        ContentType::Agendas,
        include_str!("../content/agendas.json"),
    ),
    (
        ContentType::Attachments,
        include_str!("../content/attachments.json"),
    ),
    (
        ContentType::Breakthroughs,
        include_str!("../content/breakthroughs.json"),
    ),
    (ContentType::Colors, include_str!("../content/colors.json")),
    (
        ContentType::CombatModifiers,
        include_str!("../content/combat_modifiers.json"),
    ),
    (ContentType::Decks, include_str!("../content/decks.json")),
    (
        ContentType::Explores,
        include_str!("../content/explores.json"),
    ),
    (
        ContentType::Factions,
        include_str!("../content/factions.json"),
    ),
    (
        ContentType::FrankenErrata,
        include_str!("../content/franken_errata.json"),
    ),
    (
        ContentType::GalacticEvents,
        include_str!("../content/galactic_events.json"),
    ),
    (
        ContentType::GenericCards,
        include_str!("../content/genericcards.json"),
    ),
    (
        ContentType::Leaders,
        include_str!("../content/leaders.json"),
    ),
    (
        ContentType::MapTemplates,
        include_str!("../content/map_templates.json"),
    ),
    (
        ContentType::Planets,
        include_str!("../content/planets.json"),
    ),
    (
        ContentType::PromissoryNotes,
        include_str!("../content/promissory_notes.json"),
    ),
    (
        ContentType::PublicObjectives,
        include_str!("../content/public_objectives.json"),
    ),
    (ContentType::Relics, include_str!("../content/relics.json")),
    (ContentType::Rules, include_str!("../content/rules.json")),
    (
        ContentType::SecretObjectives,
        include_str!("../content/secret_objectives.json"),
    ),
    (
        ContentType::Sources,
        include_str!("../content/sources.json"),
    ),
    (
        ContentType::StrategyCardSets,
        include_str!("../content/strategy_card_sets.json"),
    ),
    (
        ContentType::StrategyCards,
        include_str!("../content/strategy_cards.json"),
    ),
    (
        ContentType::Systems,
        include_str!("../content/systems.json"),
    ),
    (
        ContentType::Technologies,
        include_str!("../content/technologies.json"),
    ),
    (ContentType::Tokens, include_str!("../content/tokens.json")),
    (ContentType::Units, include_str!("../content/units.json")),
];

const EMBEDDED_MANIFEST: &str = include_str!("../content/manifest.json");

static STORE: OnceLock<ContentStore> = OnceLock::new();

/// One category: its records in file order, plus an identity index into them.
#[derive(Debug, Clone)]
struct Category {
    records: Vec<Record>,
    /// Identity to position. `BTreeMap` so that iterating the index is deterministic;
    /// empty for `franken_errata`, which has no single-field key.
    by_id: BTreeMap<String, usize>,
}

/// The loaded content corpus.
#[derive(Debug, Clone)]
pub struct ContentStore {
    categories: BTreeMap<ContentType, Category>,
    manifest: Manifest,
}

impl ContentStore {
    /// The corpus compiled into this binary, parsed once per process.
    ///
    /// # Panics
    /// Panics if the compiled-in corpus fails to parse or disagrees with its manifest.
    /// That is a build-time defect rather than a runtime condition — the data cannot
    /// change after compilation, and `embedded_corpus_parses` proves it does not happen.
    /// Use [`ContentStore::parse_embedded`] where a fallible form is wanted.
    #[must_use]
    pub fn embedded() -> &'static Self {
        STORE.get_or_init(|| {
            Self::parse_embedded().expect("the compiled-in content corpus must be well formed")
        })
    }

    /// Parse the compiled-in corpus without caching.
    ///
    /// # Errors
    /// Returns [`ContentError`] if any category is malformed or the counts disagree with
    /// `manifest.json`.
    pub fn parse_embedded() -> Result<Self, ContentError> {
        Self::parse(&EMBEDDED, EMBEDDED_MANIFEST)
    }

    /// Read a corpus from a directory of category files plus `manifest.json`.
    ///
    /// Used to load a corpus that is not the compiled-in one — a regenerated extraction,
    /// or a reduced fixture corpus.
    ///
    /// # Errors
    /// Returns [`ContentError::Io`] if a file is missing or unreadable, or any parse or
    /// manifest-consistency error.
    pub fn from_dir(dir: &Path) -> Result<Self, ContentError> {
        let mut texts = Vec::with_capacity(ALL_CONTENT_TYPES.len());
        for &category in ALL_CONTENT_TYPES {
            let path = dir.join(category.json_filename());
            let text = std::fs::read_to_string(&path).map_err(|source| ContentError::Io {
                path: path.clone(),
                source,
            })?;
            texts.push((category, text));
        }
        let manifest_path = dir.join("manifest.json");
        let manifest_text =
            std::fs::read_to_string(&manifest_path).map_err(|source| ContentError::Io {
                path: manifest_path,
                source,
            })?;

        let borrowed: Vec<(ContentType, &str)> =
            texts.iter().map(|(c, t)| (*c, t.as_str())).collect();
        Self::parse(&borrowed, &manifest_text)
    }

    fn parse(
        categories: &[(ContentType, &str)],
        manifest_json: &str,
    ) -> Result<Self, ContentError> {
        let manifest = Manifest::parse(manifest_json)?;
        let mut loaded = BTreeMap::new();

        for &(category, json) in categories {
            let file = category.json_filename();
            let value: Value =
                serde_json::from_str(json).map_err(|source| ContentError::Json { file, source })?;
            let Value::Array(items) = value else {
                return Err(ContentError::NotAnArray {
                    file,
                    found: type_name(&value),
                });
            };

            let mut records = Vec::with_capacity(items.len());
            let mut by_id = BTreeMap::new();
            for (index, item) in items.into_iter().enumerate() {
                let record = Record::new(category, index, item)?;
                if let Some(id) = record.id() {
                    if let Some(&first) = by_id.get(id) {
                        return Err(ContentError::DuplicateIdentity {
                            category,
                            id: id.to_owned(),
                            first,
                            second: index,
                        });
                    }
                    by_id.insert(id.to_owned(), index);
                }
                records.push(record);
            }
            loaded.insert(category, Category { records, by_id });
        }

        let store = Self {
            categories: loaded,
            manifest,
        };
        store.check_manifest()?;
        Ok(store)
    }

    /// Verify the loaded records against the counts the manifest claims.
    ///
    /// A corpus and a manifest that came from different extractions is the failure this
    /// catches: the records would load fine and every downstream count would be quietly
    /// wrong.
    fn check_manifest(&self) -> Result<(), ContentError> {
        let mismatch = |detail: String| ContentError::ManifestMismatch { detail };

        let claimed = usize::try_from(self.manifest.totals.categories).unwrap_or(usize::MAX);
        if claimed != self.categories.len() {
            return Err(mismatch(format!(
                "manifest claims {claimed} categories, corpus has {}",
                self.categories.len()
            )));
        }

        let mut total = 0_u64;
        let mut untagged_total = 0_u64;
        for (&category, loaded) in &self.categories {
            let name = category.to_string();
            let counts = self
                .manifest
                .category(&name)
                .ok_or_else(|| mismatch(format!("manifest has no entry for {name}")))?;

            let records = loaded.records.len() as u64;
            if counts.records != records {
                return Err(mismatch(format!(
                    "manifest claims {} {name} records, corpus has {records}",
                    counts.records
                )));
            }

            let untagged = loaded
                .records
                .iter()
                .filter(|r| r.source().is_none())
                .count() as u64;
            if counts.untagged != untagged {
                return Err(mismatch(format!(
                    "manifest claims {} untagged {name} records, corpus has {untagged}",
                    counts.untagged
                )));
            }

            for (tag, &expected) in &counts.by_source {
                let actual = loaded
                    .records
                    .iter()
                    .filter(|r| r.source().is_some_and(|s| s.to_string() == *tag))
                    .count() as u64;
                if expected != actual {
                    return Err(mismatch(format!(
                        "manifest claims {expected} {name} records from {tag}, corpus has {actual}"
                    )));
                }
            }

            total += records;
            untagged_total += untagged;
        }

        if self.manifest.totals.records != total {
            return Err(mismatch(format!(
                "manifest claims {} records in total, corpus has {total}",
                self.manifest.totals.records
            )));
        }
        if self.manifest.totals.untagged != untagged_total {
            return Err(mismatch(format!(
                "manifest claims {} untagged records in total, corpus has {untagged_total}",
                self.manifest.totals.untagged
            )));
        }
        Ok(())
    }

    /// Provenance for this corpus.
    #[must_use]
    pub const fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Every record in a category, in file order.
    #[must_use]
    pub fn records(&self, category: ContentType) -> &[Record] {
        self.categories
            .get(&category)
            .map_or(&[], |c| c.records.as_slice())
    }

    /// Records in a category limited to a set of expansions, in file order.
    ///
    /// Yields nothing for the three untagged categories; use [`Self::records`] for those.
    pub fn from_sources(
        &self,
        category: ContentType,
        sources: SourceSet,
    ) -> impl Iterator<Item = &Record> {
        self.records(category)
            .iter()
            .filter(move |r| r.in_sources(sources))
    }

    /// One record by its identity, ignoring source scope.
    #[must_use]
    pub fn get(&self, category: ContentType, id: &str) -> Option<&Record> {
        let category = self.categories.get(&category)?;
        category.by_id.get(id).map(|&i| &category.records[i])
    }

    /// An identity index for a category within a source scope.
    ///
    /// This is the Rust form of the per-module `catalogue(sources)` pattern the oracle
    /// repeats in `units.py`, `factions.py`, `leaders.py`, and the rest.
    #[must_use]
    pub fn catalogue(&self, category: ContentType, sources: SourceSet) -> BTreeMap<&str, &Record> {
        self.from_sources(category, sources)
            .filter_map(|r| r.id().map(|id| (id, r)))
            .collect()
    }

    /// Resolve an id within a source scope, falling back to the base record of a
    /// Thunder's Edge replacement.
    ///
    /// Returns the id that actually exists in scope, or `None`.
    #[must_use]
    pub fn resolve_id<'a>(
        &'a self,
        category: ContentType,
        id: &'a str,
        sources: SourceSet,
    ) -> Option<&'a str> {
        let in_scope = |candidate: &str| {
            self.get(category, candidate)
                .is_some_and(|r| r.in_sources(sources))
        };
        // A newer printing in scope is the card in the box, so it wins over the original. Checked
        // before the plain id: asking for `xxchahero` in a Thunder's Edge game should find the
        // Thunder's Edge hero, not the one it replaced.
        if !NEWER_PRINTINGS.iter().any(|suffix| id.ends_with(suffix))
            && let Some(newer) = NEWER_PRINTINGS.iter().find_map(|suffix| {
                let candidate = format!("{id}{suffix}");
                self.get(category, &candidate)
                    .filter(|record| record.in_sources(sources))
                    .and_then(Record::id)
            })
        {
            return Some(newer);
        }
        if in_scope(id) {
            return Some(id);
        }
        // A newer printing asked for but out of scope falls back to what it replaced.
        SOURCE_SUFFIXES.iter().find_map(|suffix| {
            let base = id.strip_suffix(suffix)?;
            in_scope(base).then_some(base)
        })
    }

    /// Strategy cards for a source set, in initiative order.
    ///
    /// The only category the oracle re-sorts. Ties keep file order.
    #[must_use]
    pub fn strategy_cards(&self, sources: SourceSet) -> Vec<&Record> {
        let mut cards: Vec<&Record> = self
            .from_sources(ContentType::StrategyCards, sources)
            .collect();
        cards.sort_by_key(|c| c.int("initiative").unwrap_or(99));
        cards
    }

    /// Factions in a source set, in file order.
    pub fn factions(&self, sources: SourceSet) -> impl Iterator<Item = &Record> {
        self.from_sources(ContentType::Factions, sources)
    }

    /// Factions at a stated complexity, e.g. `"Low"`.
    ///
    /// The corpus rates mechanical simplicity, not strategic difficulty — Jol-Nar is rated
    /// Low despite being awkward to pilot.
    pub fn factions_by_complexity<'a>(
        &'a self,
        level: &'a str,
        sources: SourceSet,
    ) -> impl Iterator<Item = &'a Record> {
        self.factions(sources)
            .filter(move |f| f.text("complexity") == Some(level))
    }

    /// Total records across every category.
    #[must_use]
    pub fn total_records(&self) -> usize {
        self.categories.values().map(|c| c.records.len()).sum()
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_lookup_finds_the_newest_printing_in_scope() {
        // The standing rule for this project: latest components, Thunder's Edge and PoK enabled,
        // Omega where it exists. A component reprinted by a later release keeps its original id and
        // gains a suffixed sibling, and when both are in the box the newer one is the card being
        // played — so asking for the original must find the reprint rather than what it replaced.
        let store = ContentStore::embedded();

        assert_eq!(
            store.resolve_id(ContentType::Leaders, "xxchahero", FULL),
            Some("xxchahero-te"),
            "Xxcha's hero has a Thunder's Edge printing and Xxcha is a faction we play"
        );
        assert_eq!(
            store.resolve_id(ContentType::Units, "naalu_mech", POK),
            Some("naalu_mech_omega"),
            "the Codex Omega printing applies even without Thunder's Edge"
        );
        assert_eq!(
            store.resolve_id(ContentType::Units, "naalu_mech", FULL),
            Some("naalu_mech_te"),
            "and Thunder's Edge is newer than Omega"
        );
    }

    #[test]
    fn a_component_with_no_reprint_resolves_to_itself() {
        // Nearly everything. A rule that quietly rewrote ids it should not touch would be far
        // worse than one that missed a reprint.
        let store = ContentStore::embedded();
        for id in ["solhero", "solagent", "hacanhero", "letnevhero"] {
            assert_eq!(
                store.resolve_id(ContentType::Leaders, id, FULL),
                Some(id),
                "{id} was rewritten and has no newer printing"
            );
        }
    }

    #[test]
    fn asking_for_a_reprint_out_of_scope_still_falls_back_to_what_it_replaced() {
        // The original behaviour, which has to survive: a game scoped without Thunder's Edge that
        // names a Thunder's Edge id should get the card it reprinted, not nothing.
        let store = ContentStore::embedded();
        assert_eq!(
            store.resolve_id(ContentType::Leaders, "xxchahero-te", POK),
            Some("xxchahero")
        );
    }

    #[test]
    fn asking_for_a_reprint_directly_returns_it_rather_than_looping() {
        // An id that already carries a newer-printing suffix must not have another appended.
        let store = ContentStore::embedded();
        assert_eq!(
            store.resolve_id(ContentType::Leaders, "xxchahero-te", FULL),
            Some("xxchahero-te")
        );
    }

    #[test]
    fn the_project_default_scope_is_everything() {
        // Stated as an assertion so that narrowing it is a deliberate edit rather than a drift.
        use ti4_model::content_types::DEFAULT;
        assert_eq!(DEFAULT, FULL);
        for source in [
            Source::Base,
            Source::Pok,
            Source::ThundersEdge,
            Source::Codex4,
        ] {
            assert!(
                DEFAULT.contains(source),
                "{source:?} is not in the default scope"
            );
        }
    }
    use super::*;
    use ti4_model::content_types::{BASE, FULL, IdentityKey, POK, Source};

    fn store() -> &'static ContentStore {
        ContentStore::embedded()
    }

    #[test]
    fn embedded_corpus_parses_and_agrees_with_its_manifest() {
        // parse_embedded runs check_manifest, so a clean parse is the whole assertion.
        let store = ContentStore::parse_embedded().expect("embedded corpus must parse");
        assert_eq!(store.total_records(), 1800);
        assert_eq!(store.manifest().totals.records, 1800);
        assert_eq!(store.manifest().totals.categories, 28);
        assert_eq!(store.manifest().totals.untagged, 237);
    }

    #[test]
    fn the_corpus_reports_its_own_provenance() {
        let m = store().manifest();
        assert_eq!(m.upstream.project, "AsyncTI4/TI4_map_generator_bot");
        assert_eq!(
            m.upstream.commit,
            "8e90459d789fb767b9d5aff3a55bd7dd0b3e781b"
        );
        assert_eq!(m.schema_version, "1.1.0");
        assert_eq!(m.official_sources.len(), 7);
    }

    #[test]
    fn every_category_declared_in_the_model_is_present_and_non_empty() {
        for &category in ALL_CONTENT_TYPES {
            assert!(
                !store().records(category).is_empty(),
                "{category} loaded no records"
            );
        }
    }

    /// The reverse direction: a file added to the corpus must be added to `ContentType`.
    /// This is the check that would have caught the previous, invented category list.
    #[test]
    fn every_corpus_file_is_declared_in_the_model() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("content");
        let declared: Vec<&str> = ALL_CONTENT_TYPES
            .iter()
            .map(|c| c.json_filename())
            .collect();
        let mut found = Vec::new();
        for entry in std::fs::read_dir(&dir).expect("content directory must exist") {
            let name = entry.expect("readable entry").file_name();
            let name = name.to_string_lossy().into_owned();
            if std::path::Path::new(&name)
                .extension()
                .is_some_and(|e| e == "json")
                && name != "manifest.json"
            {
                assert!(
                    declared.contains(&name.as_str()),
                    "{name} has no ContentType"
                );
                found.push(name);
            }
        }
        assert_eq!(found.len(), declared.len());
    }

    #[test]
    fn category_record_counts_match_the_oracle() {
        // Spot counts taken from the corpus; the manifest cross-check covers the rest.
        assert_eq!(store().records(ContentType::Units).len(), 125);
        assert_eq!(store().records(ContentType::Planets).len(), 159);
        assert_eq!(store().records(ContentType::Systems).len(), 231);
        assert_eq!(store().records(ContentType::Factions).len(), 34);
        assert_eq!(store().records(ContentType::Technologies).len(), 102);
    }

    #[test]
    fn file_order_is_preserved() {
        let records = store().records(ContentType::StrategyCards);
        for (i, r) in records.iter().enumerate() {
            assert_eq!(r.index(), i);
        }
        // First record of the file, not of any sorted view.
        assert_eq!(records[0].index(), 0);
    }

    #[test]
    fn the_base_game_has_seventeen_factions() {
        assert_eq!(store().factions(BASE).count(), 17);
    }

    #[test]
    fn every_official_faction_is_reachable() {
        assert_eq!(store().catalogue(ContentType::Factions, FULL).len(), 34);
    }

    #[test]
    fn thunders_edge_factions_are_out_of_scope_under_pok() {
        let full = store().factions(FULL).count();
        let pok = store().factions(POK).count();
        assert!(pok < full, "TE must add factions: {pok} vs {full}");
        assert!(
            store()
                .factions(POK)
                .all(|f| f.source() != Some(Source::ThundersEdge))
        );
    }

    #[test]
    fn low_complexity_factions_are_listed() {
        let low: Vec<&str> = store()
            .factions_by_complexity("Low", FULL)
            .filter_map(super::Record::id)
            .collect();
        assert!(!low.is_empty());
        assert!(
            low.contains(&"sol"),
            "Sol should be a low-complexity faction"
        );
    }

    #[test]
    fn the_eight_classic_strategy_cards_come_from_the_corpus() {
        let cards = store().strategy_cards(BASE);
        assert_eq!(cards.len(), 8);
        let initiatives: Vec<i64> = cards.iter().map(|c| c.int("initiative").unwrap()).collect();
        assert_eq!(initiatives, vec![1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(cards[0].text("name"), Some("Leadership"));
    }

    #[test]
    fn strategy_cards_are_sorted_by_initiative_not_file_order() {
        let sorted = store().strategy_cards(FULL);
        let mut previous = 0;
        for card in &sorted {
            let initiative = card.int("initiative").unwrap_or(99);
            assert!(initiative >= previous, "initiative went backwards");
            previous = initiative;
        }
    }

    #[test]
    fn an_untagged_category_is_empty_under_every_source_filter() {
        for category in [
            ContentType::Colors,
            ContentType::CombatModifiers,
            ContentType::MapTemplates,
        ] {
            assert_eq!(
                store().from_sources(category, FULL).count(),
                0,
                "{category} must be read unfiltered"
            );
            assert!(!store().records(category).is_empty());
        }
    }

    #[test]
    fn a_source_filter_keeps_file_order() {
        let filtered: Vec<usize> = store()
            .from_sources(ContentType::Units, POK)
            .map(super::Record::index)
            .collect();
        let mut sorted = filtered.clone();
        sorted.sort_unstable();
        assert_eq!(filtered, sorted);
    }

    #[test]
    fn a_known_id_resolves_to_itself() {
        assert_eq!(
            store().resolve_id(ContentType::Units, "carrier", FULL),
            Some("carrier")
        );
    }

    #[test]
    fn a_unit_upgrade_shares_its_base_type_and_improves_it() {
        let store = store();
        let carrier = store.get(ContentType::Units, "carrier").unwrap();
        let carrier2 = store.get(ContentType::Units, "carrier2").unwrap();
        assert_eq!(carrier.text("baseType"), carrier2.text("baseType"));
        assert_eq!(carrier.text("upgradesToUnitId"), Some("carrier2"));
        assert_eq!(carrier2.text("upgradesFromUnitId"), Some("carrier"));
        assert!(carrier2.int("moveValue") > carrier.int("moveValue"));
    }

    #[test]
    fn a_te_suffixed_id_falls_back_to_its_base_record_when_te_is_out_of_scope() {
        // naaluagent-te exists only under Thunder's Edge; naaluagent is the POK record.
        assert_eq!(
            store().resolve_id(ContentType::Leaders, "naaluagent-te", FULL),
            Some("naaluagent-te")
        );
        assert_eq!(
            store().resolve_id(ContentType::Leaders, "naaluagent-te", POK),
            Some("naaluagent")
        );
    }

    #[test]
    fn an_unknown_id_resolves_to_nothing() {
        assert_eq!(
            store().resolve_id(ContentType::Units, "nonesuch", FULL),
            None
        );
        assert_eq!(
            store().resolve_id(ContentType::Units, "nonesuch-te", FULL),
            None
        );
    }

    #[test]
    fn identities_are_unique_within_every_keyed_category() {
        // Duplicates are rejected at load; this asserts the index is complete, i.e. that
        // no record was silently skipped.
        for &category in ALL_CONTENT_TYPES {
            if category.identity_key() == IdentityKey::Composite {
                continue;
            }
            let store = store();
            let indexed = store.catalogue(category, FULL).len();
            let in_scope = store.from_sources(category, FULL).count();
            let expected = if category.is_source_tagged() {
                in_scope
            } else {
                0
            };
            assert_eq!(indexed, expected, "{category} index is incomplete");
        }
    }

    #[test]
    fn franken_errata_is_loaded_but_not_indexed() {
        assert_eq!(store().records(ContentType::FrankenErrata).len(), 140);
        assert!(
            store()
                .catalogue(ContentType::FrankenErrata, FULL)
                .is_empty()
        );
    }

    #[test]
    fn repeated_loads_of_the_same_corpus_are_identical() {
        let a = ContentStore::parse_embedded().unwrap();
        let b = ContentStore::parse_embedded().unwrap();
        assert_eq!(a.total_records(), b.total_records());
        for &category in ALL_CONTENT_TYPES {
            assert_eq!(a.records(category), b.records(category));
        }
    }

    #[test]
    fn a_corpus_read_from_disk_matches_the_embedded_one() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("content");
        let on_disk = ContentStore::from_dir(&dir).expect("content directory must load");
        for &category in ALL_CONTENT_TYPES {
            assert_eq!(on_disk.records(category), store().records(category));
        }
        assert_eq!(on_disk.manifest(), store().manifest());
    }

    #[test]
    fn a_manifest_that_disagrees_with_the_corpus_is_rejected() {
        let mut manifest: serde_json::Value = serde_json::from_str(EMBEDDED_MANIFEST).unwrap();
        manifest["totals"]["records"] = serde_json::json!(1799);
        let err = ContentStore::parse(&EMBEDDED, &manifest.to_string()).unwrap_err();
        assert!(
            matches!(err, ContentError::ManifestMismatch { .. }),
            "{err}"
        );
    }

    #[test]
    fn a_duplicate_identity_is_rejected() {
        let doubled = r#"[{"id": "x", "source": "base"}, {"id": "x", "source": "base"}]"#;
        let err =
            ContentStore::parse(&[(ContentType::Units, doubled)], EMBEDDED_MANIFEST).unwrap_err();
        assert!(
            matches!(
                err,
                ContentError::DuplicateIdentity {
                    first: 0,
                    second: 1,
                    ..
                }
            ),
            "{err}"
        );
    }
}
