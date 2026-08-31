//! Structural status-phase bookkeeping.

use ti4_model::id::{ObjectiveId, PlanetId, PlayerId, StrategyCardId, SystemId, TechnologyId};
use ti4_model::state::{GameState, Phase};

/// The observable structural changes made during one status phase.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StatusPhaseReport {
    /// Players owed an action card at the end of the phase (Minister of Policy).
    pub action_card_draws: Vec<PlayerId>,
    /// The mandatory public objective revealed at step 81.2.
    pub revealed_objective: Option<ObjectiveId>,
    /// Action cards drawn per player, retained in the initiative order used at step 81.3.
    pub action_cards_drawn: Vec<(PlayerId, usize)>,
    /// Command tokens returned from systems, in board then holder order.
    pub returned_command_tokens: Vec<(SystemId, PlayerId)>,
    /// Planet cards readied at step 81.6.
    pub readied_planets: Vec<PlanetId>,
    /// Leaders readied at step 81.6, per player.
    pub readied_leaders: Vec<(PlayerId, usize)>,
    /// Strategy cards returned at step 81.8, in player holding order.
    pub returned_strategy_cards: Vec<(PlayerId, StrategyCardId)>,
    /// Initiative order as it stood before step 81.8 returned the strategy cards.
    ///
    /// Captured because steps 81.3 and 81.5 both use it, and 81.8 destroys it — reading it
    /// afterwards yields seating order instead.
    pub initiative_order: Vec<PlayerId>,
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

/// Resolve every choice-free part of LRR 81, in order.
///
/// Convenience for callers with no choice driver: it runs the steps before the token gain and
/// then those after it, skipping 81.5 entirely. A driver that can ask questions should call
/// [`resolve_before_token_gain`] and [`resolve_after_token_gain`] around a
/// [`crate::tokens::TokenGain`] window instead, so tokens are gained in their real position.
///
/// Objective scoring (81.1) is excluded from both: it needs the scoreability predicates.
///
/// # Errors
/// [`StatusPhaseError::WrongPhase`] unless `state` is currently in [`Phase::Status`].
pub fn resolve_status_phase(state: &mut GameState) -> Result<StatusPhaseReport, StatusPhaseError> {
    let mut report = resolve_before_token_gain(state)?;
    if !report.game_ended {
        resolve_after_token_gain(state, &mut report);
    }
    Ok(report)
}

/// Steps 81.2 to 81.4: reveal, draw action cards, and recall command tokens from the board.
///
/// Stops after 81.4 so that the caller can run the 81.5 token gain — a real choice — before
/// the remaining bookkeeping. The returned report carries `initiative_order` because 81.5 needs
/// it and step 81.8 destroys it.
///
/// # Errors
/// [`StatusPhaseError::WrongPhase`] unless `state` is currently in [`Phase::Status`].
pub fn resolve_before_token_gain(
    state: &mut GameState,
) -> Result<StatusPhaseReport, StatusPhaseError> {
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
    report.initiative_order.clone_from(&initiative);
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

    Ok(report)
}

