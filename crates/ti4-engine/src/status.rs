//! Structural status-phase bookkeeping.

use ti4_model::id::{ObjectiveId, PlanetId, PlayerId, StrategyCardId, SystemId, TechnologyId};
use ti4_model::state::{GameState, Phase};

/// The observable structural changes made during one status phase.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StatusPhaseReport {
    /// The mandatory public objective revealed at step 81.2.
    pub revealed_objective: Option<ObjectiveId>,
    /// Action cards drawn per player, retained in the initiative order used at step 81.3.
    pub action_cards_drawn: Vec<(PlayerId, usize)>,
    /// Command tokens returned from systems, in board then holder order.
    pub returned_command_tokens: Vec<(SystemId, PlayerId)>,
    /// Planet cards readied at step 81.6.
    pub readied_planets: Vec<PlanetId>,
    /// Strategy cards returned at step 81.8, in player holding order.
    pub returned_strategy_cards: Vec<(PlayerId, StrategyCardId)>,
    /// Damaged space units repaired at step 81.7.
    pub repaired_units: usize,
    /// The objective deck was exhausted, so the game ended before later status steps.
    pub game_ended: bool,
}

/// A status phase was requested outside the status phase.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StatusPhaseError {
    #[error("cannot resolve status bookkeeping while in {0:?} phase")]
    WrongPhase(Phase),
}

