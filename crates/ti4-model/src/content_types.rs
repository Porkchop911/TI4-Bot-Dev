//! The content corpus taxonomy: categories, identity keys, and expansion sources.
//!
//! These 28 categories are exactly the JSON files extracted from AsyncTI4 by the oracle's
//! `tools/extract_asyncti4.py`; the oracle reads them through `engine/content.py`. The names,
//! the identity key of each category, and the source tags are all properties of the corpus,
//! not choices made here — `ti4-content` has a test that fails if this enum and the files on
//! disk ever disagree in either direction.

use enumset::{EnumSet, EnumSetType, enum_set};
use strum::{Display, EnumCount, EnumIter, EnumString, VariantNames};

/// A category of content records, one per JSON file in the corpus.
///
/// `Display`/`FromStr` use the category name as it appears in the corpus (the file stem),
/// which is snake_case for every category except `genericcards`.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Display,
    EnumIter,
    EnumString,
    VariantNames,
    EnumCount,
)]
#[strum(serialize_all = "snake_case")]
pub enum ContentType {
    Abilities,
    ActionCards,
    Agendas,
    Attachments,
    Breakthroughs,
    Colors,
    CombatModifiers,
    Decks,
    Explores,
    Factions,
    FrankenErrata,
    GalacticEvents,
    /// One word in the corpus, unlike every other multi-word category.
    #[strum(serialize = "genericcards")]
    GenericCards,
    Leaders,
    MapTemplates,
    Planets,
    PromissoryNotes,
    PublicObjectives,
    Relics,
    Rules,
    SecretObjectives,
    Sources,
    StrategyCardSets,
    StrategyCards,
    Systems,
    Technologies,
    Tokens,
    Units,
}

/// Every content category, in corpus (alphabetical-by-filename) order.
pub const ALL_CONTENT_TYPES: &[ContentType] = &[
    ContentType::Abilities,
    ContentType::ActionCards,
    ContentType::Agendas,
    ContentType::Attachments,
    ContentType::Breakthroughs,
    ContentType::Colors,
    ContentType::CombatModifiers,
    ContentType::Decks,
    ContentType::Explores,
    ContentType::Factions,
    ContentType::FrankenErrata,
    ContentType::GalacticEvents,
    ContentType::GenericCards,
    ContentType::Leaders,
    ContentType::MapTemplates,
    ContentType::Planets,
    ContentType::PromissoryNotes,
    ContentType::PublicObjectives,
    ContentType::Relics,
    ContentType::Rules,
    ContentType::SecretObjectives,
    ContentType::Sources,
    ContentType::StrategyCardSets,
    ContentType::StrategyCards,
    ContentType::Systems,
    ContentType::Technologies,
    ContentType::Tokens,
    ContentType::Units,
];

/// Which field carries a record's identity within its category.
///
/// AsyncTI4 is not consistent about this, so a generic index has to ask the category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityKey {
    /// The `id` field.
    Id,
    /// The `alias` field.
    Alias,
    /// The `source` field — only `sources.json`, which is one record per expansion.
    Source,
    /// No single field is unique. `franken_errata` is keyed by `itemCategory` + `itemId`.
    Composite,
}

impl IdentityKey {
    /// The JSON field name, for the three categories that have a single-field key.
    #[must_use]
    pub const fn field(self) -> Option<&'static str> {
        match self {
            Self::Id => Some("id"),
            Self::Alias => Some("alias"),
            Self::Source => Some("source"),
            Self::Composite => None,
        }
    }
}

impl ContentType {
    /// The corpus file this category is read from.
    #[must_use]
    pub const fn json_filename(self) -> &'static str {
        match self {
            Self::Abilities => "abilities.json",
            Self::ActionCards => "action_cards.json",
            Self::Agendas => "agendas.json",
            Self::Attachments => "attachments.json",
            Self::Breakthroughs => "breakthroughs.json",
            Self::Colors => "colors.json",
            Self::CombatModifiers => "combat_modifiers.json",
            Self::Decks => "decks.json",
            Self::Explores => "explores.json",
            Self::Factions => "factions.json",
            Self::FrankenErrata => "franken_errata.json",
            Self::GalacticEvents => "galactic_events.json",
            Self::GenericCards => "genericcards.json",
            Self::Leaders => "leaders.json",
            Self::MapTemplates => "map_templates.json",
            Self::Planets => "planets.json",
            Self::PromissoryNotes => "promissory_notes.json",
            Self::PublicObjectives => "public_objectives.json",
            Self::Relics => "relics.json",
            Self::Rules => "rules.json",
            Self::SecretObjectives => "secret_objectives.json",
            Self::Sources => "sources.json",
            Self::StrategyCardSets => "strategy_card_sets.json",
            Self::StrategyCards => "strategy_cards.json",
            Self::Systems => "systems.json",
            Self::Technologies => "technologies.json",
            Self::Tokens => "tokens.json",
            Self::Units => "units.json",
        }
    }

    /// Which field identifies a record in this category.
    #[must_use]
    pub const fn identity_key(self) -> IdentityKey {
        match self {
            Self::Abilities
            | Self::Attachments
            | Self::Explores
            | Self::Leaders
            | Self::Planets
            | Self::Rules
            | Self::StrategyCards
            | Self::Systems
            | Self::Tokens
            | Self::Units => IdentityKey::Id,
            Self::ActionCards
            | Self::Agendas
            | Self::Breakthroughs
            | Self::Colors
            | Self::CombatModifiers
            | Self::Decks
            | Self::Factions
            | Self::GalacticEvents
            | Self::GenericCards
            | Self::MapTemplates
            | Self::PromissoryNotes
            | Self::PublicObjectives
            | Self::Relics
            | Self::SecretObjectives
            | Self::StrategyCardSets
            | Self::Technologies => IdentityKey::Alias,
            Self::Sources => IdentityKey::Source,
            Self::FrankenErrata => IdentityKey::Composite,
        }
    }

    /// Whether records in this category carry a `source` tag at all.
    ///
    /// `colors`, `combat_modifiers`, and `map_templates` are untagged (237 records), so a
    /// source filter always yields nothing for them — they must be read unfiltered. This is
    /// inherited behaviour, not a defect: the oracle's `from_sources` filters the same way.
    #[must_use]
    pub const fn is_source_tagged(self) -> bool {
        !matches!(
            self,
            Self::Colors | Self::CombatModifiers | Self::MapTemplates
        )
    }
}

