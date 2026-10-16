//! Semantic effect tags for cards (card-text features, 2026-10-09).
//!
//! The artifact `card_tags.json` maps every card with rules text to a sorted list of effect tags
//! drawn from a closed taxonomy (`grants-move`, `plus1-combat`, ...). A card the artifact does not
//! know -- homebrew, or one added to the corpus later -- has **no tags**: absent, never an error.
//!
//! # Why the tags exist
//!
//! The faction decomposition in `features::ability_facts` names *identities* (`ability:mitosis`).
//! A faction the policy has never trained on brings identity strings it has no column weights for.
//! A tag names a *meaning*, so a column trained on one faction's `plus1-combat` fires for another
//! faction's card with the same effect.
//!
//! # Keys
//!
//! Cards key on `(kind, id)`, not on the id alone: seven ids are shared across kinds (`sar` is both
//! a secret objective and a technology). The artifact stores `"<kind>:<id>"`; the kind names are
//! the ones in [`kind`].
//!
//! # Provenance
//!
//! Generated from `plans/CARD_TAGS_DRAFT_2026-10-09.json` (labelled, then twice reviewed) with the
//! tag `deal-enforcement` removed. Its SHA-256 is pinned by a test; changing the artifact is a
//! deliberate act that moves the pin and the vocabulary together. The taxonomy is append-only once
//! a checkpoint has been migrated against it.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;

/// The embedded artifact. Compact JSON with sorted keys, so the bytes are deterministic.
const ARTIFACT: &str = include_str!("card_tags.json");

/// The artifact's `schema` field this code understands.
const SCHEMA: u32 = 1;

/// The artifact kind names (the left half of each `"<kind>:<id>"` key).
pub mod kind {
    /// Faction ability (`abilities.json`).
    pub const ABILITY: &str = "ability";
    /// Technology, generic or faction (`technologies.json`).
    pub const TECHNOLOGY: &str = "technology";
    /// Unit or unit upgrade with rules text (`units.json`).
    pub const UNIT: &str = "unit";
    /// Agent, commander or hero (`leaders.json`).
    pub const LEADER: &str = "leader";
    /// Thunder's Edge breakthrough (`breakthroughs.json`).
    pub const BREAKTHROUGH: &str = "breakthrough";
    /// Promissory note (`promissory_notes.json`).
    pub const PROMISSORY: &str = "promissory";
    /// Action card (`action_cards.json`).
    pub const ACTION_CARD: &str = "action_card";
    /// Strategy card (`strategy_cards.json`).
    pub const STRATEGY_CARD: &str = "strategy_card";
}

#[derive(Deserialize)]
struct Raw<'a> {
    schema: u32,
    taxonomy_version: u32,
    #[serde(borrow)]
    tags: Vec<&'a str>,
    #[serde(borrow)]
    cards: BTreeMap<&'a str, Vec<&'a str>>,
}

struct Table {
    taxonomy_version: u32,
    tags: Vec<&'static str>,
    /// kind -> id -> tags.
    cards: BTreeMap<&'static str, BTreeMap<&'static str, Vec<&'static str>>>,
}

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| {
        let raw: Raw<'static> =
            serde_json::from_str(ARTIFACT).expect("card_tags.json is valid: pinned by a test");
        assert_eq!(raw.schema, SCHEMA, "card_tags.json schema");
        let mut cards: BTreeMap<&'static str, BTreeMap<&'static str, Vec<&'static str>>> =
            BTreeMap::new();
        for (key, tags) in raw.cards {
            let (kind, id) = key
                .split_once(':')
                .expect("card_tags.json keys are kind:id: pinned by a test");
            cards.entry(kind).or_default().insert(id, tags);
        }
        Table {
            taxonomy_version: raw.taxonomy_version,
            tags: raw.tags,
            cards,
        }
    })
}

/// The tags of one card, sorted. Empty for a card the artifact does not know.
#[must_use]
pub fn tags_of(kind: &str, id: &str) -> &'static [&'static str] {
    table()
        .cards
        .get(kind)
        .and_then(|by_id| by_id.get(id))
        .map_or(&[], Vec::as_slice)
}

/// The whole taxonomy, sorted.
#[must_use]
pub fn all_tags() -> &'static [&'static str] {
    &table().tags
}

/// The taxonomy version recorded in the artifact.
#[must_use]
pub fn taxonomy_version() -> u32 {
    table().taxonomy_version
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    /// SHA-256 of the embedded artifact bytes. Moving it is a deliberate act: it means the tags a
    /// migrated checkpoint's columns were named for may have changed meaning.
    const ARTIFACT_SHA256: &str = "df3bb5584f3b4feff54810c1fc056d043aa4d6fab8c3771df69faf678850355b";

    #[test]
    fn the_embedded_bytes_are_the_pinned_artifact() {
        let digest = Sha256::digest(ARTIFACT.as_bytes());
        let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(hex, ARTIFACT_SHA256);
    }

    #[test]
    fn the_artifact_parses_and_is_non_trivial() {
        assert_eq!(taxonomy_version(), 1);
        assert_eq!(all_tags().len(), 138);
        let cards: usize = table().cards.values().map(BTreeMap::len).sum();
        assert_eq!(cards, 896);
    }

    #[test]
    fn the_taxonomy_is_sorted_and_unique() {
        let tags = all_tags();
        assert!(tags.windows(2).all(|pair| pair[0] < pair[1]), "{tags:?}");
    }

    #[test]
    fn every_card_has_sorted_unique_tags_from_the_taxonomy() {
        let taxonomy: std::collections::BTreeSet<&str> = all_tags().iter().copied().collect();
        for (kind, by_id) in &table().cards {
            for (id, tags) in by_id {
                assert!(
                    tags.windows(2).all(|pair| pair[0] < pair[1]),
                    "{kind}:{id} tags are not sorted and unique: {tags:?}"
                );
                for tag in tags {
                    assert!(
                        taxonomy.contains(tag),
                        "{kind}:{id} carries {tag}, which is not in the taxonomy"
                    );
                }
            }
        }
    }

    #[test]
    fn deal_enforcement_is_gone() {
        assert!(!all_tags().contains(&"deal-enforcement"));
        assert!(
            !table()
                .cards
                .values()
                .flat_map(BTreeMap::values)
                .flatten()
                .any(|tag| *tag == "deal-enforcement")
        );
    }

    #[test]
    fn lookup_is_by_kind_and_id_and_an_unknown_card_is_empty() {
        // `sar` is both a technology and a secret objective: the key is the pair.
        let tech = tags_of(kind::TECHNOLOGY, "sar");
        assert!(!tech.is_empty());
        assert!(tags_of("secret_objective", "sar") != tech);
        assert!(tags_of(kind::ABILITY, "sar").is_empty());
        assert!(tags_of(kind::ABILITY, "no-such-card").is_empty());
        assert!(tags_of("no-such-kind", "sar").is_empty());
        assert!(tags_of(kind::ABILITY, "mitosis").contains(&"place-units-free"));
    }
}
