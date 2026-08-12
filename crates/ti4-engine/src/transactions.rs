//! Transactions between neighbours (LRR 60, and 21.5 for commodities).
//!
//! Ported from the oracle's `engine/transactions.py`: `_presence`, `are_neighbours`,
//! `_holdings`, `_can_pay`, `why_illegal`, `_take`, `_give` and `resolve`.

use std::collections::BTreeSet;

use ti4_content::galaxy::Galaxy;
use ti4_model::id::{PlayerId, SystemId};
use ti4_model::state::GameState;

/// What one side of a deal hands over.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Terms {
    pub trade_goods: i32,
    pub commodities: i32,
    /// Relic fragments by trait, one entry per fragment.
    pub fragments: Vec<String>,
}

impl Terms {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.trade_goods == 0 && self.commodities == 0 && self.fragments.is_empty()
    }
}

/// A proposed deal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub proposer: PlayerId,
    pub partner: PlayerId,
    pub given: Terms,
    pub received: Terms,
}

/// Why an offer cannot be resolved.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OfferError {
    #[error("a player cannot transact with themselves")]
    SamePlayer,
    #[error("{0} and {1} are not neighbours")]
    NotNeighbours(PlayerId, PlayerId),
    #[error("{0} cannot pay what they offered")]
    CannotPay(PlayerId),
    #[error("player {0} is not seated")]
    PlayerMissing(PlayerId),
    #[error("an empty transaction exchanges nothing")]
    Empty,
}

/// Systems where a player has a unit or controls a planet.
#[must_use]
pub fn presence(state: &GameState, player: &PlayerId) -> BTreeSet<SystemId> {
    let mut systems: BTreeSet<SystemId> = state
        .systems_with_units_of(player)
        .into_iter()
        .cloned()
        .collect();
    for (system_id, board) in &state.board {
        if board.controls_a_planet(player)
            || board
                .planet_units
                .values()
                .flatten()
                .any(|unit| &unit.owner == player)
        {
            systems.insert(system_id.clone());
        }
    }
    systems
}

/// LRR 60: neighbours share a system, or occupy adjacent ones.
///
/// Wormholes count because they are adjacency as far as the galaxy is concerned (60.2), so
/// nothing special is needed here — asking the galaxy is asking the right question.
#[must_use]
pub fn are_neighbours(state: &GameState, galaxy: &Galaxy, a: &PlayerId, b: &PlayerId) -> bool {
    if a == b {
        return false;
    }
    let (here, there) = (presence(state, a), presence(state, b));
    if here.intersection(&there).next().is_some() {
        return true;
    }
    here.iter().any(|system| {
        galaxy
            .adjacent(system.as_str())
            .into_iter()
            .any(|adjacent| there.contains(&SystemId::new(adjacent)))
    })
}

/// Everyone this player may transact with.
#[must_use]
pub fn neighbours(state: &GameState, galaxy: &Galaxy, player: &PlayerId) -> Vec<PlayerId> {
    state
        .seating_order
        .iter()
        .filter(|other| are_neighbours(state, galaxy, player, other))
        .cloned()
        .collect()
}

/// Whether a player holds what they offered.
#[must_use]
pub fn can_pay(state: &GameState, player: &PlayerId, terms: &Terms) -> bool {
    let Some(seat) = state.player(player) else {
        return false;
    };
    if terms.trade_goods > seat.trade_goods || terms.commodities > seat.commodities {
        return false;
    }
    let mut held = seat.relic_fragments.clone();
    for trait_name in &terms.fragments {
        let entry = held.entry(trait_name.clone()).or_insert(0);
        if *entry <= 0 {
            return false;
        }
        *entry -= 1;
    }
    true
}