/// An expansion that content can come from.
///
/// The extraction step already dropped 56 homebrew source tags, so these seven are the only
/// values that appear in the corpus. An eighth would be a corpus change and is rejected at
/// load time rather than silently ignored.
#[derive(Debug, EnumSetType, Display, EnumString, EnumIter)]
#[strum(serialize_all = "snake_case")]
pub enum Source {
    Base,
    Codex1,
    Codex2,
    Codex3,
    Codex4,
    Pok,
    ThundersEdge,
}

/// A set of expansion sources to read content for.
pub type SourceSet = EnumSet<Source>;

/// The original boxed game only.
///
/// AsyncTI4 tags the classic set `base` even where a record id carries a `pok` prefix, so
/// filter on source rather than on the shape of an id.
pub const BASE: SourceSet = enum_set!(Source::Base);

/// Base game, Prophecy of Kings, and all four codices — the standard competitive corpus.
pub const POK: SourceSet = enum_set!(
    Source::Base | Source::Pok | Source::Codex1 | Source::Codex2 | Source::Codex3 | Source::Codex4
);

/// Everything, including Thunder's Edge.
pub const FULL: SourceSet = enum_set!(
    Source::Base
        | Source::Pok
        | Source::Codex1
        | Source::Codex2
        | Source::Codex3
        | Source::Codex4
        | Source::ThundersEdge
);

#[cfg(test)]
mod tests {
    use super::*;
    use strum::{EnumCount, IntoEnumIterator};

    #[test]
    fn every_category_is_listed_once_in_all_content_types() {
        assert_eq!(ALL_CONTENT_TYPES.len(), ContentType::COUNT);
        for category in ContentType::iter() {
            assert_eq!(
                ALL_CONTENT_TYPES.iter().filter(|c| **c == category).count(),
                1,
                "{category} is not listed exactly once"
            );
        }
    }

    #[test]
    fn category_names_round_trip_through_display_and_from_str() {
        for category in ContentType::iter() {
            let name = category.to_string();
            assert_eq!(name.parse::<ContentType>().unwrap(), category);
            assert_eq!(category.json_filename(), format!("{name}.json"));
        }
    }

    #[test]
    fn genericcards_is_one_word() {
        assert_eq!(ContentType::GenericCards.to_string(), "genericcards");
        assert_eq!(
            ContentType::GenericCards.json_filename(),
            "genericcards.json"
        );
    }

    #[test]
    fn only_franken_errata_lacks_a_single_field_key() {
        let composite: Vec<_> = ALL_CONTENT_TYPES
            .iter()
            .filter(|c| c.identity_key() == IdentityKey::Composite)
            .collect();
        assert_eq!(composite, vec![&ContentType::FrankenErrata]);
    }

    #[test]
    fn source_sets_nest() {
        assert!(BASE.is_subset(POK));
        assert!(POK.is_subset(FULL));
        assert_eq!(BASE.len(), 1);
        assert_eq!(POK.len(), 6);
        assert_eq!(FULL.len(), 7);
        assert_eq!(FULL.len(), Source::iter().count());
    }

    #[test]
    fn source_names_match_the_corpus_tags() {
        assert_eq!(Source::ThundersEdge.to_string(), "thunders_edge");
        assert_eq!("codex4".parse::<Source>().unwrap(), Source::Codex4);
        assert!("ds".parse::<Source>().is_err());
    }

    #[test]
    fn the_three_untagged_categories_are_marked() {
        let untagged: Vec<_> = ALL_CONTENT_TYPES
            .iter()
            .filter(|c| !c.is_source_tagged())
            .copied()
            .collect();
        assert_eq!(
            untagged,
            vec![
                ContentType::Colors,
                ContentType::CombatModifiers,
                ContentType::MapTemplates
            ]
        );
    }
}
