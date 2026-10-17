//! The printed side of what a player has in the play area: the faction abilities, the leaders,
//! the technologies, the relics and the legendary planet cards. A client asks for it once; a view
//! names these by id only.
//!
//! Nothing here decides a rule of the game. Whether a technology is a card that a player uses is
//! read from its printed text.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ti4_content::factions::{Faction, Placement};
use ti4_content::{
    ActionCardTiming, AgendaGroup, ContentStore, PublicObjectiveGroup, Record,
    SecretObjectiveGroup, action_card_timing, agenda_group, public_objective_group,
    secret_objective_group,
};
use ti4_engine::deck::build_starting_decks;
use ti4_model::content_types::{ContentType, SourceSet};

use crate::view::exploration::{ExplorationCatalog, exploration_catalog};
use crate::view::records::{ContentRecords, content_records};

/// One faction ability as printed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AbilityCard {
    pub id: String,
    pub name: String,
    /// When the ability is used, as printed: `ACTION`, or a timing. Empty for an ability that
    /// always holds.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub window: String,
    pub text: String,
}

/// One leader as printed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LeaderCard {
    pub id: String,
    /// `agent`, `commander` or `hero`.
    pub kind: String,
    pub name: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub window: String,
    pub text: String,
    /// How the leader is unlocked. Empty for a leader that is never locked.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unlock: String,
}

/// A card that is a name and a text: a technology, a relic, the ability card of a legendary
/// planet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PrintedCard {
    pub id: String,
    pub name: String,
    pub text: String,
    /// The card is used: its text has an `ACTION`, or it exhausts. A card that always holds is
    /// not.
    pub used: bool,
}

/// One action card as printed. Copies of a card have ids of their own and one name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ActionCard {
    pub id: String,
    pub name: String,
    /// The phase of the card: `Action`, `Agenda`, `Strategy`, `Status` or `Any`.
    pub phase: String,
    /// When the card is played, as printed: `Action`, or a timing.
    pub window: String,
    /// The group of the timing, one of [`ComponentCatalog::action_card_groups`].
    pub group: String,
    pub text: String,
}

/// One secret objective as printed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SecretObjective {
    pub id: String,
    pub name: String,
    /// The phase in which the objective is scored, as printed: `Status`, `Action` or `Agenda`.
    pub phase: String,
    /// What the objective asks for, one of [`ComponentCatalog::secret_objective_groups`].
    pub group: String,
    pub text: String,
}

/// One public objective as printed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PublicObjectiveCard {
    pub id: String,
    pub name: String,
    /// What the objective is worth: 1 for stage I, 2 for stage II.
    pub points: i64,
    /// What the objective asks for, one of [`ComponentCatalog::public_objective_groups`].
    pub group: String,
    pub text: String,
}

/// One agenda as printed. Every agenda is in the deck once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AgendaCard {
    pub id: String,
    pub name: String,
    /// `Law` or `Directive`.
    pub kind: String,
    /// What the vote is about, as printed: `For/Against`, `Elect Player`, `Elect Planet`. Empty
    /// for an agenda that has no outcomes of its own (Covert Legislation).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub elect: String,
    /// The text of the outcome "For", or of the election. A note that the card prints with what
    /// is elected is before it, in its brackets.
    pub text: String,
    /// The text of the outcome "Against". Empty for an election.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub against: String,
    /// What the agenda affects, one of [`ComponentCatalog::agenda_groups`].
    pub group: String,
}

/// What a table says of a faction wherever it names a seat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FactionSheet {
    /// The name that a table calls the faction by: "Sol".
    pub name: String,
    /// The name as printed: "The Federation of Sol".
    pub title: String,
    /// The commodity value of the faction sheet.
    pub commodities: i32,
    /// The agent, the commander and the hero, by id ([`ComponentCatalog::leaders`]).
    pub leaders: Vec<String>,
    /// The technologies that the faction begins with, by id ([`ComponentCatalog::technologies`]).
    pub starting_technologies: Vec<String>,
    /// A faction that chooses its starting technologies: how many, and of which.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starting_technology_choice: Option<TechnologyChoice>,
    /// The faction technologies, by id.
    pub faction_technologies: Vec<String>,
    /// The units that the faction begins with, in the order of the sheet.
    pub starting_fleet: Vec<StartingUnits>,
    /// The planets of the home system.
    pub home_planets: Vec<HomePlanet>,
}

