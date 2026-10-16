//! Every record of the corpus that a page names by id, with the fields that it shows: the text
//! and the numbers as printed, of every source. [`crate::view::components::Catalog::records`]
//! hands them to a page once, with the cards of the play areas.
//!
//! A field is read from the corpus as it is. A field that a record does not print is left out.

use serde::{Deserialize, Serialize};
use ti4_content::{ContentStore, Record};
use ti4_model::content_types::ContentType;

/// One strategy card as printed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct StrategyCardRecord {
    pub id: String,
    pub name: String,
    pub initiative: i64,
    /// The primary ability, one line for each paragraph.
    pub primary_text: String,
    /// The secondary ability, one line for each paragraph.
    pub secondary_text: String,
}

/// One secret or public objective as printed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ObjectiveRecord {
    pub id: String,
    pub name: String,
    /// The phase in which it is scored: `Status`, `Action` or `Agenda`. `Status` where the
    /// corpus has none.
    pub phase: String,
    /// What it is worth. 1 where the corpus has none.
    pub points: i64,
    pub text: String,
}

/// One action card as printed. Copies of a card have ids of their own and one name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ActionCardRecord {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub phase: String,
    /// When the card is played, as printed: `Action`, or a timing.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub window: String,
    pub text: String,
    /// The id that the engine may name the card by (`direct_hit` for `dh1` to `dh4`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub automation_id: String,
}

/// One technology as printed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TechnologyRecord {
    pub id: String,
    pub name: String,
    /// The colours of the technology (`PROPULSION`, …), or `unit_upgrade`.
    pub types: Vec<String>,
    /// The prerequisites, one letter for each: `B`, `G`, `Y`, `R`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub requirements: String,
    /// The faction of a faction technology.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub faction: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    /// The technology that a faction technology replaces.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub base_upgrade: String,
    pub text: String,
}

/// One exploration card as printed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ExploreRecord {
    pub id: String,
    pub name: String,
    /// The deck, as the corpus names it: `cultural`, `hazardous`, `industrial`, `frontier`.
    pub kind: String,
    /// `Instant`, `Fragment`, `Attach` or `Token`.
    pub resolution: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub flavor_text: String,
}

/// One planet as printed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PlanetRecord {
    pub id: String,
    pub name: String,
    pub resources: i64,
    pub influence: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tech_specialties: Vec<String>,
    /// The ability card of a legendary planet: its name and its text.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub legendary_ability_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub legendary_ability_text: String,
}

/// One attachment of a planet, with what it adds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AttachmentRecord {
    pub id: String,
    /// The id where the corpus has no name.
    pub name: String,
    pub resources_modifier: i64,
    pub influence_modifier: i64,
}

/// One faction sheet: its name and what belongs to it, by id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FactionRecord {
    pub id: String,
    /// The name as printed: "The Federation of Sol".
    pub name: String,
    pub commodities: i64,
    /// The technologies that the faction begins with. Empty for a faction that chooses them.
    pub starting_tech: Vec<String>,
    pub faction_tech: Vec<String>,
    pub abilities: Vec<String>,
    pub leaders: Vec<String>,
    pub promissory_notes: Vec<String>,
    /// The unit cards of the faction: its own, and the generic ones that it has.
    pub units: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
}

/// One unit card as printed: its numbers and its text. A value that the card does not print is
/// left out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct UnitRecord {
    pub id: String,
    /// The unit type: `carrier`, `flagship`, `spacedock`, …
    pub base_type: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub subtitle: String,
    /// The faction of a unit of a faction. Empty for a generic unit.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub faction: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    /// The cost of one unit: 0.5 for a unit that is bought two for one resource.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub move_value: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity_value: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combat_hits_on: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combat_die_count: Option<i64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sustain_damage: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub afb_hits_on: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub afb_die_count: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bombard_hits_on: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bombard_die_count: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space_cannon_hits_on: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space_cannon_die_count: Option<i64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deep_space_cannon: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub planetary_shield: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disables_planetary_shield: bool,
    /// The PRODUCTION value as printed: a number (`"5"`), or what is added to the resources of
    /// the planet (`"+2"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub production_value: Option<String>,
    /// The technology that makes this the card of the unit.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub required_tech_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub upgrades_from_unit_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub upgrades_to_unit_id: String,
    /// The text of the card, one line for each ability.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ability: String,
}

/// One leader as printed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LeaderRecord {
    pub id: String,
    pub faction: String,
    /// `agent`, `commander` or `hero`.
    pub kind: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ability_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ability_window: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ability_text: String,
    /// As printed, `Always Unlocked` too.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unlock_condition: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
}

