//! Pure projections transforming engine state and choices into redacted client views, and the
//! protocol messages that carry them.

mod messages;

pub use messages::*;

use std::collections::{BTreeMap, BTreeSet};
use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_engine::choice::Choice;
use ti4_model::content_types::{ContentType, POK, SourceSet};
use ti4_model::id::{ObjectiveId, PlanetId, PlayerId};
use ti4_model::state::{GameState, Player};
use ti4_model::view::{HIDDEN, redact_player, view_for};

use serde::{Deserialize, Serialize};

use crate::protocol::status::{PublicTurnStatus, ViewerRole};
use crate::protocol::view::{
    BoardTileView, BoardView, GameView, ObjectiveProgressView, PlacedUnitView, PlanetValueBoost,
    PlanetView, PlayerView, SystemView, TableView,
};

/// Projects one player record that has already passed through the model's redaction boundary.
#[must_use]
pub fn project_player_view(player: &Player) -> PlayerView {
    PlayerView {
        id: player.id.clone(),
        faction: player.faction.clone(),
        victory_points: player.victory_points,
        trade_goods: player.trade_goods,
        commodities: player.commodities,
        tactic_tokens: player.tactic_tokens,
        fleet_tokens: player.fleet_tokens,
        strategic_tokens: player.strategic_tokens,
        passed: player.passed,
        strategy_cards: player.strategy_cards.clone(),
        exhausted_strategy_cards: player.exhausted_strategy_cards.clone(),
        technologies: player.technologies.clone(),
        exhausted_technologies: player.exhausted_technologies.clone(),
        relics: player.relics.clone(),
        exhausted_relics: player.exhausted_relics.clone(),
        exhausted_legendary: player.exhausted_legendary.clone(),
        relic_fragments: player
            .relic_fragments
            .iter()
            .filter(|(_, count)| **count > 0)
            .map(|(deck, count)| (deck.clone(), *count))
            .collect(),
        exploration_cards: player.exploration_cards.clone(),
        action_cards_count: player.action_cards.len(),
        secret_objectives_count: player.secret_objectives.len(),
        held_action_cards: player
            .action_cards
            .iter()
            .filter(|card| card.as_str() != HIDDEN)
            .cloned()
            .collect(),
        held_secret_objectives: player
            .secret_objectives
            .iter()
            .filter(|objective| objective.as_str() != HIDDEN)
            .cloned()
            .collect(),
        scored_secret_objectives: player.plot_objectives.iter().cloned().collect(),
        secret_objective_progress: BTreeMap::new(),
        leaders: player.leaders.clone(),
        held_foreign_tokens: Vec::new(),
        agenda_peek: Vec::new(),
    }
}

/// Helper to construct public combat view if a space combat is detected
#[must_use]
pub fn project_combat_view(
    state: &GameState,
    pending_choice: Option<&Choice>,
    dice_rolls: &[crate::protocol::view::CombatDieRoll],
) -> Option<crate::protocol::view::CombatView> {
    project_combat_view_for(state, pending_choice, dice_rolls, None)
}

/// [`project_combat_view`] as `viewer` sees it. While a reaction window is open, only the asked
/// seat learns whose window it is: everybody else gets a view as if no question were pending.
#[must_use]
pub fn project_combat_view_for(
    state: &GameState,
    pending_choice: Option<&Choice>,
    dice_rolls: &[crate::protocol::view::CombatDieRoll],
    viewer: Option<&ViewerRole>,
) -> Option<crate::protocol::view::CombatView> {
    let pending_choice = pending_choice.filter(|choice| {
        !is_reaction_window(choice) || viewer.is_some_and(|v| v.is_actor(&choice.player))
    });
    let choice_ctx = pending_choice.and_then(|c| c.context.as_ref());
    let (sys_id, attacker, defender) = state.active_space_combat.clone()?;
    state.board.get(&sys_id)?;

    let active_player = pending_choice.map(|c| c.player.clone());
    let stage = choice_ctx.map(|ctx| ctx.subtype.clone());
    let attacker_hits = state.combat_round_hits.get(&attacker).copied();
    let defender_hits = state.combat_round_hits.get(&defender).copied();

    let dice_rolls: Vec<crate::protocol::view::CombatDieRoll> = if !dice_rolls.is_empty() {
        dice_rolls.to_vec()
    } else {
        state
            .combat_round_dice
            .iter()
            .map(|r| crate::protocol::view::CombatDieRoll {
                player: Some(r.player.clone()),
                unit: r.unit.clone(),
                roll: r.roll,
                target: r.target,
                hit: r.hit,
            })
            .collect()
    };

    let hits_to_assign = choice_ctx
        .and_then(|ctx| ctx.outstanding.first())
        .map(|o| usize::try_from(o.amount).unwrap_or(0))
        .or_else(|| {
            active_player.as_ref().and_then(|p| {
                let produced = if *p == attacker {
                    defender_hits
                } else if *p == defender {
                    attacker_hits
                } else {
                    None
                }?;
                // The HITS_TO_ASSIGN window is still open after Shields Holding resolves.
                // The round's produced-hit total stays unchanged, but the player's pending
                // cancellation grant must already be reflected in the live hit counter.
                let cancelled = choice_ctx
                    .filter(|ctx| {
                        ctx.subtype.starts_with("reaction_")
                            && ctx.subtype.ends_with("HITS_TO_ASSIGN")
                    })
                    .map_or(0, |_| ti4_engine::combat::cancellable_hits(state, p));
                Some((produced as usize).saturating_sub(cancelled))
            })
        });

    Some(crate::protocol::view::CombatView {
        system_id: sys_id,
        round: state.combat_presentation.round.max(1),
        battle_seq: state.combat_presentation.battle_seq,
        phase: state.combat_presentation.phase.clone(),
        round_start: state
            .combat_presentation
            .round_start
            .iter()
            .map(|u| PlacedUnitView {
                unit_type: u.type_id.clone(),
                owner: u.owner.clone(),
                planet: None,
                damaged: u.sustained_damage,
                galvanized: u.galvanized,
            })
            .collect(),
        barrage_start: state
            .combat_presentation
            .barrage_start
            .iter()
            .map(|u| PlacedUnitView {
                unit_type: u.type_id.clone(),
                owner: u.owner.clone(),
                planet: None,
                damaged: u.sustained_damage,
                galvanized: u.galvanized,
            })
            .collect(),
        barrage_hits: state.combat_presentation.barrage_hits.clone(),
        barrage_dice: state
            .combat_presentation
            .barrage_dice
            .iter()
            .map(|r| crate::protocol::view::CombatDieRoll {
                player: Some(r.player.clone()),
                unit: r.unit.clone(),
                roll: r.roll,
                target: r.target,
                hit: r.hit,
            })
            .collect(),
        remaining_hits: {
            let mut remaining = state.combat_presentation.remaining_hits.clone();
            if choice_ctx.is_some_and(|ctx| {
                ctx.subtype.starts_with("reaction_") && ctx.subtype.ends_with("HITS_TO_ASSIGN")
            }) {
                if let Some(player) = active_player.as_ref() {
                    if let Some(hits) = remaining.get_mut(player) {
                        *hits = hits
                            .saturating_sub(ti4_engine::combat::cancellable_hits(state, player));
                    }
                }
            }
            remaining
        },
        attacker,
        defender,
        active_player,
        stage,
        hits_to_assign,
        attacker_hits,
        defender_hits,
        dice_rolls,
    })
}

