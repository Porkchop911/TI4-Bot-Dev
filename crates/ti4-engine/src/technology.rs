//! Technology: prerequisites and research (LRR 90).
//!
//! Ported from the oracle's `engine/technology.py`: `owned_colours`, `can_research`,
//! `researchable`, `research` and `grant`.

use std::collections::{BTreeMap, BTreeSet};

use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{PlanetId, PlayerId, SystemId, TechnologyId, UnitTypeId};
use ti4_model::state::GameState;

use crate::choice::{Choice, ChoiceOption, IllegalChoice, Observed, Table};

/// The four research tracks. Unit upgrades have no colour (90.7b), which is why
/// `UNITUPGRADE` is deliberately absent.
pub const COLOURS: [&str; 4] = ["BIOTIC", "CYBERNETIC", "PROPULSION", "WARFARE"];

/// Technologies in the authoritative current PoK/Codex deck.
///
/// The raw corpus deliberately contains original and replacement printings together.  The oracle
/// uses `techs_pok_c4` to select the active printing; treating every corpus record as researchable
/// offers obsolete Magen and X-89 variants as separate technologies.
#[must_use]
pub fn active_aliases(content: &ContentStore) -> BTreeSet<TechnologyId> {
    content
        .get(ContentType::Decks, "techs_pok_c4")
        .map(|deck| {
            deck.strings("cardIDs")
                .into_iter()
                .map(TechnologyId::new)
                .collect()
        })
        .unwrap_or_default()
}

/// Printed technology name used by learned choice labels.
#[must_use]
pub fn name(content: &ContentStore, alias: &TechnologyId) -> String {
    content
        .get(ContentType::Technologies, alias.as_str())
        .and_then(|record| record.text("name"))
        .unwrap_or_else(|| alias.as_str())
        .to_owned()
}

/// Whether Gravity Drive may still raise one ship's move in this tactical action.
#[must_use]
pub fn gravity_drive_available(state: &GameState, player: &PlayerId) -> bool {
    let Some(seat) = state.player(player) else {
        return false;
    };
    seat.technologies.contains(&TechnologyId::new("gd"))
        && seat.gravity_drive_used_activation != Some(state.activation_seq)
}

/// Spend Gravity Drive's once-per-tactical-action movement bonus.
#[must_use]
pub fn use_gravity_drive(state: &mut GameState, player: &PlayerId) -> bool {
    if !gravity_drive_available(state, player) {
        return false;
    }
    let activation = state.activation_seq;
    if let Some(seat) = state.player_mut(player) {
        seat.gravity_drive_used_activation = Some(activation);
        true
    } else {
        false
    }
}

