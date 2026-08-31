//! Phase transitions and turn order.
//!
//! Ported from the oracle's `engine/game.py` — `strategy_pick_order`,
//! `_next_strategy_picker`, `_advance_turn`, and `_advance_phase`.
//!
//! These operate on a [`GameState`] directly rather than through a `Game` object, because
//! everything they need is state. The parts of the oracle's versions that call out to
//! laws, technology, promissory notes, and the event emitter are **not** here yet; see
//! [`advance_phase`] for exactly what is missing.

use ti4_model::id::PlayerId;
use ti4_model::state::{GameState, Phase};

/// Who picks a strategy card, in order, for the whole phase.
///
/// One card each is a single lap clockwise from the speaker. Two cards each — the three and
/// four player deal — snakes, so the speaker picks first and last and the seat that picked
/// last picks again immediately:
///
/// ```text
/// speaker -> A -> B -> B -> A -> speaker
/// ```
///
/// Without the reversal the speaker would take the best card of both laps and the last seat
/// the worst of both, which is the compensation the snake exists to pay.
///
/// A seat under Political Stability — holding strategy cards it kept instead of returning
/// them in the status phase — skips the draft entirely: the card says it does not choose
/// cards in the strategy phase that follows, and the retained cards are already in hand.
#[must_use]
pub fn strategy_pick_order(state: &GameState) -> Vec<PlayerId> {
    let seats = state.clockwise_from(&state.speaker);
    let seats: Vec<PlayerId> = seats
        .into_iter()
        .filter(|seat| state.player(seat).is_none_or(|p| !p.stability))
        .collect();
    let mut picks = Vec::with_capacity(seats.len() * state.strategy_cards_per_player);
    for lap in 0..state.strategy_cards_per_player {
        if lap % 2 == 0 {
            picks.extend(seats.iter().cloned());
        } else {
            picks.extend(seats.iter().rev().cloned());
        }
    }
    picks
}

/// The seat whose turn it is to pick, or `None` when the draft is done.
///
/// Indexed by how many cards have been dealt rather than by scanning for a seat that is
/// short of its quota: with a snake order the same seat picks twice in a row, and a scan
/// cannot tell that second pick from the first.
///
/// Seats under Political Stability dealt cards they kept, not cards they chose, so the
/// draft's progress counts only the picks made from the mat: total held minus what the
/// marked seats retained.
#[must_use]
pub fn next_strategy_picker(state: &GameState) -> Option<PlayerId> {
    let order = strategy_pick_order(state);
    if order.is_empty() || state.unclaimed_strategy_cards.is_empty() {
        return None;
    }
    let dealt: usize = state.players.iter().map(|p| p.strategy_cards.len()).sum();
    let retained: usize = state
        .players
        .iter()
        .filter(|p| p.stability)
        .map(|p| p.strategy_cards.len().min(state.strategy_cards_per_player))
        .sum();
    let picks_this_round = dealt.saturating_sub(retained);
    if picks_this_round >= order.len() {
        return None;
    }
    order.get(picks_this_round).cloned()
}

/// LRR 83.4: every card still on the mat gains a trade good.
///
/// Done as the strategy phase *ends*, once the draft is over, so a card cannot gain one and
/// be picked up in the same breath. Whoever eventually takes the card collects the pile with
/// it, which is the compensation the rules pay for going late — without it a low-initiative
/// card is worth exactly as much in round nine as in round one.
pub fn stock_unclaimed_cards(state: &mut GameState) {
    for card in state.unclaimed_strategy_cards.clone() {
        *state.strategy_card_goods.entry(card).or_insert(0) += 1;
    }
}

/// Start one action-phase turn.
///
/// Increments `turn_seq`, which is what the duration-scoped once-per-turn effects compare
/// against. Start-of-turn ability hooks (Military Support, technology triggers) are not
/// wired up yet.
pub fn begin_action_turn(state: &mut GameState, player: &PlayerId) {
    state.active = Some(player.clone());
    state.turn_seq += 1;
}