/// "Choose 2 of the following", of a faction sheet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TechnologyChoice {
    pub amount: i64,
    pub options: Vec<String>,
}

/// One entry of a starting fleet: so many of a unit, in one place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct StartingUnits {
    /// The unit as the sheet names it: `carrier`, not the unit of the faction (`sol_carrier`).
    pub unit: String,
    pub count: u32,
    /// The id of the home planet that the units are on. Absent: the space area.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planet: Option<String>,
}

/// One planet of a home system, with its printed values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct HomePlanet {
    pub id: String,
    pub name: String,
    pub resources: i64,
    pub influence: i64,
}

/// One strategy card as printed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct StrategyCard {
    pub id: String,
    pub name: String,
    pub initiative: i64,
}

/// The printed cards of the play areas. It does not change during a game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ComponentCatalog {
    pub abilities: Vec<AbilityCard>,
    /// The abilities of each faction, in the order of the faction sheet.
    pub factions: BTreeMap<String, Vec<String>>,
    /// The name and the commodity value of each faction.
    pub faction_sheets: BTreeMap<String, FactionSheet>,
    /// The strategy cards of every set, by initiative: a game names its cards by id, and the id
    /// of a card of an older printing has no name in it (`base4`). They are not only those of
    /// the sources: a game can play a set of another source than its other cards (`te`).
    pub strategy_cards: Vec<StrategyCard>,
    pub leaders: Vec<LeaderCard>,
    pub technologies: Vec<PrintedCard>,
    pub relics: Vec<PrintedCard>,
    /// The ability cards of the legendary planets, by the id of the planet.
    pub legendary: Vec<PrintedCard>,
    /// The action card deck as a game begins with it, sorted by id.
    pub action_cards: Vec<ActionCard>,
    /// The groups of the timings of the action cards, in the order in which a reader is shown them.
    pub action_card_groups: Vec<String>,
    /// The secret objective deck as a game begins with it, sorted by id.
    pub secret_objectives: Vec<SecretObjective>,
    /// The groups of what the secret objectives ask for, in the order in which a reader is shown
    /// them.
    pub secret_objective_groups: Vec<String>,
    /// Every public objective that a game can reveal: stage I, then stage II, each sorted by id.
    /// A game draws five of each stage, and no fact says which.
    pub public_objectives: Vec<PublicObjectiveCard>,
    /// The groups of what the public objectives ask for, in the order in which a reader is shown
    /// them.
    pub public_objective_groups: Vec<String>,
    /// The agenda deck as a game begins with it, sorted by id.
    pub agendas: Vec<AgendaCard>,
    /// The groups of what the agendas affect, in the order in which a reader is shown them.
    pub agenda_groups: Vec<String>,
}

/// Everything printed that a client is handed once.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Catalog {
    /// The digest of the corpus that the texts are of ([`ti4_content::provenance::digest_of`]).
    /// A page compares it with `content` of the snapshot of a server: with another corpus the
    /// ids of a game could name other cards.
    pub content: String,
    pub exploration: ExplorationCatalog,
    pub components: ComponentCatalog,
    /// Every record of the corpus that a page names by id, of every source: the exploration and
    /// the components above are those of `sources` only.
    pub records: ContentRecords,
}

/// The catalogue of `sources`, and the records of the whole corpus.
#[must_use]
pub fn catalog(content: &ContentStore, sources: SourceSet) -> Catalog {
    Catalog {
        content: ti4_content::provenance::digest_of(content).corpus,
        exploration: exploration_catalog(content, sources),
        components: component_catalog(content, sources),
        records: content_records(content),
    }
}

/// The group of a card whose timing `ti4-content` does not know. No card of the corpus has it.
const OTHER_TIMING: &str = "Other";

/// The group of a secret objective that `ti4-content` does not know. No objective of the corpus
/// has it.
const OTHER_GROUP: &str = "Other";

fn text(record: &Record, key: &str) -> String {
    record.text(key).map(ToOwned::to_owned).unwrap_or_default()
}

/// Whether a card with this printed text is one that its owner uses.
fn is_used(text: &str) -> bool {
    text.contains("ACTION:") || text.to_ascii_lowercase().contains("exhaust this card")
}