/// Ready technology component actions currently implemented by the Rust engine.
#[must_use]
pub fn component_actions(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<ChoiceOption> {
    let Some(seat) = state.player(player) else {
        return Vec::new();
    };
    let sling = TechnologyId::new("sr");
    if seat.technologies.contains(&sling)
        && !seat.exhausted_technologies.contains(&sling)
        && crate::production::can_sling_relay(state, content, sources, player)
    {
        vec![ChoiceOption::labelled(
            "component|tech|sr",
            "component",
            "use Sling Relay",
        )]
    } else {
        Vec::new()
    }
}

/// Resolve one implemented technology component action.
///
/// # Errors
/// Returns [`IllegalChoice`] when a nested production or payment choice is invalid.
pub fn perform_component(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    option: &ChoiceOption,
) -> Result<bool, IllegalChoice> {
    if option.id != "component|tech|sr"
        || !state
            .player(player)
            .is_some_and(|seat| seat.technologies.contains(&TechnologyId::new("sr")))
    {
        return Ok(false);
    }
    let produced = crate::production::sling_relay(state, content, sources, galaxy, table, player)?;
    if produced && let Some(seat) = state.player_mut(player) {
        seat.exhausted_technologies.insert(TechnologyId::new("sr"));
    }
    Ok(produced)
}

/// Resolve technology effects offered for free at the start of an action-phase turn.
///
/// Transit Diodes is deliberately here rather than in [`component_actions`]: its printed timing
/// is not an action, and charging a turn for it changes both the opening and the learned prompt.
///
/// # Errors
/// Returns [`IllegalChoice`] if the decider does not select an offered redeployment.
#[allow(
    clippy::too_many_lines,
    reason = "the start window sequences several independent optional technologies"
)]
pub fn start_turn(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    table: &mut Table,
    player: &PlayerId,
) -> Result<(), IllegalChoice> {
    // Psychoarchaeology is a free action-phase window, not a component action.  Each ready
    // specialty planet may be converted once; declining preserves it for a later use.
    if state
        .player(player)
        .is_some_and(|seat| seat.technologies.contains(&TechnologyId::new("pa")))
    {
        let planets = ti4_content::galaxy::all_planets(content, sources);
        loop {
            let candidates: Vec<PlanetId> = state
                .controlled_planets(player)
                .into_iter()
                .map(|(_, planet)| planet.clone())
                .filter(|planet| !state.exhausted_planets.contains(planet))
                .filter(|planet| {
                    planets
                        .get(planet.as_str())
                        .is_some_and(|record| !record.tech_specialties().is_empty())
                })
                .collect();
            if candidates.is_empty() {
                break;
            }
            let mut options: Vec<ChoiceOption> = candidates
                .iter()
                .map(|planet| {
                    ChoiceOption::labelled(
                        planet.to_string(),
                        "ability",
                        format!("exhaust {planet}"),
                    )
                    .with("planet", planet.to_string())
                    .with("technology", "pa")
                })
                .collect();
            options.push(ChoiceOption::decline());
            let choice = Choice::new(
                player.clone(),
                "Psychoarchaeology: exhaust a specialty for 1 trade good",
                options,
            );
            let answer =
                table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
            if answer.is_decline() {
                break;
            }
            state.exhaust_planet(PlanetId::new(answer.id));
            if let Some(seat) = state.player_mut(player) {
                seat.trade_goods += 1;
            }
        }
    }

    let transit = TechnologyId::new("td");
    let has_transit = state.player(player).is_some_and(|seat| {
        seat.technologies.contains(&transit) && !seat.exhausted_technologies.contains(&transit)
    });
    if has_transit {
        let mut moved = 0;
        let mut arrivals = BTreeSet::new();
        while moved < 4 {
            let options = transit_options(state, content, sources, player, &arrivals);
            if options.is_empty() {
                break;
            }
            let mut offered = options;
            offered.push(ChoiceOption::labelled(
                "decline",
                crate::choice::DECLINE_KIND,
                "finish redeployment",
            ));
            let choice = Choice::new(
                player.clone(),
                format!(
                    "Transit Diodes: redeploy ground forces ({} left)",
                    4 - moved
                ),
                offered,
            );
            let answer =
                table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
            if answer.is_decline() {
                break;
            }
            let Some((source_system, source, unit, destination_system, planet)) =
                parse_transit(&answer.id)
            else {
                break;
            };
            let Some(unit) = take_transit_unit(
                state,
                content,
                sources,
                player,
                &source_system,
                &source,
                &unit,
            ) else {
                break;
            };
            state
                .system_mut(&destination_system)
                .planet_units
                .entry(planet.clone())
                .or_default()
                .push(unit);
            arrivals.insert((destination_system, planet));
            moved += 1;
        }
        if moved > 0
            && let Some(seat) = state.player_mut(player)
        {
            seat.exhausted_technologies.insert(transit);
        }
    }

    if state
        .player(player)
        .is_some_and(|seat| seat.technologies.contains(&TechnologyId::new("cm")))
    {
        let systems: Vec<SystemId> = state
            .board
            .keys()
            .filter(|system| {
                crate::production::capacity(state, content, sources, player, system) > 0
            })
            .cloned()
            .collect();
        if !systems.is_empty() {
            let mut options: Vec<ChoiceOption> = systems
                .iter()
                .map(|system| {
                    ChoiceOption::labelled(
                        system.to_string(),
                        "production",
                        format!("produce one unit in {system}"),
                    )
                    .with("system", system.to_string())
                    .with("technology", "cm")
                })
                .collect();
            options.push(ChoiceOption::decline());
            let choice = Choice::new(player.clone(), "Chaos Mapping", options);
            let answer =
                table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
            if !answer.is_decline() {
                let system = SystemId::new(answer.id);
                let _ = crate::production::produce_one(
                    state, content, sources, galaxy, table, player, &system,
                )?;
            }
        }
    }
    Ok(())
}