/// Pass the turn to the next player in initiative order who has not passed.
///
/// Returns the new active player, or `None` when everyone has passed.
///
/// LRR 94.1 allows one transaction per neighbour per turn, so the tally resets here.
/// Fleet Logistics — which lets a player retain the turn for a second action — is not
/// modelled yet; when it is, it must return early *before* the transaction reset, because
/// the two actions are explicitly the same turn.
pub fn advance_turn(state: &mut GameState) -> Option<PlayerId> {
    state.clear_transactions();
    if state.all_passed() {
        state.active = None;
        return None;
    }

    let order = state.initiative_order();
    if order.is_empty() {
        state.active = None;
        return None;
    }

    // Named rather than "next": initiative order is not seating order, and advancing by
    // seating hands the turn to the wrong player the moment the two disagree.
    let start = state
        .active
        .as_ref()
        .and_then(|current| order.iter().position(|p| p == current));
    for step in 1..=order.len() {
        // No active player yet: begin at the front of initiative order.
        let index = start.map_or(step - 1, |start| (start + step) % order.len());
        let candidate = &order[index];
        if state.player(candidate).is_some_and(|p| !p.passed) {
            let candidate = candidate.clone();
            begin_action_turn(state, &candidate);
            return Some(candidate);
        }
    }
    state.active = None;
    None
}

/// Begin the next game round: back to the strategy phase with a fresh deck.
///
/// The caller supplies the strategy card ids, since choosing them needs the content store.
pub fn begin_next_round(state: &mut GameState, strategy_cards: Vec<ti4_model::id::StrategyCardId>) {
    state.phase = Phase::Strategy;
    state.round += 1;
    state.active = None;
    state.unclaimed_strategy_cards = strategy_cards;
    // Lie in Wait counts *this* round's transactions.
    state.transactions_this_round.clear();
}

/// What [`advance_phase`] did, so a caller can drive the parts not modelled here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhaseOutcome {
    /// The action phase began; this player has the first turn.
    ActionBegan(PlayerId),
    /// The status phase began. Its steps are not implemented yet.
    StatusBegan,
    /// The agenda phase began. Its steps are not implemented yet.
    AgendaBegan,
    /// The round ended; the caller must supply the next round's strategy cards.
    RoundEnded,
}

