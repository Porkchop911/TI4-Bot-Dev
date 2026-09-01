//! Agenda effects (LRR 8, M06-014).
//!
//! Ported from the oracle's `engine/agenda.py` effect handlers.
//!
//! A first tranche. An agenda with no registered handler resolves its vote and announces the
//! effect unresolved — the same design every other registry here uses, and the same one the
//! oracle uses via `AGENDA_EFFECT_UNRESOLVED`.

use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;

use crate::objectives::VICTORY_TARGET;
use crate::vote::{AGAINST, Ballot, FOR};

/// What an effect did, for the caller to announce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// The effect ran.
    Resolved { agenda: String },
    /// No handler is registered for this agenda.
    Unresolved { agenda: String },
    /// The handler ran, but this outcome's half needs machinery the engine does not have.
    ///
    /// Distinct from [`Effect::Unresolved`] on purpose: an unregistered agenda is a gap in
    /// coverage, while this is a known half that was deliberately not applied. Reporting the
    /// second as the first would hide a card that is otherwise complete.
    Deferred { agenda: String, what: String },
}

/// Agendas this engine can resolve.
#[must_use]
pub fn registered_aliases() -> Vec<&'static str> {
    vec![
        "abolishment",
        "arms_reduction",
        "classified",
        "censure",
        "artifact",
        "constitution",
        "conscription",
        "conventions",
        "core_mining",
        "crown_of_emphidia",
        "defense_act",
        "execution",
        "demilitarized_zone",
        "disarmament",
        "holy_planet_of_ixth",
        "miscount",
        "redistribution",
        "plowshares",
        "regulations",
        "representative_government",
        "revolution",
        "sanctions",
        "shard_of_the_throne",
        "secret",
        "schematics",
        "shared_research",
        "travel_ban",
        "wormhole_research",
        "wormhole_recon",
        "economic_equality",
        "incentive",
        "mutiny",
        "unconventional",
        "seed_empire",
        // Standing rules enforced in `laws`; the arm exists so the agenda is available at all.
        "rep_govt",
        "rt_biotic",
        "rt_cybernetic",
        "rt_propulsion",
        "rt_warfare",
        "senate_sanctuary",
        "terraforming_initiative",
        // More availability-only arms: each law's standing rule is enforced by the subsystem
        // that owns it (the driver for the agenda-phase laws, `laws` for the standing ones),
        // so the vote needs no state change here. Checks' Against half ("only 3 planets ready
        // up") is a standing restriction the driver applies at the end of the agenda phase.
        "committee",
        "arbiter",
        "minister_commerce",
        "minister_industry",
        "minister_peace",
        "minister_sciences",
        "minister_war",
        "prophecy",
        "crown_of_thalnos",
        "checks",
        "covert",
        "crisis",
        // Ministries and standing rules, enforced in `laws`.
        "articles_war",
        "minister_exploration",
        "minister_policy",
        // Immediate effects, resolved here.
        "cladenstine",
        "grant_reallocation",
        "nexus",
        "warrant",
        "minister_antiquities",
        "rearmament",
        "standardization",
    ]
}

/// Agendas in the corpus that have no registered effect.
#[must_use]
pub fn unimplemented(content: &ContentStore, sources: SourceSet) -> Vec<String> {
    let known = registered_aliases();
    content
        .from_sources(ContentType::Agendas, sources)
        .filter_map(|record| record.text("alias"))
        .filter(|alias| !known.contains(alias))
        .map(str::to_owned)
        .collect()
}

/// 98.4a caps a player at the target; a loss cannot take them below zero.
fn adjust_victory_points(state: &mut GameState, player: &PlayerId, delta: i32) {
    if let Some(seat) = state.player_mut(player) {
        seat.victory_points = (seat.victory_points + delta).clamp(0, VICTORY_TARGET);
    }
}

fn everyone(state: &GameState) -> Vec<PlayerId> {
    state.seating_order.clone()
}

/// Everything of one base type this player has on planets, as (system, planet, index).
fn structures_of(
    state: &GameState,
    content: &ti4_content::ContentStore,
    sources: ti4_model::content_types::SourceSet,
    player: &PlayerId,
    base_type: &str,
) -> Vec<(ti4_model::id::SystemId, ti4_model::id::PlanetId, usize)> {
    let types = ti4_content::units::catalogue(content, sources);
    let mut found = Vec::new();
    for (system, board) in &state.board {
        for (planet, units) in &board.planet_units {
            for (index, unit) in units.iter().enumerate() {
                if &unit.owner == player
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(|kind| kind.base_type() == base_type)
                {
                    found.push((system.clone(), planet.clone(), index));
                }
            }
        }
    }
    found
}

/// Ask a player which of their structures to give up.
///
/// One is not a decision, and none is not a question — asking either would put a line in the
/// decision log that no player ever chose.
fn choose_structure(
    ctx: &mut crate::choice::Resolving<'_>,
    state: &GameState,
    player: &PlayerId,
    held: &[(ti4_model::id::SystemId, ti4_model::id::PlanetId, usize)],
) -> Option<(ti4_model::id::SystemId, ti4_model::id::PlanetId, usize)> {
    match held {
        [] => None,
        [only] => Some(only.clone()),
        many => {
            let choice = crate::choice::Choice::new(
                player.clone(),
                "destroy one of your structures",
                many.iter()
                    .enumerate()
                    .map(|(index, (_, planet, _))| {
                        crate::choice::ChoiceOption::labelled(
                            index.to_string(),
                            "scuttle",
                            format!("destroy the one on {planet}"),
                        )
                    })
                    .collect(),
            );
            let answer = ctx.ask_seeing(state, &choice).ok()?;
            let index: usize = answer.id.parse().ok()?;
            many.get(index).cloned()
        }
    }
}

/// Discard a player's whole hand of action cards.
///
/// The cards leave the hand and are gone. This engine keeps no discard pile — nothing reads one
/// yet — so a card that returns them (The Codex) will need one before it can be written.
fn discard_hand(state: &mut GameState, player: &PlayerId) -> usize {
    let Some(seat) = state.player_mut(player) else {
        return 0;
    };
    std::mem::take(&mut seat.action_cards).len()
}

/// The system a planet sits in, according to the board.
fn system_of(state: &GameState, planet: &str) -> Option<ti4_model::id::SystemId> {
    let planet = ti4_model::id::PlanetId::new(planet);
    state
        .board
        .iter()
        .find(|(_, board)| {
            board.planet_units.contains_key(&planet) || board.planet_control.contains_key(&planet)
        })
        .map(|(id, _)| id.clone())
}

/// Who controls a planet, if anybody.
fn controller_of(state: &GameState, planet: &str) -> Option<PlayerId> {
    let planet = ti4_model::id::PlanetId::new(planet);
    state
        .board
        .values()
        .find_map(|board| board.planet_control.get(&planet).cloned())
}

/// Remove units from a planet, keeping those the filter rejects, and report how many died.
fn clear_planet(
    state: &mut GameState,
    content: &ti4_content::ContentStore,
    sources: ti4_model::content_types::SourceSet,
    planet: &str,
    doomed: impl Fn(&str) -> bool,
    limit: Option<usize>,
) -> usize {
    let Some(system) = system_of(state, planet) else {
        return 0;
    };
    let types = ti4_content::units::catalogue(content, sources);
    let planet = ti4_model::id::PlanetId::new(planet);
    let Some(units) = state.system_mut(&system).planet_units.get_mut(&planet) else {
        return 0;
    };
    let mut destroyed = 0;
    units.retain(|unit| {
        if limit.is_some_and(|cap| destroyed >= cap) {
            return true;
        }
        let hit = types
            .get(unit.type_id.as_str())
            .is_some_and(|kind| doomed(kind.base_type()));
        if hit {
            destroyed += 1;
        }
        !hit
    });
    destroyed
}

/// Reduce a player to `keep` ships of one base type, destroying the rest.
fn cull_ships(
    state: &mut GameState,
    content: &ti4_content::ContentStore,
    sources: ti4_model::content_types::SourceSet,
    player: &PlayerId,
    base_type: &str,
    keep: usize,
) -> usize {
    let types = ti4_content::units::catalogue(content, sources);
    let matches = |unit: &ti4_model::units::Unit| {
        &unit.owner == player
            && types
                .get(unit.type_id.as_str())
                .is_some_and(|kind| kind.base_type() == base_type)
    };
    let held: usize = state
        .board
        .values()
        .map(|board| board.units.iter().filter(|unit| matches(unit)).count())
        .sum();
    let mut over = held.saturating_sub(keep);
    let destroyed = over;
    // Board order, which is stable: the *rule* does not say which ships go, so the choice must
    // at least be reproducible rather than depending on map iteration.
    for board in state.board.values_mut() {
        if over == 0 {
            break;
        }
        board.units.retain(|unit| {
            if over > 0 && matches(unit) {
                over -= 1;
                return false;
            }
            true
        });
    }
    destroyed
}

/// Place one infantry for this player on a planet.
fn place_infantry(
    state: &mut GameState,
    content: &ti4_content::ContentStore,
    sources: ti4_model::content_types::SourceSet,
    player: &PlayerId,
    system: &ti4_model::id::SystemId,
    planet: &ti4_model::id::PlanetId,
) {
    let faction = state
        .player(player)
        .map(|seat| seat.faction.to_string())
        .unwrap_or_default();
    let generic = ti4_content::units::catalogue(content, sources)
        .get("infantry")
        .map(|unit| unit.id().to_owned());
    let Some(id) = ti4_content::units::faction_unit(content, &faction, "infantry", sources)
        .map(|unit| unit.id().to_owned())
        .or(generic)
    else {
        return;
    };
    let type_id = ti4_model::id::UnitTypeId::new(id);
    if crate::supply::allowed(state, content, sources, player, &type_id, 1) == 0 {
        return; // 31.4, through the one door
    }
    state
        .system_mut(system)
        .planet_units
        .entry(planet.clone())
        .or_default()
        .push(ti4_model::units::Unit::new(type_id, player.clone()));
}