/// Why an offer cannot be resolved, or `None` if it can.
#[must_use]
pub fn why_illegal(state: &GameState, galaxy: &Galaxy, offer: &Offer) -> Option<OfferError> {
    if offer.proposer == offer.partner {
        return Some(OfferError::SamePlayer);
    }
    for player in [&offer.proposer, &offer.partner] {
        if state.player(player).is_none() {
            return Some(OfferError::PlayerMissing(player.clone()));
        }
    }
    // A deal in which nothing changes hands is not a deal. Without this an "offer nothing for
    // nothing" resolves successfully, spends the pair's one transaction for the turn, and emits
    // a TRANSACTION event recording a trade that never happened.
    if offer.given.is_empty() && offer.received.is_empty() {
        return Some(OfferError::Empty);
    }
    if !are_neighbours(state, galaxy, &offer.proposer, &offer.partner) {
        return Some(OfferError::NotNeighbours(
            offer.proposer.clone(),
            offer.partner.clone(),
        ));
    }
    if !can_pay(state, &offer.proposer, &offer.given) {
        return Some(OfferError::CannotPay(offer.proposer.clone()));
    }
    if !can_pay(state, &offer.partner, &offer.received) {
        return Some(OfferError::CannotPay(offer.partner.clone()));
    }
    None
}

/// Take what a player is giving away.
fn take(state: &mut GameState, player: &PlayerId, terms: &Terms) {
    let Some(seat) = state.player_mut(player) else {
        return;
    };
    seat.trade_goods -= terms.trade_goods;
    seat.commodities -= terms.commodities;
    for trait_name in &terms.fragments {
        if let Some(held) = seat.relic_fragments.get_mut(trait_name) {
            *held -= 1;
        }
    }
    seat.relic_fragments.retain(|_, held| *held > 0);
}

/// Give a player what they are receiving.
fn give(state: &mut GameState, player: &PlayerId, terms: &Terms) {
    let Some(seat) = state.player_mut(player) else {
        return;
    };
    // 21.5: a commodity becomes a trade good the moment it changes hands. This is the whole
    // economy of the game — commodities are worthless to their owner and valuable to everyone
    // else, which is what makes a deal worth making.
    seat.trade_goods += terms.trade_goods + terms.commodities;
    for trait_name in &terms.fragments {
        *seat.relic_fragments.entry(trait_name.clone()).or_insert(0) += 1;
    }
}

/// Execute a transaction. Changes nothing and reports why if it is not legal.
///
/// # Errors
/// [`OfferError`] describing the first reason the deal cannot be made.
pub fn resolve(state: &mut GameState, galaxy: &Galaxy, offer: &Offer) -> Result<(), OfferError> {
    if let Some(reason) = why_illegal(state, galaxy, offer) {
        return Err(reason);
    }
    // Both sides are taken before either is given, so a deal cannot be paid for with what it
    // is about to receive.
    take(state, &offer.proposer, &offer.given);
    take(state, &offer.partner, &offer.received);
    give(state, &offer.partner, &offer.given);
    give(state, &offer.proposer, &offer.received);
    Ok(())
}

// -- opening a transaction on your turn (94.1a) -------------------------------------------------

/// The kind of the option that opens negotiations.
///
/// Deliberately not the action kind: a component action costs the whole turn (22.1) while a
/// transaction costs nothing and the turn continues. Filed as an action it would read, in the
/// decision log, like a player burning their turn on a trade.
pub const OPEN_KIND: &str = "open_transaction";
/// The kind of a proposed deal.
pub const OFFER_KIND: &str = "offer";
/// The kind of an answer to a proposal.
pub const ANSWER_KIND: &str = "transaction";

/// The prefix of an option that opens a transaction.
const OPEN_PREFIX: &str = "trade|";