/// One faction ability as printed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AbilityRecord {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub faction: String,
    /// The part of the ability that always holds.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub permanent_effect: String,
    /// When the other part is used: `ACTION`, or a timing.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub window: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub window_effect: String,
}

/// One promissory note as printed. A note that every faction has is `<color>_<alias>`, and its
/// text names the issuer `<color>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PromissoryNoteRecord {
    pub id: String,
    pub name: String,
    /// The faction of a note of a faction. Empty for a note that every faction has.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub faction: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
}

/// One relic as printed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RelicRecord {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
}

/// One breakthrough of a faction (Thunder's Edge) as printed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BreakthroughRecord {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub faction: String,
    /// The two technology colours that its synergy reads (`BIOTIC`, `WARFARE`).
    pub synergy: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
}

/// The records of the corpus that a page names by id, of every source. Each kind is in the
/// order of the corpus, and each record has its id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ContentRecords {
    pub strategy_cards: Vec<StrategyCardRecord>,
    pub secret_objectives: Vec<ObjectiveRecord>,
    pub public_objectives: Vec<ObjectiveRecord>,
    pub action_cards: Vec<ActionCardRecord>,
    pub technologies: Vec<TechnologyRecord>,
    pub explores: Vec<ExploreRecord>,
    pub planets: Vec<PlanetRecord>,
    pub attachments: Vec<AttachmentRecord>,
    pub factions: Vec<FactionRecord>,
    pub units: Vec<UnitRecord>,
    pub leaders: Vec<LeaderRecord>,
    pub abilities: Vec<AbilityRecord>,
    pub promissory_notes: Vec<PromissoryNoteRecord>,
    pub relics: Vec<RelicRecord>,
    pub breakthroughs: Vec<BreakthroughRecord>,
}

fn text(record: &Record, key: &str) -> String {
    record.text(key).map(ToOwned::to_owned).unwrap_or_default()
}

fn strings(record: &Record, key: &str) -> Vec<String> {
    record
        .strings(key)
        .into_iter()
        .map(ToOwned::to_owned)
        .collect()
}

/// Every record of `kind`, in the order of the corpus, as `read` gives it.
fn every<T>(
    content: &ContentStore,
    kind: ContentType,
    read: impl Fn(&str, &Record) -> T,
) -> Vec<T> {
    content
        .records(kind)
        .iter()
        .filter_map(|record| Some(read(record.id()?, record)))
        .collect()
}

fn objective(id: &str, record: &Record) -> ObjectiveRecord {
    ObjectiveRecord {
        id: id.to_owned(),
        name: text(record, "name"),
        phase: record.text("phase").unwrap_or("Status").to_owned(),
        points: record.int("points").unwrap_or(1),
        text: text(record, "text"),
    }
}