/// Projects the board systems, planets, and units without map tiles.
#[must_use]
pub fn project_board_view(state: &GameState) -> BoardView {
    project_board_view_with_map(state, &[])
}

/// Projects the board systems, planets, and units.
#[must_use]
pub fn project_board_view_with_map(state: &GameState, map_tiles: &[BoardTileView]) -> BoardView {
    project_board_view_full(state, map_tiles, None, &[], None)
}

/// Projects the full board view with combat and dice information.
#[must_use]
pub fn project_board_view_full(
    state: &GameState,
    map_tiles: &[BoardTileView],
    pending_choice: Option<&Choice>,
    dice_rolls: &[crate::protocol::view::CombatDieRoll],
    viewer: Option<&ViewerRole>,
) -> BoardView {
    let mut systems = BTreeMap::new();

    for (sys_id, sys_state) in &state.board {
        let mut planet_ids: BTreeSet<PlanetId> = sys_state.planet_control.keys().cloned().collect();
        planet_ids.extend(sys_state.planet_units.keys().cloned());
        planet_ids.extend(sys_state.purged_planets.iter().cloned());

        let mut planets = BTreeMap::new();
        for planet_id in planet_ids {
            planets.insert(
                planet_id.clone(),
                PlanetView {
                    planet_id: planet_id.clone(),
                    controlled_by: sys_state.planet_control.get(&planet_id).cloned(),
                    exhausted: state.exhausted_planets.contains(&planet_id),
                    attachments: state
                        .planet_attachments
                        .get(&planet_id)
                        .cloned()
                        .unwrap_or_default(),
                    value_boosts: ti4_engine::production::planet_value_boosts(
                        state,
                        ti4_content::ContentStore::embedded(),
                        &planet_id,
                    )
                    .into_iter()
                    .map(|(source, r, i)| PlanetValueBoost {
                        source,
                        resources: i32::try_from(r).unwrap_or(0),
                        influence: i32::try_from(i).unwrap_or(0),
                    })
                    .collect(),
                },
            );
        }

        let mut units = Vec::new();
        // Space units
        for u in &sys_state.units {
            units.push(PlacedUnitView {
                unit_type: u.type_id.clone(),
                owner: u.owner.clone(),
                planet: None,
                damaged: u.sustained_damage,
                galvanized: u.galvanized,
            });
        }
        // Planet units
        for (p_id, p_units) in &sys_state.planet_units {
            for u in p_units {
                units.push(PlacedUnitView {
                    unit_type: u.type_id.clone(),
                    owner: u.owner.clone(),
                    planet: Some(p_id.clone()),
                    damaged: u.sustained_damage,
                    galvanized: u.galvanized,
                });
            }
        }

        systems.insert(
            sys_id.clone(),
            SystemView {
                system_id: sys_id.clone(),
                command_tokens: sys_state.command_tokens.clone(),
                planets,
                units,
            },
        );
    }

    let invasion = state.active_invasion.as_ref().and_then(|active| {
        let content = ti4_content::ContentStore::embedded();
        let types = ti4_content::units::catalogue(content, ti4_model::content_types::POK);
        let space = state.system_state(&active.system);
        let mut odds_context = BTreeMap::new();
        if let Some(system) = systems.get(&active.system) {
            for planet in system.planets.keys() {
                let ground_force_types: Vec<String> = space
                    .on_planet(planet)
                    .iter()
                    .filter(|unit| {
                        types
                            .get(unit.type_id.as_str())
                            .is_some_and(ti4_content::units::UnitType::is_ground_force)
                    })
                    .map(|unit| unit.type_id.to_string())
                    .collect();
                let owners: BTreeSet<PlayerId> = space
                    .on_planet(planet)
                    .iter()
                    .filter(|unit| {
                        unit.owner != active.invader
                            && types
                                .get(unit.type_id.as_str())
                                .is_some_and(ti4_content::units::UnitType::is_ground_force)
                    })
                    .map(|unit| unit.owner.clone())
                    .collect();
                let opponent = space
                    .planet_control
                    .get(planet)
                    .filter(|owner| owners.contains(*owner))
                    .cloned()
                    .or_else(|| owners.iter().next().cloned());
                let mut additional_guns = BTreeMap::new();
                let mut available = true;
                if ti4_engine::entropic_scars::abilities_usable(
                    content,
                    ti4_model::content_types::POK,
                    &active.system,
                    Some(&active.system),
                ) {
                    for unit in space
                        .on_planet(planet)
                        .iter()
                        .filter(|unit| unit.owner != active.invader)
                    {
                        let Some(kind) = types.get(unit.type_id.as_str()) else {
                            continue;
                        };
                        if kind.space_cannon_hits_on().is_none() || kind.space_cannon_dice() == 0 {
                            continue;
                        }
                        if kind.base_type() == "pds"
                            && state.players.iter().any(|seat| {
                                seat.id != unit.owner
                                    && seat.disable_invasion.contains(&state.activation_seq)
                            })
                        {
                            if opponent.as_ref() == Some(&unit.owner) && kind.is_ground_force() {
                                available = false;
                            }
                            continue;
                        }
                        if opponent.as_ref() == Some(&unit.owner) && kind.is_ground_force() {
                            continue;
                        }
                        *additional_guns.entry(unit.type_id.to_string()).or_insert(0) += 1;
                    }
                } else if space.on_planet(planet).iter().any(|unit| {
                    opponent.as_ref() == Some(&unit.owner)
                        && types.get(unit.type_id.as_str()).is_some_and(|kind| {
                            kind.is_ground_force() && kind.space_cannon_hits_on().is_some()
                        })
                }) {
                    available = false;
                }
                let mut harrow_units = BTreeMap::new();
                if ti4_engine::faction_abilities::has(state, content, &active.invader, "harrow")
                    && ti4_engine::invasion::bombardable(
                        state,
                        content,
                        ti4_model::content_types::POK,
                        &active.system,
                        planet,
                        &active.invader,
                    )
                {
                    for unit in space.units_of(&active.invader) {
                        if types
                            .get(unit.type_id.as_str())
                            .is_some_and(|kind| kind.has_bombardment() && kind.bombard_dice() > 0)
                        {
                            *harrow_units.entry(unit.type_id.to_string()).or_insert(0) += 1;
                        }
                    }
                }
                odds_context.insert(
                    planet.clone(),
                    crate::protocol::view::InvasionOddsContext {
                        opponent,
                        available,
                        ground_force_types,
                        additional_guns,
                        harrow_units,
                    },
                );
            }
        }

        // H2: Filter out boring invasions (no defenders) from spectator views
        let has_defenders = odds_context.values().any(|ctx| ctx.opponent.is_some());
        if !has_defenders && viewer.is_some() && !viewer.as_ref().unwrap().is_actor(&active.invader)
        {
            // Hide this invasion for non-invading players when no defenders are present
            return None;
        }

        Some(crate::protocol::view::InvasionView {
            system_id: active.system.clone(),
            invasion_seq: active.seq,
            invader: active.invader.clone(),
            phase: active.phase.clone(),
            planets: systems.get(&active.system).map_or_else(
                Vec::new,
                |system: &crate::protocol::view::SystemView| {
                    system.planets.keys().cloned().collect()
                },
            ),
            current_planet: active.planet.clone(),
            defender: active.defender.clone(),
            ground_round: active.ground_round,
            odds_context,
            last_step: active.last_step.as_ref().map(|step| {
                let placed = |unit: &ti4_model::units::Unit| PlacedUnitView {
                    unit_type: unit.type_id.clone(),
                    owner: unit.owner.clone(),
                    planet: Some(step.planet.clone()),
                    damaged: unit.sustained_damage,
                    galvanized: unit.galvanized,
                };
                crate::protocol::view::InvasionStepView {
                    planet: step.planet.clone(),
                    kind: step.kind.clone(),
                    round: step.round,
                    before: step.before.iter().map(placed).collect(),
                    after: step.after.iter().map(placed).collect(),
                    dice: step.dice.clone(),
                    hits: step.hits.clone(),
                    harrow_hits: step.harrow_hits,
                }
            }),
        })
    });
    BoardView {
        systems,
        active_system: state.active_system.clone(),
        map_tiles: map_tiles.to_vec(),
        combat: project_combat_view_for(state, pending_choice, dice_rolls, viewer),
        invasion,
    }
}