/// Everyone this player may still open a transaction with this turn.
///
/// 94.1 allows one transaction per neighbour per turn, so a partner already dealt with is not
/// offered again — which is also what stops a free action from being taken forever.
#[must_use]
pub fn available_actions(
    state: &GameState,
    galaxy: &Galaxy,
    player: &PlayerId,
) -> Vec<crate::choice::ChoiceOption> {
    let already = state.transacted_with(player);
    neighbours(state, galaxy, player)
        .into_iter()
        .filter(|other| !already.contains(other))
        .map(|other| {
            crate::choice::ChoiceOption::labelled(
                format!("{OPEN_PREFIX}{other}"),
                OPEN_KIND,
                format!("open a transaction with {other}"),
            )
        })
        .collect()
}

/// The partner an opening option names, or `None` for any other option.
#[must_use]
pub fn opens_with(option: &crate::choice::ChoiceOption) -> Option<PlayerId> {
    option.id.strip_prefix(OPEN_PREFIX).map(PlayerId::new)
}

/// What one side holds that a deal can be built from.
fn holdings(state: &GameState, player: &PlayerId) -> (i32, i32) {
    state
        .player(player)
        .map_or((0, 0), |seat| (seat.trade_goods, seat.commodities))
}

/// The deals this player can put on the table.
///
/// Every shape is written once. A shape written twice is drawn twice as often by a sampling
/// decider, which is how "give 1 for 1" quietly became the table's favourite deal.
#[must_use]
pub fn offer_options(
    state: &GameState,
    proposer: &PlayerId,
    partner: &PlayerId,
) -> Vec<crate::choice::ChoiceOption> {
    let (mine_goods, mine_commodities) = holdings(state, proposer);
    let (their_goods, their_commodities) = holdings(state, partner);
    let mut options = Vec::new();
    let mut offer = |id: String, label: String| {
        options.push(crate::choice::ChoiceOption::labelled(id, OFFER_KIND, label));
    };

    // 21.5: a commodity becomes a trade good the moment it changes hands, so a straight swap
    // pays both sides. This is the standard Twilight Imperium deal and the reason the subsystem
    // exists — an engine that cannot propose it has a trade economy in name only.
    let swap = mine_commodities.min(their_commodities);
    if swap > 0 {
        offer(
            format!("cc{swap}"),
            format!("swap {swap} commodities each — both gain"),
        );
        if swap > 1 {
            offer("cc1".to_owned(), "swap 1 commodity each".to_owned());
        }
    }
    if mine_commodities > 0 && their_goods > 0 {
        let give = mine_commodities.min(3);
        let want = their_goods.min(give);
        if want > 0 {
            offer(
                format!("ct{give}:{want}"),
                format!("give {give} commodities for {want} trade goods"),
            );
        }
    }
    if their_commodities > 0 && mine_goods > 0 {
        let want = their_commodities.min(3);
        let give = mine_goods.min(want);
        if give > 0 {
            offer(
                format!("tc{give}:{want}"),
                format!("give {give} trade goods for {want} commodities"),
            );
        }
    }
    for give in 0..=mine_goods.min(3) {
        for want in 0..=their_goods.min(3) {
            if give == 0 && want == 0 {
                continue; // nothing for nothing is not an offer
            }
            offer(format!("{give}:{want}"), format!("give {give} for {want}"));
        }
    }
    if mine_commodities > 0 {
        offer(
            format!("c{mine_commodities}:0"),
            format!("gift {mine_commodities} commodities"),
        );
    }
    options
}

/// Split a `"{a}:{b}"` pair of numbers.
fn pair(text: &str) -> Option<(i32, i32)> {
    let (left, right) = text.split_once(':')?;
    Some((left.parse().ok()?, right.parse().ok()?))
}