/// Whether this player owns a technology whose name mentions a war sun.
fn owns_a_war_sun_technology(
    state: &GameState,
    content: &ti4_content::ContentStore,
    player: &PlayerId,
) -> bool {
    state.player(player).is_some_and(|seat| {
        seat.technologies.iter().any(|alias| {
            content
                .get(
                    ti4_model::content_types::ContentType::Technologies,
                    alias.as_str(),
                )
                .and_then(|record| record.text("name"))
                .is_some_and(|name| name.to_ascii_lowercase().contains("war sun"))
        })
    })
}

/// Ask the speaker which of several tied players an agenda names (8.18).
fn ask_the_speaker(
    state: &GameState,
    ctx: &mut crate::choice::Resolving<'_>,
    tied: &[PlayerId],
) -> Option<PlayerId> {
    let choice = crate::choice::Choice::new(
        state.speaker.clone(),
        "which tied player does the agenda name",
        tied.iter()
            .map(|player| {
                crate::choice::ChoiceOption::labelled(
                    player.to_string(),
                    "elect",
                    player.to_string(),
                )
            })
            .collect(),
    );
    ctx.ask_seeing(state, &choice)
        .ok()
        .map(|answer| PlayerId::new(answer.id))
}

/// Resolve one agenda's effect.
///
/// Ties are broken by asking the speaker (8.18), through a default table.
pub fn resolve(
    state: &mut GameState,
    content: &ti4_content::ContentStore,
    agenda: &str,
    outcome: &str,
    ballot: &Ballot,
) -> Effect {
    let mut dice = crate::dice::Dice::new();
    let mut rng = crate::rng::GameRng::new(0);
    let mut table = crate::choice::Table::new();
    let mut ctx = crate::choice::Resolving {
        content,
        sources: ti4_model::content_types::POK,
        dice: &mut dice,
        rng: &mut rng,
        table: &mut table,
        timing: None,
    };
    resolve_with(state, &mut ctx, None, agenda, outcome, ballot)
}