/// The cards of the play areas of `sources`, as printed.
#[must_use]
pub fn component_catalog(content: &ContentStore, sources: SourceSet) -> ComponentCatalog {
    let records = |kind: ContentType| {
        content
            .records(kind)
            .iter()
            .filter(move |record| record.in_sources(sources))
    };
    let card = |id: &str, name: &str, printed: String| PrintedCard {
        id: id.to_owned(),
        name: name.to_owned(),
        used: is_used(&printed),
        text: printed,
    };
    let named = |record: &Record| {
        let id = record.id()?;
        Some(card(
            id,
            record.text("name").unwrap_or(id),
            text(record, "text"),
        ))
    };
    ComponentCatalog {
        abilities: records(ContentType::Abilities)
            .filter_map(|record| {
                let id = record.id()?;
                Some(AbilityCard {
                    id: id.to_owned(),
                    name: record.text("name").unwrap_or(id).to_owned(),
                    window: text(record, "window"),
                    text: [
                        text(record, "permanentEffect"),
                        text(record, "windowEffect"),
                    ]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n"),
                })
            })
            .collect(),
        factions: ti4_content::factions::catalogue(content, sources)
            .into_iter()
            .map(|(alias, faction)| {
                (
                    alias.to_owned(),
                    faction
                        .abilities()
                        .into_iter()
                        .map(ToOwned::to_owned)
                        .collect(),
                )
            })
            .collect(),
        faction_sheets: ti4_content::factions::catalogue(content, sources)
            .into_iter()
            .map(|(alias, faction)| {
                (
                    alias.to_owned(),
                    faction_sheet(content, sources, alias, &faction),
                )
            })
            .collect(),
        strategy_cards: {
            let mut cards: Vec<StrategyCard> = content
                .records(ContentType::StrategyCards)
                .iter()
                .filter_map(|record| {
                    let id = record.id()?;
                    Some(StrategyCard {
                        id: id.to_owned(),
                        name: record.text("name").unwrap_or(id).to_owned(),
                        initiative: record.int("initiative")?,
                    })
                })
                .collect();
            cards.sort_by(|a, b| (a.initiative, &a.id).cmp(&(b.initiative, &b.id)));
            cards
        },
        leaders: records(ContentType::Leaders)
            .filter_map(|record| {
                let id = record.id()?;
                Some(LeaderCard {
                    id: id.to_owned(),
                    kind: text(record, "type"),
                    name: record.text("name").unwrap_or(id).to_owned(),
                    title: text(record, "title"),
                    window: text(record, "abilityWindow"),
                    text: text(record, "abilityText"),
                    unlock: Some(text(record, "unlockCondition"))
                        .filter(|unlock| !unlock.eq_ignore_ascii_case("Always Unlocked"))
                        .unwrap_or_default(),
                })
            })
            .collect(),
        technologies: records(ContentType::Technologies)
            .filter_map(named)
            .collect(),
        relics: records(ContentType::Relics).filter_map(named).collect(),
        legendary: records(ContentType::Planets)
            .filter_map(|record| {
                let name = record.text("legendaryAbilityName")?;
                Some(card(
                    record.id()?,
                    name,
                    text(record, "legendaryAbilityText"),
                ))
            })
            .collect(),
        action_cards: {
            let mut deck = build_starting_decks(content, sources, 0).action_cards;
            deck.sort();
            deck.iter()
                .map(|id| {
                    let record = content.get(ContentType::ActionCards, id.as_str());
                    let printed = |key: &str| record.map(|record| text(record, key));
                    let phase = printed("phase").unwrap_or_default();
                    let window = printed("window").unwrap_or_default();
                    ActionCard {
                        id: id.as_str().to_owned(),
                        name: printed("name").unwrap_or_else(|| id.as_str().to_owned()),
                        group: action_card_timing(&phase, &window)
                            .map_or(OTHER_TIMING, ActionCardTiming::label)
                            .to_owned(),
                        phase,
                        window,
                        text: printed("text").unwrap_or_default(),
                    }
                })
                .collect()
        },
        action_card_groups: ActionCardTiming::ALL
            .iter()
            .map(|timing| timing.label())
            .chain([OTHER_TIMING])
            .map(ToOwned::to_owned)
            .collect(),
        secret_objectives: {
            let mut deck = build_starting_decks(content, sources, 0).secrets;
            deck.sort();
            deck.iter()
                .map(|id| {
                    let record = content.get(ContentType::SecretObjectives, id.as_str());
                    let printed = |key: &str| record.map(|record| text(record, key));
                    SecretObjective {
                        id: id.as_str().to_owned(),
                        name: printed("name").unwrap_or_else(|| id.as_str().to_owned()),
                        phase: printed("phase").unwrap_or_default(),
                        group: secret_objective_group(id.as_str())
                            .map_or(OTHER_GROUP, SecretObjectiveGroup::label)
                            .to_owned(),
                        text: printed("text").unwrap_or_default(),
                    }
                })
                .collect()
        },
        secret_objective_groups: SecretObjectiveGroup::ALL
            .iter()
            .map(|group| group.label())
            .chain([OTHER_GROUP])
            .map(ToOwned::to_owned)
            .collect(),
        public_objectives: {
            let mut cards: Vec<PublicObjectiveCard> = records(ContentType::PublicObjectives)
                .filter_map(|record| {
                    Some(PublicObjectiveCard {
                        id: record.id()?.to_owned(),
                        name: text(record, "name"),
                        points: record.int("points")?,
                        group: public_objective_group(record.id()?)
                            .map_or(OTHER_GROUP, PublicObjectiveGroup::label)
                            .to_owned(),
                        text: text(record, "text"),
                    })
                })
                .collect();
            cards.sort_by(|a, b| (a.points, &a.id).cmp(&(b.points, &b.id)));
            cards
        },
        public_objective_groups: PublicObjectiveGroup::ALL
            .iter()
            .map(|group| group.label())
            .chain([OTHER_GROUP])
            .map(ToOwned::to_owned)
            .collect(),
        agendas: {
            let mut deck = build_starting_decks(content, sources, 0).agendas;
            deck.sort();
            deck.iter()
                .map(|id| agenda_card(id, content.get(ContentType::Agendas, id)))
                .collect()
        },
        agenda_groups: AgendaGroup::ALL
            .iter()
            .map(|group| group.label())
            .chain([OTHER_GROUP])
            .map(ToOwned::to_owned)
            .collect(),
    }
}