/// The deal an offer option stands for.
///
/// Prefixes are tested longest-first: `"c3:0"` is a gift and `"cc3"` a swap, so a plain `"c"`
/// test placed first would read every swap as a gift of three commodities for nothing.
#[must_use]
pub fn offer_from(id: &str, proposer: &PlayerId, partner: &PlayerId) -> Option<Offer> {
    let goods = |n: i32| Terms {
        trade_goods: n,
        ..Terms::default()
    };
    let commodities = |n: i32| Terms {
        commodities: n,
        ..Terms::default()
    };
    let deal = |given: Terms, received: Terms| {
        Some(Offer {
            proposer: proposer.clone(),
            partner: partner.clone(),
            given,
            received,
        })
    };

    if let Some(rest) = id.strip_prefix("cc") {
        let many = rest.parse().ok()?;
        return deal(commodities(many), commodities(many));
    }
    if let Some(rest) = id.strip_prefix("ct") {
        let (give, want) = pair(rest)?;
        return deal(commodities(give), goods(want));
    }
    if let Some(rest) = id.strip_prefix("tc") {
        let (give, want) = pair(rest)?;
        return deal(goods(give), commodities(want));
    }
    if let Some(rest) = id.strip_prefix('c') {
        let (give, _) = pair(rest)?;
        return deal(commodities(give), Terms::default());
    }
    let (give, want) = pair(id)?;
    deal(goods(give), goods(want))
}

/// How a negotiation ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Traded {
    /// The deal was struck.
    Resolved,
    /// The proposer had nothing to offer, or chose to offer nothing.
    NothingOffered,
    /// The partner refused.
    Refused,
    /// A deal was put on the table and awaits an answer.
    Offered,
    /// The partner countered; negotiations continue.
    Countered,
    /// The offer on the table was not legal, so nothing happened.
    Rejected(OfferError),
}

/// Which side of the table a window is waiting on.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Stage {
    /// The proposer is choosing what to put on the table.
    Proposing,
    /// The partner is answering a proposal.
    Answering(Offer),
    /// Negotiations are over.
    Done,
}

/// An open negotiation between two neighbours (94.1a).
///
/// A counteroffer is the same structure seen from the other chair, so haggling needs no separate
/// representation — it is the offer, mirrored, going back the other way.
#[derive(Debug, Clone)]
pub struct TradeWindow {
    proposer: PlayerId,
    partner: PlayerId,
    stage: Stage,
    /// Proposals still to be answered. Two, as in the oracle: an offer and one counter.
    rounds_left: u8,
}

impl TradeWindow {
    /// Open negotiations, spending this pair's one transaction for the turn.
    ///
    /// 94.1 is spent on *opening*, not on closing. Charging only for a completed deal lets a
    /// player who keeps declining their own offer reopen the same talks without limit, which is
    /// both wrong and non-terminating.
    #[must_use]
    pub fn open(state: &mut GameState, proposer: &PlayerId, partner: &PlayerId) -> Self {
        state.record_transaction(proposer, partner);
        Self {
            proposer: proposer.clone(),
            partner: partner.clone(),
            stage: Stage::Proposing,
            rounds_left: 2,
        }
    }