fn canonical_objective_alias<'a>(content: &'a ContentStore, raw: &'a str) -> Option<&'a str> {
    if content.get(ContentType::PublicObjectives, raw).is_some() {
        return Some(raw);
    }
    for record in content.records(ContentType::PublicObjectives) {
        if let Some(name) = record.text("name") {
            let slug = name.to_ascii_lowercase().replace(' ', "_");
            if slug == raw {
                return record.id();
            }
        }
    }
    None
}

/// The placed galaxy the static map tiles describe, for map-shaped objective counts.
///
/// Only tiles on the hex grid count: the Fracture (`special_area: "fracture"`) is drawn beside
/// the map at made-up coordinates and is outside the PoK catalogue, and an off-map Wormhole Nexus
/// is parked at the origin, on top of Mecatol Rex. Feeding either to [`Galaxy::placed`] used to
/// fail the whole build (or overwrite Mecatol), which silently dropped every map-shaped
/// objective's progress — Push Boundaries, Intimidate Council, Learn the Secrets and the rest.
#[must_use]
pub fn placed_galaxy(map_tiles: &[BoardTileView]) -> Option<Galaxy> {
    let mut taken = std::collections::BTreeSet::new();
    let tiles: Vec<_> = map_tiles
        .iter()
        .filter(|tile| tile.special_area.as_deref() != Some("fracture"))
        .filter(|tile| {
            // On-map tiles first, so a parked nexus never displaces the tile it overlaps.
            tile.special_area.is_none()
                || !map_tiles.iter().any(|other| {
                    other.special_area.is_none() && (other.q, other.r) == (tile.q, tile.r)
                })
        })
        .filter(|tile| taken.insert((tile.q, tile.r)))
        .map(|tile| {
            (
                tile.system_id.as_str(),
                ti4_model::hex::Hex::new(tile.q, tile.r),
            )
        })
        .collect();
    if tiles.is_empty() {
        return None;
    }
    Galaxy::placed(ContentStore::embedded(), &tiles, POK).ok()
}

/// Exact progress toward `seat`'s own held secrets, by the engine helper that backs their
/// requirements. Event-conditioned secrets and redacted placeholders get no entry.
///
/// Privacy: callers pass only the viewing seat, and `state` is already that seat's redacted
/// view, so nothing here reads another seat's hidden hand.
#[must_use]
pub fn project_secret_progress(
    state: &GameState,
    seat: &PlayerId,
    map_tiles: &[BoardTileView],
) -> BTreeMap<ti4_model::id::SecretObjectiveId, ObjectiveProgressView> {
    let Some(player) = state.player(seat) else {
        return BTreeMap::new();
    };
    let galaxy = placed_galaxy(map_tiles);
    let position = ti4_engine::secrets::Position {
        state,
        content: ContentStore::embedded(),
        sources: POK,
        player: seat,
        galaxy: galaxy.as_ref(),
        imagined: ti4_engine::objectives::Imagined::NONE,
    };
    player
        .secret_objectives
        .iter()
        .filter(|alias| alias.as_str() != HIDDEN)
        .filter_map(|alias| {
            let prog = ti4_engine::secrets::progress(alias, &position)?;
            // Which systems count, so the holder can see where their 2 of 3 are.
            let systems = if alias.as_str() == "lsc" {
                ti4_engine::secrets::ship_systems_beside_anomaly(&position).unwrap_or_default()
            } else {
                Vec::new()
            };
            Some((
                alias.clone(),
                ObjectiveProgressView {
                    have: u32::try_from(prog.have).unwrap_or(0),
                    threshold: u32::try_from(prog.threshold).unwrap_or(0),
                    satisfied: prog.satisfied(),
                    systems,
                    ..ObjectiveProgressView::default()
                },
            ))
        })
        .collect()
}

/// Push Boundaries' per-neighbour planet comparison for `position.player`, in seating order.
fn neighbour_progress_views(
    position: &ti4_engine::objectives::Position<'_>,
) -> Vec<crate::protocol::view::NeighbourProgressView> {
    ti4_engine::objectives::neighbour_planet_counts(position)
        .unwrap_or_default()
        .into_iter()
        .map(|row| crate::protocol::view::NeighbourProgressView {
            ahead: row.ahead(),
            mine: u32::try_from(row.mine).unwrap_or(0),
            theirs: u32::try_from(row.theirs).unwrap_or(0),
            player: row.neighbour,
        })
        .collect()
}