/// Steps 81.6 to 81.8: ready cards, repair damaged units, and return the strategy cards.
///
/// Separate from [`resolve_before_token_gain`] so the 81.5 token gain sits between them, where
/// LRR 81 puts it. Extends `report` rather than returning its own, so one status phase is
/// described by one report however it was driven.
pub fn resolve_after_token_gain(state: &mut GameState, report: &mut StatusPhaseReport) {
    // 81.6: ready exhausted technology, planet and leader cards. 81.7 then repairs units.
    for player in &mut state.players {
        player.exhausted_technologies.clear();
        // A relic exhausted for its ability readies here too, beside the technologies: the three
        // that say "exhaust this card" are once per round, and nothing else clears them.
        player.exhausted_relics.clear();
    }
    // Leaders ready here too. An exhausted agent that never readies reads, after a round or
    // two, as a player who has simply run out of agents.
    let seats: Vec<PlayerId> = state
        .players
        .iter()
        .map(|player| player.id.clone())
        .collect();
    for player in seats {
        let readied = crate::leaders::ready_all(state, &player).len();
        if readied > 0 {
            report.readied_leaders.push((player, readied));
        }
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

    // Minister of Policy: its owner draws an action card at the end of the status phase. Recorded
    // on the report rather than drawn here, because this function has no deck access and inventing
    // one would put the draw somewhere the action-card deck does not know about.
    let ministers: Vec<PlayerId> = state
        .players
        .iter()
        .map(|player| player.id.clone())
        .filter(|player| crate::laws::draws_at_status_end(state, player))
        .collect();
    report.action_card_draws.extend(ministers);

    // 81.8 comes last: clearing earlier would turn later initiative reads into seating order.
    // A seat under Political Stability keeps the cards this step would return, and the
    // retained cards it spent during the round are readied: they stay in play, so a
    // spent one should be as ready as any card a player still holds.
    let holders: Vec<(PlayerId, Vec<StrategyCardId>, bool)> = state
        .players
        .iter()
        .map(|player| {
            (
                player.id.clone(),
                player.strategy_cards.clone(),
                player.stability,
            )
        })
        .collect();
    for (player_id, cards, retained) in holders {
        if retained {
            if let Some(player) = state.player_mut(&player_id) {
                player.exhausted_strategy_cards.clear();
                player.passed = false;
            }
            continue;
        }
        report
            .returned_strategy_cards
            .extend(cards.iter().cloned().map(|card| (player_id.clone(), card)));
        state.clear_strategy_cards(&player_id);
        if let Some(player) = state.player_mut(&player_id) {
            player.passed = false;
        }
    }
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
            ti4_model::id::StrategyCardId::new("pok1leadership"),
        );
        state.deal_strategy_card(
            &PlayerId::new("a"),
            ti4_model::id::StrategyCardId::new("pok8imperial"),
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
    fn leaders_ready_in_the_status_phase() {
        // 81.6. An exhausted agent that never readies reads, after a round or two, as a
        // player who has simply run out of agents.
        let players = [PlayerId::new("a")];
        let mut state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        state.phase = Phase::Status;
        crate::leaders::deploy(
            &mut state,
            ContentStore::embedded(),
            POK,
            &PlayerId::new("a"),
        );
        let Some(agent) = crate::leaders::of_kind(
            &state,
            ContentStore::embedded(),
            &PlayerId::new("a"),
            crate::leaders::AGENT,
        )
        .first()
        .cloned() else {
            return;
        };
        crate::leaders::exhaust(&mut state, &PlayerId::new("a"), &agent);

        let report = resolve_status_phase(&mut state).unwrap();

        assert_eq!(
            crate::leaders::status(&state, &PlayerId::new("a"), &agent),
            Some(ti4_model::state::LeaderStatus::Readied)
        );
        assert!(!report.readied_leaders.is_empty(), "and it is reported");
    }

    #[test]
    fn the_two_halves_compose_into_the_whole() {
        // The split exists so 81.5 can sit between them. If driving the halves by hand ever
        // stopped matching the single call, the phase would silently depend on how it was run.
        let players = [PlayerId::new("a"), PlayerId::new("b")];
        let mut whole = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        whole.phase = Phase::Status;
        let mut halves = whole.clone();

        let whole_report = resolve_status_phase(&mut whole).unwrap();

        let mut halves_report = resolve_before_token_gain(&mut halves).unwrap();
        resolve_after_token_gain(&mut halves, &mut halves_report);

        assert_eq!(whole_report, halves_report);
        assert!(whole.identical(&halves));
    }

    #[test]
    fn initiative_order_is_captured_before_step_818_destroys_it() {
        let players = [PlayerId::new("a"), PlayerId::new("b")];
        let mut state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        state.phase = Phase::Status;
        // Real corpus ids: a bare "leadership" is not a card, and an unknown card sorts at
        // initiative 99, which silently degrades the order back to seating.
        state.deal_strategy_card(
            &PlayerId::new("b"),
            ti4_model::id::StrategyCardId::new("pok1leadership"),
        );
        state.deal_strategy_card(
            &PlayerId::new("a"),
            ti4_model::id::StrategyCardId::new("pok8imperial"),
        );
        let expected = state.initiative_order();
        assert_eq!(
            expected,
            vec![PlayerId::new("b"), PlayerId::new("a")],
            "Leadership (1) precedes Imperial (8), against seating order"
        );

        let report = resolve_status_phase(&mut state).unwrap();

        assert_eq!(report.initiative_order, expected);
        assert_ne!(
            state.initiative_order(),
            report.initiative_order,
            "after 81.8 the live order has degraded to seating order"
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