    /// Whether negotiations have ended.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.stage == Stage::Done
    }

    /// The decision this negotiation owes, if any.
    #[must_use]
    pub fn pending_choice(&self, state: &GameState) -> Option<crate::choice::Choice> {
        match &self.stage {
            Stage::Done => None,
            Stage::Proposing => {
                let mut options = offer_options(state, &self.proposer, &self.partner);
                if options.is_empty() {
                    return None;
                }
                options.push(crate::choice::ChoiceOption::decline());
                Some(crate::choice::Choice::new(
                    self.proposer.clone(),
                    format!("transaction with {}", self.partner),
                    options,
                ))
            }
            Stage::Answering(offer) => Some(crate::choice::Choice::new(
                self.partner.clone(),
                format!("{} offers — accept?", offer.proposer),
                vec![
                    crate::choice::ChoiceOption::labelled("accept", ANSWER_KIND, "accept"),
                    crate::choice::ChoiceOption::labelled("counter", ANSWER_KIND, "counter-offer"),
                    crate::choice::ChoiceOption::decline(),
                ],
            )),
        }
    }

    /// Apply one answer.
    pub fn resolve(
        &mut self,
        state: &mut GameState,
        galaxy: &Galaxy,
        answer: &crate::choice::ChoiceOption,
    ) -> Traded {
        match std::mem::replace(&mut self.stage, Stage::Done) {
            Stage::Done => Traded::NothingOffered,
            Stage::Proposing => {
                if answer.is_decline() {
                    return Traded::NothingOffered;
                }
                let Some(offer) = offer_from(&answer.id, &self.proposer, &self.partner) else {
                    return Traded::NothingOffered;
                };
                // Legality is checked before the partner is troubled with it, so an offer that
                // could not be paid never reaches the table as though it could.
                if let Some(reason) = why_illegal(state, galaxy, &offer) {
                    return Traded::Rejected(reason);
                }
                self.stage = Stage::Answering(offer);
                Traded::Offered
            }
            Stage::Answering(offer) => {
                if answer.is_decline() {
                    return Traded::Refused;
                }
                if answer.id == "accept" {
                    return match resolve(state, galaxy, &offer) {
                        Ok(()) => Traded::Resolved,
                        Err(reason) => Traded::Rejected(reason),
                    };
                }
                self.rounds_left = self.rounds_left.saturating_sub(1);
                if self.rounds_left == 0 {
                    // Haggling is bounded. Without this the two seats can mirror the same deal
                    // back and forth for as long as the decider keeps countering.
                    return Traded::NothingOffered;
                }
                // The counter is the same negotiation from the other chair.
                std::mem::swap(&mut self.proposer, &mut self.partner);
                self.stage = Stage::Proposing;
                Traded::Countered
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::choice::ChoiceOption;
    use crate::fixtures::{game, plain_hub, put};

    fn a() -> PlayerId {
        PlayerId::new("a")
    }
    fn b() -> PlayerId {
        PlayerId::new("b")
    }

    fn goods(n: i32) -> Terms {
        Terms {
            trade_goods: n,
            ..Terms::default()
        }
    }
    fn commodities(n: i32) -> Terms {
        Terms {
            commodities: n,
            ..Terms::default()
        }
    }

    #[test]
    fn players_in_the_same_system_are_neighbours() {
        let hub = plain_hub();
        let mut state = game(&["a", "b"]);
        let centre = SystemId::new(hub.centre.clone());
        put(&mut state, &centre, "cruiser", &a(), 1);
        put(&mut state, &centre, "cruiser", &b(), 1);

        assert!(are_neighbours(&state, &hub.galaxy, &a(), &b()));
    }

    #[test]
    fn players_in_adjacent_systems_are_neighbours() {
        let hub = plain_hub();
        let mut state = game(&["a", "b"]);
        put(
            &mut state,
            &SystemId::new(hub.centre.clone()),
            "cruiser",
            &a(),
            1,
        );
        put(
            &mut state,
            &SystemId::new(hub.outer[0].clone()),
            "cruiser",
            &b(),
            1,
        );

        assert!(are_neighbours(&state, &hub.galaxy, &a(), &b()));
    }

    #[test]
    fn players_two_systems_apart_are_not() {
        let hub = plain_hub();
        let mut state = game(&["a", "b"]);
        let far = hub.across(&hub.outer[0]);
        put(
            &mut state,
            &SystemId::new(hub.outer[0].clone()),
            "cruiser",
            &a(),
            1,
        );
        put(&mut state, &SystemId::new(far), "cruiser", &b(), 1);

        assert!(!are_neighbours(&state, &hub.galaxy, &a(), &b()));
    }

    #[test]
    fn a_player_is_not_their_own_neighbour() {
        let hub = plain_hub();
        let mut state = game(&["a", "b"]);
        put(
            &mut state,
            &SystemId::new(hub.centre.clone()),
            "cruiser",
            &a(),
            1,
        );
        assert!(!are_neighbours(&state, &hub.galaxy, &a(), &a()));
    }

    #[test]
    fn a_commodity_becomes_a_trade_good_when_it_changes_hands() {
        // 21.5, and the whole economy of the game: a commodity is worthless to its owner and
        // valuable to everyone else, which is what makes a deal worth making.
        let hub = plain_hub();
        let mut state = game(&["a", "b"]);
        let centre = SystemId::new(hub.centre.clone());
        put(&mut state, &centre, "cruiser", &a(), 1);
        put(&mut state, &centre, "cruiser", &b(), 1);
        state.player_mut(&a()).unwrap().commodities = 3;

        let offer = Offer {
            proposer: a(),
            partner: b(),
            given: commodities(3),
            received: Terms::default(),
        };
        resolve(&mut state, &hub.galaxy, &offer).unwrap();

        assert_eq!(state.player(&a()).unwrap().commodities, 0);
        assert_eq!(
            state.player(&b()).unwrap().commodities,
            0,
            "it did not arrive as a commodity"
        );
        assert_eq!(
            state.player(&b()).unwrap().trade_goods,
            3,
            "it arrived as trade goods"
        );
    }

    #[test]
    fn a_deal_between_strangers_is_refused() {
        let hub = plain_hub();
        let mut state = game(&["a", "b"]);
        let far = hub.across(&hub.outer[0]);
        put(
            &mut state,
            &SystemId::new(hub.outer[0].clone()),
            "cruiser",
            &a(),
            1,
        );
        put(&mut state, &SystemId::new(far), "cruiser", &b(), 1);
        state.player_mut(&a()).unwrap().trade_goods = 5;
        let before = state.clone();

        let offer = Offer {
            proposer: a(),
            partner: b(),
            given: goods(2),
            received: Terms::default(),
        };
        assert_eq!(
            resolve(&mut state, &hub.galaxy, &offer),
            Err(OfferError::NotNeighbours(a(), b()))
        );
        assert!(state.identical(&before), "nothing changed hands");
    }

    #[test]
    fn a_deal_nobody_can_pay_changes_nothing() {
        let hub = plain_hub();
        let mut state = game(&["a", "b"]);
        let centre = SystemId::new(hub.centre.clone());
        put(&mut state, &centre, "cruiser", &a(), 1);
        put(&mut state, &centre, "cruiser", &b(), 1);
        let before = state.clone();

        let offer = Offer {
            proposer: a(),
            partner: b(),
            given: goods(99),
            received: Terms::default(),
        };
        assert_eq!(
            resolve(&mut state, &hub.galaxy, &offer),
            Err(OfferError::CannotPay(a()))
        );
        assert!(state.identical(&before));
    }

    #[test]
    fn a_deal_cannot_be_paid_for_with_what_it_receives() {
        // Both sides are taken before either is given, so a player cannot spend incoming
        // goods to fund their own half of the same deal.
        let hub = plain_hub();
        let mut state = game(&["a", "b"]);
        let centre = SystemId::new(hub.centre.clone());
        put(&mut state, &centre, "cruiser", &a(), 1);
        put(&mut state, &centre, "cruiser", &b(), 1);
        state.player_mut(&b()).unwrap().trade_goods = 4;

        let offer = Offer {
            proposer: a(),
            partner: b(),
            given: goods(2),
            received: goods(4),
        };
        assert_eq!(
            resolve(&mut state, &hub.galaxy, &offer),
            Err(OfferError::CannotPay(a())),
            "a holds nothing, so cannot give two whatever b is sending"
        );
    }

    #[test]
    fn fragments_change_hands_by_trait() {
        let hub = plain_hub();
        let mut state = game(&["a", "b"]);
        let centre = SystemId::new(hub.centre.clone());
        put(&mut state, &centre, "cruiser", &a(), 1);
        put(&mut state, &centre, "cruiser", &b(), 1);
        crate::exploration::gain_fragment(&mut state, &a(), "CULTURAL");

        let offer = Offer {
            proposer: a(),
            partner: b(),
            given: Terms {
                fragments: vec!["CULTURAL".to_owned()],
                ..Terms::default()
            },
            received: Terms::default(),
        };
        resolve(&mut state, &hub.galaxy, &offer).unwrap();

        assert!(
            state.player(&a()).unwrap().relic_fragments.is_empty(),
            "the empty pile is dropped, not left at zero"
        );
        assert_eq!(
            state.player(&b()).unwrap().relic_fragments.get("CULTURAL"),
            Some(&1)
        );
    }

    #[test]
    fn a_player_cannot_transact_with_themselves() {
        let hub = plain_hub();
        let mut state = game(&["a", "b"]);
        let offer = Offer {
            proposer: a(),
            partner: a(),
            given: Terms::default(),
            received: Terms::default(),
        };
        assert_eq!(
            resolve(&mut state, &hub.galaxy, &offer),
            Err(OfferError::SamePlayer)
        );
    }

    // -- the negotiation window ------------------------------------------------------------

    /// Two neighbours with something to trade.
    fn trading_partners() -> (crate::fixtures::Hub, GameState) {
        let hub = plain_hub();
        let mut state = game(&["a", "b"]);
        let centre = SystemId::new(hub.centre.clone());
        put(&mut state, &centre, "cruiser", &a(), 1);
        put(&mut state, &centre, "cruiser", &b(), 1);
        for player in [a(), b()] {
            let seat = state.player_mut(&player).unwrap();
            seat.trade_goods = 2;
            seat.commodities = 3;
        }
        (hub, state)
    }

    #[test]
    fn opening_a_transaction_spends_the_pair_for_the_turn() {
        // 94.1 is spent on opening, not on closing. If it were spent on closing, a seat that
        // keeps declining its own offer could reopen the same talks forever, and the free
        // action would never stop being offered.
        let (hub, mut state) = trading_partners();
        assert_eq!(
            available_actions(&state, &hub.galaxy, &a()).len(),
            1,
            "b is a neighbour and has not been dealt with"
        );

        let mut window = TradeWindow::open(&mut state, &a(), &b());
        let choice = window.pending_choice(&state).expect("a deal to propose");
        window.resolve(&mut state, &hub.galaxy, &ChoiceOption::decline());

        assert!(
            window.is_complete(),
            "declining to offer ends the negotiation"
        );
        assert!(
            available_actions(&state, &hub.galaxy, &a()).is_empty(),
            "the opportunity is gone even though no deal was struck"
        );
        assert!(choice.ids().contains(&"cc3"), "a swap was on the table");
    }

    #[test]
    fn a_commodity_swap_pays_both_sides() {
        // 21.5 is the whole economy: a commodity is worth nothing to its owner and a trade good
        // to anybody else, so the standard deal leaves both seats richer.
        let (hub, mut state) = trading_partners();
        let mut window = TradeWindow::open(&mut state, &a(), &b());

        let offered = window.resolve(
            &mut state,
            &hub.galaxy,
            &ChoiceOption::labelled("cc3", OFFER_KIND, ""),
        );
        assert_eq!(offered, Traded::Offered);
        let answering = window.pending_choice(&state).expect("b answers");
        assert_eq!(answering.player, b());

        let outcome = window.resolve(
            &mut state,
            &hub.galaxy,
            &ChoiceOption::labelled("accept", ANSWER_KIND, ""),
        );

        assert_eq!(outcome, Traded::Resolved);
        for player in [a(), b()] {
            let seat = state.player(&player).unwrap();
            assert_eq!(
                seat.commodities, 0,
                "{player} handed their commodities over"
            );
            assert_eq!(seat.trade_goods, 5, "{player} banked 2 + 3 traded goods");
        }
        assert!(window.is_complete());
    }

    #[test]
    fn a_counter_is_the_same_deal_from_the_other_chair() {
        let (hub, mut state) = trading_partners();
        let mut window = TradeWindow::open(&mut state, &a(), &b());
        window.resolve(
            &mut state,
            &hub.galaxy,
            &ChoiceOption::labelled("cc3", OFFER_KIND, ""),
        );

        let outcome = window.resolve(
            &mut state,
            &hub.galaxy,
            &ChoiceOption::labelled("counter", ANSWER_KIND, ""),
        );

        assert_eq!(outcome, Traded::Countered);
        let counter = window.pending_choice(&state).expect("b now proposes");
        assert_eq!(counter.player, b(), "the counter comes from the other side");
    }

    #[test]
    fn haggling_is_bounded() {
        // Two seats mirroring the same deal back and forth is a game that never advances. The
        // oracle answers two proposals and no more.
        let (hub, mut state) = trading_partners();
        let mut window = TradeWindow::open(&mut state, &a(), &b());
        let offer = ChoiceOption::labelled("cc3", OFFER_KIND, "");
        let counter = ChoiceOption::labelled("counter", ANSWER_KIND, "");

        window.resolve(&mut state, &hub.galaxy, &offer);
        window.resolve(&mut state, &hub.galaxy, &counter);
        window.resolve(&mut state, &hub.galaxy, &offer);
        let outcome = window.resolve(&mut state, &hub.galaxy, &counter);

        assert_eq!(outcome, Traded::NothingOffered);
        assert!(window.is_complete(), "the second counter ends it");
    }

    #[test]
    fn a_gift_is_not_a_swap() {
        // "c3:0" gifts three commodities; "cc3" swaps three each. A plain "c" prefix tested
        // first reads every swap as a gift, and one side silently gets nothing.
        let gift = offer_from("c3:0", &a(), &b()).unwrap();
        assert_eq!(gift.given.commodities, 3);
        assert!(gift.received.is_empty(), "a gift asks for nothing back");

        let swap = offer_from("cc3", &a(), &b()).unwrap();
        assert_eq!(swap.given.commodities, 3);
        assert_eq!(swap.received.commodities, 3);
    }

    #[test]
    fn every_offered_option_is_a_deal_that_can_be_paid_for() {
        // An option nobody can honour is an offer the partner is asked to accept and the engine
        // then rejects, which spends the turn's transaction on nothing.
        let (hub, mut state) = trading_partners();
        state.player_mut(&b()).unwrap().trade_goods = 0;

        for option in offer_options(&state, &a(), &b()) {
            let offer = offer_from(&option.id, &a(), &b())
                .unwrap_or_else(|| panic!("{} does not parse back into a deal", option.id));
            assert_eq!(
                why_illegal(&state, &hub.galaxy, &offer),
                None,
                "{} was offered but cannot be paid for",
                option.id
            );
        }
    }

    #[test]
    fn nothing_for_nothing_is_never_offered() {
        let (hub, state) = trading_partners();
        for option in offer_options(&state, &a(), &b()) {
            let offer = offer_from(&option.id, &a(), &b()).unwrap();
            assert!(
                !(offer.given.is_empty() && offer.received.is_empty()),
                "{} exchanges nothing",
                option.id
            );
        }
        let empty = Offer {
            proposer: a(),
            partner: b(),
            given: Terms::default(),
            received: Terms::default(),
        };
        assert_eq!(
            why_illegal(&state, &hub.galaxy, &empty),
            Some(OfferError::Empty)
        );
    }

    #[test]
    fn no_deal_shape_is_written_twice() {
        // A sampling decider draws per option, so a shape written twice is twice as likely as
        // an equally good one written once.
        let (_, state) = trading_partners();
        let options = offer_options(&state, &a(), &b());
        let mut deals: Vec<(Terms, Terms)> = options
            .iter()
            .map(|option| {
                let offer = offer_from(&option.id, &a(), &b()).unwrap();
                (offer.given, offer.received)
            })
            .collect();
        let before = deals.len();
        deals.dedup_by(|left, right| left == right);
        deals.sort_by_key(|deal| format!("{deal:?}"));
        deals.dedup();

        assert_eq!(before, deals.len(), "a deal shape appears more than once");
    }
}
