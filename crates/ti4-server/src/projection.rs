//! Pure projections transforming engine state and choices into redacted client views.

use std::collections::{BTreeMap, BTreeSet};
use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_engine::choice::Choice;
use ti4_model::content_types::{ContentType, POK};
use ti4_model::id::{ObjectiveId, PlanetId, PlayerId};
use ti4_model::state::{GameState, Player};
use ti4_model::view::{HIDDEN, redact_player, view_for};

use crate::map::GalaxyLayout;
use crate::protocol::PROTOCOL_VERSION;
use crate::protocol::server::{
    GameEvent, InitialSnapshotMsg, PendingChoiceEnvelope, StateUpdateMsg,
};
use crate::protocol::status::{PublicTurnStatus, ViewerRole};
use crate::protocol::view::{
    BoardTileView, BoardView, GameView, ObjectiveProgressView, PlacedUnitView, PlanetView,
    PlayerView, SystemView, TableView,
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
        leaders: player.leaders.clone(),
    }
}

/// Helper to construct public combat view if a space combat is detected
#[must_use]
pub fn project_combat_view(
    state: &GameState,
    pending_choice: Option<&Choice>,
    dice_rolls: &[crate::protocol::view::CombatDieRoll],
) -> Option<crate::protocol::view::CombatView> {
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
    project_board_view_full(state, map_tiles, None, &[])
}