/// Resolve free technology windows at the end of an action-phase turn.
///
/// # Errors
/// Returns [`IllegalChoice`] when a redistribution or readying answer was not offered.
#[allow(
    clippy::too_many_lines,
    reason = "the end window sequences several independent optional technologies"
)]
pub fn end_turn(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    table: &mut Table,
    player: &PlayerId,
) -> Result<(), IllegalChoice> {
    let predictive = TechnologyId::new("pi");
    if state.player(player).is_some_and(|seat| {
        seat.technologies.contains(&predictive)
            && !seat.exhausted_technologies.contains(&predictive)
    }) {
        let mut changed = false;
        let maximum = state.player(player).map_or(0, |seat| {
            seat.tactic_tokens + seat.fleet_tokens + seat.strategic_tokens
        });
        let mut moved = 0;
        while moved < maximum {
            let Some(seat) = state.player(player) else {
                break;
            };
            let pools = [
                ("tactic", seat.tactic_tokens),
                ("fleet", seat.fleet_tokens),
                ("strategy", seat.strategic_tokens),
            ];
            let mut options = Vec::new();
            for (source, held) in pools {
                if held <= 0 {
                    continue;
                }
                for destination in ["tactic", "fleet", "strategy"] {
                    if source != destination {
                        options.push(ChoiceOption::labelled(
                            format!("{source}|{destination}"),
                            "redistribute",
                            format!("move 1 token from {source} to {destination}"),
                        ));
                    }
                }
            }
            if options.is_empty() {
                break;
            }
            options.push(ChoiceOption::labelled(
                "done",
                crate::choice::DECLINE_KIND,
                "finish redistribution",
            ));
            let choice = Choice::new(
                player.clone(),
                "Predictive Intelligence: redistribute command tokens",
                options,
            );
            let answer =
                table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
            if answer.is_decline() {
                break;
            }
            let mut parts = answer.id.split('|');
            let (Some(source), Some(destination)) = (parts.next(), parts.next()) else {
                break;
            };
            if let Some(seat) = state.player_mut(player) {
                let take = match source {
                    "tactic" => &mut seat.tactic_tokens,
                    "fleet" => &mut seat.fleet_tokens,
                    "strategy" => &mut seat.strategic_tokens,
                    _ => break,
                };
                *take -= 1;
                let give = match destination {
                    "tactic" => &mut seat.tactic_tokens,
                    "fleet" => &mut seat.fleet_tokens,
                    "strategy" => &mut seat.strategic_tokens,
                    _ => break,
                };
                *give += 1;
                changed = true;
                moved += 1;
            }
        }
        if changed && let Some(seat) = state.player_mut(player) {
            seat.exhausted_technologies.insert(predictive);
        }
    }

    let bio_stims = TechnologyId::new("bs");
    if state.player(player).is_some_and(|seat| {
        seat.technologies.contains(&bio_stims) && !seat.exhausted_technologies.contains(&bio_stims)
    }) {
        let planets = ti4_content::galaxy::all_planets(content, sources);
        let mut options: Vec<ChoiceOption> = state
            .controlled_planets(player)
            .into_iter()
            .map(|(_, planet)| planet.clone())
            .filter(|planet| state.exhausted_planets.contains(planet))
            .filter(|planet| {
                planets
                    .get(planet.as_str())
                    .is_some_and(|record| !record.tech_specialties().is_empty())
            })
            .map(|planet| {
                ChoiceOption::labelled(
                    format!("ready|planet|{planet}"),
                    "ready",
                    format!("Bio-Stims: ready {planet}"),
                )
                .with("planet", planet.to_string())
                .with("technology", "bs")
            })
            .collect();
        if let Some(seat) = state.player(player) {
            options.extend(
                seat.exhausted_technologies
                    .iter()
                    .filter(|technology| *technology != &bio_stims)
                    .map(|technology| {
                        ChoiceOption::labelled(
                            format!("ready|technology|{technology}"),
                            "ready_technology",
                            format!("Bio-Stims: ready {}", name(content, technology)),
                        )
                        .with("technology", technology.to_string())
                        .with("bio_stims", true)
                    }),
            );
        }
        if !options.is_empty() {
            options.push(ChoiceOption::decline());
            let choice = Choice::new(player.clone(), "Bio-Stims", options);
            let answer =
                table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
            if !answer.is_decline() {
                if let Some(planet) = answer.id.strip_prefix("ready|planet|") {
                    state.ready_planet(&PlanetId::new(planet));
                } else if let Some(technology) = answer.id.strip_prefix("ready|technology|")
                    && let Some(seat) = state.player_mut(player)
                {
                    seat.exhausted_technologies
                        .remove(&TechnologyId::new(technology));
                }
                if let Some(seat) = state.player_mut(player) {
                    seat.exhausted_technologies.insert(bio_stims);
                }
            }
        }
    }
    Ok(())
}

/// Resolve technology effects caused by this player gaining control of a planet.
///
/// Integrated Economy is an `AFTER PLANET_CONTROL_GAINED` effect in the oracle.  Keeping the
/// ownership check here gives every control-gain path one technology boundary instead of making
/// invasion, diplomacy, and future annexation rules know the card text themselves.
///
/// # Errors
/// Returns [`IllegalChoice`] if the triggered production or one of its payments is invalid.
#[allow(
    clippy::too_many_arguments,
    reason = "a control-gain trigger needs the complete observed rules position"
)]
pub fn control_gained(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) -> Result<bool, IllegalChoice> {
    if !state
        .player(player)
        .is_some_and(|seat| seat.technologies.contains(&TechnologyId::new("ie")))
    {
        return Ok(false);
    }
    crate::production::integrated_economy(
        state, content, sources, galaxy, table, player, system, planet,
    )
}

