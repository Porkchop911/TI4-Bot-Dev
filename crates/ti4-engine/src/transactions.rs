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

#[cfg(test)]
mod tests {
    use super::*;
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
}