/// Projects table-level public objectives, laws, strategy cards, and objective progress.
#[must_use]
pub fn project_table_view_with_map(state: &GameState, map_tiles: &[BoardTileView]) -> TableView {
    let content = ContentStore::embedded();
    let sources = POK;
    let galaxy_opt = placed_galaxy(map_tiles);

    let mut objective_progress = BTreeMap::new();
    let mut home_system_lost = BTreeMap::new();
    for player in &state.players {
        let mut position =
            ti4_engine::objectives::Position::new(state, content, sources, &player.id);
        if let Some(ref galaxy) = galaxy_opt {
            position = position.with_galaxy(galaxy);
        }
        let lost = ti4_engine::objectives::home_planets_not_controlled(&position);
        if !lost.is_empty() {
            home_system_lost.insert(player.id.clone(), lost);
        }
        let mut player_progress = BTreeMap::new();
        for raw_alias in &state.revealed_objectives {
            let canonical = canonical_objective_alias(content, raw_alias.as_str())
                .map(ObjectiveId::new)
                .unwrap_or_else(|| raw_alias.clone());

            if let Some(prog) = ti4_engine::objectives::counting_progress(&canonical, &position)
                .or_else(|| {
                    ti4_engine::objectives::remaining_position_progress(&canonical, &position)
                })
            {
                // Push Boundaries: each neighbour's planet count beside ours. Planet control is
                // public, so every viewer may see every seat's comparison.
                let neighbours = if canonical.as_str() == "push_boundaries" {
                    neighbour_progress_views(&position)
                } else {
                    Vec::new()
                };
                player_progress.insert(
                    raw_alias.clone(),
                    ObjectiveProgressView {
                        have: u32::try_from(prog.have).unwrap_or(0),
                        threshold: u32::try_from(prog.threshold).unwrap_or(0),
                        satisfied: prog.satisfied(),
                        neighbours,
                        ..ObjectiveProgressView::default()
                    },
                );
            } else if let Some(cost) =
                ti4_engine::objectives::bought_progress_at(&position, &canonical)
            {
                player_progress.insert(
                    raw_alias.clone(),
                    ObjectiveProgressView {
                        have: u32::try_from(cost.have.max(0)).unwrap_or(0),
                        threshold: u32::try_from(cost.target.max(0)).unwrap_or(0),
                        satisfied: cost.satisfied(),
                        ..ObjectiveProgressView::default()
                    },
                );
            }
        }
        if !player_progress.is_empty() {
            objective_progress.insert(player.id.clone(), player_progress);
        }
    }

    TableView {
        revealed_objectives: state.revealed_objectives.clone(),
        scored_objectives: state.scored_objectives.clone(),
        objective_progress,
        home_system_lost,
        unclaimed_strategy_cards: state.unclaimed_strategy_cards.clone(),
        strategy_card_goods: state.strategy_card_goods.clone(),
        laws: state.laws.clone(),
    }
}

/// Projects table-level public objectives, laws, and strategy cards.
#[must_use]
pub fn project_table_view(state: &GameState) -> TableView {
    project_table_view_with_map(state, &[])
}

/// Projects the entire game state for a specific viewer role with static map tiles.
#[must_use]
pub fn project_game_view_with_map(
    state: &GameState,
    viewer: &ViewerRole,
    map_tiles: &[BoardTileView],
) -> GameView {
    project_game_view_full(state, viewer, map_tiles, None, &[])
}

/// Projects the entire game state for a specific viewer role with pending choice and combat dice.
#[must_use]
pub fn project_game_view_full(
    state: &GameState,
    viewer: &ViewerRole,
    map_tiles: &[BoardTileView],
    pending_choice: Option<&Choice>,
    dice_rolls: &[crate::protocol::view::CombatDieRoll],
) -> GameView {
    let redacted = redacted_state(state, viewer);
    let players = redacted
        .players
        .iter()
        .map(|player| {
            let mut view = project_player_view(player);
            view.held_foreign_tokens =
                ti4_engine::factions::mahact::fleet_pool_owners(&redacted, &player.id);
            // Only the viewing seat's own hand: a law-revealed rival secret stays bare.
            if let ViewerRole::Player(seat) = viewer
                && *seat == player.id
            {
                view.secret_objective_progress =
                    project_secret_progress(&redacted, seat, map_tiles);
                view.agenda_peek = ti4_engine::action_cards::insider_peek(&redacted, seat)
                    .iter()
                    .map(|alias| project_agenda_peek_card(alias))
                    .collect();
            }
            view
        })
        .collect();

    GameView {
        round: redacted.round,
        turn: if redacted.phase == ti4_model::state::Phase::Action {
            redacted.action_turn
        } else {
            0
        },
        phase: redacted.phase,
        speaker: redacted.speaker.clone(),
        seating_order: redacted.seating_order.clone(),
        active_player: redacted.active.clone(),
        finished: redacted.finished,
        players,
        board: project_board_view_full(
            &redacted,
            map_tiles,
            pending_choice,
            dice_rolls,
            Some(viewer),
        ),
        table: project_table_view_with_map(&redacted, map_tiles),
        active_effects: crate::view::effects::project_active_effects(state),
    }
}

/// One agenda as printed (an id the content does not know shows as its id, with no text).
fn project_agenda_peek_card(alias: &str) -> crate::protocol::view::AgendaPeekCard {
    let record = ContentStore::embedded().get(ContentType::Agendas, alias);
    let field = |key: &str| {
        record
            .as_ref()
            .and_then(|record| record.text(key))
            .unwrap_or_default()
            .to_owned()
    };
    let name = Some(field("name"))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| alias.to_owned());
    crate::protocol::view::AgendaPeekCard {
        id: alias.to_owned(),
        name,
        kind: field("type"),
        target: field("target"),
        text1: field("text1"),
        text2: field("text2"),
    }
}

/// Applies the model's authoritative redaction for a protocol viewer.
#[must_use]
pub fn redacted_state(state: &GameState, viewer: &ViewerRole) -> GameState {
    match viewer {
        ViewerRole::Player(seat) => view_for(state, seat),
        ViewerRole::Spectator => {
            let mut spectator = state.clone();
            for player in &mut spectator.players {
                *player = redact_player(player);
            }
            // A spectator is nobody: the private rows of faction_marks (a seat's own secret, cards
            // shown to one seat, Insider Information's peek) are for their seat alone.
            ti4_model::view::redact_marks(&mut spectator, &PlayerId::new(""));
            spectator
        }
    }
}

/// Projects the entire game state for a specific viewer role.
#[must_use]
pub fn project_game_view(state: &GameState, viewer: &ViewerRole) -> GameView {
    project_game_view_with_map(state, viewer, &[])
}

/// Whether `choice` is a reaction window: a seat is asked whether to play an action card (or a
/// reaction ability) in response to an event. Who is asked tells everybody who holds a card.
#[must_use]
pub fn is_reaction_window(choice: &Choice) -> bool {
    choice.context.as_ref().is_some_and(|context| {
        matches!(
            context.source,
            ti4_engine::decision_context::DecisionSource::Reaction(_)
        ) || context.subtype.starts_with("reaction_")
            || context.subtype.starts_with("play_reaction_")
            || matches!(
                context.subtype.as_str(),
                "instinct_training_cancel" | "l1z1x_agent_swap"
            )
    })
}