fn transit_options(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    barred_sources: &BTreeSet<(SystemId, PlanetId)>,
) -> Vec<ChoiceOption> {
    let types = ti4_content::units::catalogue(content, sources);
    let destinations: Vec<(SystemId, PlanetId)> = state
        .controlled_planets(player)
        .into_iter()
        .map(|(system, planet)| (system.clone(), planet.clone()))
        .collect();
    let mut seen = BTreeSet::new();
    let mut options = Vec::new();
    for (source_system, board) in &state.board {
        let sources_here = board
            .units
            .iter()
            .filter(|unit| &unit.owner == player)
            .map(|unit| ("space".to_owned(), unit))
            .chain(board.planet_units.iter().flat_map(|(planet, units)| {
                units
                    .iter()
                    .filter(|unit| &unit.owner == player)
                    .map(move |unit| (planet.to_string(), unit))
            }));
        for (source, unit) in sources_here {
            if !types
                .get(unit.type_id.as_str())
                .is_some_and(ti4_content::units::UnitType::is_ground_force)
                || (source != "space"
                    && barred_sources.contains(&(source_system.clone(), PlanetId::new(&source))))
            {
                continue;
            }
            for (destination_system, planet) in &destinations {
                if source_system == destination_system && source == planet.as_str() {
                    continue;
                }
                if !seen.insert((
                    source_system.clone(),
                    source.clone(),
                    unit.type_id.clone(),
                    planet.clone(),
                )) {
                    continue;
                }
                options.push(
                    ChoiceOption::labelled(
                        format!(
                            "transit|{source_system}|{source}|{}|{destination_system}|{planet}",
                            unit.type_id
                        ),
                        "transit",
                        format!("move {} from {source} to {planet}", unit.type_id),
                    )
                    .with("source_system", source_system.to_string())
                    .with("source", source.clone())
                    .with("unit", unit.type_id.to_string())
                    .with("destination_system", destination_system.to_string())
                    .with("planet", planet.to_string()),
                );
            }
        }
    }
    options
}

fn parse_transit(id: &str) -> Option<(SystemId, String, UnitTypeId, SystemId, PlanetId)> {
    let mut parts = id.splitn(6, '|');
    match (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) {
        (Some("transit"), Some(ss), Some(source), Some(unit), Some(ds), Some(planet)) => Some((
            SystemId::new(ss),
            source.to_owned(),
            UnitTypeId::new(unit),
            SystemId::new(ds),
            PlanetId::new(planet),
        )),
        _ => None,
    }
}

fn take_transit_unit(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    source_system: &SystemId,
    source: &str,
    unit_type: &UnitTypeId,
) -> Option<ti4_model::units::Unit> {
    let types = ti4_content::units::catalogue(content, sources);
    let is_match = |unit: &ti4_model::units::Unit| {
        &unit.owner == player
            && &unit.type_id == unit_type
            && types
                .get(unit.type_id.as_str())
                .is_some_and(ti4_content::units::UnitType::is_ground_force)
    };
    let board = state.system_mut(source_system);
    let units = if source == "space" {
        &mut board.units
    } else {
        board.planet_units.get_mut(&PlanetId::new(source))?
    };
    units
        .iter()
        .position(is_match)
        .map(|index| units.remove(index))
}

/// The letter each colour is written as in a technology's `requirements` string.
///
/// The corpus spells prerequisites as e.g. `RRRY` — three warfare and one cybernetic — rather
/// than as counts per named track.
#[must_use]
pub fn colour_of(letter: char) -> Option<&'static str> {
    match letter.to_ascii_uppercase() {
        'G' => Some("BIOTIC"),
        'Y' => Some("CYBERNETIC"),
        'B' => Some("PROPULSION"),
        'R' => Some("WARFARE"),
        _ => None,
    }
}

/// What a technology needs, as counts per colour.
#[must_use]
pub fn prerequisites(
    content: &ContentStore,
    alias: &TechnologyId,
) -> BTreeMap<&'static str, usize> {
    let mut needs = BTreeMap::new();
    let Some(record) = content.get(ContentType::Technologies, alias.as_str()) else {
        return needs;
    };
    let printed = record.text("requirements").unwrap_or("").trim();
    // The corpus writes "no prerequisites" as the literal strings "null" and "None" as well as
    // an absent field. They are spelled out here rather than being caught by the
    // is-not-a-colour-letter fallback, because that fallback would make any future typo a free
    // technology instead of an error.
    if printed.is_empty()
        || printed.eq_ignore_ascii_case("null")
        || printed.eq_ignore_ascii_case("none")
    {
        return needs;
    }
    for letter in printed.chars() {
        if let Some(colour) = colour_of(letter) {
            *needs.entry(colour).or_insert(0) += 1;
        }
    }
    needs
}

/// The colour a technology itself counts as, if any.
#[must_use]
pub fn colour_type(content: &ContentStore, alias: &TechnologyId) -> Option<&'static str> {
    let record = content.get(ContentType::Technologies, alias.as_str())?;
    let types = record.strings("types");
    COLOURS
        .iter()
        .find(|colour| types.contains(colour))
        .copied()
}