/// The sheet of `faction` as printed. An id that the sources do not have is left out.
fn faction_sheet(
    content: &ContentStore,
    sources: SourceSet,
    alias: &str,
    faction: &Faction<'_>,
) -> FactionSheet {
    let technologies = |ids: Vec<&str>| -> Vec<String> {
        ids.into_iter()
            .filter_map(|id| content.resolve_id(ContentType::Technologies, id, sources))
            .map(ToOwned::to_owned)
            .collect()
    };
    let record = faction.record();
    FactionSheet {
        name: faction.short_name().to_owned(),
        title: faction.name().unwrap_or(alias).to_owned(),
        commodities: faction.commodities(),
        leaders: faction
            .leaders()
            .into_iter()
            .filter_map(|id| content.resolve_id(ContentType::Leaders, id, sources))
            .map(ToOwned::to_owned)
            .collect(),
        starting_technologies: technologies(faction.starting_tech()),
        starting_technology_choice: record
            .int("startingTechAmount")
            .map(|amount| TechnologyChoice {
                amount,
                options: technologies(record.strings("startingTechOptions")),
            })
            // Sardakk N'orr begins with no technology: the corpus says it as a choice of none.
            .filter(|choice| choice.amount > 0 && !choice.options.is_empty()),
        faction_technologies: technologies(faction.faction_tech()),
        // A fleet that cannot be read is a fault of the corpus, which its validator reports.
        starting_fleet: faction
            .deployments(content)
            .unwrap_or_default()
            .into_iter()
            .map(|entry| StartingUnits {
                unit: entry.unit_id.as_str().to_owned(),
                count: entry.count,
                planet: match entry.placement {
                    Placement::Space => None,
                    Placement::Planet(planet) => Some(planet.as_str().to_owned()),
                },
            })
            .collect(),
        home_planets: faction
            .home_planets()
            .into_iter()
            .map(|id| {
                let planet = content.get(ContentType::Planets, id);
                HomePlanet {
                    id: id.to_owned(),
                    name: planet
                        .and_then(|planet| planet.text("name"))
                        .unwrap_or(id)
                        .to_owned(),
                    resources: planet
                        .and_then(|planet| planet.int("resources"))
                        .unwrap_or(0),
                    influence: planet
                        .and_then(|planet| planet.int("influence"))
                        .unwrap_or(0),
                }
            })
            .collect(),
    }
}