/// The status everybody but the asked seat sees during a reaction window, and the status a bluff
/// hold puts up. It names no seat and carries no stage, so real windows and holds are identical.
#[must_use]
pub fn waiting_for_reactions_status(state: &GameState) -> PublicTurnStatus {
    PublicTurnStatus::WaitingForReactions {
        phase: state.phase,
        round: state.round,
    }
}

/// The status everybody else sees while `seat` is being waited for (not for a reaction window).
#[must_use]
pub fn waiting_for_player_status(state: &GameState, seat: &PlayerId) -> PublicTurnStatus {
    PublicTurnStatus::WaitingForDecision {
        seat: seat.clone(),
        phase: state.phase,
        round: state.round,
        // The existence of a choice is public; its context can reveal a private reaction.
        stage: "Waiting for player".to_owned(),
    }
}

/// The turn status `viewer` is shown: the asked seat sees it named even in a reaction window.
#[must_use]
pub fn project_turn_status_for(
    state: &GameState,
    pending_choice: Option<&Choice>,
    viewer: &ViewerRole,
) -> PublicTurnStatus {
    match pending_choice {
        Some(choice) if !state.finished && viewer.is_actor(&choice.player) => {
            waiting_for_player_status(state, &choice.player)
        }
        _ => project_turn_status(state, pending_choice),
    }
}

/// Projects public turn status without disclosing another player's private reactions or legal
/// choices. This is the view of everybody but the asked seat.
#[must_use]
pub fn project_turn_status(state: &GameState, pending_choice: Option<&Choice>) -> PublicTurnStatus {
    if state.finished {
        let winner = ti4_engine::objectives::leader(state);
        return PublicTurnStatus::GameOver { winner };
    }

    if let Some(choice) = pending_choice {
        if is_reaction_window(choice) {
            return waiting_for_reactions_status(state);
        }
        return waiting_for_player_status(state, &choice.player);
    }

    if let Some(active) = &state.active {
        PublicTurnStatus::ActiveTurn {
            player: active.clone(),
            phase: state.phase,
            round: state.round,
        }
    } else {
        PublicTurnStatus::PhaseTransition {
            phase: state.phase,
            round: state.round,
        }
    }
}

/// Projects a pending choice only to its owning seat.
#[must_use]
pub fn project_pending_choice(
    viewer: &ViewerRole,
    pending_choice: Option<(&Choice, &str)>,
) -> Option<PendingChoiceEnvelope> {
    pending_choice.and_then(|(choice, nonce)| {
        viewer
            .is_actor(&choice.player)
            .then(|| PendingChoiceEnvelope {
                nonce: nonce.to_owned(),
                choice: choice.clone(),
            })
    })
}

/// Submission metadata around the engine's wire-serialized choice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PendingChoiceEnvelope {
    pub nonce: String,
    pub choice: Choice,
}

/// What a client is shown of a game: the part of a state update that depends on the game alone.
///
/// The server's `StateUpdateMsg` has these four fields under the same names, and the wasm build
/// hands them to its host as they are, so one client reads both.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SessionUpdate {
    pub viewer: ViewerRole,
    pub view: GameView,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_choice: Option<PendingChoiceEnvelope>,
    pub turn_status: PublicTurnStatus,
    /// What the viewer needs to stage the pending choice of a tactical action. It is there
    /// for the seat that is asked, on a game that has its map ([`crate::view::tactical`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tactical: Option<crate::view::tactical::TacticalFacts>,
    /// The exploration decks and the planets that are still explored, on a game that has its
    /// map ([`crate::view::exploration`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exploration: Option<crate::view::exploration::ExplorationFacts>,
    /// The action card deck, its discard pile and the hand limit of the viewer
    /// ([`crate::view::action_cards`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_cards: Option<crate::view::action_cards::ActionCardFacts>,
    /// The secret objective deck and what each seat has scored of it
    /// ([`crate::view::secret_objectives`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_objectives: Option<crate::view::secret_objectives::SecretObjectiveFacts>,
    /// Where the victory points of every seat come from ([`crate::view::score`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<crate::view::score::ScoreFacts>,
    /// The agenda deck ([`crate::view::agendas`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agendas: Option<crate::view::agendas::AgendaFacts>,
    /// The strategic action that is resolved now, from the card that is played to the last
    /// follower ([`crate::view::secondary`]). It is there when the host follows its game
    /// ([`FactsContext::secondary`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary: Option<crate::view::secondary::SecondaryFacts>,
    /// The reaction window that the viewer is asked in, with what it offers as printed
    /// ([`crate::view::reaction`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reaction: Option<crate::view::reaction::ReactionFacts>,
    /// The technology tree of the viewer, while it is asked to research ([`crate::view::research`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub research: Option<crate::view::research::ResearchFacts>,
    /// Who sits in each seat: the name of its player and who plays it ([`crate::view::seats::SeatFact`]).
    /// It is of the table and not of the game, so the host of the game sets it; a game with no
    /// table has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seats: Option<crate::view::seats::TableSeats>,
}

/// What the facts of an update are made from, beside the state of the game.
#[derive(Clone, Copy)]
pub struct FactsContext<'a> {
    pub content: &'a ContentStore,
    pub sources: SourceSet,
    /// The map of the game. Without it an update has no `tactical` and no `exploration`.
    pub galaxy: Option<&'a Galaxy>,
    /// The strategic action of the game, as its host follows it. Without it an update has no
    /// `secondary`: a copy of the game for a draft has none.
    pub secondary: Option<&'a crate::view::secondary::SecondaryWatch>,
}

/// Projects what `viewer` is shown of `state` while the game waits in `pending_choice`.
///
/// The update has its nine facts ([`fill_facts`]), made from `state` as it is: the caller gives
/// the full state of the game, and each fact shows `viewer` only what that seat may know.
#[must_use]
pub fn project_session_update(
    state: &GameState,
    viewer: &ViewerRole,
    pending_choice: Option<(&Choice, &str)>,
    map_tiles: &[BoardTileView],
    facts: &FactsContext<'_>,
) -> SessionUpdate {
    let choice = pending_choice.map(|(choice, _)| choice);
    let mut update = SessionUpdate {
        viewer: viewer.clone(),
        view: project_game_view_full(state, viewer, map_tiles, choice, &[]),
        pending_choice: project_pending_choice(viewer, pending_choice),
        turn_status: project_turn_status_for(state, choice, viewer),
        tactical: None,
        exploration: None,
        action_cards: None,
        secret_objectives: None,
        score: None,
        agendas: None,
        secondary: None,
        reaction: None,
        research: None,
        seats: None,
    };
    fill_facts(&mut update, state, facts);
    update
}

