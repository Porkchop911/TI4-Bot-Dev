//! Technology: prerequisites and research (LRR 90).
//!
//! Ported from the oracle's `engine/technology.py`: `owned_colours`, `can_research`,
//! `researchable`, `research` and `grant`.

use std::collections::{BTreeMap, BTreeSet};

use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, Source, SourceSet};
use ti4_model::id::{PlanetId, PlayerId, SystemId, TechnologyId, UnitTypeId};
use ti4_model::state::GameState;

use crate::choice::{Choice, ChoiceOption, IllegalChoice, Observed, Table};
use crate::decision_context::{DecisionContext, DecisionSource};

/// The four research tracks. Unit upgrades have no colour (90.7b), which is why
/// `UNITUPGRADE` is deliberately absent.
pub const COLOURS: [&str; 4] = ["BIOTIC", "CYBERNETIC", "PROPULSION", "WARFARE"];

/// Plasma Scoring: "When 1 or more of your units use BOMBARDMENT or SPACE CANNON, 1 of those units
/// may roll 1 additional die." Read where those dice are rolled (reported 2026-09-23 as missing).
#[must_use]
pub fn plasma_scoring(state: &GameState, player: &PlayerId) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.technologies.contains(&TechnologyId::new("ps")))
}

/// Technologies in the authoritative current PoK/Codex deck.
///
/// The raw corpus deliberately contains original and replacement printings together.  The oracle
/// uses `techs_pok_c4` to select the active printing; treating every corpus record as researchable
/// offers obsolete Magen and X-89 variants as separate technologies.
///
/// `sources` adds the Thunder's Edge technologies (all faction cards) when that expansion is in play.
#[must_use]
pub fn active_aliases(content: &ContentStore, sources: SourceSet) -> BTreeSet<TechnologyId> {
    let mut active: BTreeSet<TechnologyId> = content
        .get(ContentType::Decks, "techs_pok_c4")
        .map(|deck| {
            deck.strings("cardIDs")
                .into_iter()
                .map(TechnologyId::new)
                .collect()
        })
        .unwrap_or_default();
    // Thunder's Edge adds only faction technologies (no generic card, no replaced printing), and
    // the PoK + Codex deck predates it. When the game uses the expansion its technologies are active.
    if sources.contains(Source::ThundersEdge) {
        active.extend(
            content
                .records(ContentType::Technologies)
                .iter()
                .filter(|record| record.source() == Some(Source::ThundersEdge))
                .filter_map(|record| record.text("alias"))
                .map(TechnologyId::new),
        );
    }
    active
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

/// Whether this player owns the Dark Energy Tap technology (alias `det`, POK).
#[must_use]
pub fn owns_det(state: &GameState, player: &PlayerId) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.technologies.contains(&TechnologyId::new("det")))
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
        loop {
            let candidates: Vec<PlanetId> = state
                .controlled_planets(player)
                .into_iter()
                .map(|(_, planet)| planet.clone())
                .filter(|planet| !state.exhausted_planets.contains(planet))
                .filter(|planet| {
                    !crate::planets::tech_specialties_now(state, content, sources, planet)
                        .is_empty()
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
            )
            .contextualized(DecisionContext::new(
                player.clone(),
                DecisionSource::Content("pa".to_owned()),
                "psychoarchaeology_exhaust_specialty",
                state.phase,
                state.round,
            ));
            let answer =
                table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
            if answer.is_decline() {
                break;
            }
            state.exhaust_planet(PlanetId::new(answer.id));
            crate::supply::gain_trade_goods_staged(state, player, 1, "psychoarchaeology");
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
            )
            .contextualized(DecisionContext::new(
                player.clone(),
                DecisionSource::Content("td".to_owned()),
                "transit_diodes_redeploy",
                state.phase,
                state.round,
            ));
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

    if has_technology_text(state, player, "cm") {
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
            let choice = Choice::new(player.clone(), "Chaos Mapping", options).contextualized(
                DecisionContext::new(
                    player.clone(),
                    DecisionSource::Content("cm".to_owned()),
                    "chaos_mapping_choose_system",
                    state.phase,
                    state.round,
                ),
            );
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
            )
            .contextualized(DecisionContext::new(
                player.clone(),
                DecisionSource::Content("pi".to_owned()),
                "predictive_intelligence_redistribute",
                state.phase,
                state.round,
            ));
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
        let mut options: Vec<ChoiceOption> = state
            .controlled_planets(player)
            .into_iter()
            .map(|(_, planet)| planet.clone())
            .filter(|planet| state.exhausted_planets.contains(planet))
            .filter(|planet| {
                !crate::planets::tech_specialties_now(state, content, sources, planet).is_empty()
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
            let choice = Choice::new(player.clone(), "Bio-Stims", options).contextualized(
                DecisionContext::new(
                    player.clone(),
                    DecisionSource::Content("bs".to_owned()),
                    "bio_stims_ready",
                    state.phase,
                    state.round,
                ),
            );
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

/// Sarween Tools and AI Development Algorithm, "when 1 or more of your units use PRODUCTION".
///
/// Sarween Tools is unconditional: it names no "may" and nothing to exhaust, so it always
/// contributes its flat 1 while owned. AI Development Algorithm is a genuine decision — exhausting
/// it here forecloses its other ability (ignoring a prerequisite) until it readies, so a player who
/// wants that instead must be asked, not defaulted into spending it.
///
/// The result lands in [`GameState::production_discount_remaining`] rather than being returned,
/// because the caller opens a [`crate::production::ProductionWindow`] immediately afterward and has
/// no window to hand a value to yet; the window reads the field back out in
/// [`crate::production::ProductionWindow::refresh`]. Writing rather than adding is deliberate: this
/// runs exactly once per use, so a value left over from an unrelated earlier use — one that never
/// spent it because its combined cost was already zero — must not leak forward into this one.
///
/// # Errors
/// [`IllegalChoice`] if a decider answers the exhaust prompt with something not offered.
pub fn production_used(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    table: &mut Table,
    player: &PlayerId,
) -> Result<(), IllegalChoice> {
    let sarween = TechnologyId::new("st");
    let mut discount: i32 = i32::from(
        state
            .player(player)
            .is_some_and(|seat| seat.technologies.contains(&sarween)),
    );

    let aida = TechnologyId::new("aida");
    let offered = state.player(player).is_some_and(|seat| {
        seat.technologies.contains(&aida) && !seat.exhausted_technologies.contains(&aida)
    });
    if offered {
        let owned_upgrades = state.player(player).map_or(0, |seat| {
            seat.technologies
                .iter()
                .filter(|owned| is_unit_upgrade(content, owned))
                .count()
        });
        if owned_upgrades > 0 {
            let choice = Choice::new(
                player.clone(),
                format!(
                    "AI Development Algorithm: exhaust to reduce this use's combined cost by {owned_upgrades}"
                ),
                vec![
                    ChoiceOption::labelled(
                        "exhaust",
                        "production_discount",
                        format!("exhaust to reduce the combined cost by {owned_upgrades}"),
                    )
                    .with("technology", "aida")
                    .with(
                        "discount_offered",
                        i64::try_from(owned_upgrades).unwrap_or(i64::MAX),
                    ),
                    ChoiceOption::decline(),
                ],
            )
            .contextualized(DecisionContext::new(
                player.clone(),
                DecisionSource::Content("aida".to_owned()),
                "exhaust_for_production_discount",
                state.phase,
                state.round,
            ));
            let answer =
                table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
            if !answer.is_decline() {
                if let Some(seat) = state.player_mut(player) {
                    seat.exhausted_technologies.insert(aida.clone());
                }
                discount += i32::try_from(owned_upgrades).unwrap_or(i32::MAX);
            }
        }
    }

    state.production_discount_remaining = discount;
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

/// Whether this faction has its own upgrade standing in for `alias`, making `alias` unresearchable
/// for them (90.11 and the faction sheets).
///
/// `baseUpgrade` on a faction technology names the generic unit upgrade it replaces.
#[must_use]
pub fn replaced_for_faction(content: &ContentStore, faction: &str, alias: &TechnologyId) -> bool {
    if faction.is_empty() {
        return false;
    }
    content
        .records(ContentType::Technologies)
        .iter()
        .any(|record| {
            record.text("faction") == Some(faction)
                && record.text("baseUpgrade") == Some(alias.as_str())
        })
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
    let mut found = BTreeMap::new();
    for (_, planet) in state.controlled_planets(player) {
        for specialty in crate::planets::tech_specialties_now(state, content, sources, planet) {
            let upper = specialty.to_ascii_uppercase();
            if let Some(colour) = COLOURS.iter().find(|c| **c == upper) {
                *found.entry(*colour).or_insert(0) += 1;
            }
        }
    }
    found
}

/// The research track a module names by track or by colour (`"green"` is BIOTIC).
fn track_named(name: &str) -> Option<&'static str> {
    match name.to_ascii_uppercase().as_str() {
        "BIOTIC" | "GREEN" => Some("BIOTIC"),
        "CYBERNETIC" | "YELLOW" => Some("CYBERNETIC"),
        "PROPULSION" | "BLUE" => Some("PROPULSION"),
        "WARFARE" | "RED" => Some("WARFARE"),
        _ => None,
    }
}

/// Every faction module's way to research `alias` while ignoring its prerequisites. A resolver
/// with a decision table must offer the waiver and its exact payment targets explicitly, then call
/// [`research_with_waiver`].
#[must_use]
pub(crate) fn research_waiver_offers(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    alias: &TechnologyId,
) -> Vec<(usize, crate::factions::hooks_strategy::ResearchWaiver)> {
    crate::factions::hooks_strategy::research_waiver_offers(state, content, player, alias)
}

/// The first faction module's way to research `alias` while ignoring its prerequisites.
#[must_use]
pub fn research_waiver_offer(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    alias: &TechnologyId,
) -> Option<crate::factions::hooks_strategy::ResearchWaiver> {
    research_waiver_offers(state, content, player, alias)
        .into_iter()
        .next()
        .map(|(_, waiver)| waiver)
}

/// Whether `alias` can only be researched now by taking a faction waiver rather than satisfying
/// prerequisites or using Inheritance Systems. A decision-table caller uses this to ask for the
/// optional waiver before it mutates any other research payment.
#[must_use]
pub(crate) fn faction_waiver_required(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    alias: &TechnologyId,
) -> bool {
    can_research(state, content, sources, player, alias)
        && !prerequisites_met(state, content, sources, player, alias)
        && !inheritance_systems_ready(state, content, sources, player)
        && !research_waiver_offers(state, content, player, alias).is_empty()
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
    // Ta Zern purged this card from every deck.
    if crate::factions::deepwrought::technology_purged(state, alias) {
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

    // A faction whose own unit upgrade REPLACES a generic one cannot research the generic. Sol's
    // Advanced Carrier II names Carrier II as its `baseUpgrade`, so Carrier II is not Sol's to
    // research -- it upgrades a carrier Sol does not have. The check above only stops other
    // factions taking `ac2`; nothing stopped Sol taking `cv2`, because the field that says so was
    // validated by the content validator and never read by the engine.
    if replaced_for_faction(content, seat.faction.as_str(), alias) {
        return false;
    }
    prerequisites_met(state, content, sources, player, alias)
        || inheritance_systems_ready(state, content, sources, player)
        || research_waiver_offer(state, content, player, alias).is_some()
}

/// Inheritance Systems (L1Z1X): "You may exhaust this card and spend 2 resources when you research
/// a technology; ignore all of that technology's prerequisites." Ready, and 2 resources affordable.
fn inheritance_systems_ready(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> bool {
    let card = TechnologyId::new("is");
    state.player(player).is_some_and(|seat| {
        seat.technologies.contains(&card) && !seat.exhausted_technologies.contains(&card)
    }) && crate::payment::affordable(
        state,
        content,
        sources,
        player,
        INHERITANCE_SYSTEMS_COST,
        crate::production::Spend::Resources,
    )
}

/// What Inheritance Systems charges to ignore a technology's prerequisites.
pub const INHERITANCE_SYSTEMS_COST: i64 = 2;

/// Whether this player meets `alias`'s prerequisites, with every standing waiver counted.
pub(crate) fn prerequisites_met(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    alias: &TechnologyId,
) -> bool {
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
    // Faction modules that stand in for a technology of some colour (Yin commander: green).
    for (name, count) in
        crate::factions::hooks_strategy::extra_prerequisite_colours(state, content, player)
    {
        if let Some(colour) = track_named(&name) {
            *holdings.entry(colour).or_insert(0) += count;
        }
    }
    // Research Team laws attach to a planet and are exhausted to ignore one prerequisite of their
    // colour. They add to the same waiver budget the faction abilities use, because both are
    // "ignore a prerequisite" and the requirement is checked once.
    for colour in COLOURS {
        waivable += crate::laws::research_team_waivers(state, player, colour);
    }
    // The Prophet's Tears: exhaust to ignore one prerequisite. Same budget as the faction waivers
    // and the Research Teams, for the same reason -- they are all "ignore a prerequisite" and the
    // requirement is checked once.
    waivable += crate::relics::prerequisite_waivers(state, player);
    // Publicize Weapon Schematics waives war sun prerequisites for everyone once anybody owns one.
    // A whole-requirement waiver, not a budget: the card says "ignore all prerequisites".
    if crate::laws::war_sun_prerequisites_waived(state, content, alias) {
        return true;
    }
    let joined = crate::synergy::joined(state, content, sources, player);
    crate::synergy::satisfies(&prerequisites(content, alias), &holdings, &joined, waivable)
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
    let active = active_aliases(content, sources);
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

/// Whether `player` has the printed text of technology `alias` to use: they own it, or they are
/// the Nekro Virus and a Valefar Assimilator token (X or Y) sits on it while another player still
/// owns it, so that card "gains that technology's text".
///
/// This is the gate for what a **faction technology's effect** does. It is *not* ownership: an
/// assimilated technology never enters `Player::technologies`, so it counts for no prerequisite,
/// objective, unit upgrade or "technologies you own" total. Generic technologies and prerequisites
/// keep reading the owned set.
#[must_use]
pub fn has_technology_text(state: &GameState, player: &PlayerId, alias: &str) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.technologies.contains(&TechnologyId::new(alias)))
        || crate::factions::nekro::assimilated_card(state, player, alias).is_some()
}

/// [`has_technology_text`] for a card that exhausts: the owner's copy is ready while unexhausted;
/// an assimilated text is ready while the Valefar Assimilator carrying it is (its exhaustion is
/// the Valefar card's, not the owner's).
#[must_use]
pub fn technology_text_ready(state: &GameState, player: &PlayerId, alias: &str) -> bool {
    let id = TechnologyId::new(alias);
    let Some(seat) = state.player(player) else {
        return false;
    };
    if seat.technologies.contains(&id) {
        return !seat.exhausted_technologies.contains(&id);
    }
    crate::factions::nekro::assimilated_card(state, player, alias).is_some_and(|card| {
        !seat
            .exhausted_technologies
            .contains(&TechnologyId::new(card))
    })
}

/// Exhaust the card that carries technology `alias`'s text for `player`: the technology itself, or
/// the Valefar Assimilator carrying it. `false`, changing nothing, when the player has neither.
pub fn exhaust_technology_text(state: &mut GameState, player: &PlayerId, alias: &str) -> bool {
    let id = TechnologyId::new(alias);
    let carrier = if state
        .player(player)
        .is_some_and(|seat| seat.technologies.contains(&id))
    {
        id
    } else if let Some(card) = crate::factions::nekro::assimilated_card(state, player, alias) {
        TechnologyId::new(card)
    } else {
        return false;
    };
    let Some(seat) = state.player_mut(player) else {
        return false;
    };
    seat.exhausted_technologies.insert(carrier);
    true
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

/// Gain a technology by any route other than research, and let a unit upgrade reach the units
/// already on the board (90.8). Research does this in `complete_research`.
pub fn gain(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    alias: &TechnologyId,
) {
    grant(state, player, alias);
    apply_unit_upgrades(state, content, sources, player);
}

/// Purge one of a player's technologies: the card leaves the game.
///
/// Returns `false`, changing nothing, when the player does not own it. A unit upgrade that is
/// purged takes its unit form with it: the player's units of the upgraded type on the board go back
/// to the form the card replaced, as [`apply_unit_upgrades`] put them there.
pub fn purge(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    alias: &TechnologyId,
) -> bool {
    if !state
        .player(player)
        .is_some_and(|seat| seat.technologies.contains(alias))
    {
        return false;
    }
    let downgrades: std::collections::BTreeMap<String, String> =
        ti4_content::units::catalogue(content, sources)
            .values()
            .filter(|kind| kind.required_technology() == Some(alias.as_str()))
            .filter_map(|kind| {
                kind.upgrades_from()
                    .map(|before| (kind.id().to_owned(), before.to_owned()))
            })
            .collect();
    if let Some(seat) = state.player_mut(player) {
        seat.technologies.remove(alias);
        seat.exhausted_technologies.remove(alias);
    }
    if downgrades.is_empty() {
        return true;
    }
    for board in state.board.values_mut() {
        let standing = board
            .units
            .iter_mut()
            .chain(board.planet_units.values_mut().flatten());
        for unit in standing {
            if unit.owner == *player
                && let Some(before) = downgrades.get(unit.type_id.as_str())
            {
                unit.type_id = ti4_model::id::UnitTypeId::new(before);
            }
        }
    }
    true
}

/// Replace this player's units on the board with the versions their upgrades unlock (90.8).
///
/// The physical card is placed *over* the unit on the faction sheet, so every one of that player's
/// units of that type becomes the upgraded version at once -- not only the ones built afterwards.
/// Doing it here, where the technology arrives, is what keeps the board and the sheet agreeing
/// without every stat lookup having to ask who owns the unit.
pub fn apply_unit_upgrades(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) {
    let Some(seat) = state.player(player) else {
        return;
    };
    let faction = seat.faction.to_string();
    let held: Vec<String> = seat
        .technologies
        .iter()
        .map(|tech| tech.as_str().to_owned())
        .collect();

    let catalogue = ti4_content::units::catalogue(content, sources);
    let mut swaps: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for kind in catalogue.values() {
        let base = kind.base_type();
        let chosen = ti4_content::units::unlocked_upgrade(content, sources, base, &faction, &held)
            .map_or_else(|| kind.id().to_owned(), |better| better.id().to_owned());
        // A module may name another form for this base type (Mentak Corsair's acquisition).
        let chosen = crate::factions::hooks_strategy::unit_form_override(
            state, content, sources, player, base, &chosen,
        )
        .map_or(chosen, |form| form.as_str().to_owned());
        if chosen != kind.id() {
            swaps.insert(kind.id().to_owned(), chosen);
        }
    }
    if swaps.is_empty() {
        return;
    }
    // The upgraded id is its own key in `swaps` only if it upgrades further, which no unit does --
    // so one pass is enough and cannot loop.
    for board in state.board.values_mut() {
        for unit in &mut board.units {
            if unit.owner == *player
                && let Some(better) = swaps.get(unit.type_id.as_str())
            {
                unit.type_id = ti4_model::id::UnitTypeId::new(better);
            }
        }
        for standing in board.planet_units.values_mut() {
            for unit in standing {
                if unit.owner == *player
                    && let Some(better) = swaps.get(unit.type_id.as_str())
                {
                    unit.type_id = ti4_model::id::UnitTypeId::new(better);
                }
            }
        }
    }
}

/// Research `alias` by taking a selected module waiver and paying its selected legal cost.
///
/// This is intentionally separate from [`research`]: a table-less caller cannot silently spend an
/// optional faction ability. The payment hook revalidates its target, and a failed payment restores
/// the state before returning `false`.
pub fn research_with_waiver(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    alias: &TechnologyId,
    waiver_index: usize,
    payment: &str,
) -> bool {
    if !can_research(state, content, sources, player, alias)
        || prerequisites_met(state, content, sources, player, alias)
    {
        return false;
    }
    let Some((_, waiver)) = research_waiver_offers(state, content, player, alias)
        .into_iter()
        .find(|(index, _)| *index == waiver_index)
    else {
        return false;
    };
    if !waiver
        .payments
        .iter()
        .any(|candidate| candidate.id == payment)
    {
        return false;
    }
    let before = state.clone();
    if !crate::factions::hooks_strategy::research_waiver_paid(
        state,
        content,
        player,
        alias,
        waiver_index,
        payment,
    ) {
        *state = before;
        return false;
    }
    complete_research(state, content, sources, player, alias);
    true
}

/// Research a technology, having satisfied its prerequisites or paying Inheritance Systems.
/// Table-less callers never silently take an optional faction waiver. `false` if it could not be.
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
    // Researchable only through Inheritance Systems: exhaust it and pay its 2 resources now, with
    // the cheapest plan (this path has no table to ask which planets; the plans are minimal).
    if !prerequisites_met(state, content, sources, player, alias) {
        if !inheritance_systems_ready(state, content, sources, player) {
            return false;
        }
        let Some(plan) = crate::payment::plans(
            state,
            content,
            sources,
            player,
            INHERITANCE_SYSTEMS_COST,
            crate::production::Spend::Resources,
        )
        .into_iter()
        .next() else {
            return false;
        };
        // `payment::apply` also spends commodities the Keleres agent turned into trade goods.
        if !crate::payment::apply(state, player, &plan) {
            return false;
        }
        if let Some(seat) = state.player_mut(player) {
            seat.exhausted_technologies.insert(TechnologyId::new("is"));
        }
    }
    complete_research(state, content, sources, player, alias);
    true
}

/// Apply every common consequence after a legal research and any required price have settled.
fn complete_research(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    alias: &TechnologyId,
) {
    // Propagation (Nekro): "When you would research a technology: Gain 3 command tokens instead."
    // Every research route ends here, so none can forget it. The technology is not gained and
    // nothing that fires on research fires; the game opens the token window at its next step.
    if crate::factions::nekro::propagation_replaces_research(state, player) {
        crate::factions::nekro::note_propagation(state, player);
        return;
    }
    grant(state, player, alias);
    // 90.8: the upgrade covers the unit on the faction sheet, so units already on the board
    // become the new version too -- not only the ones built after this.
    apply_unit_upgrades(state, content, sources, player);
    // Anti-Intellectual Revolution: "After a player researches a technology, that player must
    // destroy 1 of their non-fighter ships." Here rather than at the strategy card, because
    // researching happens by several routes and the law says "researches", not "uses Technology".
    crate::laws::revolution_tax(state, content, sources, player);
    // Research Agreement (Jol-Nar): "After the Jol-Nar player researches a technology that is not
    // a faction technology: Gain that technology. Then, return this card to the Jol-Nar player."
    // Not optional: the card has no "may", so the holder gains it whenever the condition holds.
    // The holder gains it rather than researching it, so nothing that fires on research fires for
    // them. A holder who already owns the technology has nothing to gain and keeps the card.
    if faction_of(content, alias).is_none()
        && let Some(holder) = crate::promissory::holder_of(state, "ra", player)
        && state
            .player(&holder)
            .is_some_and(|seat| !seat.technologies.contains(alias))
    {
        gain(state, content, sources, &holder, alias);
        let name = crate::promissory::faction_name(state, player);
        crate::promissory::give_back(state, &crate::promissory::note_id("ra", &name));
    }
}

#[cfg(test)]
mod tests {

    /// A seat that has researched nothing is offered only what needs nothing.
    ///
    /// Written after the operator reported non-Euclidean shielding offered to a Letnev seat - a faction
    /// with no prerequisite waiver of any kind - and Space Dock II offered to Jol-Nar without two yellow.
    /// Their table might have had planetary specialties, Research Team laws or a legitimate waiver in it;
    /// this pins the clean case, where there is one right answer: with no technologies, no specialties
    /// and no laws, a prerequisite is a wall and nothing gets offered through it.
    #[test]
    fn a_seat_with_nothing_researched_is_offered_only_prerequisite_free_technologies() {
        let content = ContentStore::embedded();
        let state = crate::fixtures::game(&["a"]);
        let player = ti4_model::id::PlayerId::new("a");
        let offered = researchable(&state, content, ti4_model::content_types::FULL, &player);
        assert!(
            !offered.is_empty(),
            "a fresh seat can still research something, or the fixture is not comparable to a game"
        );
        let offenders: Vec<String> = offered
            .iter()
            .filter(|alias| !prerequisites(content, alias).is_empty())
            .map(ToString::to_string)
            .collect();
        assert!(
            offenders.is_empty(),
            "offered to a seat with nothing, though they need prerequisites: {offenders:?}"
        );
    }

    /// 90.7/90.8: a unit upgrade replaces the unit, on the board and in what you build.
    ///
    /// This was researched and never applied. `UNLOCKED_BY` gated the war sun and nothing mapped
    /// any other upgrade to its unit, so Cruiser II was owned and every cruiser still moved 2,
    /// Fighter II never gained its ability, and PDS II never fired into an adjacent system because
    /// no `pds2` ever reached the board. The corpus had both halves of the mapping the whole time
    /// (`requiredTechId` and `upgradesFromUnitId`).
    #[test]
    fn a_unit_upgrade_replaces_the_units_already_on_the_board() {
        let content = ContentStore::embedded();
        let sources = ti4_model::content_types::DEFAULT;
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        let system = ti4_model::id::SystemId::new(crate::fixtures::plain_systems(1)[0].clone());
        state.board.entry(system.clone()).or_default();
        crate::fixtures::put(&mut state, &system, "cruiser", &player, 2);

        let printed_move = ti4_content::units::catalogue(content, sources)
            .get("cruiser")
            .map(ti4_content::units::UnitType::move_value)
            .expect("a cruiser has a move value");

        apply_unit_upgrades(&mut state, content, sources, &player);
        assert!(
            state
                .system_state(&system)
                .units_of(&player)
                .iter()
                .all(|unit| unit.type_id.as_str() == "cruiser"),
            "without the technology nothing changes"
        );

        grant(&mut state, &player, &TechnologyId::new("cr2"));
        apply_unit_upgrades(&mut state, content, sources, &player);
        assert!(
            state
                .system_state(&system)
                .units_of(&player)
                .iter()
                .all(|unit| unit.type_id.as_str() == "cruiser2"),
            "the card covers the unit on the sheet, so both cruisers are upgraded"
        );

        let upgraded_move = ti4_content::units::catalogue(content, sources)
            .get("cruiser2")
            .map(ti4_content::units::UnitType::move_value)
            .expect("Cruiser II has a move value");
        assert!(
            upgraded_move > printed_move,
            "and it is a real upgrade: move {printed_move} -> {upgraded_move}"
        );

        assert!(
            crate::production::buildable_for(&state, content, sources, &player)
                .contains(&"cruiser2".to_owned()),
            "and new ones are built upgraded too"
        );
    }
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

        state.player_mut(&player).expect("seated").breakthrough =
            Some(ti4_model::id::BreakthroughId::new("jolnarbt"));

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

    fn give_to(state: &mut GameState, who: &PlayerId, aliases: &[&str]) {
        for alias in aliases {
            state
                .player_mut(who)
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

    /// OBS-003e: `start_turn`/`end_turn`'s remaining reactive asks -- Chaos Mapping and
    /// Bio-Stims -- each carry a distinct, stable subtype naming the technology, rather than
    /// being told apart only by prompt text. Psychoarchaeology and Transit Diodes are the same
    /// shape (`technology.rs::start_turn`'s own `.contextualized` calls) and are covered by the
    /// package's file-wide suite passing unmodified rather than by a bespoke fixture here.
    #[test]
    fn obs003e_technology_reactive_asks_carry_typed_context() {
        let content = ContentStore::embedded();

        let mut state = game(&["a"]);
        give(&mut state, &["cm"]);
        let (system, planet) = crate::fixtures::a_placed_planet();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);
        state.player_mut(&player()).unwrap().trade_goods = 1;
        let (decider, seen) =
            crate::choice::Capturing::new(Box::new(crate::choice::Scripted::new([
                system.to_string(),
                "build|destroyer|1".to_owned(),
                "trade_good".to_owned(),
            ])));
        let mut table = Table::with_default(Box::new(decider));
        start_turn(&mut state, content, POK, None, &mut table, &player()).unwrap();
        let cm = seen.borrow()[0].context.clone().expect("typed context");
        assert_eq!(cm.source, DecisionSource::Content("cm".to_owned()));
        assert_eq!(cm.subtype, "chaos_mapping_choose_system");

        let mut state = game(&["a"]);
        give(&mut state, &["bs", "td"]);
        state
            .player_mut(&player())
            .unwrap()
            .exhausted_technologies
            .insert(TechnologyId::new("td"));
        let (decider, seen) =
            crate::choice::Capturing::new(Box::new(crate::choice::Scripted::new([
                "ready|technology|td".to_owned(),
            ])));
        let mut table = Table::with_default(Box::new(decider));
        end_turn(&mut state, content, POK, None, &mut table, &player()).unwrap();
        let bs = seen.borrow()[0].context.clone().expect("typed context");
        assert_eq!(bs.source, DecisionSource::Content("bs".to_owned()));
        assert_eq!(bs.subtype, "bio_stims_ready");
        assert_ne!(cm.subtype, bs.subtype);
    }

    /// Sarween Tools names no "may" and nothing to exhaust: it always contributes its flat 1,
    /// unconditionally, every time PRODUCTION is used.
    #[test]
    fn sarween_tools_contributes_automatically_with_nothing_to_ask() {
        let mut state = game(&["a"]);
        give(&mut state, &["st"]);
        let mut table =
            Table::with_default(Box::new(crate::choice::Scripted::new(Vec::<String>::new())));

        production_used(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            &player(),
        )
        .unwrap();

        assert_eq!(state.production_discount_remaining, 1);
        assert!(
            table.log.records.is_empty(),
            "an automatic effect asks nothing"
        );
    }

    /// AI Development Algorithm is a real choice: accepting exhausts the card and forecloses its
    /// other ability (ignoring a prerequisite) until it readies, so a decider must be asked rather
    /// than defaulted into spending it.
    #[test]
    fn ai_development_algorithm_asks_and_reduces_by_owned_unit_upgrades() {
        let mut state = game(&["a"]);
        give(&mut state, &["aida", "cr2", "dn2"]);
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([
            "exhaust".to_owned()
        ])));

        production_used(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            &player(),
        )
        .unwrap();

        assert_eq!(
            state.production_discount_remaining, 2,
            "two owned unit-upgrade technologies"
        );
        assert!(
            state
                .player(&player())
                .unwrap()
                .exhausted_technologies
                .contains(&TechnologyId::new("aida")),
            "accepting exhausts the card"
        );
    }

    #[test]
    fn declining_ai_development_algorithm_leaves_it_ready_and_grants_nothing() {
        let mut state = game(&["a"]);
        give(&mut state, &["aida", "cr2"]);
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([
            "decline".to_owned()
        ])));

        production_used(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            &player(),
        )
        .unwrap();

        assert_eq!(state.production_discount_remaining, 0);
        assert!(
            !state
                .player(&player())
                .unwrap()
                .exhausted_technologies
                .contains(&TechnologyId::new("aida")),
            "declining leaves it ready for its other ability"
        );
    }

    /// An already-exhausted AI Development Algorithm offers nothing: it cannot be exhausted twice
    /// in the same round, whichever ability spent it first.
    #[test]
    fn an_exhausted_ai_development_algorithm_is_not_offered_again() {
        let mut state = game(&["a"]);
        give(&mut state, &["aida", "cr2"]);
        state
            .player_mut(&player())
            .unwrap()
            .exhausted_technologies
            .insert(TechnologyId::new("aida"));
        let mut table =
            Table::with_default(Box::new(crate::choice::Scripted::new(Vec::<String>::new())));

        production_used(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            &player(),
        )
        .unwrap();

        assert_eq!(state.production_discount_remaining, 0);
        assert!(table.log.records.is_empty(), "nothing left to ask");
    }

    /// Both together: Sarween Tools' automatic 1 and AI Development Algorithm's asked-for amount
    /// combine into one bill, exactly as their card texts each independently claim.
    #[test]
    fn sarween_tools_and_ai_development_algorithm_combine() {
        let mut state = game(&["a"]);
        give(&mut state, &["st", "aida", "cr2"]);
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([
            "exhaust".to_owned()
        ])));

        production_used(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            &player(),
        )
        .unwrap();

        assert_eq!(state.production_discount_remaining, 2);
    }

    /// Called a second time, for a new and unrelated use of PRODUCTION, the field is overwritten
    /// rather than accumulated: an unused discount from a use whose combined cost never reached it
    /// must not leak into a later, separate use.
    #[test]
    fn a_later_use_overwrites_rather_than_accumulates() {
        let mut state = game(&["a"]);
        give(&mut state, &["st"]);
        state.production_discount_remaining = 7; // stale, as if left over from an earlier use
        let mut table =
            Table::with_default(Box::new(crate::choice::Scripted::new(Vec::<String>::new())));

        production_used(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            &player(),
        )
        .unwrap();

        assert_eq!(state.production_discount_remaining, 1);
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
        let active = active_aliases(content, POK);
        assert!(active.contains(&TechnologyId::new("md")));
        assert!(!active.contains(&TechnologyId::new("md_base")));
        assert!(!active.contains(&TechnologyId::new("md_c1")));

        let offered = researchable(&game(&["a"]), content, POK, &player());
        assert!(!offered.is_empty());
        assert!(!offered.contains(&TechnologyId::new("md_base")));
        assert!(!offered.contains(&TechnologyId::new("md_c1")));
    }

    #[test]
    fn thunders_edge_faction_techs_are_active_only_when_the_expansion_is_in_play() {
        let content = ContentStore::embedded();
        let pok = active_aliases(content, POK);
        let full = active_aliases(content, ti4_model::content_types::DEFAULT);
        for alias in ["proxima", "helios2", "hydrothermal", "nanomachines", "linkship2"] {
            assert!(!pok.contains(&TechnologyId::new(alias)), "{alias} under PoK");
            assert!(full.contains(&TechnologyId::new(alias)), "{alias} under TE");
        }
        // The PoK-only set is exactly the deck, and TE only adds to it.
        assert!(pok.is_subset(&full));
        assert!(pok.contains(&TechnologyId::new("md")) && !full.contains(&TechnologyId::new("md_base")));
        assert!(pok.len() < full.len());
    }

    #[test]
    fn a_thunders_edge_faction_can_research_its_own_faction_tech_and_no_one_else_can() {
        let content = ContentStore::embedded();
        let mut state = game(&["a", "b"]);
        state.player_mut(&player()).unwrap().faction = ti4_model::id::FactionId::new("bastion");
        give(&mut state, &["md"]);
        let proxima = TechnologyId::new("proxima");
        let full = ti4_model::content_types::DEFAULT;
        assert!(researchable(&state, content, full, &player()).contains(&proxima));
        assert!(!researchable(&state, content, POK, &player()).contains(&proxima));
        let other = PlayerId::new("b");
        give_to(&mut state, &other, &["md"]);
        assert!(!researchable(&state, content, full, &other).contains(&proxima));
    }

    #[test]
    fn learned_research_labels_use_the_printed_name() {
        assert_eq!(
            name(ContentStore::embedded(), &TechnologyId::new("sr")),
            "Sling Relay"
        );
    }

    /// Faction unit upgrades swap the faction's units already on the board, by the faction's own
    /// ids: Advanced Carrier II (Sol), Spec Ops II (Sol) and Super Dreadnought II (L1Z1X).
    #[test]
    fn faction_unit_upgrades_replace_the_units_already_on_the_board() {
        let content = ContentStore::embedded();
        let sources = ti4_model::content_types::DEFAULT;
        let (planet_system, planet) = crate::fixtures::a_placed_planet();
        let plain = ti4_model::id::SystemId::new(crate::fixtures::plain_systems(1)[0].clone());
        for (faction, tech, before, after, on_planet) in [
            ("sol", "ac2", "sol_carrier", "sol_carrier2", false),
            ("sol", "so2", "sol_infantry", "sol_infantry2", true),
            (
                "l1z1x",
                "sdn2",
                "l1z1x_dreadnought",
                "l1z1x_dreadnought2",
                false,
            ),
        ] {
            let mut state =
                crate::fixtures::seated_game(&[("a", faction), ("b", "hacan")], sources);
            let player = PlayerId::new("a");
            let system = if on_planet {
                planet_system.clone()
            } else {
                plain.clone()
            };
            state.board.entry(system.clone()).or_default();
            let place = |state: &mut GameState| {
                if on_planet {
                    crate::fixtures::put_on_planet(state, &system, &planet, before, &player, 2);
                } else {
                    crate::fixtures::put(state, &system, before, &player, 2);
                }
            };
            let ids = |state: &GameState| -> Vec<String> {
                let board = state.system_state(&system);
                let units = if on_planet {
                    board.on_planet(&planet).to_vec()
                } else {
                    board.units_of(&player).into_iter().cloned().collect()
                };
                units
                    .iter()
                    .filter(|unit| unit.owner == player)
                    .map(|unit| unit.type_id.to_string())
                    .filter(|id| id == before || id == after)
                    .collect()
            };
            let standing = ids(&state).len();
            place(&mut state);
            assert_eq!(
                ids(&state),
                vec![before.to_owned(); standing + 2],
                "{faction} starts on {before}"
            );

            grant(&mut state, &player, &TechnologyId::new(tech));
            apply_unit_upgrades(&mut state, content, sources, &player);

            assert_eq!(
                ids(&state),
                vec![after.to_owned(); standing + 2],
                "{faction} {tech}"
            );
        }
    }

    /// Every faction unit upgrade in the corpus -- not a hand-picked few -- replaces that faction's
    /// units already on the board once `apply_unit_upgrades` runs.
    #[test]
    fn every_faction_unit_upgrade_replaces_the_units_already_on_the_board() {
        let content = ContentStore::embedded();
        let sources = ti4_model::content_types::FULL;
        let (planet_system, planet) = crate::fixtures::a_placed_planet();
        let plain = ti4_model::id::SystemId::new(crate::fixtures::plain_systems(1)[0].clone());
        let mut failures = Vec::new();
        let mut checked = 0;
        for after in ti4_content::units::catalogue(content, sources).values() {
            let (Some(faction), Some(tech), Some(before)) = (
                after.faction(),
                after.required_technology(),
                after.upgrades_from(),
            ) else {
                continue;
            };
            if content.get(ti4_model::content_types::ContentType::Factions, faction).is_none() {
                continue; // homebrew faction with no faction record (Ghemina)
            }
            checked += 1;
            let on_planet = ti4_content::units::unit_type(content, before, sources)
                .is_some_and(|unit| !unit.is_ship() && !unit.is_space_only_structure());
            let mut state =
                crate::fixtures::seated_game(&[("a", faction), ("b", "hacan")], sources);
            let player = PlayerId::new("a");
            let system = if on_planet { &planet_system } else { &plain };
            state.board.entry(system.clone()).or_default();
            if on_planet {
                crate::fixtures::put_on_planet(&mut state, system, &planet, before, &player, 2);
            } else {
                crate::fixtures::put(&mut state, system, before, &player, 2);
            }
            grant(&mut state, &player, &TechnologyId::new(tech));
            apply_unit_upgrades(&mut state, content, sources, &player);
            let board = state.system_state(system);
            let ids: Vec<String> = if on_planet {
                board.on_planet(&planet).to_vec()
            } else {
                board.units_of(&player).into_iter().cloned().collect()
            }
            .iter()
            .filter(|unit| unit.owner == player)
            .map(|unit| unit.type_id.to_string())
            .collect();
            if !ids.iter().any(|id| id == after.id()) || ids.iter().any(|id| id == before) {
                failures.push(format!("{faction} {tech}: {before} -> {} left {ids:?}", after.id()));
            }
        }
        assert!(checked >= 17, "only {checked} faction upgrades found");
        assert!(failures.is_empty(), "{failures:#?}");
    }

    /// Gaining (not researching) an upgrade converts the board too, and so does an Entropic Scar grant.
    #[test]
    fn a_gained_unit_upgrade_replaces_units_on_the_board() {
        let content = ContentStore::embedded();
        let sources = ti4_model::content_types::FULL;
        let plain = ti4_model::id::SystemId::new(crate::fixtures::plain_systems(1)[0].clone());
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], sources);
        let player = PlayerId::new("a");
        state.board.entry(plain.clone()).or_default();
        crate::fixtures::put(&mut state, &plain, "sol_carrier", &player, 2);
        gain(&mut state, content, sources, &player, &TechnologyId::new("ac2"));
        let ids: Vec<String> = state
            .system_state(&plain)
            .units_of(&player)
            .iter()
            .map(|unit| unit.type_id.to_string())
            .filter(|id| id.starts_with("sol_carrier"))
            .collect();
        assert!(ids.len() >= 2 && ids.iter().all(|id| id == "sol_carrier2"), "{ids:?}");
    }

    #[test]
    fn a_research_agreement_hands_the_holder_the_same_technology_and_goes_home() {
        // "After the Jol-Nar player researches a technology that is not a faction technology: Gain
        // that technology. Then, return this card to the Jol-Nar player." Priced for trading, it
        // never did anything.
        let content = ContentStore::embedded();
        let jolnar = PlayerId::new("a");
        let holder = PlayerId::new("b");
        let mut state = game(&["a", "b"]);
        state.player_mut(&jolnar).unwrap().faction = ti4_model::id::FactionId::new("jolnar");
        state.player_mut(&holder).unwrap().faction = ti4_model::id::FactionId::new("hacan");
        let note = crate::promissory::note_id("ra", "jolnar");
        state.promissory_notes.insert(note.clone(), holder.clone());
        let technology = researchable(&state, content, POK, &jolnar)
            .into_iter()
            .find(|alias| {
                faction_of(content, alias).is_none()
                    && !state.player(&holder).unwrap().technologies.contains(alias)
            })
            .expect("a generic technology Jol-Nar can research now");

        assert!(research(&mut state, content, POK, &jolnar, &technology));

        assert!(
            state
                .player(&holder)
                .unwrap()
                .technologies
                .contains(&technology),
            "the holder gained the same technology"
        );
        assert_eq!(
            state.promissory_notes.get(&note),
            Some(&jolnar),
            "the card went home"
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

    /// Inheritance Systems: exhaust it and pay 2 resources to ignore every prerequisite. Once
    /// exhausted, prerequisites are a wall again until the status phase readies it.
    #[test]
    fn inheritance_systems_buys_past_the_prerequisites_once() {
        let content = ContentStore::embedded();
        let mut state = game(&["a"]);
        let seat = state.player_mut(&player()).unwrap();
        seat.faction = ti4_model::id::FactionId::new("l1z1x");
        seat.technologies.insert(TechnologyId::new("is"));
        seat.trade_goods = 2;
        let (war_sun, dreadnought) = (TechnologyId::new("ws"), TechnologyId::new("dn2"));
        assert!(!prerequisites(content, &war_sun).is_empty());
        assert!(can_research(&state, content, POK, &player(), &war_sun));
        assert!(research(&mut state, content, POK, &player(), &war_sun));
        let seat = state.player(&player()).unwrap();
        assert!(seat.technologies.contains(&war_sun));
        assert!(
            seat.exhausted_technologies
                .contains(&TechnologyId::new("is"))
        );
        assert_eq!(seat.trade_goods, 0, "the 2 resources were paid");
        assert!(
            !can_research(&state, content, POK, &player(), &dreadnought),
            "exhausted, it waives nothing more"
        );

        // Without the 2 resources it cannot be used at all.
        let mut broke = game(&["a"]);
        let seat = broke.player_mut(&player()).unwrap();
        seat.faction = ti4_model::id::FactionId::new("l1z1x");
        seat.technologies.insert(TechnologyId::new("is"));
        seat.trade_goods = 1;
        assert!(!can_research(&broke, content, POK, &player(), &war_sun));
    }

    /// A research facility on a planet without a specialty gives it one (LRR 35.8), and that
    /// specialty stands in for a prerequisite like a printed one.
    #[test]
    fn an_attached_specialty_stands_in_for_a_prerequisite() {
        let mut state = game(&["a"]);
        let target = TechnologyId::new("gd"); // one propulsion
        let (id, record) = ti4_content::galaxy::all_planets(ContentStore::embedded(), POK)
            .into_iter()
            .find(|(_, record)| {
                record.tech_specialties().is_empty() && !record.is_placed_during_play()
            })
            .expect("a planet without a specialty");
        let planet = ti4_model::id::PlanetId::new(id);
        let system = ti4_model::id::SystemId::new(record.system_id().unwrap_or("18"));
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        assert!(!can_research(
            &state,
            ContentStore::embedded(),
            POK,
            &player(),
            &target
        ));

        state
            .planet_attachments
            .insert(planet, vec!["propulsion".to_owned()]);
        assert!(
            can_research(&state, ContentStore::embedded(), POK, &player(), &target),
            "the facility's specialty covers the prerequisite"
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

#[cfg(test)]
mod replaced_upgrades {
    use super::*;

    /// Sol cannot research Carrier II: its Advanced Carrier replaces the carrier, and Advanced
    /// Carrier II names `cv2` as its `baseUpgrade`.
    ///
    /// The field was validated by the content validator and read by nothing, so the generic
    /// upgrade was offered to every faction that replaces it -- an illegal option in the list a
    /// learned policy chooses from.
    #[test]
    fn a_faction_that_replaces_a_unit_cannot_research_the_generic_upgrade() {
        let content = ContentStore::embedded();
        let carrier_two = TechnologyId::new("cv2");
        let advanced = TechnologyId::new("ac2");

        assert!(
            replaced_for_faction(content, "sol", &carrier_two),
            "Sol's Advanced Carrier II replaces Carrier II"
        );
        assert!(
            !replaced_for_faction(content, "sol", &advanced),
            "its own upgrade is still Sol's to research"
        );
        assert!(
            !replaced_for_faction(content, "hacan", &carrier_two),
            "a faction with no carrier replacement researches the generic one"
        );
        assert!(
            !replaced_for_faction(content, "", &carrier_two),
            "an unseated faction blocks nothing"
        );
    }
}

#[cfg(test)]
mod bf_f3_tests {
    use super::*;
    use crate::factions::hooks_strategy::{
        ResearchWaiver, ResearchWaiverPayment, StrategyHooks, with_test_hooks,
    };
    use crate::fixtures::game;
    use ti4_model::content_types::POK;
    use ti4_model::id::UnitTypeId;

    fn a() -> PlayerId {
        PlayerId::new("a")
    }

    #[test]
    fn an_extra_prerequisite_colour_stands_in_for_an_owned_technology() {
        let state = game(&["a"]);
        let content = ContentStore::embedded();
        let gd = TechnologyId::new("gd"); // one PROPULSION (blue) prerequisite
        assert!(!can_research(&state, content, POK, &a(), &gd));
        let blue = StrategyHooks {
            extra_prerequisite_colours: Some(|_, _, _| vec![("blue".to_owned(), 1)]),
            ..StrategyHooks::NONE
        };
        with_test_hooks(blue, || {
            assert!(can_research(&state, content, POK, &a(), &gd));
            assert!(researchable(&state, content, POK, &a()).contains(&gd));
        });
        let wrong = StrategyHooks {
            extra_prerequisite_colours: Some(|_, _, _| {
                vec![("green".to_owned(), 1), ("mauve".to_owned(), 9)]
            }),
            ..StrategyHooks::NONE
        };
        with_test_hooks(wrong, || {
            assert!(!can_research(&state, content, POK, &a(), &gd));
        });
    }

    #[test]
    fn a_module_waiver_requires_a_selected_payment_and_is_paid_once() {
        let content = ContentStore::embedded();
        let gd = TechnologyId::new("gd");
        let waiver = StrategyHooks {
            research_waiver_offer: Some(|_, _, _, tech| {
                (tech.as_str() == "gd").then(|| ResearchWaiver {
                    id: "w".to_owned(),
                    label: "ignore prerequisites".to_owned(),
                    payments: vec![ResearchWaiverPayment {
                        id: "infantry:a".to_owned(),
                        label: "return infantry a".to_owned(),
                    }],
                })
            }),
            research_waiver_paid: Some(|state, _, player, tech, payment| {
                if payment != "infantry:a" {
                    return false;
                }
                state
                    .faction_marks
                    .insert(format!("paid:{player}:{tech}"), String::new());
                true
            }),
            ..StrategyHooks::NONE
        };
        let mut state = game(&["a"]);
        assert!(!research(&mut state, content, POK, &a(), &gd), "no hook");
        assert!(state.faction_marks.is_empty());
        with_test_hooks(waiver, || {
            assert!(research_waiver_offer(&state, content, &a(), &gd).is_some());
            assert!(
                research_waiver_offer(&state, content, &a(), &TechnologyId::new("ws")).is_none()
            );
            // The waiver index is the one `research_waiver_offers` reports for this module, not a
            // literal position: with Yin's Brother Omar registered, this test table is no longer
            // the first table that has a `research_waiver_offer`.
            let index = research_waiver_offers(&state, content, &a(), &gd)
                .into_iter()
                .find(|(_, offered)| offered.id == "w")
                .map(|(index, _)| index)
                .expect("the test waiver is offered");
            let before = state.clone();
            assert!(!research_with_waiver(
                &mut state,
                content,
                POK,
                &a(),
                &gd,
                index,
                "not-a-payment"
            ));
            assert_eq!(state, before, "an illegal payment is atomic");
            assert!(
                !research(&mut state, content, POK, &a(), &gd),
                "table-less research does not take an optional waiver"
            );
            assert!(research_with_waiver(
                &mut state,
                content,
                POK,
                &a(),
                &gd,
                index,
                "infantry:a"
            ));
        });
        assert!(state.player(&a()).unwrap().technologies.contains(&gd));
        assert_eq!(state.faction_marks.len(), 1);
        assert!(state.faction_marks.contains_key("paid:a:gd"));
    }

    #[test]
    fn a_unit_form_override_replaces_units_on_the_board_and_the_build_list() {
        let content = ContentStore::embedded();
        let mut state = game(&["a", "b"]);
        let (system, _) = crate::fixtures::a_placed_planet();
        crate::fixtures::put(&mut state, &system, "cruiser", &a(), 1);
        assert!(
            crate::production::buildable_for(&state, content, POK, &a())
                .contains(&"cruiser".to_owned())
        );
        let corsair = StrategyHooks {
            unit_form_override: Some(|_, _, _, player, base, _| {
                (player.as_str() == "a" && base == "cruiser")
                    .then(|| UnitTypeId::new("mentak_cruiser3"))
            }),
            ..StrategyHooks::NONE
        };
        with_test_hooks(corsair, || {
            let list = crate::production::buildable_for(&state, content, POK, &a());
            assert!(list.contains(&"mentak_cruiser3".to_owned()));
            assert!(!list.contains(&"cruiser".to_owned()));
            let other = crate::production::buildable_for(&state, content, POK, &PlayerId::new("b"));
            assert!(other.contains(&"cruiser".to_owned()));
            apply_unit_upgrades(&mut state, content, POK, &a());
        });
        assert_eq!(
            state.system_state(&system).units[0].type_id.as_str(),
            "mentak_cruiser3"
        );
    }
}