/// The agenda `id` as printed. The corpus has what is elected and a note of the card in one
/// field ("Elect Law (When this agenda is revealed, …)"), and for an agenda with no outcomes of
/// its own the whole text there.
fn agenda_card(id: &str, record: Option<&Record>) -> AgendaCard {
    let printed = |key: &str| record.map(|record| text(record, key)).unwrap_or_default();
    let (target, text1, against) = (printed("target"), printed("text1"), printed("text2"));
    let (elect, text) = if text1.is_empty() && against.is_empty() {
        (String::new(), target)
    } else {
        match target.split_once(" (") {
            Some((elect, note)) => (elect.to_owned(), format!("({note} {text1}")),
            None => (target, text1),
        }
    };
    AgendaCard {
        id: id.to_owned(),
        name: record
            .and_then(|record| record.text("name"))
            .unwrap_or(id)
            .to_owned(),
        kind: printed("type"),
        elect,
        text,
        against,
        group: agenda_group(id)
            .map_or(OTHER_GROUP, AgendaGroup::label)
            .to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::content_types::POK;

    fn printed() -> ComponentCatalog {
        component_catalog(ContentStore::embedded(), POK)
    }

    #[test]
    fn a_faction_has_its_abilities_as_printed() {
        let catalog = printed();
        assert_eq!(catalog.factions["sol"], ["orbital_drop", "versatile"]);
        for (faction, abilities) in &catalog.factions {
            for id in abilities {
                let card = catalog
                    .abilities
                    .iter()
                    .find(|card| card.id == *id)
                    .unwrap_or_else(|| panic!("{id} of {faction} is in the catalogue"));
                assert!(!card.text.is_empty(), "{id} has its text");
            }
        }
        let drop = catalog
            .abilities
            .iter()
            .find(|card| card.id == "orbital_drop")
            .expect("Orbital Drop");
        assert_eq!(drop.name, "Orbital Drop");
        assert_eq!(drop.window, "ACTION");
        assert!(drop.text.contains("2 infantry"));
    }

    #[test]
    fn every_faction_of_a_lobby_has_its_name_and_its_commodities() {
        let catalog = printed();
        for faction in ti4_engine::seating::LOBBY_FACTIONS {
            let sheet = catalog
                .faction_sheets
                .get(faction)
                .unwrap_or_else(|| panic!("{faction} has a sheet"));
            assert!(sheet.title.contains(sheet.name.split(' ').next().unwrap()));
            assert!(sheet.commodities > 0, "{faction}");
        }
        let sol = &catalog.faction_sheets["sol"];
        assert_eq!(
            (sol.name.as_str(), sol.title.as_str(), sol.commodities),
            ("Sol", "The Federation of Sol", 4)
        );
        assert_eq!(catalog.faction_sheets["sardakk"].name, "Sardakk N'orr");
    }

    #[test]
    fn a_faction_sheet_has_what_a_player_chooses_a_faction_by() {
        let catalog = printed();
        let known = |ids: &[String], cards: &[&str], what: &str| {
            for id in ids {
                assert!(
                    cards.contains(&id.as_str()),
                    "{what} {id} is in the catalogue"
                );
            }
        };
        let technologies: Vec<&str> = catalog.technologies.iter().map(|c| c.id.as_str()).collect();
        let leaders: Vec<&str> = catalog.leaders.iter().map(|c| c.id.as_str()).collect();
        for faction in ti4_engine::seating::LOBBY_FACTIONS {
            let sheet = &catalog.faction_sheets[faction];
            // The Nomad has three agents.
            assert!(sheet.leaders.len() >= 3, "{faction}");
            known(&sheet.leaders, &leaders, "the leader");
            known(
                &sheet.starting_technologies,
                &technologies,
                "the technology",
            );
            known(&sheet.faction_technologies, &technologies, "the technology");
            assert!(!sheet.faction_technologies.is_empty(), "{faction}");
            assert!(!sheet.starting_fleet.is_empty(), "{faction}");
            assert!(!sheet.home_planets.is_empty(), "{faction}");
            for units in &sheet.starting_fleet {
                if let Some(planet) = &units.planet {
                    assert!(
                        sheet.home_planets.iter().any(|home| &home.id == planet),
                        "{faction}: {planet} is a home planet"
                    );
                }
            }
            if let Some(choice) = &sheet.starting_technology_choice {
                assert!(sheet.starting_technologies.is_empty(), "{faction}");
                assert!(choice.options.len() > choice.amount as usize, "{faction}");
                known(&choice.options, &technologies, "the technology");
            }
        }
        let sol = &catalog.faction_sheets["sol"];
        assert_eq!(sol.leaders, ["solagent", "solcommander", "solhero"]);
        assert_eq!(sol.starting_technologies, ["amd", "nm"]);
        assert_eq!(sol.faction_technologies, ["so2", "ac2"]);
        let fleet: Vec<(&str, u32, Option<&str>)> = sol
            .starting_fleet
            .iter()
            .map(|units| (units.unit.as_str(), units.count, units.planet.as_deref()))
            .collect();
        assert_eq!(
            fleet,
            [
                ("carrier", 2, None),
                ("destroyer", 1, None),
                ("fighter", 3, None),
                ("infantry", 5, Some("jord")),
                ("spacedock", 1, Some("jord")),
            ]
        );
        let jord = &sol.home_planets[0];
        assert_eq!(
            (jord.name.as_str(), jord.resources, jord.influence),
            ("Jord", 4, 2)
        );
        let argent = catalog.faction_sheets["argent"]
            .starting_technology_choice
            .as_ref()
            .expect("the Argent Flight chooses");
        assert_eq!((argent.amount, argent.options.len()), (2, 3));
        assert_eq!(
            catalog.faction_sheets["sol"].starting_technology_choice,
            None
        );
    }

    #[test]
    fn a_strategy_card_of_every_set_has_its_name() {
        let catalog = printed();
        let name = |id: &str| {
            catalog
                .strategy_cards
                .iter()
                .find(|card| card.id == id)
                .map(|card| (card.initiative, card.name.as_str()))
        };
        // The sets that a table can play: `pok`, `te` and `base_game_codex1`.
        assert_eq!(name("pok4construction"), Some((4, "Construction")));
        assert_eq!(name("te4construction"), Some((4, "Construction")));
        assert_eq!(name("te6warfare"), Some((6, "Warfare")));
        assert_eq!(name("base4"), Some((4, "Construction")));
        assert_eq!(name("pok8imperial"), Some((8, "Imperial")));
    }

    #[test]
    fn a_leader_says_how_it_is_unlocked() {
        let catalog = printed();
        let leader = |id: &str| {
            catalog
                .leaders
                .iter()
                .find(|card| card.id == id)
                .unwrap_or_else(|| panic!("no leader {id}"))
        };
        let agent = leader("solagent");
        assert_eq!(agent.kind, "agent");
        assert_eq!(agent.name, "Evelyn DeLouis");
        assert!(agent.unlock.is_empty(), "an agent is never locked");
        assert!(agent.text.contains("1 additional die"));
        assert!(leader("solcommander").unlock.contains("12 resources"));
        assert_eq!(leader("solhero").kind, "hero");
        assert_eq!(leader("solhero").unlock, "Have 3 scored objectives.");
    }

    #[test]
    fn a_technology_that_always_holds_is_not_used() {
        let catalog = printed();
        let used = |id: &str| {
            catalog
                .technologies
                .iter()
                .find(|card| card.id == id)
                .unwrap_or_else(|| panic!("no technology {id}"))
                .used
        };
        assert!(used("sr"), "Sling Relay has an ACTION");
        assert!(used("bs"), "Bio-Stims exhausts");
        assert!(!used("amd"));
        assert!(!used("nm"));
        assert!(!used("cr2"), "a unit upgrade");
    }

    #[test]
    fn the_public_objectives_are_as_printed_by_stage() {
        let catalog = printed();
        let stage = |points| {
            catalog
                .public_objectives
                .iter()
                .filter(|card| card.points == points)
                .count()
        };
        assert_eq!((stage(1), stage(2)), (20, 20));
        assert_eq!(catalog.public_objectives.len(), 40);
        assert!(
            catalog
                .public_objectives
                .is_sorted_by_key(|card| card.points)
        );
        let borders = catalog
            .public_objectives
            .iter()
            .find(|card| card.id == "expand_borders")
            .expect("Expand Borders");
        assert_eq!(borders.name, "Expand Borders");
        assert_eq!(borders.group, "Planets you control");
        for card in &catalog.public_objectives {
            assert!(catalog.public_objective_groups.contains(&card.group));
            assert_ne!(card.group, OTHER_GROUP, "{} has no group", card.id);
        }
        assert_eq!(borders.text, "Control 6 planets in non-home systems.");
    }

    #[test]
    fn the_secret_objectives_are_as_printed_and_have_their_group() {
        let catalog = component_catalog(ContentStore::embedded(), POK);
        assert_eq!(catalog.secret_objectives.len(), 40);
        for objective in &catalog.secret_objectives {
            assert!(!objective.text.is_empty(), "{} has its text", objective.id);
            assert!(
                catalog.secret_objective_groups.contains(&objective.group)
                    && objective.group != OTHER_GROUP,
                "{} has its group",
                objective.id
            );
        }
        let gatekeeper = catalog
            .secret_objectives
            .iter()
            .find(|objective| objective.id == "btgk")
            .expect("Become the Gatekeeper");
        assert_eq!(gatekeeper.name, "Become the Gatekeeper");
        assert_eq!(gatekeeper.phase, "Status");
        assert_eq!(gatekeeper.group, "Ships on the board");
    }

    #[test]
    fn the_agendas_are_as_printed_and_have_their_group() {
        let catalog = component_catalog(ContentStore::embedded(), POK);
        assert_eq!(catalog.agendas.len(), 50);
        let names: std::collections::BTreeSet<&str> = catalog
            .agendas
            .iter()
            .map(|card| card.name.as_str())
            .collect();
        assert_eq!(names.len(), 50, "every agenda is in the deck once");
        for card in &catalog.agendas {
            assert!(!card.text.is_empty(), "{} has its text", card.id);
            assert!(["Law", "Directive"].contains(&card.kind.as_str()));
            assert!(
                catalog.agenda_groups.contains(&card.group) && card.group != OTHER_GROUP,
                "{} has its group",
                card.id
            );
        }
        let agenda = |id: &str| {
            catalog
                .agendas
                .iter()
                .find(|card| card.id == id)
                .unwrap_or_else(|| panic!("{id}"))
        };
        let mutiny = agenda("mutiny");
        assert_eq!(
            (mutiny.kind.as_str(), mutiny.elect.as_str()),
            ("Directive", "For/Against")
        );
        assert!(mutiny.text.starts_with("For: "));
        assert!(mutiny.against.starts_with("Against: "));
        assert_eq!(mutiny.group, "Victory points and objectives");
        // A note of the card is with the text, not with what is elected.
        let abolishment = agenda("abolishment");
        assert_eq!(abolishment.elect, "Elect Law");
        assert!(
            abolishment
                .text
                .starts_with("(When this agenda is revealed")
        );
        assert!(
            abolishment
                .text
                .ends_with("Discard the elected law from play.")
        );
        // Covert Legislation has no outcomes of its own.
        let covert = agenda("covert");
        assert_eq!(covert.elect, "");
        assert!(covert.text.starts_with("When this agenda is revealed"));
        assert_eq!(covert.against, "");
    }

    #[test]
    fn the_action_card_deck_has_every_copy_as_printed() {
        let catalog = printed();
        assert!(!catalog.action_cards.is_empty());
        for card in &catalog.action_cards {
            assert!(!card.window.is_empty(), "{} has its window", card.id);
            assert!(!card.text.is_empty(), "{} has its text", card.id);
            assert!(
                catalog.action_card_groups.contains(&card.group) && card.group != OTHER_TIMING,
                "{} has its group",
                card.id
            );
        }
        let boosts: Vec<_> = catalog
            .action_cards
            .iter()
            .filter(|card| card.name == "Morale Boost")
            .collect();
        assert_eq!(boosts.len(), 4);
        assert_eq!(boosts[0].window, "At the start of a combat round");
        assert_eq!(boosts[0].phase, "Action");
        assert_eq!(boosts[0].group, "Combat");
    }

    #[test]
    fn relics_and_legendary_planets_have_their_text() {
        let catalog = printed();
        let core = catalog
            .relics
            .iter()
            .find(|card| card.id == "dynamiscore")
            .expect("Dynamis Core");
        assert_eq!(core.name, "Dynamis Core");
        assert!(core.used, "Dynamis Core has an ACTION");
        let shard = catalog.relics.iter().find(|card| card.id == "shard");
        assert!(!shard.expect("Shard of the Throne").used);
        assert!(!catalog.legendary.is_empty());
        for card in &catalog.legendary {
            assert!(!card.text.is_empty(), "{} has its text", card.id);
        }
    }
}