/// The records of the corpus that a page names by id.
#[must_use]
pub fn content_records(content: &ContentStore) -> ContentRecords {
    ContentRecords {
        strategy_cards: every(content, ContentType::StrategyCards, |id, record| {
            StrategyCardRecord {
                id: id.to_owned(),
                name: text(record, "name"),
                initiative: record.int("initiative").unwrap_or(0),
                primary_text: strings(record, "primaryTexts").join("\n"),
                secondary_text: strings(record, "secondaryTexts").join("\n"),
            }
        }),
        secret_objectives: every(content, ContentType::SecretObjectives, objective),
        public_objectives: every(content, ContentType::PublicObjectives, objective),
        action_cards: every(content, ContentType::ActionCards, |id, record| {
            ActionCardRecord {
                id: id.to_owned(),
                name: text(record, "name"),
                phase: text(record, "phase"),
                window: record.text("window").unwrap_or_default().trim().to_owned(),
                text: text(record, "text"),
                automation_id: text(record, "automationID"),
            }
        }),
        technologies: every(content, ContentType::Technologies, |id, record| {
            TechnologyRecord {
                id: id.to_owned(),
                name: text(record, "name"),
                types: strings(record, "types"),
                requirements: text(record, "requirements"),
                faction: text(record, "faction"),
                source: text(record, "source"),
                base_upgrade: text(record, "baseUpgrade"),
                text: text(record, "text"),
            }
        }),
        explores: every(content, ContentType::Explores, |id, record| ExploreRecord {
            id: id.to_owned(),
            name: text(record, "name"),
            kind: text(record, "type"),
            resolution: text(record, "resolution"),
            text: text(record, "text"),
            flavor_text: record
                .text("flavorText")
                .or_else(|| record.text("flavourText"))
                .unwrap_or_default()
                .to_owned(),
        }),
        planets: every(content, ContentType::Planets, |id, record| PlanetRecord {
            id: id.to_owned(),
            name: text(record, "name"),
            resources: record.int("resources").unwrap_or(0),
            influence: record.int("influence").unwrap_or(0),
            tech_specialties: strings(record, "techSpecialties"),
            legendary_ability_name: text(record, "legendaryAbilityName"),
            legendary_ability_text: text(record, "legendaryAbilityText"),
        }),
        attachments: every(content, ContentType::Attachments, |id, record| {
            AttachmentRecord {
                id: id.to_owned(),
                name: record.text("name").unwrap_or(id).to_owned(),
                resources_modifier: record.int("resourcesModifier").unwrap_or(0),
                influence_modifier: record.int("influenceModifier").unwrap_or(0),
            }
        }),
        factions: every(content, ContentType::Factions, |id, record| FactionRecord {
            id: id.to_owned(),
            name: text(record, "factionName"),
            commodities: record.int("commodities").unwrap_or(0),
            starting_tech: strings(record, "startingTech"),
            faction_tech: strings(record, "factionTech"),
            abilities: strings(record, "abilities"),
            leaders: strings(record, "leaders"),
            promissory_notes: strings(record, "promissoryNotes"),
            units: strings(record, "units"),
            source: text(record, "source"),
        }),
        units: every(content, ContentType::Units, |id, record| UnitRecord {
            id: id.to_owned(),
            base_type: text(record, "baseType"),
            name: text(record, "name"),
            subtitle: text(record, "subtitle"),
            faction: text(record, "faction"),
            source: text(record, "source"),
            cost: record.float("cost"),
            move_value: record.int("moveValue"),
            capacity_value: record.int("capacityValue"),
            combat_hits_on: record.int("combatHitsOn"),
            combat_die_count: record.int("combatDieCount"),
            sustain_damage: record.flag("sustainDamage"),
            afb_hits_on: record.int("afbHitsOn"),
            afb_die_count: record.int("afbDieCount"),
            bombard_hits_on: record.int("bombardHitsOn"),
            bombard_die_count: record.int("bombardDieCount"),
            space_cannon_hits_on: record.int("spaceCannonHitsOn"),
            space_cannon_die_count: record.int("spaceCannonDieCount"),
            deep_space_cannon: record.flag("deepSpaceCannon"),
            planetary_shield: record.flag("planetaryShield"),
            disables_planetary_shield: record.flag("disablesPlanetaryShield"),
            production_value: record.raw("productionValue").and_then(|value| match value {
                serde_json::Value::String(text) => Some(text.clone()),
                serde_json::Value::Number(number) => Some(number.to_string()),
                _ => None,
            }),
            required_tech_id: text(record, "requiredTechId"),
            upgrades_from_unit_id: text(record, "upgradesFromUnitId"),
            upgrades_to_unit_id: text(record, "upgradesToUnitId"),
            ability: text(record, "ability"),
        }),
        leaders: every(content, ContentType::Leaders, |id, record| LeaderRecord {
            id: id.to_owned(),
            faction: text(record, "faction"),
            kind: text(record, "type"),
            name: text(record, "name"),
            title: text(record, "title"),
            ability_name: text(record, "abilityName"),
            ability_window: text(record, "abilityWindow"),
            ability_text: text(record, "abilityText"),
            unlock_condition: text(record, "unlockCondition"),
            source: text(record, "source"),
        }),
        abilities: every(content, ContentType::Abilities, |id, record| {
            AbilityRecord {
                id: id.to_owned(),
                name: text(record, "name"),
                faction: text(record, "faction"),
                permanent_effect: text(record, "permanentEffect"),
                window: text(record, "window"),
                window_effect: text(record, "windowEffect"),
            }
        }),
        promissory_notes: every(content, ContentType::PromissoryNotes, |id, record| {
            PromissoryNoteRecord {
                id: id.to_owned(),
                name: text(record, "name"),
                faction: text(record, "faction"),
                text: text(record, "text"),
            }
        }),
        relics: every(content, ContentType::Relics, |id, record| RelicRecord {
            id: id.to_owned(),
            name: text(record, "name"),
            text: text(record, "text"),
            source: text(record, "source"),
        }),
        breakthroughs: every(content, ContentType::Breakthroughs, |id, record| {
            BreakthroughRecord {
                id: id.to_owned(),
                name: text(record, "name"),
                faction: text(record, "faction"),
                synergy: strings(record, "synergy"),
                text: text(record, "text"),
            }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn records() -> ContentRecords {
        content_records(ContentStore::embedded())
    }

    #[test]
    fn every_record_of_each_kind_that_a_page_names_is_there_once() {
        let content = ContentStore::embedded();
        let records = records();
        let kinds = [
            (ContentType::StrategyCards, records.strategy_cards.len(), 12),
            (
                ContentType::SecretObjectives,
                records.secret_objectives.len(),
                40,
            ),
            (
                ContentType::PublicObjectives,
                records.public_objectives.len(),
                40,
            ),
            (ContentType::ActionCards, records.action_cards.len(), 142),
            (ContentType::Technologies, records.technologies.len(), 102),
            (ContentType::Explores, records.explores.len(), 80),
            (ContentType::Planets, records.planets.len(), 159),
            (ContentType::Attachments, records.attachments.len(), 22),
            (ContentType::Factions, records.factions.len(), 34),
            (ContentType::Units, records.units.len(), 136),
            (ContentType::Leaders, records.leaders.len(), 103),
            (ContentType::Abilities, records.abilities.len(), 73),
            (
                ContentType::PromissoryNotes,
                records.promissory_notes.len(),
                40,
            ),
            (ContentType::Relics, records.relics.len(), 24),
            (ContentType::Breakthroughs, records.breakthroughs.len(), 31),
        ];
        for (kind, len, count) in kinds {
            assert_eq!(content.records(kind).len(), len, "every record of {kind:?}");
            assert_eq!(len, count, "{kind:?}");
            let ids: std::collections::BTreeSet<&str> = content
                .records(kind)
                .iter()
                .filter_map(Record::id)
                .collect();
            assert_eq!(ids.len(), len, "each id of {kind:?} once");
        }
    }

    /// The record `id` of `records`.
    fn find<'a, T>(records: &'a [T], id: &str, id_of: impl Fn(&T) -> &str) -> &'a T {
        records
            .iter()
            .find(|record| id_of(record) == id)
            .unwrap_or_else(|| panic!("no record {id}"))
    }

    #[test]
    fn a_record_has_what_is_printed_on_it() {
        let records = records();
        let technology = |id| find(&records.technologies, id, |card| &card.id);
        let tech = technology("so2");
        assert_eq!(
            (
                tech.faction.as_str(),
                tech.base_upgrade.as_str(),
                tech.requirements.as_str()
            ),
            ("sol", "inf2", "GG")
        );
        assert!(!tech.text.is_empty());
        let gravity = technology("gd");
        assert_eq!(
            (gravity.types.as_slice(), gravity.requirements.as_str()),
            (["PROPULSION".to_owned()].as_slice(), "B")
        );

        let unit = |id| find(&records.units, id, |card| &card.id);
        let fighter = unit("fighter");
        assert_eq!((fighter.cost, fighter.combat_hits_on), (Some(0.5), Some(9)));
        assert_eq!(unit("spacedock").production_value.as_deref(), Some("+2"));
        let dreadnought = unit("dreadnought");
        assert!(dreadnought.sustain_damage);
        assert_eq!(dreadnought.bombard_hits_on, Some(5));
        assert_eq!(dreadnought.upgrades_to_unit_id, "dreadnought2");

        let sol = find(&records.factions, "sol", |faction| &faction.id);
        assert_eq!(
            (sol.name.as_str(), sol.commodities),
            ("The Federation of Sol", 4)
        );
        assert_eq!(sol.abilities, ["orbital_drop", "versatile"]);

        let direct_hit = find(&records.action_cards, "dh1", |card| &card.id);
        assert_eq!(direct_hit.automation_id, "direct_hit");
        let imperial = find(&records.strategy_cards, "pok8imperial", |card| &card.id);
        assert_eq!(imperial.initiative, 8);
        assert!(imperial.primary_text.contains('\n'));
        let agent = find(&records.leaders, "solagent", |leader| &leader.id);
        assert_eq!(agent.kind, "agent");
        let primor = find(&records.planets, "primor", |planet| &planet.id);
        assert!(!primor.legendary_ability_name.is_empty());
        assert!(!primor.legendary_ability_text.is_empty());
        let ceasefire = find(&records.promissory_notes, "<color>_cf", |note| &note.id);
        assert!(ceasefire.faction.is_empty());
        assert!(ceasefire.text.contains("<color>"));
        assert!(
            records
                .breakthroughs
                .iter()
                .all(|card| !card.faction.is_empty() && !card.synergy.is_empty())
        );
    }
}