/// Projects the full board view with combat and dice information.
#[must_use]
pub fn project_board_view_full(
    state: &GameState,
    map_tiles: &[BoardTileView],
    pending_choice: Option<&Choice>,
    dice_rolls: &[crate::protocol::view::CombatDieRoll],
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

    let invasion = state.active_invasion.as_ref().map(|active| {
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
        crate::protocol::view::InvasionView {
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
        }
    });
    BoardView {
        systems,
        active_system: state.active_system.clone(),
        map_tiles: map_tiles.to_vec(),
        combat: project_combat_view(state, pending_choice, dice_rolls),
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

/// Projects table-level public objectives, laws, strategy cards, and objective progress.
#[must_use]
pub fn project_table_view_with_map(state: &GameState, map_tiles: &[BoardTileView]) -> TableView {
    let content = ContentStore::embedded();
    let sources = POK;
    let galaxy_opt = if !map_tiles.is_empty() {
        let tiles: Vec<_> = map_tiles
            .iter()
            .map(|tile| {
                (
                    tile.system_id.as_str(),
                    ti4_model::hex::Hex::new(tile.q, tile.r),
                )
            })
            .collect();
        Galaxy::placed(content, &tiles, sources).ok()
    } else {
        None
    };

    let mut objective_progress = BTreeMap::new();
    for player in &state.players {
        let mut position =
            ti4_engine::objectives::Position::new(state, content, sources, &player.id);
        if let Some(ref galaxy) = galaxy_opt {
            position = position.with_galaxy(galaxy);
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
                player_progress.insert(
                    raw_alias.clone(),
                    ObjectiveProgressView {
                        have: u32::try_from(prog.have).unwrap_or(0),
                        threshold: u32::try_from(prog.threshold).unwrap_or(0),
                        satisfied: prog.satisfied(),
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
    let players = redacted.players.iter().map(project_player_view).collect();

    GameView {
        round: redacted.round,
        phase: redacted.phase,
        speaker: redacted.speaker.clone(),
        seating_order: redacted.seating_order.clone(),
        active_player: redacted.active.clone(),
        finished: redacted.finished,
        players,
        board: project_board_view_full(&redacted, map_tiles, pending_choice, dice_rolls),
        table: project_table_view_with_map(&redacted, map_tiles),
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
            spectator
        }
    }
}

/// Projects the entire game state for a specific viewer role.
#[must_use]
pub fn project_game_view(state: &GameState, viewer: &ViewerRole) -> GameView {
    project_game_view_with_map(state, viewer, &[])
}

/// Projects public turn status without disclosing another player's private reactions or legal choices.
#[must_use]
pub fn project_turn_status(state: &GameState, pending_choice: Option<&Choice>) -> PublicTurnStatus {
    if state.finished {
        let winner = state
            .players
            .iter()
            .max_by_key(|p| p.victory_points)
            .map(|p| p.id.clone());
        return PublicTurnStatus::GameOver { winner };
    }

    if let Some(choice) = pending_choice {
        return PublicTurnStatus::WaitingForDecision {
            seat: choice.player.clone(),
            phase: state.phase,
            round: state.round,
            // The existence of a choice is public; its context can reveal a private reaction.
            stage: "Waiting for player".to_owned(),
        };
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

/// Projects an initial snapshot for a connecting viewer with static map tiles.
#[must_use]
pub fn project_initial_snapshot_with_map(
    game_id: &str,
    game_version: u64,
    state: &GameState,
    viewer: &ViewerRole,
    pending_choice: Option<(&Choice, &str)>,
    map_tiles: &[BoardTileView],
    galaxy_layout: &GalaxyLayout,
    events: &[GameEvent],
) -> InitialSnapshotMsg {
    InitialSnapshotMsg {
        protocol_version: PROTOCOL_VERSION,
        game_id: game_id.to_owned(),
        game_version,
        viewer: viewer.clone(),
        view: project_game_view_full(
            state,
            viewer,
            map_tiles,
            pending_choice.map(|(c, _)| c),
            &[],
        ),
        state: redacted_state(state, viewer),
        galaxy_layout: galaxy_layout.clone(),
        pending_choice: project_pending_choice(viewer, pending_choice),
        turn_status: project_turn_status(state, pending_choice.map(|(c, _)| c)),
        events: events
            .iter()
            .filter_map(|event| event.for_viewer(viewer))
            .collect(),
        history: crate::protocol::server::HistoryStatus::default(),
        current_path: None,
    }
}

/// Projects an initial snapshot for a connecting viewer.
#[must_use]
pub fn project_initial_snapshot(
    game_id: &str,
    game_version: u64,
    state: &GameState,
    viewer: &ViewerRole,
    pending_choice: Option<(&Choice, &str)>,
) -> InitialSnapshotMsg {
    project_initial_snapshot_with_map(
        game_id,
        game_version,
        state,
        viewer,
        pending_choice,
        &[],
        &GalaxyLayout {
            version: 1,
            active_sources: Vec::new(),
            placements: Vec::new(),
            off_map_system_ids: Vec::new(),
        },
        &[],
    )
}

/// Projects a versioned state update message with static map tiles.
#[must_use]
pub fn project_state_update_with_map(
    game_id: &str,
    game_version: u64,
    state: &GameState,
    viewer: &ViewerRole,
    pending_choice: Option<(&Choice, &str)>,
    map_tiles: &[BoardTileView],
    galaxy_layout: &GalaxyLayout,
) -> StateUpdateMsg {
    StateUpdateMsg {
        history: crate::protocol::server::HistoryStatus::default(),
        current_path: None,
        protocol_version: PROTOCOL_VERSION,
        game_id: game_id.to_owned(),
        game_version,
        viewer: viewer.clone(),
        view: project_game_view_full(
            state,
            viewer,
            map_tiles,
            pending_choice.map(|(c, _)| c),
            &[],
        ),
        state: redacted_state(state, viewer),
        galaxy_layout: galaxy_layout.clone(),
        pending_choice: project_pending_choice(viewer, pending_choice),
        turn_status: project_turn_status(state, pending_choice.map(|(c, _)| c)),
    }
}

/// Projects a versioned state update message.
#[must_use]
pub fn project_state_update(
    game_id: &str,
    game_version: u64,
    state: &GameState,
    viewer: &ViewerRole,
    pending_choice: Option<(&Choice, &str)>,
) -> StateUpdateMsg {
    project_state_update_with_map(
        game_id,
        game_version,
        state,
        viewer,
        pending_choice,
        &[],
        &GalaxyLayout {
            version: 1,
            active_sources: Vec::new(),
            placements: Vec::new(),
            off_map_system_ids: Vec::new(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::id::{FactionId, ObjectiveId, PlayerId};
    use ti4_model::state::GameState;

    #[test]
    fn project_table_view_calculates_objective_progress() {
        let p1 = PlayerId::new("player_1");
        let mut state = GameState::new(&[p1.clone()], &[], BTreeMap::new(), None, 1);
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
}