/// Whether this is a unit upgrade, which has no colour (90.7b).
#[must_use]
pub fn is_unit_upgrade(content: &ContentStore, alias: &TechnologyId) -> bool {
    content
        .get(ContentType::Technologies, alias.as_str())
        .is_some_and(|record| record.strings("types").contains(&"UNITUPGRADE"))
}

/// The faction a technology belongs to, if it is faction-specific (90.11).
#[must_use]
pub fn faction_of<'a>(content: &'a ContentStore, alias: &TechnologyId) -> Option<&'a str> {
    content
        .get(ContentType::Technologies, alias.as_str())
        .and_then(|record| record.text("faction"))
        .filter(|faction| !faction.is_empty())
}

/// How many technologies of each colour this player owns (90.7a).
#[must_use]
pub fn owned_colours(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
) -> BTreeMap<&'static str, usize> {
    let mut held = BTreeMap::new();
    let Some(seat) = state.player(player) else {
        return held;
    };
    for alias in &seat.technologies {
        if let Some(colour) = colour_type(content, alias) {
            *held.entry(colour).or_insert(0) += 1;
        }
    }
    held
}

/// Technology specialties on the planets this player controls, by colour.
///
/// A specialty stands in for one prerequisite of its colour (90.8), which is most of why a
/// planet with one is worth taking.
#[must_use]
pub fn specialties(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> BTreeMap<&'static str, usize> {
    let catalogue = ti4_content::galaxy::all_planets(content, sources);
    let mut found = BTreeMap::new();
    for (_, planet) in state.controlled_planets(player) {
        let Some(record) = catalogue.get(planet.as_str()) else {
            continue;
        };
        for specialty in record.tech_specialties() {
            let upper = specialty.to_ascii_uppercase();
            if let Some(colour) = COLOURS.iter().find(|c| **c == upper) {
                *found.entry(*colour).or_insert(0) += 1;
            }
        }
    }
    found
}

/// Whether this player may research a technology now.
#[must_use]
pub fn can_research(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    alias: &TechnologyId,
) -> bool {
    let Some(record) = content.get(ContentType::Technologies, alias.as_str()) else {
        return false;
    };
    let Some(seat) = state.player(player) else {
        return false;
    };
    if seat.technologies.contains(alias) {
        return false;
    }
    // Some cards say so of themselves.
    if record.text("text").is_some_and(|printed| {
        printed
            .to_ascii_lowercase()
            .contains("cannot be researched")
    }) {
        return false;
    }
    // 90.11: a faction technology belongs to that faction alone.
    if let Some(faction) = faction_of(content, alias)
        && faction != seat.faction.as_str()
    {
        return false;
    }

    let held = owned_colours(state, content, player);
    let specialties = specialties(state, content, sources, player);
    // A faction may waive whole prerequisite slots — Jol-Nar's Brilliant on anything, Analytical
    // on anything that is not a unit upgrade. Applied as a budget across the requirement rather
    // than per colour, because the card says "ignore 1 prerequisite", not "one of each".
    let mut waivable = crate::faction_abilities::waived_prerequisites(
        state,
        content,
        sources,
        player,
        alias.as_str(),
    );
    // Synergy rule 2: when researching, a technology owned or a specialty controlled that matches
    // one colour of the synergy may be treated as either colour of it. Owned technologies and
    // specialties are pooled first, because the rule names both and treats them alike.
    let mut holdings: std::collections::BTreeMap<&'static str, usize> = held;
    for (colour, count) in specialties {
        *holdings.entry(colour).or_insert(0) += count;
    }
    let pair = crate::synergy::pair(state, content, sources, player);
    crate::synergy::satisfies(
        &prerequisites(content, alias),
        &holdings,
        pair.as_ref(),
        waivable,
    )
}