/// Resolve one agenda's effect with the game's own dice, table and map.
///
/// Several cards roll, ask, or read the shape of the board. Given none of those they cannot be
/// written at all, and given borrowed ones from nowhere they would roll off a stream no seed
/// covers — so a driver that has them must pass its own.
#[allow(
    clippy::too_many_lines,
    clippy::match_same_arms,
    reason = "one arm per agenda: the list is the point, and splitting it hides the set; the empty arms are the availability-only laws documented beside them"
)]
pub fn resolve_with(
    state: &mut GameState,
    ctx: &mut crate::choice::Resolving<'_>,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    agenda: &str,
    outcome: &str,
    ballot: &Ballot,
) -> Effect {
    // Every effect that touches units needs the unit catalogue, and the scope it is read under
    // decides what a "dreadnought" is. PoK, as everywhere else in this engine.
    let (content, sources) = (ctx.content, ctx.sources);
    match agenda {
        "secret" => {
            // Archived Secret: the elected player draws a secret objective.
            if state.player(&PlayerId::new(outcome)).is_some() {
                let player = PlayerId::new(outcome);
                let _ = crate::secrets::draw(state, content, ctx.table, &player);
            }
        }
        "censure" | "shard_of_the_throne" | "crown_of_emphidia" => {
            // All three pay the elected player a point when the card arrives. Only Emphidia's
            // transfer rule has anywhere to hook (a planet changing hands); the Shard's needs
            // "win a combat against the owner", which combat does not report.
            if state.player(&PlayerId::new(outcome)).is_some() {
                adjust_victory_points(state, &PlayerId::new(outcome), 1);
            }
        }
        "classified" => {
            // Classified Document Leaks: the elected secret becomes public. A law by type, but
            // nothing standing survives it — the card is spent the instant the objective moves.
            //
            // Whoever already scored it keeps their point (61.8 stops anyone scoring the same
            // objective twice); everyone else may now score it from the public area.
            let alias = ti4_model::id::ObjectiveId::new(outcome);
            let is_secret = content
                .get(
                    ti4_model::content_types::ContentType::SecretObjectives,
                    outcome,
                )
                .is_some();
            if is_secret && !state.revealed_objectives.contains(&alias) {
                state.revealed_objectives.push(alias);
            }
        }
        "execution" => {
            // Public Execution: the elected player loses their hand and, if they are speaker,
            // the chair. 8.16 bars them from the rest of this agenda phase, which this path
            // cannot express — the vote session owns that.
            let player = PlayerId::new(outcome);
            if state.player(&player).is_none() {
                return Effect::Resolved {
                    agenda: agenda.to_owned(),
                };
            }
            discard_hand(state, &player);
            if state.speaker == player {
                // The card's own note says left means clockwise.
                let order = state.clockwise_from(&player);
                if let Some(next) = order.get(1) {
                    state.speaker = next.clone();
                }
            }
            return Effect::Deferred {
                agenda: agenda.to_owned(),
                what: "8.16 bars the elected player from voting, which the session owns".to_owned(),
            };
        }
        "redistribution" => {
            // Colonial Redistribution: wipe the elected planet, then hand it to whoever is
            // furthest behind. Several may be level, and the controller chooses between them.
            let controller = controller_of(state, outcome);
            let system = system_of(state, outcome);
            clear_planet(state, content, sources, outcome, |_| true, None);
            let (Some(controller), Some(system)) = (controller, system) else {
                return Effect::Resolved {
                    agenda: agenda.to_owned(),
                };
            };
            let fewest = state
                .players
                .iter()
                .map(|seat| seat.victory_points)
                .min()
                .unwrap_or(0);
            let trailing: Vec<PlayerId> = state
                .players
                .iter()
                .filter(|seat| seat.victory_points == fewest)
                .map(|seat| seat.id.clone())
                .collect();
            let chosen = match trailing.as_slice() {
                [] => None,
                [only] => Some(only.clone()),
                many => {
                    let choice = crate::choice::Choice::new(
                        controller,
                        "which of the trailing players may settle the planet",
                        many.iter()
                            .map(|player| {
                                crate::choice::ChoiceOption::labelled(
                                    player.to_string(),
                                    "elect",
                                    player.to_string(),
                                )
                            })
                            .collect(),
                    );
                    ctx.ask_seeing(state, &choice)
                        .ok()
                        .map(|answer| PlayerId::new(answer.id))
                }
            };
            if let Some(chosen) = chosen {
                let planet = ti4_model::id::PlanetId::new(outcome);
                place_infantry(state, content, sources, &chosen, &system, &planet);
            }
        }
        "artifact" => {
            // Ixthian Artifact: the speaker rolls, and it is either a windfall or a crater.
            if outcome != FOR {
                return Effect::Resolved {
                    agenda: agenda.to_owned(),
                };
            }
            let roll = ctx
                .dice
                .roll(ctx.rng, 1, "ixthian_artifact", None)
                .faces
                .first()
                .copied()
                .unwrap_or(0);
            if roll >= 6 {
                return Effect::Deferred {
                    agenda: agenda.to_owned(),
                    what: "the windfall researches two technologies each, which needs a payer"
                        .to_owned(),
                };
            }
            // The crater: three damage to every ship in every system, which this engine
            // models as destruction only for units that cannot sustain.
            let types = ti4_content::units::catalogue(content, sources);
            for board in state.board.values_mut() {
                board.units.retain(|unit| {
                    types
                        .get(unit.type_id.as_str())
                        .is_none_or(|kind| !kind.is_ship() || kind.sustain_damage())
                });
            }
        }
        "wormhole_research" => {
            // Wormhole Research: a technology for the brave, then the wormholes eat the fleet.
            if outcome != FOR {
                return Effect::Deferred {
                    agenda: agenda.to_owned(),
                    what: "Against voters return a command token".to_owned(),
                };
            }
            let Some(galaxy) = galaxy else {
                return Effect::Deferred {
                    agenda: agenda.to_owned(),
                    what: "needs the map to know which systems hold wormholes".to_owned(),
                };
            };
            let systems = ti4_content::galaxy::all_systems(content, sources);
            let types = ti4_content::units::catalogue(content, sources);
            for id in galaxy.system_ids() {
                let kinds = systems.get(id).map(ti4_content::galaxy::System::wormholes);
                let alpha_or_beta =
                    kinds.is_some_and(|holes| holes.contains("ALPHA") || holes.contains("BETA"));
                if !alpha_or_beta {
                    continue;
                }
                let system = ti4_model::id::SystemId::new(id);
                if let Some(board) = state.board.get_mut(&system) {
                    board.units.retain(|unit| {
                        types
                            .get(unit.type_id.as_str())
                            .is_none_or(|kind| !kind.is_ship())
                    });
                }
            }
            return Effect::Deferred {
                agenda: agenda.to_owned(),
                what: "the technology it offers first needs a payer".to_owned(),
            };
        }
        "regulations" => {
            // Against: everyone gains a fleet token. The For half is the standing cap, and
            // belongs to `laws`.
            if outcome == FOR {
                return Effect::Resolved {
                    agenda: agenda.to_owned(),
                };
            }
            for player in everyone(state) {
                state.gain_token(&player, ti4_model::state::TokenPool::Fleet, 1);
            }
        }
        "sanctions" => {
            // Against: each player discards one *random* action card — random through the
            // seeded roller, because an unseeded pick here would break replay.
            if outcome == FOR {
                return Effect::Resolved {
                    agenda: agenda.to_owned(),
                };
            }
            for player in everyone(state) {
                let held = state
                    .player(&player)
                    .map_or(0, |seat| seat.action_cards.len());
                if held == 0 {
                    continue;
                }
                let face = ctx
                    .dice
                    .roll(ctx.rng, 1, "sanctions", None)
                    .faces
                    .first()
                    .copied()
                    .unwrap_or(1) as usize;
                crate::action_cards::discard(state, &player, face % held);
            }
        }
        "travel_ban" => {
            // Against: every PDS in or beside a wormhole system is destroyed. Without a map
            // there is no "beside", so the card cannot be applied at all.
            if outcome == FOR {
                return Effect::Resolved {
                    agenda: agenda.to_owned(),
                };
            }
            let Some(galaxy) = galaxy else {
                return Effect::Deferred {
                    agenda: agenda.to_owned(),
                    what: "needs the map to know what is adjacent to a wormhole".to_owned(),
                };
            };
            let systems = ti4_content::galaxy::all_systems(content, sources);
            let mut exposed = std::collections::BTreeSet::new();
            // Only the systems *on this map*. Reading every wormhole system in the corpus
            // destroys garrisons in systems the game was never set up with.
            for id in galaxy.system_ids() {
                if systems
                    .get(id)
                    .is_none_or(|system| system.wormholes().is_empty())
                {
                    continue;
                }
                exposed.insert(id.to_owned());
                exposed.extend(galaxy.adjacent(id).into_iter().map(ToOwned::to_owned));
            }
            let types = ti4_content::units::catalogue(content, sources);
            for board in state
                .board
                .iter_mut()
                .filter_map(|(id, board)| exposed.contains(id.as_str()).then_some(board))
            {
                for units in board.planet_units.values_mut() {
                    units.retain(|unit| {
                        types
                            .get(unit.type_id.as_str())
                            .is_none_or(|kind| kind.base_type() != "pds")
                    });
                }
            }
        }
        "defense_act" => {
            // Against: each player destroys one of their own PDS, and chooses which. The For
            // half lifts a cap on PDS per planet that this engine does not enforce, so there
            // is nothing for it to relax.
            if outcome == FOR {
                return Effect::Resolved {
                    agenda: agenda.to_owned(),
                };
            }
            for player in everyone(state) {
                let held = structures_of(state, content, sources, &player, "pds");
                let Some((system, planet, index)) = choose_structure(ctx, state, &player, &held)
                else {
                    continue;
                };
                if let Some(units) = state.system_mut(&system).planet_units.get_mut(&planet)
                    && index < units.len()
                {
                    units.remove(index);
                }
            }
        }
        "shared_research" => {
            // Against: a command token into each player's home system, if they have one.
            if outcome == FOR {
                return Effect::Resolved {
                    agenda: agenda.to_owned(),
                };
            }
            for player in everyone(state) {
                let home = state.player(&player).and_then(|seat| {
                    seat.home_system.clone().or_else(|| {
                        ti4_content::factions::get(content, seat.faction.as_str())
                            .and_then(|faction| faction.home_system())
                            .map(ti4_model::id::SystemId::new)
                    })
                });
                if let Some(home) = home {
                    state.system_mut(&home).command_tokens.insert(player);
                }
            }
        }
        "miscount" => {
            // Miscount Disclosed: the elected law comes off the table. The oracle then puts it
            // to an immediate re-vote; this repeals it, which is the half that changes state.
            // Re-opening a vote from inside an effect needs the agenda window, not this path.
            if !state.laws.contains_key(outcome) {
                return Effect::Resolved {
                    agenda: agenda.to_owned(),
                };
            }
            crate::laws::repeal(state, outcome);
            return Effect::Deferred {
                agenda: agenda.to_owned(),
                what: "the elected law is repealed, but the re-vote needs the agenda window"
                    .to_owned(),
            };
        }
        "disarmament" => {
            // The elected planet's ground forces are bought out, and its controller is paid for
            // them — so a planet nobody controls destroys its garrison for nothing.
            let controller = controller_of(state, outcome);
            let destroyed = clear_planet(
                state,
                content,
                sources,
                outcome,
                |base| matches!(base, "infantry" | "mech"),
                None,
            );
            if let Some(controller) = controller
                && destroyed > 0
                && let Some(seat) = state.player_mut(&controller)
            {
                seat.trade_goods += i32::try_from(destroyed).unwrap_or(i32::MAX);
            }
        }
        "plowshares" => {
            // For: half of everyone's infantry, rounded *up*, bought back as trade goods.
            // Rounding down would leave a lone infantry standing, which the card does not.
            //
            // Against is the opposite card: everyone *arms*, one infantry per planet held. An
            // effect that did nothing on Against would make voting it down free, when it is the
            // half that puts troops on the board.
            if outcome != FOR {
                for player in everyone(state) {
                    let held: Vec<(ti4_model::id::SystemId, ti4_model::id::PlanetId)> = state
                        .controlled_planets(&player)
                        .into_iter()
                        .map(|(system, planet)| (system.clone(), planet.clone()))
                        .collect();
                    for (system, planet) in held {
                        place_infantry(state, content, sources, &player, &system, &planet);
                    }
                }
                return Effect::Resolved {
                    agenda: agenda.to_owned(),
                };
            }
            for player in everyone(state) {
                let mut destroyed = 0;
                let held_planets: Vec<(ti4_model::id::SystemId, ti4_model::id::PlanetId)> = state
                    .controlled_planets(&player)
                    .into_iter()
                    .map(|(system, planet)| (system.clone(), planet.clone()))
                    .collect();
                for (system, planet) in held_planets {
                    let types = ti4_content::units::catalogue(content, sources);
                    let held: Vec<usize> = state
                        .system_state(&system)
                        .planet_units
                        .get(&planet)
                        .map(|units| {
                            units
                                .iter()
                                .enumerate()
                                .filter(|(_, unit)| {
                                    unit.owner == player
                                        && types
                                            .get(unit.type_id.as_str())
                                            .is_some_and(|kind| kind.base_type() == "infantry")
                                })
                                .map(|(index, _)| index)
                                .collect()
                        })
                        .unwrap_or_default();
                    let losses = held.len().div_ceil(2);
                    if let Some(units) = state.system_mut(&system).planet_units.get_mut(&planet) {
                        for index in held.into_iter().take(losses).rev() {
                            units.remove(index);
                        }
                    }
                    destroyed += losses;
                }
                if let Some(seat) = state.player_mut(&player) {
                    seat.trade_goods += i32::try_from(destroyed).unwrap_or(i32::MAX);
                }
            }
        }
        "arms_reduction" => {
            if outcome != FOR {
                // Against exhausts planets with a technology specialty, which needs an exhaust
                // this effect cannot ask for. Deferred rather than silently skipped.
                return Effect::Deferred {
                    agenda: agenda.to_owned(),
                    what: "exhaust planets with a technology specialty".to_owned(),
                };
            }
            for player in everyone(state) {
                cull_ships(state, content, sources, &player, "dreadnought", 2);
                cull_ships(state, content, sources, &player, "cruiser", 4);
            }
        }
        "conventions" => {
            // Against: everyone who voted Against loses their hand. Voting for a law that
            // fails costs nothing; voting against one that fails costs everything.
            if outcome == FOR {
                return Effect::Resolved {
                    agenda: agenda.to_owned(),
                };
            }
            for player in ballot.voted_for(AGAINST) {
                discard_hand(state, &player);
            }
        }
        "schematics" => {
            if outcome == FOR {
                return Effect::Resolved {
                    agenda: agenda.to_owned(),
                };
            }
            for player in everyone(state) {
                if owns_a_war_sun_technology(state, content, &player) {
                    discard_hand(state, &player);
                }
            }
        }
        "conscription" => {
            // Against does nothing. The For half is a standing rule and belongs to `laws`,
            // which is why this arm exists at all: an agenda with no arm is *unavailable*.
        }
        // Five more of the same shape: the card does nothing when it passes, and everything
        // afterwards. Each is enforced in `laws` -- the attachment changes what a planet is worth,
        // the Research Teams waive a prerequisite, Representative Government flattens the vote.
        "rep_govt"
        | "rt_biotic"
        | "rt_cybernetic"
        | "rt_propulsion"
        | "rt_warfare"
        | "senate_sanctuary"
        | "terraforming_initiative" => {}
        // Ministries: the card does nothing when it passes and everything afterwards, so the arm
        // exists only to make the agenda available. Articles of War is the same shape -- its For
        // half is a standing rule, and its Against half pays the players who voted For.
        "minister_exploration" | "minister_policy" => {}
        // Availability-only arms. Each of these laws is a standing rule owned by the subsystem
        // that runs it, so closing the vote records the outcome and nothing else:
        // - Committee, the seven Ministers, Prophecy of Kings and the Crown of Thalnos are
        //   standing rules the turn driver and `laws` apply on every later turn; their vote
        //   outcome is recorded by `game` (the law enters play) before this effect runs.
        // - Checks: the Against half is a standing cap on planet readiness at the end of the
        //   agenda phase, which the driver owns.
        // - Covert Operations and Crisis are standing rules of the agenda and turn phases; the
        //   driver applies them, the vote only puts the law in play.
        "committee"
        | "arbiter"
        | "minister_commerce"
        | "minister_industry"
        | "minister_peace"
        | "minister_sciences"
        | "minister_war"
        | "prophecy"
        | "crown_of_thalnos"
        | "checks"
        | "covert"
        | "crisis" => {}
        "articles_war" => {
            if outcome == AGAINST {
                for player in ballot.voted_for(FOR) {
                    if let Some(seat) = state.player_mut(&player) {
                        seat.trade_goods += 3;
                    }
                }
            }
        }
        "nexus" => {
            // For is a standing rule about the wormhole nexus. Against places a gamma wormhole in
            // Mecatol Rex -- gamma, so neither wormhole law touches it: both name alpha and beta.
            if outcome == AGAINST {
                state.wormhole_tokens.insert(
                    "GAMMA".to_owned(),
                    ti4_model::id::SystemId::new(crate::seating::MECATOL),
                );
            }
        }
        "warrant" => {
            // "draws 2 secret objectives". The revealed-hand half is information, not mechanics,
            // and needs no state: nothing in this engine hides a hand from a decider.
            let elected = PlayerId::new(outcome);
            if state.player(&elected).is_some() {
                let mut table = crate::choice::Table::new();
                for _ in 0..2 {
                    let _ = crate::secrets::draw(state, content, &mut table, &elected);
                }
            }
        }
        "grant_reallocation" => {
            // "gains any 1 technology of their choice. Then, for each prerequisite on that
            // technology, they remove 1 token from their fleet pool." The choice is the elected
            // player's; with no decider here the technology is not chosen for them -- the fleet
            // cost is what this arm can honour without inventing a pick, and a driver that has a
            // table resolves the gain through `technology`.
            let elected = PlayerId::new(outcome);
            if let Some(seat) = state.player_mut(&elected) {
                seat.spend_token(ti4_model::state::TokenPool::Fleet);
            }
        }
        "standardization" => {
            // "so that they have 3 in tactic, 3 in fleet, and 2 in strategy" -- a target, not a
            // gain. A seat already above a target keeps what it has: the card says place tokens
            // from reinforcements, and there is no clause taking any away.
            let elected = PlayerId::new(outcome);
            if let Some(seat) = state.player_mut(&elected) {
                for (pool, target) in [
                    (ti4_model::state::TokenPool::Tactic, 3),
                    (ti4_model::state::TokenPool::Fleet, 3),
                    (ti4_model::state::TokenPool::Strategic, 2),
                ] {
                    let short = target - seat.tokens(pool);
                    if short > 0 {
                        // Uncapped: this agenda restores a *starting* spread rather than granting
                        // new plastic, and the tokens it puts back came off the same sheet.
                        seat.gain_token_uncapped(pool, short);
                    }
                }
            }
        }
        "cladenstine" => {
            // For: two tokens off the command sheet, the player's choice of pools. Against: one
            // specifically from the fleet pool. Taken from the fullest pool first, so a seat is
            // never asked to pay from an empty one while another is stocked.
            if outcome == FOR {
                for player in everyone(state) {
                    for _ in 0..2 {
                        let Some(seat) = state.player(&player) else {
                            continue;
                        };
                        let fullest = ti4_model::state::TokenPool::ALL
                            .into_iter()
                            .max_by_key(|pool| seat.tokens(*pool));
                        if let Some(pool) = fullest
                            && let Some(seat) = state.player_mut(&player)
                        {
                            seat.spend_token(pool);
                        }
                    }
                }
            } else {
                for player in everyone(state) {
                    if let Some(seat) = state.player_mut(&player) {
                        seat.spend_token(ti4_model::state::TokenPool::Fleet);
                    }
                }
            }
        }
        "rearmament" => {
            // For: a mech onto a home planet. Against: every mech becomes an infantry.
            if outcome == FOR {
                for player in everyone(state) {
                    let home: Option<(ti4_model::id::SystemId, ti4_model::id::PlanetId)> = state
                        .controlled_planets(&player)
                        .into_iter()
                        .find(|(system, _)| {
                            state.player(&player).is_some_and(|seat| {
                                ti4_content::factions::get(content, seat.faction.as_str())
                                    .and_then(|faction| faction.home_system())
                                    .is_some_and(|home| home == system.as_str())
                            })
                        })
                        .map(|(system, planet)| (system.clone(), planet.clone()));
                    if let Some((system, planet)) = home {
                        state
                            .system_mut(&system)
                            .planet_units
                            .entry(planet)
                            .or_default()
                            .push(ti4_model::units::Unit::new(
                                ti4_model::id::UnitTypeId::new("mech"),
                                player.clone(),
                            ));
                    }
                }
            } else {
                let types = ti4_content::units::catalogue(content, ti4_model::content_types::DEFAULT);
                for record in state.board.values_mut() {
                    for units in record.planet_units.values_mut() {
                        for unit in units.iter_mut() {
                            if types
                                .get(unit.type_id.as_str())
                                .is_some_and(|kind| kind.base_type() == "mech")
                            {
                                unit.type_id = ti4_model::id::UnitTypeId::new("infantry");
                                unit.sustained_damage = false;
                            }
                        }
                    }
                }
            }
        }
        "minister_antiquities" => {
            let elected = PlayerId::new(outcome);
            crate::relics::gain(state, &elected);
        }
        "core_mining" => {
            // An infantry pays for the seam. The planet's +2 resources is the law itself.
            clear_planet(
                state,
                content,
                sources,
                outcome,
                |base| base == "infantry",
                Some(1),
            );
        }
        "demilitarized_zone" => {
            // Everything on the planet dies; the standing ban is the law.
            clear_planet(state, content, sources, outcome, |_| true, None);
        }
        "holy_planet_of_ixth" => {
            if let Some(controller) = controller_of(state, outcome) {
                adjust_victory_points(state, &controller, 1);
            }
        }
        "wormhole_recon" => {
            // Against: a command token in each wormhole system holding one of your ships.
            if outcome == FOR {
                return Effect::Resolved {
                    agenda: agenda.to_owned(),
                };
            }
            let systems = ti4_content::galaxy::all_systems(content, sources);
            for (id, board) in state.board.clone() {
                let has_wormhole = systems
                    .get(id.as_str())
                    .is_some_and(|system| !system.wormholes().is_empty());
                if !has_wormhole {
                    continue;
                }
                for player in everyone(state) {
                    if !board.units_of(&player).is_empty() {
                        state.system_mut(&id).command_tokens.insert(player);
                    }
                }
            }
        }
        "revolution" | "representative_government" => {
            // Both Against halves exhaust planets by a rule this effect cannot ask for.
            if outcome == FOR {
                return Effect::Resolved {
                    agenda: agenda.to_owned(),
                };
            }
            return Effect::Deferred {
                agenda: agenda.to_owned(),
                what: "Against voters exhaust planets".to_owned(),
            };
        }
        "economic_equality" => {
            // Everyone's trade goods go back to the supply first, then For pays five each.
            // Doing it in that order matters: on Against, the card is purely destructive.
            for player in everyone(state) {
                if let Some(seat) = state.player_mut(&player) {
                    seat.trade_goods = 0;
                }
            }
            if outcome == FOR {
                for player in everyone(state) {
                    if let Some(seat) = state.player_mut(&player) {
                        seat.trade_goods += 5;
                    }
                }
            }
        }
        "mutiny" => {
            // Those who voted For gain a point on For, and lose one on Against. Read from the
            // ballot, not from the outcome: who voted which way is the whole card.
            let delta = if outcome == FOR { 1 } else { -1 };
            for player in ballot.voted_for(FOR) {
                adjust_victory_points(state, &player, delta);
            }
        }
        "seed_empire" => {
            let mut points: Vec<(PlayerId, i32)> = state
                .players
                .iter()
                .map(|seat| (seat.id.clone(), seat.victory_points))
                .collect();
            if points.is_empty() {
                return Effect::Resolved {
                    agenda: agenda.to_owned(),
                };
            }
            points.sort_by(|a, b| a.0.cmp(&b.0));
            let target = if outcome == FOR {
                points.iter().map(|(_, n)| *n).max()
            } else {
                points.iter().map(|(_, n)| *n).min()
            };
            let Some(target) = target else {
                return Effect::Resolved {
                    agenda: agenda.to_owned(),
                };
            };
            let tied: Vec<PlayerId> = points
                .into_iter()
                .filter(|(_, n)| *n == target)
                .map(|(player, _)| player)
                .collect();
            let winner = match tied.as_slice() {
                [only] => Some(only.clone()),
                // 8.18 makes resolving the outcome the speaker's job, so a tie where the card
                // names one player is a decision rather than a guess at an unwritten rule.
                _ => ask_the_speaker(state, ctx, &tied),
            };
            if let Some(winner) = winner {
                adjust_victory_points(state, &winner, 1);
            }
        }
        "abolishment" => {
            // Judicial Abolishment discards the *elected law*, so the outcome names a law
            // rather than For/Against. A repeal that ignored the outcome would discard
            // whatever happened to be first.
            crate::laws::repeal(state, outcome);
        }
        "constitution" => {
            // New Constitution discards every law in play, on For only.
            if outcome == FOR {
                for alias in crate::laws::in_play(state) {
                    crate::laws::repeal(state, &alias);
                }
            }
        }
        "unconventional" => {
            // Unconventional Measures pays the For voters, or purges them. Either way it acts
            // on who voted, not on what won.
            for player in ballot.voted_for(FOR) {
                if outcome == FOR {
                    // Two action cards. The hand limit is enforced by the caller that owns a
                    // table; here the draw is unconditional and the limit applies later.
                    for _ in 0..2 {
                        if state.action_card_deck.is_empty() {
                            break;
                        }
                        let top = state.action_card_deck.remove(0);
                        if let Some(seat) = state.player_mut(&player) {
                            seat.action_cards.push(top);
                        }
                    }
                } else if let Some(seat) = state.player_mut(&player) {
                    seat.action_cards.clear();
                }
            }
        }
        "incentive" => {
            // Stage I on For, stage II on Against. Not the top card: the deck is stage I then
            // stage II in order, so taking the top reveals the wrong stage while any stage I
            // remains, and the card would quietly do the opposite of what it says.
            let stage = if outcome == FOR { 1 } else { 2 };
            crate::objectives::reveal_stage(state, content, stage);
        }
        _ => {
            return Effect::Unresolved {
                agenda: agenda.to_owned(),
            };
        }
    }
    let _ = AGAINST;
    Effect::Resolved {
        agenda: agenda.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use ti4_model::content_types::POK;

    /// Resolve one agenda with a given outcome and ballot, with no speaker on hand.
    fn run(state: &mut GameState, agenda: &str, outcome: &str, ballot: &Ballot) -> Effect {
        resolve(
            state,
            ti4_content::ContentStore::embedded(),
            agenda,
            outcome,
            ballot,
        )
    }

    fn no_votes() -> Ballot {
        Ballot::default()
    }

    /// Nexus Sovereignty, Against: a gamma wormhole into Mecatol Rex.
    #[test]
    fn nexus_sovereignty_against_puts_a_gamma_wormhole_on_mecatol() {
        let mut state = crate::fixtures::game(&["a", "b"]);
        run(&mut state, "nexus", AGAINST, &no_votes());
        assert_eq!(
            state.wormhole_tokens.get("GAMMA").map(ti4_model::id::SystemId::as_str),
            Some(crate::seating::MECATOL)
        );

        // For is a standing rule and places nothing.
        let mut state = crate::fixtures::game(&["a", "b"]);
        run(&mut state, "nexus", FOR, &no_votes());
        assert!(state.wormhole_tokens.is_empty());
    }

    /// Search Warrant deals the elected player two secrets.
    #[test]
    fn a_search_warrant_deals_two_secrets() {
        let mut state = crate::fixtures::game(&["a", "b"]);
        let elected = PlayerId::new("a");
        let before = state
            .player(&elected)
            .map_or(0, |seat| seat.secret_objectives.len());

        run(&mut state, "warrant", "a", &no_votes());

        let after = state
            .player(&elected)
            .map_or(0, |seat| seat.secret_objectives.len());
        assert_eq!(after, before + 2, "two secret objectives");
    }

    /// Articles of War, Against: three trade goods to everyone who voted For.
    #[test]
    fn articles_of_war_pays_the_players_who_voted_for_it() {
        let mut state = crate::fixtures::game(&["a", "b"]);
        let mut ballot = Ballot::default();
        ballot.votes.insert(PlayerId::new("a"), FOR.to_owned());

        let before = state
            .player(&PlayerId::new("a"))
            .map_or(0, |seat| seat.trade_goods);
        let untouched = state
            .player(&PlayerId::new("b"))
            .map_or(0, |seat| seat.trade_goods);

        run(&mut state, "articles_war", AGAINST, &ballot);

        assert_eq!(
            state
                .player(&PlayerId::new("a"))
                .map_or(0, |seat| seat.trade_goods),
            before + 3
        );
        assert_eq!(
            state
                .player(&PlayerId::new("b"))
                .map_or(0, |seat| seat.trade_goods),
            untouched,
            "and nobody else"
        );
    }

    /// Armed Forces Standardization tops pools up to 3/3/2 and takes nothing away.
    ///
    /// The card says *place* tokens from reinforcements so that the seat has those numbers. A seat
    /// already above one of them keeps what it has: nothing in the text removes a token.
    #[test]
    fn standardization_tops_the_pools_up_without_taking_any_away() {
        use ti4_model::state::TokenPool;

        let mut state = crate::fixtures::game(&["a", "b"]);
        let elected = PlayerId::new("a");
        {
            let seat = state.player_mut(&elected).expect("seated");
            seat.tactic_tokens = 0;
            seat.fleet_tokens = 5;
            seat.strategic_tokens = 1;
        }

        let effect = run(&mut state, "standardization", "a", &no_votes());
        assert_eq!(
            effect,
            Effect::Resolved {
                agenda: "standardization".to_owned()
            }
        );

        let seat = state.player(&elected).expect("seated");
        assert_eq!(seat.tokens(TokenPool::Tactic), 3, "topped up to three");
        assert_eq!(
            seat.tokens(TokenPool::Fleet),
            5,
            "already above the target, so untouched"
        );
        assert_eq!(seat.tokens(TokenPool::Strategic), 2);
    }

    /// Clandestine Operations takes two tokens off the sheet, or one from the fleet pool.
    #[test]
    fn clandestine_operations_removes_tokens() {
        use ti4_model::state::TokenPool;

        let mut state = crate::fixtures::game(&["a", "b"]);
        let before: i32 = TokenPool::ALL
            .into_iter()
            .map(|pool| state.player(&PlayerId::new("a")).expect("seated").tokens(pool))
            .sum();

        run(&mut state, "cladenstine", FOR, &no_votes());
        let after: i32 = TokenPool::ALL
            .into_iter()
            .map(|pool| state.player(&PlayerId::new("a")).expect("seated").tokens(pool))
            .sum();
        assert_eq!(after, before - 2, "two tokens leave the command sheet");

        let mut state = crate::fixtures::game(&["a", "b"]);
        let fleet = state
            .player(&PlayerId::new("a"))
            .expect("seated")
            .tokens(TokenPool::Fleet);
        run(&mut state, "cladenstine", AGAINST, &no_votes());
        assert_eq!(
            state
                .player(&PlayerId::new("a"))
                .expect("seated")
                .tokens(TokenPool::Fleet),
            fleet - 1,
            "Against takes one, and takes it from the fleet pool"
        );
    }

    /// Rearmament Agreement, Against: every mech on the board becomes an infantry.
    #[test]
    fn rearmament_against_replaces_every_mech_with_an_infantry() {
        let (mut state, planet, player) = garrison(0);
        let (system, _) = crate::fixtures::a_placed_planet();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "mech", &player, 2);
        assert_eq!(on_planet(&state, &planet), 2);

        run(&mut state, "rearmament", AGAINST, &no_votes());

        let kinds: Vec<String> = state
            .system_state(&system)
            .on_planet(&planet)
            .iter()
            .map(|unit| unit.type_id.as_str().to_owned())
            .collect();
        assert_eq!(
            kinds,
            vec!["infantry", "infantry"],
            "each mech is replaced, not removed"
        );
    }

    /// Minister of Antiquities hands the elected player a relic off the deck.
    #[test]
    fn minister_of_antiquities_gains_a_relic() {
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.relic_deck = vec![ti4_model::id::RelicId::new("dominusori")];
        let elected = PlayerId::new("a");

        run(&mut state, "minister_antiquities", "a", &no_votes());

        assert_eq!(
            state.player(&elected).expect("seated").relics.len(),
            1,
            "the elected player gains one relic"
        );
        assert!(state.relic_deck.is_empty(), "and it came off the deck");
    }

    /// The standing-rule laws resolve to nothing and are still *available*.
    ///
    /// An agenda with no arm is unavailable, so these need one even though the card does nothing
    /// when it passes. Their effect lives in `laws`.
    #[test]
    fn a_standing_rule_law_resolves_cleanly() {
        for alias in [
            "senate_sanctuary",
            "terraforming_initiative",
            "rt_biotic",
            "rt_cybernetic",
            "rt_propulsion",
            "rt_warfare",
            "rep_govt",
        ] {
            let mut state = crate::fixtures::game(&["a", "b"]);
            let effect = run(&mut state, alias, "a", &no_votes());
            assert_eq!(
                effect,
                Effect::Resolved {
                    agenda: alias.to_owned()
                },
                "{alias} must resolve rather than report itself unregistered"
            );
        }
    }

    /// A player controlling one planet, with `infantry` infantry on it.
    fn garrison(count: usize) -> (GameState, ti4_model::id::PlanetId, PlayerId) {
        let mut state = crate::fixtures::game(&["a", "b"]);
        let player = PlayerId::new("a");
        let (system, planet) = crate::fixtures::a_placed_planet();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player.clone());
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &player, count);
        (state, planet, player)
    }

    fn on_planet(state: &GameState, planet: &ti4_model::id::PlanetId) -> usize {
        state
            .board
            .values()
            .filter_map(|board| board.planet_units.get(planet))
            .map(Vec::len)
            .sum()
    }

    /// Resolve one agenda with a scripted table and, optionally, the map.
    fn run_with(
        state: &mut GameState,
        galaxy: Option<&ti4_content::galaxy::Galaxy>,
        agenda: &str,
        outcome: &str,
        answers: &[&str],
    ) -> Effect {
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut table = crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new(
            answers.iter().map(|answer| (*answer).to_owned()),
        )));
        let mut ctx = crate::choice::Resolving {
            content: ContentStore::embedded(),
            sources: ti4_model::content_types::POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };
        resolve_with(state, &mut ctx, galaxy, agenda, outcome, &Ballot::default())
    }

    #[test]
    fn the_elected_player_takes_the_point_and_nobody_else_does() {
        for alias in ["censure", "shard_of_the_throne", "crown_of_emphidia"] {
            let mut state = game(&["a", "b"]);

            run_with(&mut state, None, alias, "b", &[]);

            assert_eq!(
                state.player(&b()).unwrap().victory_points,
                1,
                "{alias} pays the player it elected"
            );
            assert_eq!(state.player(&a()).unwrap().victory_points, 0, "{alias}");
        }
    }

    #[test]
    fn an_agenda_electing_nobody_pays_nobody() {
        let mut state = game(&["a", "b"]);
        run_with(&mut state, None, "censure", FOR, &[]);
        assert_eq!(state.player(&a()).unwrap().victory_points, 0);
        assert_eq!(state.player(&b()).unwrap().victory_points, 0);
    }

    #[test]
    fn archived_secret_deals_to_the_player_it_elected() {
        let mut state = game(&["a", "b"]);
        state.player_mut(&b()).unwrap().secret_objectives.clear();
        state.secret_deck = vec![ti4_model::id::SecretObjectiveId::new("some_secret")];

        run_with(&mut state, None, "secret", "b", &[]);

        assert_eq!(state.player(&b()).unwrap().secret_objectives.len(), 1);
    }

    #[test]
    fn a_leaked_secret_is_scoreable_by_everybody() {
        // The whole card: the objective moves into the public area and its requirement, which
        // stays registered in `secrets`, has to be found from there. A public-only lookup
        // leaves it on the table worth nothing to anybody.
        let mut state = game(&["a", "b"]);
        let leaked = "csl"; // Cut Supply Lines: a rival dock in a system holding your ships

        run_with(&mut state, None, "classified", leaked, &[]);

        assert!(
            state
                .revealed_objectives
                .contains(&ti4_model::id::ObjectiveId::new(leaked)),
            "the secret is public now"
        );

        // Satisfy it for a, who never held the card.
        let (system, planet) = crate::fixtures::a_placed_planet();
        crate::fixtures::put(&mut state, &system, "cruiser", &a(), 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "spacedock", &b(), 1);

        assert!(
            crate::objectives::scoreable(
                &state,
                ContentStore::embedded(),
                ti4_model::content_types::POK,
                &a()
            )
            .contains(&ti4_model::id::ObjectiveId::new(leaked)),
            "a can score it from the public area"
        );
    }

    #[test]
    fn a_public_execution_costs_the_hand_and_the_chair() {
        let mut state = game(&["a", "b", "c"]);
        state.speaker = b();
        state.player_mut(&b()).unwrap().action_cards =
            vec![ti4_model::id::ActionCardId::new("card")];

        let effect = run_with(&mut state, None, "execution", "b", &[]);

        assert!(state.player(&b()).unwrap().action_cards.is_empty());
        assert_ne!(state.speaker, b(), "the chair passes clockwise");
        assert!(
            matches!(effect, Effect::Deferred { .. }),
            "8.16's voting ban belongs to the session: {effect:?}"
        );
    }

    #[test]
    fn an_execution_of_a_player_who_is_not_speaker_leaves_the_chair_alone() {
        let mut state = game(&["a", "b"]);
        state.speaker = a();

        run_with(&mut state, None, "execution", "b", &[]);

        assert_eq!(state.speaker, a());
    }

    #[test]
    fn colonial_redistribution_hands_the_planet_to_a_trailing_player() {
        let mut state = game(&["a", "b", "c"]);
        let (system, planet) = crate::fixtures::a_placed_planet();
        state.system_mut(&system).set_control(planet.clone(), a());
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &a(), 2);
        state.player_mut(&a()).unwrap().victory_points = 5;
        // b and c are level at the back, so the controller chooses between them.
        run_with(&mut state, None, "redistribution", planet.as_str(), &["c"]);

        let units = state
            .system_state(&system)
            .planet_units
            .get(&planet)
            .cloned()
            .unwrap_or_default();
        assert_eq!(units.len(), 1, "the old garrison went, one settler arrived");
        assert_eq!(units[0].owner, PlayerId::new("c"), "the controller named c");
    }

    #[test]
    fn the_ixthian_artifact_is_a_windfall_or_a_crater() {
        // Both branches are forced, because a test that took whatever the stream gave would
        // exercise one and call it the card.
        for (face, survives) in [(10, true), (1, false)] {
            let mut state = game(&["a"]);
            let (system, _) = crate::fixtures::a_placed_planet();
            crate::fixtures::put(&mut state, &system, "fighter", &a(), 2);

            let mut dice = crate::dice::Dice::from_faces([face]);
            let mut rng = crate::rng::GameRng::new(0);
            let mut table = crate::choice::Table::new();
            let mut ctx = crate::choice::Resolving {
                content: ContentStore::embedded(),
                sources: ti4_model::content_types::POK,
                dice: &mut dice,
                rng: &mut rng,
                table: &mut table,
                timing: None,
            };
            resolve_with(
                &mut state,
                &mut ctx,
                None,
                "artifact",
                FOR,
                &Ballot::default(),
            );

            let left = state.system_state(&system).units.len();
            if survives {
                assert_eq!(left, 2, "a high roll is the windfall, not the crater");
            } else {
                assert_eq!(left, 0, "fighters cannot sustain three damage");
            }
        }
    }

    #[test]
    fn wormhole_research_eats_the_fleets_in_alpha_and_beta_systems() {
        let content = ContentStore::embedded();
        let sources = ti4_model::content_types::POK;
        let alpha = ti4_content::galaxy::all_systems(content, sources)
            .iter()
            .find(|(_, system)| {
                (system.wormholes().contains("ALPHA") || system.wormholes().contains("BETA"))
                    && !system.is_hyperlane()
            })
            .map(|(id, _)| (*id).to_owned())
            .expect("the corpus has an alpha or beta wormhole");

        let hub = crate::fixtures::hub_with_outer(&alpha);
        let far = hub.across(&alpha);
        let mut state = game(&["a"]);
        let hole = ti4_model::id::SystemId::new(alpha.clone());
        let elsewhere = ti4_model::id::SystemId::new(far);
        crate::fixtures::put(&mut state, &hole, "cruiser", &a(), 2);
        crate::fixtures::put(&mut state, &elsewhere, "cruiser", &a(), 1);

        run_with(&mut state, Some(&hub.galaxy), "wormhole_research", FOR, &[]);

        assert_eq!(
            state.system_state(&hole).units.len(),
            0,
            "the wormhole ate them"
        );
        assert_eq!(
            state.system_state(&elsewhere).units.len(),
            1,
            "a fleet nowhere near a wormhole is untouched"
        );
    }

    #[test]
    fn fleet_regulations_hands_out_tokens_only_when_it_fails() {
        let mut state = game(&["a", "b"]);
        let before = state
            .player(&a())
            .unwrap()
            .tokens(ti4_model::state::TokenPool::Fleet);

        run_with(&mut state, None, "regulations", FOR, &[]);
        assert_eq!(
            state
                .player(&a())
                .unwrap()
                .tokens(ti4_model::state::TokenPool::Fleet),
            before,
            "the law passed, so the Against half never happens"
        );

        run_with(&mut state, None, "regulations", AGAINST, &[]);
        assert_eq!(
            state
                .player(&a())
                .unwrap()
                .tokens(ti4_model::state::TokenPool::Fleet),
            before + 1
        );
    }

    #[test]
    fn executive_sanctions_takes_one_card_not_the_hand() {
        // The card discards *one* random card. Taking the hand is Conventions of War, and the
        // two are one word apart in the text.
        let mut state = game(&["a"]);
        state.player_mut(&a()).unwrap().action_cards = (0..3)
            .map(|n| ti4_model::id::ActionCardId::new(format!("card{n}")))
            .collect();

        run_with(&mut state, None, "sanctions", AGAINST, &[]);

        assert_eq!(state.player(&a()).unwrap().action_cards.len(), 2);
    }

    #[test]
    fn executive_sanctions_leaves_an_empty_hand_alone() {
        let mut state = game(&["a"]);
        state.player_mut(&a()).unwrap().action_cards.clear();

        run_with(&mut state, None, "sanctions", AGAINST, &[]);

        assert!(state.player(&a()).unwrap().action_cards.is_empty());
    }

    #[test]
    fn homeland_defense_act_lets_the_owner_choose_which_pds_dies() {
        let mut state = game(&["a"]);
        let hub = crate::fixtures::plain_hub();
        let planets: Vec<ti4_model::id::PlanetId> = ti4_content::galaxy::all_planets(
            ContentStore::embedded(),
            ti4_model::content_types::POK,
        )
        .into_keys()
        .map(ti4_model::id::PlanetId::new)
        .take(2)
        .collect();
        let system = ti4_model::id::SystemId::new(hub.centre.clone());
        for planet in &planets {
            crate::fixtures::put_on_planet(&mut state, &system, planet, "pds", &a(), 1);
        }

        // Answer "1": the second of the two, so a decider that ignored the answer and took the
        // first would leave the wrong one standing.
        run_with(&mut state, None, "defense_act", AGAINST, &["1"]);

        let board = state.system_state(&system);
        assert_eq!(board.planet_units.get(&planets[0]).map_or(0, Vec::len), 1);
        assert_eq!(board.planet_units.get(&planets[1]).map_or(0, Vec::len), 0);
    }

    #[test]
    fn an_enforced_travel_ban_needs_the_map_to_know_what_is_beside_a_wormhole() {
        let mut state = game(&["a"]);
        let effect = run_with(&mut state, None, "travel_ban", AGAINST, &[]);

        assert!(
            matches!(effect, Effect::Deferred { .. }),
            "without a map there is no 'beside': {effect:?}"
        );
    }

    #[test]
    fn an_enforced_travel_ban_clears_the_wormholes_and_their_neighbours() {
        // The wormhole sits on a ring seat, so the seat *across* the ring is on the map but
        // neither a wormhole nor beside one. A hub centred on the wormhole would have no such
        // seat at all — every ring seat touches the centre — and the test would prove nothing.
        let content = ContentStore::embedded();
        let sources = ti4_model::content_types::POK;
        let wormhole = ti4_content::galaxy::all_systems(content, sources)
            .iter()
            .find(|(_, system)| !system.wormholes().is_empty() && !system.is_hyperlane())
            .map(|(id, _)| (*id).to_owned())
            .expect("the corpus has a wormhole system");

        let hub = crate::fixtures::hub_with_outer(&wormhole);
        let far = hub.across(&wormhole);
        assert!(
            !hub.galaxy.adjacent(&wormhole).contains(far.as_str()),
            "the far seat must not touch the wormhole"
        );

        let planet_in = |system: &str| {
            ti4_content::galaxy::planets_in(content, system, sources)
                .first()
                .map(|planet| ti4_model::id::PlanetId::new(planet.id()))
        };
        let (Some(beside_planet), Some(far_planet)) = (planet_in(&hub.centre), planet_in(&far))
        else {
            return; // those seats hold no planet to garrison
        };

        let mut state = game(&["a"]);
        let beside = ti4_model::id::SystemId::new(hub.centre.clone());
        let far_system = ti4_model::id::SystemId::new(far.clone());
        crate::fixtures::put_on_planet(&mut state, &beside, &beside_planet, "pds", &a(), 1);
        crate::fixtures::put_on_planet(&mut state, &far_system, &far_planet, "pds", &a(), 1);

        run_with(&mut state, Some(&hub.galaxy), "travel_ban", AGAINST, &[]);

        assert_eq!(
            state
                .system_state(&beside)
                .planet_units
                .get(&beside_planet)
                .map_or(0, Vec::len),
            0,
            "a PDS beside a wormhole is destroyed"
        );
        assert_eq!(
            state
                .system_state(&far_system)
                .planet_units
                .get(&far_planet)
                .map_or(0, Vec::len),
            1,
            "one across the map from it is not"
        );
    }

    #[test]
    fn shared_research_puts_a_token_in_each_home_system() {
        let mut state = game(&["a", "b"]);
        let home = ti4_model::id::SystemId::new("some_home");
        state.player_mut(&a()).unwrap().home_system = Some(home.clone());

        run_with(&mut state, None, "shared_research", AGAINST, &[]);

        assert!(state.system_state(&home).command_tokens.contains(&a()));
    }

    #[test]
    fn miscount_disclosed_takes_the_named_law_off_the_table() {
        let mut state = game(&["a"]);
        state.enact_law("regulations", "for");
        state.enact_law("sanctions", "for");

        let effect = run_with(&mut state, None, "miscount", "sanctions", &[]);

        assert!(
            !state.laws.contains_key("sanctions"),
            "the named law is repealed"
        );
        assert!(state.laws.contains_key("regulations"), "and only that one");
        assert!(
            matches!(effect, Effect::Deferred { .. }),
            "the re-vote it calls for needs the agenda window: {effect:?}"
        );
    }

    #[test]
    fn compensated_disarmament_pays_the_controller_for_the_garrison() {
        let (mut state, planet, player) = garrison(3);
        state.player_mut(&player).unwrap().trade_goods = 0;

        run(&mut state, "disarmament", planet.as_str(), &no_votes());

        assert_eq!(on_planet(&state, &planet), 0, "the garrison was disarmed");
        assert_eq!(
            state.player(&player).unwrap().trade_goods,
            3,
            "and paid for, one trade good each"
        );
    }

    #[test]
    fn swords_to_plowshares_rounds_the_losses_up() {
        // Three infantry lose two, not one: rounding down would leave a garrison standing that
        // the card removes.
        let (mut state, planet, player) = garrison(3);
        state.player_mut(&player).unwrap().trade_goods = 0;

        run(&mut state, "plowshares", FOR, &no_votes());

        assert_eq!(on_planet(&state, &planet), 1);
        assert_eq!(state.player(&player).unwrap().trade_goods, 2);
    }

    #[test]
    fn swords_to_plowshares_arms_everyone_when_it_fails() {
        // The Against half is the opposite card. Doing nothing here would make voting it down
        // free, when it is the half that puts troops on the board.
        let (mut state, planet, _) = garrison(0);

        run(&mut state, "plowshares", AGAINST, &no_votes());

        assert_eq!(
            on_planet(&state, &planet),
            1,
            "one infantry on each planet held"
        );
    }

    #[test]
    fn arms_reduction_keeps_two_dreadnoughts_and_four_cruisers() {
        let mut state = crate::fixtures::game(&["a"]);
        let player = PlayerId::new("a");
        let (system, _) = crate::fixtures::a_placed_planet();
        crate::fixtures::put(&mut state, &system, "dreadnought", &player, 5);
        crate::fixtures::put(&mut state, &system, "cruiser", &player, 6);
        crate::fixtures::put(&mut state, &system, "carrier", &player, 3);

        run(&mut state, "arms_reduction", FOR, &no_votes());

        let count = |base: &str| {
            let types = ti4_content::units::catalogue(
                ti4_content::ContentStore::embedded(),
                ti4_model::content_types::POK,
            );
            state
                .system_state(&system)
                .units
                .iter()
                .filter(|unit| {
                    types
                        .get(unit.type_id.as_str())
                        .is_some_and(|kind| kind.base_type() == base)
                })
                .count()
        };
        assert_eq!(count("dreadnought"), 2);
        assert_eq!(count("cruiser"), 4);
        assert_eq!(count("carrier"), 3, "the card names two hulls, not three");
    }

    #[test]
    fn arms_reduction_defers_its_against_half_rather_than_claiming_it() {
        let mut state = crate::fixtures::game(&["a"]);
        let effect = run(&mut state, "arms_reduction", AGAINST, &no_votes());

        assert!(
            matches!(effect, Effect::Deferred { .. }),
            "the Against half needs an exhaust this cannot ask for: {effect:?}"
        );
    }

    #[test]
    fn conventions_of_war_burns_the_hands_of_those_who_voted_against() {
        let mut state = crate::fixtures::game(&["a", "b"]);
        for player in [PlayerId::new("a"), PlayerId::new("b")] {
            state.player_mut(&player).unwrap().action_cards =
                vec![ti4_model::id::ActionCardId::new("card")];
        }
        let ballot = Ballot {
            votes: [(PlayerId::new("a"), AGAINST.to_owned())]
                .into_iter()
                .collect(),
            counts: std::collections::BTreeMap::new(),
        };

        run(&mut state, "conventions", AGAINST, &ballot);

        assert!(
            state
                .player(&PlayerId::new("a"))
                .unwrap()
                .action_cards
                .is_empty(),
            "a voted against and lost the hand"
        );
        assert_eq!(
            state
                .player(&PlayerId::new("b"))
                .unwrap()
                .action_cards
                .len(),
            1,
            "b did not vote against and keeps it"
        );
    }

    #[test]
    fn conventions_of_war_costs_nothing_when_it_passes() {
        let mut state = crate::fixtures::game(&["a"]);
        state.player_mut(&PlayerId::new("a")).unwrap().action_cards =
            vec![ti4_model::id::ActionCardId::new("card")];
        let ballot = Ballot {
            votes: [(PlayerId::new("a"), AGAINST.to_owned())]
                .into_iter()
                .collect(),
            counts: std::collections::BTreeMap::new(),
        };

        run(&mut state, "conventions", FOR, &ballot);

        assert_eq!(
            state
                .player(&PlayerId::new("a"))
                .unwrap()
                .action_cards
                .len(),
            1,
            "the law passed, so the Against half never happens"
        );
    }

    #[test]
    fn core_mining_takes_exactly_one_infantry() {
        let (mut state, planet, _) = garrison(3);

        run(&mut state, "core_mining", planet.as_str(), &no_votes());

        assert_eq!(
            on_planet(&state, &planet),
            2,
            "one infantry paid for the seam"
        );
    }

    #[test]
    fn a_demilitarized_zone_clears_everything_on_the_planet() {
        let (mut state, planet, player) = garrison(2);
        let (system, _) = crate::fixtures::a_placed_planet();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "pds", &player, 1);

        run(
            &mut state,
            "demilitarized_zone",
            planet.as_str(),
            &no_votes(),
        );

        assert_eq!(on_planet(&state, &planet), 0, "structures go too");
    }

    #[test]
    fn the_holy_planet_scores_for_whoever_holds_it() {
        let (mut state, planet, player) = garrison(0);
        let before = state.player(&player).unwrap().victory_points;

        run(
            &mut state,
            "holy_planet_of_ixth",
            planet.as_str(),
            &no_votes(),
        );

        assert_eq!(
            state.player(&player).unwrap().victory_points,
            before + 1,
            "the controller takes the point at once"
        );
    }

    #[test]
    fn publicize_schematics_only_burns_hands_that_hold_a_war_sun() {
        let content = ti4_content::ContentStore::embedded();
        let war_sun = content
            .from_sources(
                ti4_model::content_types::ContentType::Technologies,
                ti4_model::content_types::POK,
            )
            .find(|record| {
                record
                    .text("name")
                    .is_some_and(|name| name.to_ascii_lowercase().contains("war sun"))
            })
            .and_then(|record| record.text("alias").map(ToOwned::to_owned));
        let Some(war_sun) = war_sun else {
            panic!("the corpus has a war sun technology");
        };

        let mut state = crate::fixtures::game(&["a", "b"]);
        for player in [PlayerId::new("a"), PlayerId::new("b")] {
            state.player_mut(&player).unwrap().action_cards =
                vec![ti4_model::id::ActionCardId::new("card")];
        }
        state
            .player_mut(&PlayerId::new("a"))
            .unwrap()
            .technologies
            .insert(ti4_model::id::TechnologyId::new(war_sun));

        run(&mut state, "schematics", AGAINST, &no_votes());

        assert!(
            state
                .player(&PlayerId::new("a"))
                .unwrap()
                .action_cards
                .is_empty(),
            "a owns the technology"
        );
        assert_eq!(
            state
                .player(&PlayerId::new("b"))
                .unwrap()
                .action_cards
                .len(),
            1,
            "b does not"
        );
    }

    #[test]
    fn every_registered_agenda_resolves_to_something() {
        // A registered alias whose match has no arm falls through to Unresolved, which would
        // read as a coverage gap in a card that is listed as covered.
        let mut state = crate::fixtures::game(&["a", "b"]);
        for alias in registered_aliases() {
            let effect = run(&mut state, alias, FOR, &no_votes());
            assert!(
                !matches!(effect, Effect::Unresolved { .. }),
                "{alias} is registered but has no arm"
            );
        }
    }

    use std::collections::BTreeMap;

    use ti4_content::ContentStore;

    use super::*;
    use crate::fixtures::game;

    fn a() -> PlayerId {
        PlayerId::new("a")
    }
    fn b() -> PlayerId {
        PlayerId::new("b")
    }

    fn ballot_for(voters: &[PlayerId]) -> Ballot {
        Ballot {
            votes: voters
                .iter()
                .map(|player| (player.clone(), FOR.to_owned()))
                .collect(),
            counts: BTreeMap::from([(FOR.to_owned(), 1)]),
        }
    }

    #[test]
    fn an_unregistered_agenda_reports_itself_unresolved() {
        // The same design every other registry here uses: the gap is announced, not hidden.
        let mut state = game(&["a"]);
        let effect = resolve(
            &mut state,
            ContentStore::embedded(),
            "not_an_agenda",
            FOR,
            &Ballot::default(),
        );
        assert!(matches!(effect, Effect::Unresolved { .. }));
    }

    #[test]
    fn economic_equality_empties_the_supply_before_paying() {
        // On Against the card is purely destructive, which only holds if the wipe comes first.
        let mut state = game(&["a", "b"]);
        state.player_mut(&a()).unwrap().trade_goods = 9;
        state.player_mut(&b()).unwrap().trade_goods = 2;

        resolve(
            &mut state,
            ContentStore::embedded(),
            "economic_equality",
            AGAINST,
            &Ballot::default(),
        );

        assert_eq!(state.player(&a()).unwrap().trade_goods, 0);
        assert_eq!(state.player(&b()).unwrap().trade_goods, 0);
    }

    #[test]
    fn economic_equality_pays_five_each_on_for() {
        let mut state = game(&["a", "b"]);
        state.player_mut(&a()).unwrap().trade_goods = 9;

        resolve(
            &mut state,
            ContentStore::embedded(),
            "economic_equality",
            FOR,
            &Ballot::default(),
        );

        assert_eq!(state.player(&a()).unwrap().trade_goods, 5, "not 9, not 14");
        assert_eq!(state.player(&b()).unwrap().trade_goods, 5);
    }

    #[test]
    fn mutiny_rewards_or_punishes_the_players_who_voted_for() {
        // Read from the ballot, not the outcome: who voted which way is the whole card.
        let mut state = game(&["a", "b"]);
        let ballot = ballot_for(&[a()]);

        resolve(&mut state, ContentStore::embedded(), "mutiny", FOR, &ballot);
        assert_eq!(state.player(&a()).unwrap().victory_points, 1);
        assert_eq!(
            state.player(&b()).unwrap().victory_points,
            0,
            "b did not vote for it"
        );

        resolve(
            &mut state,
            ContentStore::embedded(),
            "mutiny",
            AGAINST,
            &ballot,
        );
        assert_eq!(
            state.player(&a()).unwrap().victory_points,
            0,
            "the same voters lose one on Against"
        );
    }

    #[test]
    fn victory_points_never_go_below_zero() {
        // 98.4a caps the top; a loss must not take a player under.
        let mut state = game(&["a"]);
        resolve(
            &mut state,
            ContentStore::embedded(),
            "mutiny",
            AGAINST,
            &ballot_for(&[a()]),
        );
        assert_eq!(state.player(&a()).unwrap().victory_points, 0);
    }

    #[test]
    fn seed_of_an_empire_gives_the_point_to_the_leader_on_for() {
        let mut state = game(&["a", "b"]);
        state.player_mut(&b()).unwrap().victory_points = 4;

        resolve(
            &mut state,
            ContentStore::embedded(),
            "seed_empire",
            FOR,
            &Ballot::default(),
        );

        assert_eq!(state.player(&b()).unwrap().victory_points, 5);
        assert_eq!(state.player(&a()).unwrap().victory_points, 0);
    }

    #[test]
    fn seed_of_an_empire_gives_it_to_the_trailer_on_against() {
        let mut state = game(&["a", "b"]);
        state.player_mut(&b()).unwrap().victory_points = 4;

        resolve(
            &mut state,
            ContentStore::embedded(),
            "seed_empire",
            AGAINST,
            &Ballot::default(),
        );

        assert_eq!(state.player(&a()).unwrap().victory_points, 1);
        assert_eq!(state.player(&b()).unwrap().victory_points, 4);
    }

    #[test]
    fn a_tie_is_the_speakers_decision_not_a_guess() {
        // 8.18: resolving the outcome is the speaker's job. The engine must not pick — a tie
        // handed to whoever sorts first is a rule nobody wrote.
        let mut state = game(&["a", "b"]);
        state.speaker = a();

        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut table =
            crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new([
                "b".to_owned()
            ])));
        let mut ctx = crate::choice::Resolving {
            content: ContentStore::embedded(),
            sources: ti4_model::content_types::POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };

        resolve_with(
            &mut state,
            &mut ctx,
            None,
            "seed_empire",
            FOR,
            &Ballot::default(),
        );

        assert_eq!(
            state.player(&b()).unwrap().victory_points,
            1,
            "the speaker named b, so b takes it"
        );
        assert_eq!(state.player(&a()).unwrap().victory_points, 0);
    }

    #[test]
    fn judicial_abolishment_discards_the_law_it_elected() {
        // The outcome names a law, not For or Against. A repeal ignoring it would discard
        // whichever law happened to sort first.
        let mut state = game(&["a"]);
        state.enact_law("regulations", "for");
        state.enact_law("sanctions", "for");

        resolve(
            &mut state,
            ContentStore::embedded(),
            "abolishment",
            "sanctions",
            &Ballot::default(),
        );

        assert!(crate::laws::active(&state, "regulations"), "untouched");
        assert!(!crate::laws::active(&state, "sanctions"), "discarded");
    }

    #[test]
    fn new_constitution_clears_the_table_only_on_for() {
        let mut state = game(&["a"]);
        state.enact_law("regulations", "for");
        state.enact_law("sanctions", "for");

        resolve(
            &mut state,
            ContentStore::embedded(),
            "constitution",
            AGAINST,
            &Ballot::default(),
        );
        assert_eq!(
            crate::laws::in_play(&state).len(),
            2,
            "Against changes nothing"
        );

        resolve(
            &mut state,
            ContentStore::embedded(),
            "constitution",
            FOR,
            &Ballot::default(),
        );
        assert!(crate::laws::in_play(&state).is_empty());
    }

    #[test]
    fn incentive_program_reveals_an_objective() {
        let mut state = game(&["a"]);
        let before = state.revealed_objectives.len();

        resolve(
            &mut state,
            ContentStore::embedded(),
            "incentive",
            FOR,
            &Ballot::default(),
        );

        assert_eq!(state.revealed_objectives.len(), before + 1);
    }

    #[test]
    fn unconventional_measures_pays_or_purges_the_for_voters() {
        // It acts on who voted, not on what won.
        let mut state = game(&["a", "b"]);
        state.action_card_deck = (0..4)
            .map(|n| ti4_model::id::ActionCardId::new(format!("c{n}")))
            .collect();
        let ballot = ballot_for(&[a()]);

        resolve(
            &mut state,
            ContentStore::embedded(),
            "unconventional",
            FOR,
            &ballot,
        );
        assert_eq!(state.player(&a()).unwrap().action_cards.len(), 2);
        assert!(
            state.player(&b()).unwrap().action_cards.is_empty(),
            "b did not vote for it"
        );

        resolve(
            &mut state,
            ContentStore::embedded(),
            "unconventional",
            AGAINST,
            &ballot,
        );
        assert!(
            state.player(&a()).unwrap().action_cards.is_empty(),
            "the same voters lose their hand on Against"
        );
    }

    #[test]
    fn every_registered_alias_is_a_real_agenda() {
        for alias in registered_aliases() {
            assert!(
                ti4_content::ContentStore::embedded()
                    .get(ti4_model::content_types::ContentType::Agendas, alias)
                    .is_some(),
                "{alias} is not an agenda the corpus knows"
            );
        }
    }

    #[test]
    fn the_unimplemented_agendas_are_reported() {
        // Full coverage: every agenda the corpus knows is registered, so the gap list is empty.
        let missing = unimplemented(ContentStore::embedded(), POK);
        assert!(
            missing.is_empty(),
            "agendas with no effect: {missing:?}"
        );
    }

    /// A law whose whole effect is a standing rule still reports Resolved, not Unresolved:
    /// the vote recorded the outcome, and the standing rule lives in the subsystem that owns it.
    #[test]
    fn availability_only_laws_resolve_the_vote() {
        for alias in [
            "committee",
            "arbiter",
            "minister_commerce",
            "minister_industry",
            "minister_peace",
            "minister_sciences",
            "minister_war",
            "prophecy",
            "crown_of_thalnos",
            "checks",
            "covert",
            "crisis",
        ] {
            for outcome in [FOR, AGAINST] {
                let mut state = crate::fixtures::game(&["a", "b"]);
                let effect = run(&mut state, alias, outcome, &no_votes());
                assert!(
                    matches!(effect, Effect::Resolved { .. }),
                    "{alias} on {outcome} must resolve the vote"
                );
            }
        }
    }
}