/// Resolve the deterministic, choice-free parts of LRR 81.
///
/// Objective scoring and the two status command-token allocation choices are deliberately
/// excluded: both require a live choice driver, which M04-012 will provide. This function
/// preserves initiative order until strategy cards return at step 81.8.
///
/// # Errors
/// [`StatusPhaseError::WrongPhase`] unless `state` is currently in [`Phase::Status`].
pub fn resolve_status_phase(state: &mut GameState) -> Result<StatusPhaseReport, StatusPhaseError> {
    if state.phase != Phase::Status {
        return Err(StatusPhaseError::WrongPhase(state.phase));
    }

    let mut report = StatusPhaseReport::default();

    // 81.2: revealing is mandatory. The Python oracle finishes immediately if impossible.
    let Some(objective) = state.reveal_objective() else {
        state.finished = true;
        report.game_ended = true;
        return Ok(report);
    };
    report.revealed_objective = Some(objective);

    // 81.3: all action-card draws use the still-intact initiative order. `nm` is Neural
    // Motivator, whose status-only replacement draw is one additional card.
    let initiative = state.initiative_order();
    for player_id in initiative {
        let requested_draws = 1 + usize::from(
            state
                .player(&player_id)
                .is_some_and(|player| player.technologies.contains(&TechnologyId::new("nm"))),
        );
        let mut drawn_count = 0;
        for _ in 0..requested_draws {
            let Some(card) = state.action_card_deck.first().cloned() else {
                break;
            };
            state.action_card_deck.remove(0);
            if let Some(player) = state.player_mut(&player_id) {
                player.action_cards.push(card);
                drawn_count += 1;
            }
        }
        report.action_cards_drawn.push((player_id, drawn_count));
    }

    // 81.4: tokens leave the board; the oracle does not add them to a command-sheet pool.
    for (system_id, system) in &mut state.board {
        let holders = std::mem::take(&mut system.command_tokens);
        report.returned_command_tokens.extend(
            holders
                .into_iter()
                .map(|holder| (system_id.clone(), holder)),
        );
    }

    // 81.6: ready exhausted technology and planet cards. 81.7 then repairs space units.
    for player in &mut state.players {
        player.exhausted_technologies.clear();
    }
    report.readied_planets = state.exhausted_planets.iter().cloned().collect();
    state.ready_all_planets();
    for system in state.board.values_mut() {
        for unit in &mut system.units {
            if unit.sustained_damage {
                *unit = unit.repaired();
                report.repaired_units += 1;
            }
        }
    }

    // 81.8 comes last: clearing earlier would turn later initiative reads into seating order.
    let holders: Vec<(PlayerId, Vec<StrategyCardId>)> = state
        .players
        .iter()
        .map(|player| (player.id.clone(), player.strategy_cards.clone()))
        .collect();
    for (player_id, cards) in holders {
        report
            .returned_strategy_cards
            .extend(cards.iter().cloned().map(|card| (player_id.clone(), card)));
        state.clear_strategy_cards(&player_id);
        if let Some(player) = state.player_mut(&player_id) {
            player.passed = false;
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use ti4_content::ContentStore;
    use ti4_model::content_types::POK;
    use ti4_model::id::{PlanetId, PlayerId, TechnologyId, UnitTypeId};
    use ti4_model::state::Phase;
    use ti4_model::units::Unit;

    use super::*;
    use crate::setup::start_game;

    #[test]
    fn status_reveals_draws_readies_and_returns_strategy_cards_in_initiative_order() {
        let players = [PlayerId::new("a"), PlayerId::new("b")];
        let mut state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        state.phase = Phase::Status;

        let report = resolve_status_phase(&mut state).unwrap();

        assert_eq!(
            report.action_cards_drawn,
            vec![(PlayerId::new("a"), 1), (PlayerId::new("b"), 1)]
        );
        assert!(
            state
                .players
                .iter()
                .all(|player| player.strategy_cards.is_empty())
        );
    }

    #[test]
    fn status_keeps_initiative_for_draws_then_readies_repairs_and_resets() {
        let players = [PlayerId::new("a"), PlayerId::new("b")];
        let mut state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        state.phase = Phase::Status;
        state.deal_strategy_card(
            &PlayerId::new("b"),
            ti4_model::id::StrategyCardId::new("leadership"),
        );
        state.deal_strategy_card(
            &PlayerId::new("a"),
            ti4_model::id::StrategyCardId::new("imperial"),
        );
        state
            .player_mut(&PlayerId::new("a"))
            .unwrap()
            .technologies
            .insert(TechnologyId::new("nm"));
        state.player_mut(&PlayerId::new("a")).unwrap().passed = true;
        state
            .player_mut(&PlayerId::new("a"))
            .unwrap()
            .exhausted_technologies
            .insert(TechnologyId::new("nm"));
        state.exhaust_planet(PlanetId::new("jord"));
        state
            .system_mut(&ti4_model::id::SystemId::new("18"))
            .units
            .push(Unit::new(UnitTypeId::new("dreadnought"), PlayerId::new("a")).sustained());
        state
            .system_mut(&ti4_model::id::SystemId::new("18"))
            .place_token(PlayerId::new("b"));
        let expected_draw_order = state
            .initiative_order()
            .into_iter()
            .map(|player| {
                let draws = if player == PlayerId::new("a") { 2 } else { 1 };
                (player, draws)
            })
            .collect::<Vec<_>>();

        let report = resolve_status_phase(&mut state).unwrap();

        assert_eq!(report.action_cards_drawn, expected_draw_order);
        assert_eq!(
            report.returned_command_tokens,
            vec![(ti4_model::id::SystemId::new("18"), PlayerId::new("b"))]
        );
        assert_eq!(report.readied_planets, vec![PlanetId::new("jord")]);
        assert_eq!(report.repaired_units, 1);
        assert!(state.exhausted_planets.is_empty());
        assert!(
            state
                .player(&PlayerId::new("a"))
                .unwrap()
                .exhausted_technologies
                .is_empty()
        );
        assert!(
            state
                .player(&PlayerId::new("a"))
                .unwrap()
                .strategy_cards
                .is_empty()
        );
        assert!(!state.player(&PlayerId::new("a")).unwrap().passed);
        assert!(
            !state
                .system_state(&ti4_model::id::SystemId::new("18"))
                .units[0]
                .sustained_damage
        );
        assert!(
            state
                .system_state(&ti4_model::id::SystemId::new("18"))
                .command_tokens
                .is_empty()
        );
    }

    #[test]
    fn an_empty_objective_deck_ends_before_later_status_steps() {
        let players = [PlayerId::new("a")];
        let mut state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        state.phase = Phase::Status;
        state.objective_deck.clear();
        let before_hand = state
            .player(&PlayerId::new("a"))
            .unwrap()
            .action_cards
            .clone();

        let report = resolve_status_phase(&mut state).unwrap();

        assert!(report.game_ended);
        assert!(state.finished);
        assert_eq!(
            state.player(&PlayerId::new("a")).unwrap().action_cards,
            before_hand
        );
    }

    #[test]
    fn resolving_outside_status_is_atomic() {
        let players = [PlayerId::new("a")];
        let mut state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        let before = state.clone();

        assert_eq!(
            resolve_status_phase(&mut state),
            Err(StatusPhaseError::WrongPhase(Phase::Strategy))
        );
        assert!(state.identical(&before));
    }
}