/// Fills the nine facts of `update`, which was projected from `state`: the one place that does.
///
/// `tactical` is of the choice in the update, so only the seat that is asked has it.
pub fn fill_facts(update: &mut SessionUpdate, state: &GameState, context: &FactsContext<'_>) {
    let FactsContext {
        content,
        sources,
        galaxy,
        secondary,
    } = *context;
    update.tactical = update.pending_choice.as_ref().and_then(|pending| {
        crate::view::tactical::project_tactical_facts(
            state,
            content,
            sources,
            galaxy?,
            &pending.choice,
        )
    });
    update.exploration = galaxy.map(|galaxy| {
        crate::view::exploration::project_exploration_facts(state, content, sources, galaxy)
    });
    update.action_cards = Some(crate::view::action_cards::project_action_card_facts(
        state,
        content,
        &update.viewer,
    ));
    update.secret_objectives =
        Some(crate::view::secret_objectives::project_secret_objective_facts(state, content));
    update.score = Some(crate::view::score::project_score_facts(state, content));
    update.agendas = Some(crate::view::agendas::project_agenda_facts(state));
    update.secondary = secondary.and_then(|watch| watch.facts(state, content, &update.viewer));
    update.reaction = update.pending_choice.as_ref().and_then(|pending| {
        crate::view::reaction::project_reaction_facts(content, &pending.choice)
    });
    update.research = update.pending_choice.as_ref().and_then(|pending| {
        crate::view::research::project_research_facts(state, content, sources, &pending.choice)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::id::{FactionId, ObjectiveId, PlanetId, PlayerId, SystemId};
    use ti4_model::state::GameState;

    #[test]
    fn a_mahact_pool_reaches_every_viewer_and_other_seats_carry_none() {
        let mut state = ti4_engine::fixtures::seated_game(
            &[("a", "mahact"), ("b", "xxcha")],
            ti4_model::content_types::DEFAULT,
        );
        let (mahact, other) = (PlayerId::new("a"), PlayerId::new("b"));
        assert!(ti4_engine::factions::mahact::add_token(
            &mut state, &mahact, &other
        ));
        for viewer in [ViewerRole::Spectator, ViewerRole::Player(other.clone())] {
            let view = project_game_view(&state, &viewer);
            let held = |id: &PlayerId| {
                view.players
                    .iter()
                    .find(|player| player.id == *id)
                    .map(|player| player.held_foreign_tokens.clone())
                    .unwrap()
            };
            assert_eq!(held(&mahact), vec![other.clone()]);
            assert!(held(&other).is_empty());
        }
        // The field is additive: absent when empty, so an old client never sees it.
        let json = serde_json::to_value(project_game_view(&state, &ViewerRole::Spectator)).unwrap();
        let sheets = json["players"].as_array().unwrap();
        assert!(
            sheets
                .iter()
                .any(|p| p.get("held_foreign_tokens").is_some())
        );
        assert!(
            sheets
                .iter()
                .any(|p| p.get("held_foreign_tokens").is_none())
        );
    }

    #[test]
    fn an_insider_peek_reaches_only_the_seat_that_looked() {
        let mut state = ti4_engine::fixtures::seated_game(
            &[("a", "mahact"), ("b", "xxcha")],
            ti4_model::content_types::DEFAULT,
        );
        let (viewer, other) = (PlayerId::new("a"), PlayerId::new("b"));
        state.agenda_deck = vec!["one".into(), "two".into(), "three".into(), "four".into()];
        state.faction_marks.insert(
            ti4_engine::action_cards::insider_peek_key(&viewer),
            "one,two,three".to_owned(),
        );
        let peek = |viewer: &ViewerRole, whose: &PlayerId| {
            project_game_view(&state, viewer)
                .players
                .iter()
                .find(|player| &player.id == whose)
                .map(|player| {
                    player
                        .agenda_peek
                        .iter()
                        .map(|card| card.id.clone())
                        .collect::<Vec<_>>()
                })
                .expect("a seat")
        };

        assert_eq!(
            peek(&ViewerRole::Player(viewer.clone()), &viewer),
            vec!["one", "two", "three"]
        );
        assert!(peek(&ViewerRole::Player(other.clone()), &viewer).is_empty());
        assert!(peek(&ViewerRole::Player(other.clone()), &other).is_empty());
        assert!(peek(&ViewerRole::Spectator, &viewer).is_empty());
        // And nothing of it is on the wire for anybody else.
        let sent =
            serde_json::to_string(&project_game_view(&state, &ViewerRole::Player(other))).unwrap();
        assert!(
            !sent.contains("agenda_peek") && !sent.contains("three"),
            "{sent}"
        );
        // The viewer gets the printed card, not just an id.
        state.agenda_deck = vec!["revolution".into(), "secret".into()];
        state.faction_marks.insert(
            ti4_engine::action_cards::insider_peek_key(&viewer),
            "revolution,secret".to_owned(),
        );
        let shown = project_game_view(&state, &ViewerRole::Player(viewer.clone()));
        let cards = &shown
            .players
            .iter()
            .find(|p| p.id == viewer)
            .unwrap()
            .agenda_peek;
        assert_eq!(cards[0].name, "Anti-Intellectual Revolution");
        assert_eq!(cards[0].kind, "Law");
        assert!(cards[0].text1.starts_with("For:"));
        assert_eq!(cards[1].target, "Elect Player");
    }

    #[test]
    fn a_spectators_state_carries_no_private_mark() {
        let mut state = ti4_engine::fixtures::seated_game(
            &[("a", "mahact"), ("b", "xxcha")],
            ti4_model::content_types::DEFAULT,
        );
        let viewer = PlayerId::new("a");
        state.agenda_deck = vec!["one".into(), "two".into()];
        state.faction_marks.insert(
            ti4_engine::action_cards::insider_peek_key(&viewer),
            "one,two".to_owned(),
        );
        state
            .faction_marks
            .insert("sardakk:tekklar:round".to_owned(), "3".to_owned());

        let spectator = redacted_state(&state, &ViewerRole::Spectator);
        assert!(
            !spectator
                .faction_marks
                .keys()
                .any(|key| key.starts_with("private:"))
        );
        assert!(
            spectator
                .faction_marks
                .contains_key("sardakk:tekklar:round")
        );
        let own = redacted_state(&state, &ViewerRole::Player(viewer.clone()));
        assert!(
            own.faction_marks
                .contains_key(&ti4_engine::action_cards::insider_peek_key(&viewer))
        );
        let other = redacted_state(&state, &ViewerRole::Player(PlayerId::new("b")));
        assert!(
            !other
                .faction_marks
                .keys()
                .any(|key| key.starts_with("private:"))
        );
    }

    fn reaction_offer() -> (Choice, String) {
        use ti4_engine::choice::ChoiceOption;
        use ti4_engine::decision_context::{DecisionContext, DecisionSource, DecisionTrigger};
        let mut payload = BTreeMap::new();
        payload.insert("player".to_owned(), "player_1".into());
        payload.insert("card".to_owned(), "fs1".into());
        let event = ti4_engine::event::Event::new(4, "ACTION_CARD_PLAYED", payload);
        let choice = Choice::new(
            PlayerId::new("player_2"),
            "when ACTION_CARD_PLAYED",
            vec![
                ChoiceOption::labelled(
                    "reaction:x:ACTION_CARD_PLAYED:when",
                    "ability",
                    "Play Sabotage",
                ),
                ChoiceOption::decline(),
            ],
        )
        .contextualized(
            DecisionContext::new(
                PlayerId::new("player_2"),
                DecisionSource::Reaction("ACTION_CARD_PLAYED".to_owned()),
                "reaction_when_ACTION_CARD_PLAYED",
                ti4_model::state::Phase::Action,
                1,
            )
            .optional(true)
            .with_trigger(DecisionTrigger::from_event(&event, "when", &[])),
        );
        (choice, "7".to_owned())
    }

    #[test]
    fn the_reaction_trigger_reaches_the_asked_seat_and_only_that_seat() {
        let (choice, nonce) = reaction_offer();
        let owner = ViewerRole::Player(PlayerId::new("player_2"));
        let sent =
            project_pending_choice(&owner, Some((&choice, &nonce))).expect("the owner is asked");
        let json = serde_json::to_value(&sent).unwrap();
        let trigger = &json["choice"]["context"]["trigger"];
        assert_eq!(trigger["kind"], "action_card_played");
        assert_eq!(trigger["actor"], "player_1");
        assert_eq!(trigger["card"], "fs1");
        assert_eq!(trigger["relation"], "when");
        assert_eq!(trigger["event_id"], 4);
        assert!(
            trigger.get("chain").is_none(),
            "an empty chain stays off the wire"
        );
        let other = ViewerRole::Player(PlayerId::new("player_1"));
        assert!(project_pending_choice(&other, Some((&choice, &nonce))).is_none());
        assert!(project_pending_choice(&ViewerRole::Spectator, Some((&choice, &nonce))).is_none());
        // And a message written before triggers existed still decodes, with none.
        let mut old = json.clone();
        old["choice"]["context"]
            .as_object_mut()
            .unwrap()
            .remove("trigger");
        let back: PendingChoiceEnvelope = serde_json::from_value(old).unwrap();
        assert!(back.choice.context.unwrap().trigger.is_none());
    }

    #[test]
    fn the_effect_previews_reach_the_asked_seat_and_only_that_seat() {
        use ti4_engine::decision_context::EffectPreview;
        let (mut choice, nonce) = reaction_offer();
        let preview = EffectPreview {
            option: "reaction:x:ACTION_CARD_PLAYED:when".to_owned(),
            card: "sabo1".to_owned(),
            effect: "cancel_card".to_owned(),
            seats: vec![PlayerId::new("player_1")],
            system: None,
            planets: Vec::new(),
            units: Vec::new(),
            amount: None,
            of: None,
            refs: vec!["fs1".to_owned()],
            fizzles: None,
        };
        choice.context = choice
            .context
            .take()
            .map(|c| c.with_previews(vec![preview]));
        let owner = ViewerRole::Player(PlayerId::new("player_2"));
        let sent = project_pending_choice(&owner, Some((&choice, &nonce))).expect("asked");
        let json = serde_json::to_value(&sent).unwrap();
        let previews = &json["choice"]["context"]["previews"];
        assert_eq!(previews[0]["effect"], "cancel_card");
        assert_eq!(previews[0]["refs"][0], "fs1");
        assert!(
            previews[0].get("units").is_none(),
            "empty fields stay off the wire"
        );
        for other in [
            ViewerRole::Player(PlayerId::new("player_1")),
            ViewerRole::Spectator,
        ] {
            assert!(project_pending_choice(&other, Some((&choice, &nonce))).is_none());
        }
        // A message from before previews existed decodes, with none.
        let mut old = json.clone();
        old["choice"]["context"]
            .as_object_mut()
            .unwrap()
            .remove("previews");
        let back: PendingChoiceEnvelope = serde_json::from_value(old).unwrap();
        assert!(back.choice.context.unwrap().previews.is_empty());
    }

    #[test]
    fn an_update_has_its_facts_and_those_of_the_board_only_with_the_map() {
        let seat = PlayerId::new("player_1");
        let state = GameState::new(std::slice::from_ref(&seat), &[], BTreeMap::new(), None, 1);
        let viewer = ViewerRole::Player(seat);

        // Without the map there is nothing to say about the board.
        let context = FactsContext {
            content: ContentStore::embedded(),
            sources: POK,
            galaxy: None,
            secondary: None,
        };
        let filled = project_session_update(&state, &viewer, None, &[], &context);
        assert!(filled.tactical.is_none() && filled.exploration.is_none());
        assert!(filled.action_cards.is_some() && filled.secret_objectives.is_some());
        assert!(filled.score.is_some() && filled.agendas.is_some());
        let mut again = filled.clone();
        again.action_cards = None;
        again.score = None;
        fill_facts(&mut again, &state, &context);
        assert_eq!(again, filled);
    }

    #[test]
    fn a_reaction_window_names_no_seat_to_anyone_but_the_asked_seat() {
        let (reaction, _) = reaction_offer();
        let ids = [PlayerId::new("player_1"), PlayerId::new("player_2")];
        let state = GameState::new(&ids, &[], BTreeMap::new(), None, 1);
        let asked = ViewerRole::Player(PlayerId::new("player_2"));
        assert!(is_reaction_window(&reaction));
        let generic = waiting_for_reactions_status(&state);
        for viewer in [
            ViewerRole::Player(PlayerId::new("player_1")),
            ViewerRole::Spectator,
        ] {
            assert_eq!(
                project_turn_status_for(&state, Some(&reaction), &viewer),
                generic
            );
        }
        assert_eq!(project_turn_status(&state, Some(&reaction)), generic);
        assert_eq!(
            project_turn_status_for(&state, Some(&reaction), &asked),
            waiting_for_player_status(&state, &PlayerId::new("player_2"))
        );
        let json = serde_json::to_string(&generic).unwrap();
        assert!(!json.contains("player_2") && !json.contains("stage"));

        // Any other question keeps naming its seat, for every viewer.
        let ordinary = Choice::new(PlayerId::new("player_2"), "action phase", vec![]);
        assert!(!is_reaction_window(&ordinary));
        for viewer in [
            ViewerRole::Player(PlayerId::new("player_1")),
            ViewerRole::Spectator,
            asked,
        ] {
            assert_eq!(
                project_turn_status_for(&state, Some(&ordinary), &viewer),
                waiting_for_player_status(&state, &PlayerId::new("player_2"))
            );
        }
    }

    #[test]
    fn a_reaction_window_hides_its_seat_from_the_combat_view_too() {
        let (reaction, _) = reaction_offer();
        let ids = [PlayerId::new("player_1"), PlayerId::new("player_2")];
        let mut state = GameState::new(&ids, &[], BTreeMap::new(), None, 1);
        let system = ti4_model::id::SystemId::new("18");
        state.board.insert(system.clone(), Default::default());
        state.active_space_combat =
            Some((system, PlayerId::new("player_1"), PlayerId::new("player_2")));
        let asked = ViewerRole::Player(PlayerId::new("player_2"));
        let mine = project_combat_view_for(&state, Some(&reaction), &[], Some(&asked)).unwrap();
        assert_eq!(mine.active_player, Some(PlayerId::new("player_2")));
        for viewer in [
            ViewerRole::Player(PlayerId::new("player_1")),
            ViewerRole::Spectator,
        ] {
            let seen =
                project_combat_view_for(&state, Some(&reaction), &[], Some(&viewer)).unwrap();
            assert_eq!(seen.active_player, None);
            assert_eq!(seen.stage, None);
        }
    }

    #[test]
    fn a_tied_game_goes_to_the_first_seat_in_initiative_order() {
        let ids: Vec<PlayerId> = ["player_1", "player_2", "player_3"]
            .iter()
            .map(|id| PlayerId::new(*id))
            .collect();
        let mut state = GameState::new(&ids, &[], BTreeMap::new(), None, 1);
        state.players = ids
            .iter()
            .map(|id| {
                let mut player = Player::new(id.clone());
                player.victory_points = 3;
                player
            })
            .collect();
        state.finished = true;
        // 98.8, 61.15a: a three-way tie is broken by initiative order. With nobody holding a
        // strategy card that is seating order, so the first seat wins, not the last.
        match project_turn_status(&state, None) {
            PublicTurnStatus::GameOver { winner } => assert_eq!(winner, Some(ids[0].clone())),
            other => panic!("expected game over, got {other:?}"),
        }
    }

    #[test]
    fn project_table_view_calculates_objective_progress() {
        let p1 = PlayerId::new("player_1");
        let mut state = GameState::new(std::slice::from_ref(&p1), &[], BTreeMap::new(), None, 1);
        let mut player = Player::new(p1.clone());
        player.faction = FactionId::new("hacan");
        player.trade_goods = 7;
        player.tactic_tokens = 2;
        player.strategic_tokens = 2;
        state.players = vec![player];

        state.revealed_objectives = vec![
            ObjectiveId::new("trade_routes"),
            ObjectiveId::new("centralize_trade"),
            ObjectiveId::new("lead"),
        ];

        let table = project_table_view(&state);
        let p1_progress = table.objective_progress.get(&p1).expect("p1 progress");

        let trade_routes = p1_progress
            .get(&ObjectiveId::new("trade_routes"))
            .expect("trade_routes");
        assert_eq!(trade_routes.have, 5);
        assert_eq!(trade_routes.threshold, 5);
        assert!(trade_routes.satisfied);

        let centralize = p1_progress
            .get(&ObjectiveId::new("centralize_trade"))
            .expect("centralize");
        assert_eq!(centralize.have, 7);
        assert_eq!(centralize.threshold, 10);
        assert!(!centralize.satisfied);

        let lead = p1_progress.get(&ObjectiveId::new("lead")).expect("lead");
        assert_eq!(lead.have, 3);
        assert_eq!(lead.threshold, 3);
        assert!(lead.satisfied);
    }

    /// Game 8261fb3f: Letnev held Archon Ren when the Xxcha played Imperial. The table says so,
    /// for every viewer, so the client can explain why nothing was scored (61.16).
    #[test]
    fn project_table_view_names_the_lost_home_planets() {
        let xxcha = PlayerId::new("player_1");
        let letnev = PlayerId::new("player_2");
        let mut state = GameState::new(
            &[xxcha.clone(), letnev.clone()],
            &[],
            BTreeMap::new(),
            None,
            1,
        );
        let mut first = Player::new(xxcha.clone());
        first.faction = FactionId::new("xxcha");
        let mut second = Player::new(letnev.clone());
        second.faction = FactionId::new("letnev");
        state.players = vec![first, second];
        let home = state.system_mut(&SystemId::new("14"));
        home.set_control(PlanetId::new("archontau"), xxcha.clone());
        home.set_control(PlanetId::new("archonren"), letnev.clone());

        let table = project_table_view(&state);
        assert_eq!(
            table.home_system_lost.get(&xxcha),
            Some(&vec![PlanetId::new("archonren")])
        );
        // Letnev's own home planets are not on this board at all, so it is listed too.
        assert!(table.home_system_lost.contains_key(&letnev));

        let home = state.system_mut(&SystemId::new("14"));
        home.set_control(PlanetId::new("archonren"), xxcha.clone());
        assert!(
            !project_table_view(&state)
                .home_system_lost
                .contains_key(&xxcha)
        );
    }

    #[test]
    fn a_planet_view_lists_what_raises_its_value_and_the_sum_is_the_engine_value() {
        let mut state = ti4_engine::fixtures::seated_game(
            &[("a", "xxcha"), ("b", "letnev")],
            ti4_model::content_types::DEFAULT,
        );
        let planet = PlanetId::new("archontau");
        state
            .system_mut(&SystemId::new("14"))
            .set_control(planet.clone(), PlayerId::new("a"));
        let board = |state: &GameState| project_board_view(state);
        let boosts = |state: &GameState| {
            board(state).systems[&SystemId::new("14")].planets[&planet]
                .value_boosts
                .clone()
        };
        assert!(boosts(&state).is_empty());

        state.planet_attachments.insert(
            planet.clone(),
            vec!["miningworld".into(), "dysonsphere".into()],
        );
        state
            .laws
            .insert("senate_sanctuary".into(), planet.to_string());
        let listed = boosts(&state);
        let names: Vec<_> = listed.iter().map(|b| b.source.as_str()).collect();
        assert_eq!(names, ["Senate Sanctuary", "Mining World", "Dyson Sphere"]);

        let content = ContentStore::embedded();
        let full = ti4_model::content_types::FULL;
        for (kind, sum) in [
            (
                ti4_engine::production::Spend::Resources,
                listed.iter().map(|b| i64::from(b.resources)).sum::<i64>(),
            ),
            (
                ti4_engine::production::Spend::Influence,
                listed.iter().map(|b| i64::from(b.influence)).sum::<i64>(),
            ),
        ] {
            let printed = ti4_engine::production::planet_value(content, full, &planet, kind);
            let now =
                ti4_engine::production::planet_value_now(&state, content, full, &planet, kind);
            assert_eq!(now - printed, sum);
        }
        assert_eq!(listed[1].resources, 2);
        assert_eq!(listed[2].resources, 2);
        assert_eq!(listed[2].influence, 1);
    }
}