/// Everything this player could research now, in a stable order.
///
/// The order is canonical (sorted by technology id), not the file layout of
/// `technologies.json` — choice option order must not follow corpus extraction order
/// (F-M08-019-1; the oracle sorted here too).
#[must_use]
pub fn researchable(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<TechnologyId> {
    let active = active_aliases(content);
    let mut open: Vec<TechnologyId> = content
        .records(ContentType::Technologies)
        .iter()
        .filter_map(|record| record.text("alias"))
        .map(TechnologyId::new)
        .filter(|alias| active.contains(alias))
        .filter(|alias| can_research(state, content, sources, player, alias))
        .collect();
    open.sort();
    open
}

/// Gain a technology outright (90.5), without checking prerequisites.
///
/// Separate from [`research`] because gaining is not researching: several effects grant a
/// technology directly, and the rules that fire on *research* must not fire for those.
pub fn grant(state: &mut GameState, player: &PlayerId, alias: &TechnologyId) {
    if let Some(seat) = state.player_mut(player) {
        seat.technologies.insert(alias.clone());
    }
}

/// Research a technology, having satisfied its prerequisites. `false` if it could not be.
pub fn research(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    alias: &TechnologyId,
) -> bool {
    if !can_research(state, content, sources, player, alias) {
        return false;
    }
    grant(state, player, alias);
    true
}

#[cfg(test)]
mod tests {
    /// Synergy rule 2, end to end through `can_research`.
    ///
    /// Transit Diodes needs two cybernetic. Jol-Nar's breakthrough joins biotic and cybernetic, so
    /// two biotic technologies must be enough — and must *not* be enough without the breakthrough,
    /// which is the half that proves the synergy is doing the work rather than the waiver.
    #[test]
    fn a_synergy_lets_one_colour_pay_for_the_other() {
        use ti4_model::content_types::DEFAULT as ALL_SOURCES;

        let content = ti4_content::ContentStore::embedded();
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        {
            let seat = state.player_mut(&player).expect("seated");
            seat.faction = ti4_model::id::FactionId::new("jolnar");
            // Two biotic technologies, neither of them cybernetic.
            seat.technologies = [TechnologyId::new("nm"), TechnologyId::new("pa")]
                .into_iter()
                .collect();
        }
        let wanted = TechnologyId::new("td");

        assert!(
            !can_research(&state, content, ALL_SOURCES, &player, &wanted),
            "without the breakthrough, biotic cannot pay a cybernetic prerequisite"
        );

        state
            .player_mut(&player)
            .expect("seated")
            .breakthrough = Some(ti4_model::id::BreakthroughId::new("jolnarbt"));

        assert!(
            can_research(&state, content, ALL_SOURCES, &player, &wanted),
            "Specialist Compounds joins biotic and cybernetic (synergy rule 2)"
        );
    }

    use ti4_model::content_types::POK;

    use super::*;
    use crate::fixtures::game;

    fn player() -> PlayerId {
        PlayerId::new("a")
    }

    fn give(state: &mut GameState, aliases: &[&str]) {
        for alias in aliases {
            state
                .player_mut(&player())
                .unwrap()
                .technologies
                .insert(TechnologyId::new(*alias));
        }
    }

    #[test]
    fn transit_diodes_redeploys_ground_at_the_start_of_a_turn_and_exhausts() {
        let mut state = game(&["a"]);
        give(&mut state, &["td"]);
        let source_system = SystemId::new("01");
        let destination_system = SystemId::new("02");
        let source = PlanetId::new("source");
        let destination = PlanetId::new("destination");
        state
            .system_mut(&source_system)
            .set_control(source.clone(), player());
        state
            .system_mut(&destination_system)
            .set_control(destination.clone(), player());
        state
            .system_mut(&source_system)
            .planet_units
            .entry(source.clone())
            .or_default()
            .push(ti4_model::units::Unit::new(
                UnitTypeId::new("infantry"),
                player(),
            ));
        let move_id =
            format!("transit|{source_system}|{source}|infantry|{destination_system}|{destination}");
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([move_id])));

        start_turn(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            &player(),
        )
        .unwrap();

        assert!(
            state
                .system_state(&source_system)
                .on_planet(&source)
                .is_empty()
        );
        assert_eq!(
            state
                .system_state(&destination_system)
                .on_planet(&destination)
                .len(),
            1
        );
        assert!(
            state
                .player(&player())
                .unwrap()
                .exhausted_technologies
                .contains(&TechnologyId::new("td"))
        );
    }

    #[test]
    fn integrated_economy_is_offered_after_control_is_gained_and_builds_on_that_planet() {
        let mut state = game(&["a"]);
        give(&mut state, &["ie"]);
        let (system, planet) = crate::fixtures::a_placed_planet();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        state.player_mut(&player()).unwrap().trade_goods = 1;
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([
            "build|destroyer|1".to_owned(),
            "trade_good".to_owned(),
            "done_producing".to_owned(),
        ])));

        let built = control_gained(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            &player(),
            &system,
            &planet,
        )
        .unwrap();

        assert!(built);
        assert!(
            state
                .system_state(&system)
                .units_of(&player())
                .iter()
                .any(|unit| unit.type_id.as_str() == "destroyer"),
            "choices: {:?}; units: {:?}",
            table.log.records,
            state.system_state(&system).units
        );
        assert!(table.log.records.iter().any(|record| {
            record.prompt.starts_with("Integrated Economy on ")
                && record.offered.iter().any(|id| id == "done_producing")
        }));
    }

    #[test]
    fn control_gain_opens_no_integrated_economy_window_without_the_technology() {
        let mut state = game(&["a"]);
        let (system, planet) = crate::fixtures::a_placed_planet();
        let mut table = Table::new();

        assert!(
            !control_gained(
                &mut state,
                ContentStore::embedded(),
                POK,
                None,
                &mut table,
                &player(),
                &system,
                &planet,
            )
            .unwrap()
        );
        assert!(table.log.is_empty());
    }

    #[test]
    fn psychoarchaeology_is_a_learned_start_of_turn_conversion() {
        let mut state = game(&["a"]);
        give(&mut state, &["pa"]);
        let specialty = ti4_content::galaxy::all_planets(ContentStore::embedded(), POK)
            .values()
            .find(|planet| !planet.tech_specialties().is_empty())
            .map(|planet| PlanetId::new(planet.id()))
            .expect("the map corpus has a technology specialty");
        let system = SystemId::new("01");
        state
            .system_mut(&system)
            .set_control(specialty.clone(), player());
        let before = state.player(&player()).unwrap().trade_goods;
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([
            specialty.to_string()
        ])));

        start_turn(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            &player(),
        )
        .unwrap();

        assert!(state.exhausted_planets.contains(&specialty));
        assert_eq!(state.player(&player()).unwrap().trade_goods, before + 1);
        assert!(table.log.records[0].prompt.starts_with("Psychoarchaeology"));
    }

    #[test]
    fn bio_stims_is_a_learned_end_of_turn_readying_choice() {
        let mut state = game(&["a"]);
        give(&mut state, &["bs", "td"]);
        state
            .player_mut(&player())
            .unwrap()
            .exhausted_technologies
            .insert(TechnologyId::new("td"));
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([
            "ready|technology|td".to_owned(),
        ])));

        end_turn(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            &player(),
        )
        .unwrap();

        let exhausted = &state.player(&player()).unwrap().exhausted_technologies;
        assert!(!exhausted.contains(&TechnologyId::new("td")));
        assert!(exhausted.contains(&TechnologyId::new("bs")));
    }

    #[test]
    fn chaos_mapping_chooses_a_system_and_one_unit_at_the_start_of_turn() {
        let mut state = game(&["a"]);
        give(&mut state, &["cm"]);
        let (system, planet) = crate::fixtures::a_placed_planet();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);
        state.player_mut(&player()).unwrap().trade_goods = 1;
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([
            system.to_string(),
            "build|destroyer|1".to_owned(),
            "trade_good".to_owned(),
        ])));

        start_turn(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            &player(),
        )
        .unwrap();

        assert!(
            state
                .system_state(&system)
                .units_of(&player())
                .iter()
                .any(|unit| unit.type_id.as_str() == "destroyer"),
            "choices: {:?}; units: {:?}",
            table.log.records,
            state.system_state(&system).units
        );
        assert_eq!(table.log.records[0].prompt, "Chaos Mapping");
    }

    #[test]
    fn every_requirement_letter_names_a_track() {
        // If the corpus ever spells a prerequisite with a letter this does not know, the
        // technology silently becomes free rather than unresearchable.
        let mut unknown: Vec<char> = ContentStore::embedded()
            .records(ContentType::Technologies)
            .iter()
            .filter_map(|record| record.text("requirements"))
            .map(str::trim)
            .filter(|printed| {
                !printed.is_empty()
                    && !printed.eq_ignore_ascii_case("null")
                    && !printed.eq_ignore_ascii_case("none")
            })
            .flat_map(str::chars)
            .filter(|letter| colour_of(*letter).is_none())
            .collect();
        unknown.sort_unstable();
        unknown.dedup();
        assert!(
            unknown.is_empty(),
            "unmapped requirement letters: {unknown:?}"
        );
    }

    #[test]
    fn prerequisites_are_read_off_the_requirement_string() {
        // Gravity Drive needs one propulsion; a war sun needs three warfare and a cybernetic.
        assert_eq!(
            prerequisites(ContentStore::embedded(), &TechnologyId::new("gd")),
            BTreeMap::from([("PROPULSION", 1)])
        );
        assert_eq!(
            prerequisites(ContentStore::embedded(), &TechnologyId::new("ws")),
            BTreeMap::from([("WARFARE", 3), ("CYBERNETIC", 1)])
        );
    }

    #[test]
    fn a_unit_upgrade_has_no_colour() {
        // 90.7b, and the reason unit upgrades are counted separately from the four tracks.
        let ws = TechnologyId::new("ws");
        assert!(is_unit_upgrade(ContentStore::embedded(), &ws));
        assert_eq!(colour_type(ContentStore::embedded(), &ws), None);
    }

    #[test]
    fn a_technology_with_unmet_prerequisites_cannot_be_researched() {
        let state = game(&["a"]);
        assert!(!can_research(
            &state,
            ContentStore::embedded(),
            POK,
            &player(),
            &TechnologyId::new("ws")
        ));
    }

    #[test]
    fn owning_the_prerequisites_unlocks_it() {
        let mut state = game(&["a"]);
        // Three warfare and one cybernetic.
        let warfare: Vec<String> = ContentStore::embedded()
            .records(ContentType::Technologies)
            .iter()
            .filter(|record| record.strings("types").contains(&"WARFARE"))
            .filter_map(|record| record.text("alias"))
            .map(ToOwned::to_owned)
            .take(3)
            .collect();
        let cybernetic: Vec<String> = ContentStore::embedded()
            .records(ContentType::Technologies)
            .iter()
            .filter(|record| record.strings("types").contains(&"CYBERNETIC"))
            .filter_map(|record| record.text("alias"))
            .map(ToOwned::to_owned)
            .take(1)
            .collect();
        let held: Vec<&str> = warfare
            .iter()
            .chain(&cybernetic)
            .map(String::as_str)
            .collect();
        give(&mut state, &held);

        assert!(can_research(
            &state,
            ContentStore::embedded(),
            POK,
            &player(),
            &TechnologyId::new("ws")
        ));
    }

    #[test]
    fn a_technology_already_owned_is_not_researchable_again() {
        let mut state = game(&["a"]);
        give(&mut state, &["gd"]);
        assert!(!can_research(
            &state,
            ContentStore::embedded(),
            POK,
            &player(),
            &TechnologyId::new("gd")
        ));
    }

    #[test]
    fn researchable_uses_the_authoritative_current_printings() {
        let content = ContentStore::embedded();
        let active = active_aliases(content);
        assert!(active.contains(&TechnologyId::new("md")));
        assert!(!active.contains(&TechnologyId::new("md_base")));
        assert!(!active.contains(&TechnologyId::new("md_c1")));

        let offered = researchable(&game(&["a"]), content, POK, &player());
        assert!(!offered.is_empty());
        assert!(!offered.contains(&TechnologyId::new("md_base")));
        assert!(!offered.contains(&TechnologyId::new("md_c1")));
    }

    #[test]
    fn learned_research_labels_use_the_printed_name() {
        assert_eq!(
            name(ContentStore::embedded(), &TechnologyId::new("sr")),
            "Sling Relay"
        );
    }

    #[test]
    fn a_faction_technology_belongs_to_its_faction_alone() {
        // 90.11.
        let state = game(&["a"]);
        let foreign = ContentStore::embedded()
            .records(ContentType::Technologies)
            .iter()
            .find(|record| {
                record.text("faction").is_some_and(|f| {
                    !f.is_empty() && f != state.player(&player()).unwrap().faction.as_str()
                })
            })
            .and_then(|record| record.text("alias"))
            .map(TechnologyId::new);

        if let Some(foreign) = foreign {
            assert!(!can_research(
                &state,
                ContentStore::embedded(),
                POK,
                &player(),
                &foreign
            ));
        }
    }

    #[test]
    fn a_planet_specialty_stands_in_for_a_prerequisite() {
        // 90.8, and most of why a planet with one is worth taking.
        let mut state = game(&["a"]);
        let target = TechnologyId::new("gd"); // one propulsion
        assert!(!can_research(
            &state,
            ContentStore::embedded(),
            POK,
            &player(),
            &target
        ));

        let planet = ti4_content::galaxy::all_planets(ContentStore::embedded(), POK)
            .iter()
            .find(|(_, record)| {
                record
                    .tech_specialties()
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case("propulsion"))
            })
            .map(|(id, record)| {
                (
                    ti4_model::id::SystemId::new(record.system_id().unwrap_or("18")),
                    ti4_model::id::PlanetId::new(*id),
                )
            });

        let Some((system, planet)) = planet else {
            return; // no propulsion specialty in this scope
        };
        state.system_mut(&system).set_control(planet, player());

        assert!(
            can_research(&state, ContentStore::embedded(), POK, &player(), &target),
            "the specialty covers the prerequisite"
        );
    }

    #[test]
    fn researching_grants_it_and_gaining_does_not_need_prerequisites() {
        // 90.5: several effects grant a technology outright, and that is not researching.
        let mut state = game(&["a"]);
        let ws = TechnologyId::new("ws");

        assert!(!research(
            &mut state,
            ContentStore::embedded(),
            POK,
            &player(),
            &ws
        ));
        assert!(!state.player(&player()).unwrap().technologies.contains(&ws));

        grant(&mut state, &player(), &ws);
        assert!(state.player(&player()).unwrap().technologies.contains(&ws));
    }

    #[test]
    fn researchable_lists_only_what_is_reachable() {
        let state = game(&["a"]);
        let open = researchable(&state, ContentStore::embedded(), POK, &player());

        assert!(!open.is_empty(), "some technologies need nothing");
        assert!(
            !open.contains(&TechnologyId::new("ws")),
            "a war sun needs four prerequisites"
        );
        for alias in &open {
            assert!(prerequisites(ContentStore::embedded(), alias).is_empty());
        }
    }

    #[test]
    fn researchable_offers_options_in_canonical_sorted_order() {
        // F-M08-019-1: option order must not follow the file layout of technologies.json —
        // the oracle sorted, and "a stable order" means a canonical one.
        let state = game(&["a"]);
        let open = researchable(&state, ContentStore::embedded(), POK, &player());
        assert!(open.len() >= 2, "several technologies need nothing");
        let mut sorted = open.clone();
        sorted.sort();
        assert_eq!(
            open, sorted,
            "research options must be in canonical (sorted) order"
        );
    }
}