/// Move the game to its next phase.
///
/// **Incomplete, deliberately.** The oracle's `_advance_phase` also runs, at the points
/// marked below, effects this engine does not have yet:
///
/// * end of strategy phase — Imperial Arbiter's card swap (which must precede reading
///   initiative order, because the swap is what changes it) and technology hooks;
/// * end of action phase — the whole status phase (LRR 81): scoring, objective reveal,
///   command token gain, repair, readying;
/// * end of status phase — the agenda phase (LRR 8).
///
/// `crate::status::resolve_status_phase` now supplies the choice-free bookkeeping. Its scoring
/// and command-token allocation windows remain deliberately unwired until M04-012 owns them.
/// `crate::agenda::resolve_agenda_phase` similarly owns only reveal/order/ready bookkeeping;
/// voting, ties, and effects remain deliberately unwired until M04-012.
/// Each returns a [`PhaseOutcome`] so a caller can see which step was reached rather than
/// silently getting a phase flag flipped past unimplemented rules.
pub fn advance_phase(state: &mut GameState) -> PhaseOutcome {
    match state.phase {
        Phase::Strategy => {
            stock_unclaimed_cards(state);
            // Political Stability is spent here: the retention covers the strategy phase
            // just played, and whatever the marked seat kept goes back to the mat in the
            // status phase that follows this action phase.
            for player in &mut state.players {
                player.stability = false;
            }
            // Imperial Arbiter and the technology hooks belong here, before the read.
            let order = state.initiative_order();
            state.phase = Phase::Action;
            state.active = None;
            let first = order.first().cloned().unwrap_or_else(|| PlayerId::new(""));
            begin_action_turn(state, &first);
            PhaseOutcome::ActionBegan(first)
        }
        Phase::Action => {
            state.phase = Phase::Status;
            state.active = None;
            PhaseOutcome::StatusBegan
        }
        Phase::Status => {
            // LRR 8.1, 27.4: the agenda phase exists only once the custodians token has
            // been lifted, and then it runs every round.
            if state.custodians_removed && !state.finished {
                state.phase = Phase::Agenda;
                PhaseOutcome::AgendaBegan
            } else {
                PhaseOutcome::RoundEnded
            }
        }
        Phase::Agenda => PhaseOutcome::RoundEnded,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use ti4_model::id::StrategyCardId;

    fn pid(id: &str) -> PlayerId {
        PlayerId::new(id)
    }

    fn card(id: &str) -> StrategyCardId {
        StrategyCardId::new(id)
    }

    fn deck() -> (Vec<StrategyCardId>, BTreeMap<StrategyCardId, i32>) {
        let names = [
            "leadership",
            "diplomacy",
            "politics",
            "construction",
            "trade",
            "warfare",
            "technology",
            "imperial",
        ];
        let ids: Vec<StrategyCardId> = names.iter().map(|n| card(n)).collect();
        let initiative = ids
            .iter()
            .enumerate()
            .map(|(i, c)| (c.clone(), i32::try_from(i).unwrap() + 1))
            .collect();
        (ids, initiative)
    }

    fn game(seats: &[&str], cards_each: usize) -> GameState {
        let ids: Vec<PlayerId> = seats.iter().map(|s| pid(s)).collect();
        let (deck, initiative) = deck();
        GameState::new(&ids, &deck, initiative, None, cards_each)
    }

    // -- the draft --------------------------------------------------------------

    #[test]
    fn one_card_each_is_a_single_lap_clockwise_from_the_speaker() {
        let mut g = game(&["a", "b", "c", "d", "e", "f"], 1);
        g.speaker = pid("c");
        assert_eq!(
            strategy_pick_order(&g),
            vec![pid("c"), pid("d"), pid("e"), pid("f"), pid("a"), pid("b")]
        );
    }

    #[test]
    fn two_cards_each_snakes_so_the_speaker_picks_first_and_last() {
        // Without the reversal the speaker takes the best card of both laps and the last
        // seat the worst of both, which is what the snake exists to prevent.
        let g = game(&["a", "b", "c"], 2);
        assert_eq!(
            strategy_pick_order(&g),
            vec![pid("a"), pid("b"), pid("c"), pid("c"), pid("b"), pid("a")]
        );
    }

    #[test]
    fn the_seat_that_picked_last_picks_again_immediately() {
        let g = game(&["a", "b", "c"], 2);
        let order = strategy_pick_order(&g);
        assert_eq!(order[2], order[3], "the turn does not move between laps");
    }

    #[test]
    fn the_picker_is_indexed_by_cards_dealt_not_by_who_is_short() {
        // A scan for a seat below quota cannot tell the snake's second pick from its
        // first, and would hand the double pick to the wrong seat.
        let mut g = game(&["a", "b", "c"], 2);
        assert_eq!(next_strategy_picker(&g), Some(pid("a")));

        g.deal_strategy_card(&pid("a"), card("leadership"));
        assert_eq!(next_strategy_picker(&g), Some(pid("b")));
        g.deal_strategy_card(&pid("b"), card("diplomacy"));
        assert_eq!(next_strategy_picker(&g), Some(pid("c")));
        g.deal_strategy_card(&pid("c"), card("politics"));
        assert_eq!(
            next_strategy_picker(&g),
            Some(pid("c")),
            "c picks twice in a row"
        );
    }

    #[test]
    fn the_draft_ends_when_every_seat_has_its_quota() {
        let mut g = game(&["a", "b"], 1);
        g.deal_strategy_card(&pid("a"), card("leadership"));
        g.deal_strategy_card(&pid("b"), card("diplomacy"));
        assert_eq!(next_strategy_picker(&g), None);
    }

    #[test]
    fn the_draft_ends_when_no_cards_remain() {
        let mut g = game(&["a", "b", "c"], 2);
        g.unclaimed_strategy_cards.clear();
        assert_eq!(next_strategy_picker(&g), None);
    }

    // -- unclaimed cards ---------------------------------------------------------

    #[test]
    fn a_card_left_on_the_mat_gains_a_trade_good() {
        let mut g = game(&["a", "b"], 1);
        g.unclaimed_strategy_cards = vec![card("imperial"), card("trade")];
        stock_unclaimed_cards(&mut g);
        assert_eq!(g.strategy_card_goods[&card("imperial")], 1);
        stock_unclaimed_cards(&mut g);
        assert_eq!(
            g.strategy_card_goods[&card("imperial")],
            2,
            "and again next round"
        );
    }

    #[test]
    fn stocking_happens_after_the_draft_so_a_card_cannot_gain_and_be_taken_at_once() {
        let mut g = game(&["a", "b"], 1);
        g.unclaimed_strategy_cards = vec![card("imperial")];
        advance_phase(&mut g);
        assert_eq!(g.strategy_card_goods[&card("imperial")], 1);
    }

    // -- turn order ---------------------------------------------------------------

    #[test]
    fn the_turn_follows_initiative_order_not_seating_order() {
        let mut g = game(&["a", "b", "c"], 1);
        g.deal_strategy_card(&pid("a"), card("imperial")); // initiative 8
        g.deal_strategy_card(&pid("b"), card("leadership")); // initiative 1
        g.deal_strategy_card(&pid("c"), card("trade")); // initiative 5

        assert_eq!(advance_turn(&mut g), Some(pid("b")));
        assert_eq!(advance_turn(&mut g), Some(pid("c")));
        assert_eq!(advance_turn(&mut g), Some(pid("a")));
        assert_eq!(advance_turn(&mut g), Some(pid("b")), "and wraps round");
    }

    #[test]
    fn a_player_who_has_passed_is_skipped() {
        let mut g = game(&["a", "b", "c"], 1);
        g.deal_strategy_card(&pid("a"), card("leadership"));
        g.deal_strategy_card(&pid("b"), card("diplomacy"));
        g.deal_strategy_card(&pid("c"), card("politics"));
        g.player_mut(&pid("b")).unwrap().passed = true;

        assert_eq!(advance_turn(&mut g), Some(pid("a")));
        assert_eq!(advance_turn(&mut g), Some(pid("c")), "b is skipped");
    }

    #[test]
    fn everyone_having_passed_leaves_nobody_active() {
        let mut g = game(&["a", "b"], 1);
        for player in &mut g.players {
            player.passed = true;
        }
        assert_eq!(advance_turn(&mut g), None);
        assert_eq!(g.active, None);
    }

    #[test]
    fn each_turn_advances_the_turn_sequence() {
        // The once-per-turn effects compare against this, so it must move exactly once.
        let mut g = game(&["a", "b"], 1);
        g.deal_strategy_card(&pid("a"), card("leadership"));
        g.deal_strategy_card(&pid("b"), card("diplomacy"));

        assert_eq!(g.turn_seq, 0);
        advance_turn(&mut g);
        assert_eq!(g.turn_seq, 1);
        advance_turn(&mut g);
        assert_eq!(g.turn_seq, 2);
    }

    #[test]
    fn passing_the_turn_clears_the_transaction_tally() {
        // LRR 94.1 is once per neighbour per turn.
        let mut g = game(&["a", "b"], 1);
        g.deal_strategy_card(&pid("a"), card("leadership"));
        g.deal_strategy_card(&pid("b"), card("diplomacy"));
        g.record_transaction(&pid("a"), &pid("b"));
        assert!(!g.transacted_with(&pid("a")).is_empty());

        advance_turn(&mut g);
        assert!(g.transacted_with(&pid("a")).is_empty());
    }

    // -- phase transitions ----------------------------------------------------------

    #[test]
    fn the_strategy_phase_hands_the_first_turn_to_lowest_initiative() {
        let mut g = game(&["a", "b"], 1);
        g.deal_strategy_card(&pid("a"), card("imperial"));
        g.deal_strategy_card(&pid("b"), card("leadership"));

        assert_eq!(advance_phase(&mut g), PhaseOutcome::ActionBegan(pid("b")));
        assert_eq!(g.phase, Phase::Action);
        assert_eq!(g.active, Some(pid("b")));
        assert_eq!(g.turn_seq, 1);
    }

    #[test]
    fn the_action_phase_is_followed_by_the_status_phase() {
        let mut g = game(&["a", "b"], 1);
        g.phase = Phase::Action;
        assert_eq!(advance_phase(&mut g), PhaseOutcome::StatusBegan);
        assert_eq!(g.phase, Phase::Status);
        assert_eq!(g.active, None);
    }

    #[test]
    fn there_is_no_agenda_phase_until_the_custodians_token_is_lifted() {
        // LRR 8.1, 27.4.
        let mut g = game(&["a", "b"], 1);
        g.phase = Phase::Status;
        assert_eq!(advance_phase(&mut g), PhaseOutcome::RoundEnded);
        assert_eq!(g.phase, Phase::Status, "the round ends instead");
    }

    #[test]
    fn once_the_custodians_token_is_lifted_every_round_has_an_agenda_phase() {
        let mut g = game(&["a", "b"], 1);
        g.phase = Phase::Status;
        g.custodians_removed = true;
        assert_eq!(advance_phase(&mut g), PhaseOutcome::AgendaBegan);
        assert_eq!(g.phase, Phase::Agenda);

        assert_eq!(advance_phase(&mut g), PhaseOutcome::RoundEnded);
    }

    #[test]
    fn a_finished_game_does_not_open_an_agenda_phase() {
        let mut g = game(&["a", "b"], 1);
        g.phase = Phase::Status;
        g.custodians_removed = true;
        g.finished = true;
        assert_eq!(advance_phase(&mut g), PhaseOutcome::RoundEnded);
    }

    #[test]
    fn a_new_round_returns_to_the_strategy_phase_with_a_fresh_deck() {
        let mut g = game(&["a", "b"], 1);
        g.phase = Phase::Status;
        g.deal_strategy_card(&pid("a"), card("leadership"));
        g.unclaimed_strategy_cards.clear();

        let (fresh, _) = deck();
        begin_next_round(&mut g, fresh.clone());
        assert_eq!(g.phase, Phase::Strategy);
        assert_eq!(g.round, 2);
        assert_eq!(g.active, None);
        assert_eq!(g.unclaimed_strategy_cards, fresh);
    }

    #[test]
    fn a_full_phase_cycle_returns_to_where_it_started() {
        let mut g = game(&["a", "b"], 1);
        g.deal_strategy_card(&pid("a"), card("leadership"));
        g.deal_strategy_card(&pid("b"), card("diplomacy"));

        assert!(matches!(
            advance_phase(&mut g),
            PhaseOutcome::ActionBegan(_)
        ));
        assert_eq!(advance_phase(&mut g), PhaseOutcome::StatusBegan);
        assert_eq!(advance_phase(&mut g), PhaseOutcome::RoundEnded);

        let (fresh, _) = deck();
        begin_next_round(&mut g, fresh);
        assert_eq!(g.phase, Phase::Strategy);
        assert_eq!(g.round, 2);
    }
}
