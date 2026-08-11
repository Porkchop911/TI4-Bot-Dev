//! Content type categories indexed during M02.
//! 29 categories covering all content in the oracle corpus.

use strum::{Display, EnumIter, EnumString, EnumCount, VariantNames};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display, EnumIter, EnumString, VariantNames, EnumCount)]
#[strum(serialize_all = "snake_case")]
pub enum ContentType {
    Factions,
    Units,
    Technologies,
    ActionCards,
    StrategyCards,
    Objectives,
    Secrets,
    Leaders,
    PromissoryNotes,
    Relics,
    ExplorationCards,
    Fragments,
    Tokens,
    Planets,
    Systems,
    Maps,
    Decks,
    AgendaCards,
    Laws,
    Breakthroughs,
    ExpeditionTiles,
    FactionAbilities,
    UnitAbilities,
    TechAbilities,
    CardEffects,
    GameRules,
    BotProfiles,
    TrainingConfigs,
}

/// All content types as a static array for iteration.
pub const ALL_CONTENT_TYPES: &[ContentType] = CONTENT_TYPES;

pub(crate) const CONTENT_TYPES: &[ContentType] = &[
    ContentType::Factions,
    ContentType::Units,
    ContentType::Technologies,
    ContentType::ActionCards,
    ContentType::StrategyCards,
    ContentType::Objectives,
    ContentType::Secrets,
    ContentType::Leaders,
    ContentType::PromissoryNotes,
    ContentType::Relics,
    ContentType::ExplorationCards,
    ContentType::Fragments,
    ContentType::Tokens,
    ContentType::Planets,
    ContentType::Systems,
    ContentType::Maps,
    ContentType::Decks,
    ContentType::AgendaCards,
    ContentType::Laws,
    ContentType::Breakthroughs,
    ContentType::ExpeditionTiles,
    ContentType::FactionAbilities,
    ContentType::UnitAbilities,
    ContentType::TechAbilities,
    ContentType::CardEffects,
    ContentType::GameRules,
    ContentType::BotProfiles,
    ContentType::TrainingConfigs,
];

impl ContentType {
    pub fn json_filename(&self) -> &'static str {
        match self {
            Self::Factions => "factions.json",
            Self::Units => "units.json",
            Self::Technologies => "technologies.json",
            Self::ActionCards => "action_cards.json",
            Self::StrategyCards => "strategy_cards.json",
            Self::Objectives => "objectives.json",
            Self::Secrets => "secrets.json",
            Self::Leaders => "leaders.json",
            Self::PromissoryNotes => "promissory_notes.json",
            Self::Relics => "relics.json",
            Self::ExplorationCards => "exploration_cards.json",
            Self::Fragments => "fragments.json",
            Self::Tokens => "tokens.json",
            Self::Planets => "planets.json",
            Self::Systems => "systems.json",
            Self::Maps => "maps.json",
            Self::Decks => "decks.json",
            Self::AgendaCards => "agenda_cards.json",
            Self::Laws => "laws.json",
            Self::Breakthroughs => "breakthroughs.json",
            Self::ExpeditionTiles => "expedition_tiles.json",
            Self::FactionAbilities => "faction_abilities.json",
            Self::UnitAbilities => "unit_abilities.json",
            Self::TechAbilities => "tech_abilities.json",
            Self::CardEffects => "card_effects.json",
            Self::GameRules => "game_rules.json",
            Self::BotProfiles => "bot_profiles.json",
            Self::TrainingConfigs => "training_configs.json",
        }
    }
}
