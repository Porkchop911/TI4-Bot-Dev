//! Transactions between neighbours (LRR 60, and 21.5 for commodities).
//!
//! Ported from the oracle's `engine/transactions.py`: `_presence`, `are_neighbours`,
//! `_holdings`, `_can_pay`, `why_illegal`, `_take`, `_give` and `resolve`.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{ActionCardId, FactionId, PlayerId, SecretObjectiveId, SystemId};
use ti4_model::state::{GameState, TransientFlags};

use crate::decision_context::{DecisionContext, DecisionSource, DecisionTarget};

/// What one side of a deal hands over.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Terms {
    pub trade_goods: i32,
    pub commodities: i32,
    /// Relic fragments by trait, one entry per fragment.
    pub fragments: Vec<String>,
    /// A promissory note, by id.
    ///
    /// A note is a loan rather than a sale — every one of them says "then, return this card" —
    /// which is why what it costs to part with is not what it is worth to receive.
    pub promissory: Option<String>,
    /// An action card, by alias. 94.3 forbids exchanging them; Hacan's Arbiters is the
    /// exception, so naming one here is legal only for a table where somebody has that ability.
    pub action_card: Option<ActionCardId>,
    /// An unscored secret objective, by alias. Black Market Dealings is the one card that lets
    /// these change hands at the table; like an action card its value is entirely situational,
    /// so it prices flat in both directions.
    pub secret: Option<SecretObjectiveId>,
}

impl Terms {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.trade_goods == 0
            && self.commodities == 0
            && self.fragments.is_empty()
            && self.promissory.is_none()
            && self.action_card.is_none()
            && self.secret.is_none()
    }

    /// How these terms read in a prompt — the oracle's `Terms.describe`, same parts, order and
    /// words. An empty side reads as "nothing", which is how a gift stays readable.
    #[must_use]
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if self.trade_goods > 0 {
            parts.push(format!("{} trade goods", self.trade_goods));
        }
        if self.commodities > 0 {
            parts.push(format!("{} commodities", self.commodities));
        }
        if !self.fragments.is_empty() {
            parts.push(format!("{} relic fragments", self.fragments.len()));
        }
        if let Some(note) = &self.promissory {
            parts.push(note.clone());
        }
        if let Some(card) = &self.action_card {
            parts.push(format!("the action card {card}"));
        }
        if let Some(secret) = &self.secret {
            parts.push(format!("the secret objective {secret}"));
        }
        if parts.is_empty() {
            "nothing".to_owned()
        } else {
            parts.join(", ")
        }
    }

    /// What receiving these terms is worth, in trade goods (oracle `worth_to_receiver`).
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // fragment counts are single digits
    pub fn worth_to_receiver(&self, state: &GameState, content: &ContentStore) -> f64 {
        let note = self
            .promissory
            .as_deref()
            .map_or(0.0, |note| note_worth(state, content, note));
        // An action card prices flat at one trade good in both directions (oracle Terms):
        // there is no worth table for them because their value is entirely situational. A
        // secret objective takes the same flat one: its printed value (a few victory points)
        // is worth exactly as much as what it is, and the net the decider sees is the one
        // that matters at the table.
        f64::from(self.trade_goods)
            + f64::from(self.commodities)
            + self.fragments.len() as f64
            + note
            + f64::from(self.action_card.is_some())
            + f64::from(self.secret.is_some())
    }

    /// What giving these terms costs (oracle `cost_to_giver`): commodities barely, since a swap
    /// turns each into a trade good anyway.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // fragment counts are single digits
    pub fn cost_to_giver(&self, state: &GameState, content: &ContentStore) -> f64 {
        let note = self
            .promissory
            .as_deref()
            .map_or(0.0, |note| note_cost(state, content, note));
        // Flat one trade good to part with as well — the oracle prices it identically in
        // both directions, and the secret takes the same flat line.
        f64::from(self.trade_goods)
            + 0.2 * f64::from(self.commodities)
            + self.fragments.len() as f64
            + note
            + f64::from(self.action_card.is_some())
            + f64::from(self.secret.is_some())
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

impl Offer {
    /// "{faction} gives X for Y" — the oracle's `Offer.describe`, where the speaker is named by
    /// faction, which in the oracle is its player identity.
    #[must_use]
    pub fn describe(&self, state: &GameState) -> String {
        format!(
            "{} gives {} for {}",
            faction_name(state, &self.proposer),
            self.given.describe(),
            self.received.describe()
        )
    }
}

/// One thing that a side of a deal hands over, as a client names it (display only): the cards by
/// the ids of their records in the catalogue, so that a client shows their printed names and
/// texts without reading the ids of the engine. [`TradeDeal`] lists them.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum TradeItem {
    TradeGoods {
        count: i32,
    },
    Commodities {
        count: i32,
    },
    /// Relic fragments of one trait ("cultural", "hazardous", "industrial", "frontier").
    RelicFragments {
        #[serde(rename = "trait")]
        fragment: String,
        count: i32,
    },
    /// A promissory note: `card` is its record ([`crate::promissory::record_id`]), `owner` the
    /// faction whose copy it is (the one that "<color>" means in its text).
    Note {
        card: String,
        owner: FactionId,
    },
    ActionCard {
        card: ActionCardId,
    },
    SecretObjective {
        card: SecretObjectiveId,
    },
}

impl Terms {
    /// These terms as [`TradeItem`]s, in the order of [`Terms::describe`]. Empty for nothing.
    #[must_use]
    pub fn items(&self, content: &ContentStore) -> Vec<TradeItem> {
        let mut items = Vec::new();
        if self.trade_goods > 0 {
            items.push(TradeItem::TradeGoods {
                count: self.trade_goods,
            });
        }
        if self.commodities > 0 {
            items.push(TradeItem::Commodities {
                count: self.commodities,
            });
        }
        let mut fragments: BTreeMap<String, i32> = BTreeMap::new();
        for fragment in &self.fragments {
            *fragments
                .entry(fragment_key(fragment).to_ascii_lowercase())
                .or_insert(0) += 1;
        }
        items.extend(
            fragments
                .into_iter()
                .map(|(fragment, count)| TradeItem::RelicFragments { fragment, count }),
        );
        if let Some(note) = &self.promissory {
            items.push(TradeItem::Note {
                card: crate::promissory::record_id(content, note),
                owner: FactionId::new(crate::promissory::owner_of(note).unwrap_or_default()),
            });
        }
        if let Some(card) = &self.action_card {
            items.push(TradeItem::ActionCard { card: card.clone() });
        }
        if let Some(secret) = &self.secret {
            items.push(TradeItem::SecretObjective {
                card: secret.clone(),
            });
        }
        items
    }
}

/// The contents of one deal (display only): what the proposer hands to the partner, and what the
/// proposer receives from the partner. The seats are those of [`TransactionHints`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TradeDeal {
    pub given: Vec<TradeItem>,
    pub received: Vec<TradeItem>,
}

impl TradeDeal {
    #[must_use]
    pub fn of(offer: &Offer, content: &ContentStore) -> Self {
        Self {
            given: offer.given.items(content),
            received: offer.received.items(content),
        }
    }
}

/// The look-ahead of a negotiation's questions, `details.transaction`, written with
/// [`Choice::hinted`](crate::choice::Choice::hinted) (display only; labels, ids and payloads are
/// unchanged). The question of the proposer has `offers`, the deal of each offer option by its id
/// (the decline has none), and no `offer`; the question of the partner has `offer`, the deal on
/// the table that it accepts, refuses or counters, and `offers` is empty.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TransactionHints {
    /// The seat that proposes: it gives `given` and receives `received`.
    pub proposer: PlayerId,
    /// The seat that the proposal is made to.
    pub partner: PlayerId,
    #[serde(default)]
    pub offers: BTreeMap<String, TradeDeal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offer: Option<TradeDeal>,
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
    #[error("action cards cannot be exchanged (94.3)")]
    ActionCardsNotTradeable,
    #[error("{0} does not hold {1}")]
    MissingActionCard(PlayerId, String),
    /// Mahact, Hubris: nobody may give the Mahact player their Alliance note.
    #[error("{0} cannot be given {1}")]
    NoteRefused(PlayerId, String),
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
    // I.I.H.Q. Modernization (Keleres): neighbours with everyone in or next to Mecatol Rex.
    if crate::factions::keleres::mecatol_neighbours(state, galaxy, a, b) {
        return true;
    }
    let (here, there) = (presence(state, a), presence(state, b));
    if here.intersection(&there).next().is_some() {
        return true;
    }
    // Adjacency is either neighbour's: breach links are everyone's, a Quantum Entanglement or
    // Lazax Gate Folding link is its holder's. Only built when the plain map says "not adjacent".
    let touches = |from: &BTreeSet<SystemId>, to: &BTreeSet<SystemId>, owner: &PlayerId| {
        from.iter().any(|system| {
            galaxy
                .adjacent(system.as_str())
                .into_iter()
                .any(|adjacent| to.contains(&SystemId::new(adjacent)))
        }) || {
            let adjacency = adjacency_of(state, galaxy, owner);
            from.iter().any(|system| {
                adjacency
                    .neighbours(system.as_str())
                    .iter()
                    .any(|adjacent| to.contains(&SystemId::new(adjacent.as_str())))
            })
        }
    };
    touches(&here, &there, a) || touches(&there, &here, b)
}

/// `player`'s adjacency on `galaxy`. The movement hooks that add links (Crimson, Creuss, Winnu)
/// read neither the content nor the sources, so the embedded store and default scope stand in
/// for callers that hold neither.
pub(crate) fn adjacency_of<'g>(
    state: &GameState,
    galaxy: &'g Galaxy,
    player: &PlayerId,
) -> crate::movement::PlayerAdjacency<'g> {
    crate::movement::PlayerAdjacency::new(
        state,
        ti4_content::ContentStore::embedded(),
        ti4_model::content_types::DEFAULT,
        galaxy,
        player,
    )
}

/// Neighbours of `player` who have resolved a transaction this round.
///
/// Lie in Wait fires "after 2 of your neighbors resolve a transaction". A neighbour who traded
/// twice counts once: the card looks at *players'* hands, so the same seat twice is one hand.
#[must_use]
pub fn neighbours_who_transacted(
    state: &GameState,
    galaxy: &Galaxy,
    player: &PlayerId,
) -> Vec<PlayerId> {
    let mine = neighbours(state, galaxy, player);
    let mut seen: Vec<PlayerId> = Vec::new();
    for (a, b) in &state.transactions_this_round {
        for who in [a, b] {
            if who != player && mine.contains(who) && !seen.contains(who) {
                seen.push(who.clone());
            }
        }
    }
    seen
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

/// Everyone this player may transact with, given their faction.
///
/// Hacan's Guild Ships makes the whole table a neighbour, so the map stops deciding who they may
/// deal with. Kept beside `neighbours` rather than inside it, because 60.1 is a fact about the
/// board and this is a fact about a card.
#[must_use]
pub fn partners(
    state: &GameState,
    content: &ContentStore,
    galaxy: &Galaxy,
    player: &PlayerId,
) -> Vec<PlayerId> {
    // Trade Convoys does for a note what Guild Ships does for a faction.
    if crate::faction_abilities::ignores_neighbours(state, content, player)
        || crate::promissory::reaches_anyone(state, player)
    {
        return state
            .seating_order
            .iter()
            .filter(|other| *other != player)
            .cloned()
            .collect();
    }
    neighbours(state, galaxy, player)
}

/// Whether these two seats may transact with each other: neighbours, or either side able to reach
/// past adjacency - Guild Ships for Hacan, the Trade Convoys note for whoever holds it.
///
/// Both directions count. The ability belongs to whoever holds it, and a negotiation has two parties,
/// so a far Hacan being dealt with is that Hacan negotiating too: Xxcha, not Hacan's neighbour, may
/// still trade notes and goods with Hacan. Offering and legality both ask this, so what a contact
/// offers and what settlement accepts are the same question.
#[must_use]
pub fn may_transact(
    state: &GameState,
    content: &ContentStore,
    galaxy: &Galaxy,
    proposer: &PlayerId,
    partner: &PlayerId,
) -> bool {
    crate::factions::hooks_cards::transaction_reach(state, content, proposer, partner)
        || are_neighbours(state, galaxy, proposer, partner)
        || partners(state, content, galaxy, proposer).contains(partner)
        || partners(state, content, galaxy, partner).contains(proposer)
}

/// The key a relic fragment is held under: its trait upper-cased, as exploration stores it
/// (`CULTURAL`), with the diplomacy asset's `unknown` meaning the frontier deck's `FRONTIER`.
///
/// Diplomacy transfers name fragments in lower case and the legacy window in the stored case, and
/// comparing them exactly made every diplomacy deal with a fragment "cannot pay what they offered".
#[must_use]
pub fn fragment_key(trait_name: &str) -> String {
    let upper = trait_name.to_ascii_uppercase();
    if upper == "UNKNOWN" {
        "FRONTIER".to_owned()
    } else {
        upper
    }
}

/// Whether a player holds what they offered.
#[must_use]
pub fn can_pay(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    terms: &Terms,
) -> bool {
    let Some(seat) = state.player(player) else {
        return false;
    };
    if terms.trade_goods > seat.trade_goods || terms.commodities > seat.commodities {
        return false;
    }
    if let Some(note) = &terms.promissory {
        // Support is tracked by position rather than in the note map, so it is asked about
        // separately — and a note already lent out is not yours to lend again.
        let holds = if note.starts_with(crate::promissory::SUPPORT_PREFIX) {
            crate::promissory::available_support(state, player).as_deref() == Some(note.as_str())
        } else {
            crate::promissory::available_notes(state, content, player).contains(note)
        };
        if !holds {
            return false;
        }
    }
    let mut held = seat.relic_fragments.clone();
    for trait_name in &terms.fragments {
        let entry = held.entry(fragment_key(trait_name)).or_insert(0);
        if *entry <= 0 {
            return false;
        }
        *entry -= 1;
    }
    if let Some(secret) = &terms.secret {
        // Only what is unscored can change hands: a secret already scored belongs to the
        // objective board, not to the player's hand.
        if !seat.secret_objectives.iter().any(|held| held == secret) {
            return false;
        }
    }
    true
}

/// Why an offer cannot be resolved, or `None` if it can.
#[must_use]
pub fn why_illegal(
    state: &GameState,
    content: &ContentStore,
    galaxy: &Galaxy,
    offer: &Offer,
) -> Option<OfferError> {
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
    // Neighbours bound transactions during a turn; during the agenda phase any two players may
    // transact (94), which is when votes are bought.
    //
    // The reach test is `may_transact`, not bare `are_neighbours`, and that difference is Hacan's Guild
    // Ships ("You can negotiate transactions with players who are not your neighbor") and the Trade
    // Convoys note. Checking adjacency here while the offer list asked `partners` meant a Hacan seat was
    // offered a partner across the board and refused the moment it accepted - a legal action rejected
    // late, which from a seat is indistinguishable from a broken button.
    if state.phase != ti4_model::state::Phase::Agenda
        && !may_transact(state, content, galaxy, &offer.proposer, &offer.partner)
    {
        return Some(OfferError::NotNeighbours(
            offer.proposer.clone(),
            offer.partner.clone(),
        ));
    }
    if !can_pay(state, content, &offer.proposer, &offer.given) {
        return Some(OfferError::CannotPay(offer.proposer.clone()));
    }
    if !can_pay(state, content, &offer.partner, &offer.received) {
        return Some(OfferError::CannotPay(offer.partner.clone()));
    }
    for (receiver, terms) in [
        (&offer.partner, &offer.given),
        (&offer.proposer, &offer.received),
    ] {
        if let Some(note) = &terms.promissory
            && !crate::promissory::may_receive(state, receiver, note)
        {
            return Some(OfferError::NoteRefused(receiver.clone(), note.clone()));
        }
    }
    // 94.3: action cards are not tradeable unless somebody at the table has Arbiters, or
    // Black Market Dealings is marking this negotiation as one in which they may change hands
    // — and each side must hold whatever its own leg hands over (can_pay covers goods, notes
    // and fragments; a card is checked here, in the oracle's order).
    if offer.given.action_card.is_some() || offer.received.action_card.is_some() {
        let black_market = state
            .transient_flags
            .has(ti4_model::state::TransientFlags::BLACK_MARKET);
        let arbiters = black_market
            || trades_action_cards(state, content, &offer.proposer)
            || trades_action_cards(state, content, &offer.partner);
        if !arbiters {
            return Some(OfferError::ActionCardsNotTradeable);
        }
        for (side, terms) in [
            (&offer.proposer, &offer.given),
            (&offer.partner, &offer.received),
        ] {
            let card = terms.action_card.as_ref();
            if let Some(card) = card
                && !state
                    .player(side)
                    .is_some_and(|seat| seat.action_cards.iter().any(|held| held == card))
            {
                return Some(OfferError::MissingActionCard(
                    side.clone(),
                    card.as_str().to_owned(),
                ));
            }
        }
    }
    None
}

/// Take what a player is giving away.
fn take(state: &mut GameState, player: &PlayerId, terms: &Terms) {
    let Some(seat) = state.player_mut(player) else {
        return;
    };
    if let Some(card) = &terms.action_card {
        // The oracle removes the first matching card; a hand can in principle hold two.
        let mut left = true;
        seat.action_cards.retain(|held| {
            if left && held == card {
                left = false;
                return false;
            }
            true
        });
    }
    if let Some(secret) = &terms.secret {
        // The first matching objective, as with a duplicated action card in the same deal.
        let mut left = true;
        seat.secret_objectives.retain(|held| {
            if left && held == secret {
                left = false;
                return false;
            }
            true
        });
    }
    seat.trade_goods -= terms.trade_goods;
    seat.commodities -= terms.commodities;
    for trait_name in &terms.fragments {
        if let Some(held) = seat.relic_fragments.get_mut(&fragment_key(trait_name)) {
            *held -= 1;
        }
    }
    seat.relic_fragments.retain(|_, held| *held > 0);
}

/// Give a player what they are receiving.
fn give(state: &mut GameState, content: &ContentStore, player: &PlayerId, terms: &Terms) {
    let Some(seat) = state.player_mut(player) else {
        return;
    };
    if let Some(card) = terms.action_card.clone() {
        seat.action_cards.push(card);
    }
    if let Some(secret) = terms.secret.clone() {
        // A secret received at the table joins the hand; if that pushes it over the limit,
        // the overage is returned on the next draw that enforces it — the same convention
        // every other card that awards secrets follows, because the table is not in reach
        // from here to ask which one to return now.
        seat.secret_objectives.push(secret);
    }
    // 21.5: a commodity becomes a trade good the moment it changes hands. This is the whole
    // economy of the game — commodities are worthless to their owner and valuable to everyone
    // else, which is what makes a deal worth making.
    seat.trade_goods += terms.trade_goods + terms.commodities;
    for trait_name in &terms.fragments {
        *seat
            .relic_fragments
            .entry(fragment_key(trait_name))
            .or_insert(0) += 1;
    }
    if let Some(note) = terms.promissory.clone() {
        // Support is worth a victory point the moment it arrives, which is the whole reason the
        // note is worth trading for; every other note simply changes hands.
        if note.starts_with(crate::promissory::SUPPORT_PREFIX) {
            crate::promissory::receive(state, player, &note);
        } else {
            crate::promissory::take(state, content, player, &note);
        }
    }
}

/// Execute a transaction. Changes nothing and reports why if it is not legal.
///
/// # Errors
/// [`OfferError`] describing the first reason the deal cannot be made.
pub fn resolve(
    state: &mut GameState,
    content: &ContentStore,
    galaxy: &Galaxy,
    offer: &Offer,
) -> Result<(), OfferError> {
    if let Some(reason) = why_illegal(state, content, galaxy, offer) {
        return Err(reason);
    }
    // Both sides are taken before either is given, so a deal cannot be paid for with what it
    // is about to receive.
    take(state, &offer.proposer, &offer.given);
    take(state, &offer.partner, &offer.received);
    give(state, content, &offer.partner, &offer.given);
    give(state, content, &offer.proposer, &offer.received);
    // 21.5: received commodities arrive as trade goods, so both count as trade goods gained.
    crate::supply::note_trade_goods_gained(
        state,
        &offer.partner,
        offer.given.trade_goods + offer.given.commodities,
        "transaction",
    );
    crate::supply::note_trade_goods_gained(
        state,
        &offer.proposer,
        offer.received.trade_goods + offer.received.commodities,
        "transaction",
    );
    // Dark Pact: "When you give a number of commodities to the Empyrean player equal to your
    // maximum commodity value, you each gain 1 trade good."
    crate::factions::empyrean_units::dark_pact_gains(state, content, offer);
    // Recorded here rather than at the window that opened the deal: this is the one place a
    // transaction is *resolved*, and Lie in Wait counts resolutions.
    state
        .transactions_this_round
        .push((offer.proposer.clone(), offer.partner.clone()));
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

/// The prefix of an option that opens a transaction. Component actions carry the full
/// `component|` prefix in both engines; trade was the one Rust shape missing it, which made its
/// id tokenize differently at every action-phase decision.
const OPEN_PREFIX: &str = "component|trade|";

/// Whether this player's faction carries the Arbiters ability — the one table at which
/// action cards may change hands (94.3). Resolved through the faction record's abilities,
/// exactly as the oracle resolves `trades_action_cards`.
#[must_use]
pub fn trades_action_cards(state: &GameState, content: &ContentStore, player: &PlayerId) -> bool {
    let Some(seat) = state.player(player) else {
        return false;
    };
    let alias = seat.faction.as_str();
    content.factions(DEFAULT).any(|record| {
        record.id() == Some(alias) && record.strings("abilities").contains(&"arbiters")
    })
}

// Hacan's Arbiters (94.3 exception): sell the alphabetically first card of the hand for one
// trade good — offered when either chair at the table has the ability and can pay it, mirroring
// the oracle, which gates on proposer *or* partner. The alias goes in the payload, as with
// notes: token matching splits an option id on "|", never ":", so the card name must not leak
// into feature buckets through the id.
fn action_card_shape(
    state: &GameState,
    content: &ContentStore,
    proposer: &PlayerId,
    partner: &PlayerId,
    their_goods: i32,
    shapes: &mut Vec<(String, String, BTreeMap<String, Value>)>,
) {
    // 94.3's one exception has two doors into it: Hacan's Arbiters, or Black Market Dealings
    // marking the negotiation this table is negotiating.
    let arbiters_at_table = trades_action_cards(state, content, proposer)
        || trades_action_cards(state, content, partner);
    let black_market = state.transient_flags.has(TransientFlags::BLACK_MARKET);
    if black_market || arbiters_at_table {
        // `min` is the sorted head: ActionCardId orders by alias exactly like Python's sorted().
        if let Some(card) = state
            .player(proposer)
            .and_then(|seat| seat.action_cards.iter().min())
        {
            let card = card.as_str();
            let mut payload = BTreeMap::new();
            payload.insert("action_card".to_owned(), Value::String(card.to_owned()));
            if their_goods >= 1 {
                shapes.push((
                    format!("ac{card}:1"),
                    format!("sell the action card {card} for 1 trade good"),
                    payload.clone(),
                ));
            }
            // A card is worth one good and a note two to four, so a partner holding no goods but a
            // note could previously be offered nothing at all: the one price this card had was one
            // they did not hold. Paying in paper is the deal the operator looked for and could not
            // find -- "hacan cant sell action cards for promissory notes".
            for note in crate::promissory::available_notes(state, content, partner)
                .into_iter()
                .filter(|note| crate::promissory::may_receive(state, proposer, note))
                .take(3)
            {
                let mut payload = payload.clone();
                payload.insert("promissory".to_owned(), Value::String(note.clone()));
                shapes.push((
                    format!("cn{card}>{note}"),
                    format!("give the action card {card} for the note {note}"),
                    payload,
                ));
            }
        }
    }
}

/// What each kind of note is worth to its receiver, in trade goods.
///
/// Ported from the oracle's `promissory.WORTH` table; an unknown alias prices at the oracle's
/// default of 1.5 rather than failing a deal because its card was not in the table.
const NOTE_WORTH: [(&str, f64); 10] = [
    ("ra", 4.0),
    ("an", 3.0),
    ("convoys", 3.0),
    ("ta", 2.5),
    ("ce", 2.0),
    ("ms", 2.0),
    ("favor", 2.0),
    ("war_funding", 2.0),
    ("ps", 1.5),
    ("cf", 1.5),
];

/// The name a player goes by at the table — its faction, which in the oracle is the player's id.
#[must_use]
pub fn faction_name(state: &GameState, player: &PlayerId) -> String {
    state.player(player).map_or_else(
        || player.as_str().to_owned(),
        |seat| seat.faction.as_str().to_owned(),
    )
}

/// What a note is worth to whoever receives it (oracle `_note_worth`). Support is priced by the
/// rule's own number; a Trade Agreement prices off its live value on the table; everything else
/// takes the `NOTE_WORTH` row for its alias.
fn note_worth(state: &GameState, content: &ContentStore, note: &str) -> f64 {
    if note.starts_with(crate::promissory::SUPPORT_PREFIX) {
        return 4.0;
    }
    let alias = crate::promissory::alias_of(note);
    if alias == "ta" {
        return crate::promissory::trade_agreement_worth(state, content, note);
    }
    NOTE_WORTH
        .iter()
        .find(|(name, _)| *name == alias)
        .map_or(1.5, |(_, worth)| *worth)
}

/// What a note sale asks for, in whole trade goods — the oracle's
/// `int(round(_note_worth(note)))`, called *without* a game. That omission matters: a Trade
/// Agreement prices from its flat table row (2.5) rather than its live value on the table.
/// Live worth still flows through the `net`/`their_net` payloads; only the id and label use
/// this
/// price, so both engines name the same deal when they list it.
fn note_option_price(note: &str) -> i32 {
    let worth = if note.starts_with(crate::promissory::SUPPORT_PREFIX) {
        4.0
    } else {
        let alias = crate::promissory::alias_of(note);
        NOTE_WORTH
            .iter()
            .find(|(name, _)| *name == alias)
            .map_or(1.5, |(_, worth)| *worth)
    };
    py_round_half_even(worth)
}

/// Python's `round`: halves go to the *even* integer (`round(2.5) = 2`), not away from zero as
/// Rust's `f64::round` does. The worth table only ever produces whole numbers and exact `.5`
/// values (both binary-exact), so this is exact for it; a half-away-from-zero port would price
/// a Trade Agreement at 3 where the oracle asks 2, changing both ids and what deals demand.
#[allow(
    clippy::cast_sign_loss, // asserted non-negative above
    clippy::cast_possible_truncation // asserted below 1e9, far inside `i32` range
)]
fn py_round_half_even(value: f64) -> i32 {
    debug_assert!((0.0..1e9).contains(&value));
    let floor = value.floor();
    if (floor + 0.5 - value).abs() <= 1e-9 {
        // An exact half: the even neighbour, computed in `f64` where integers stay exact (< 2^53)
        return (if (floor % 2.0).abs() == 0.0 {
            floor
        } else {
            floor + 1.0
        }) as i32;
    }
    value.round() as i32
}

/// What giving a note costs (oracle `_note_cost`): support gives up a victory point for 3 trade
/// goods of value; every other note costs three quarters of what it is worth.
fn note_cost(state: &GameState, content: &ContentStore, note: &str) -> f64 {
    if note.starts_with(crate::promissory::SUPPORT_PREFIX) {
        return 3.0;
    }
    0.75 * note_worth(state, content, note)
}

/// Everyone this player may still open a transaction with this turn.
///
/// 94.1 allows one transaction per neighbour per turn, so a partner already dealt with is not
/// offered again — which is also what stops a free action from being taken forever.
///
/// With structured diplomacy on there is no separate transaction window: a diplomatic contact
/// with a partner carries every trade this window would have offered, so nothing is offered here.
#[must_use]
pub fn available_actions(
    state: &GameState,
    content: &ContentStore,
    galaxy: &Galaxy,
    player: &PlayerId,
) -> Vec<crate::choice::ChoiceOption> {
    if state.diplomacy.enabled || !state.may_initiate_negotiation(player) {
        return Vec::new();
    }
    state
        .seating_order
        .iter()
        .filter(|other| *other != player && may_transact(state, content, galaxy, player, other))
        .filter(|other| may_open_again(state, content, player, other))
        .map(|other| {
            let name = faction_name(state, other);
            crate::choice::ChoiceOption::labelled(
                format!("{OPEN_PREFIX}{name}"),
                OPEN_KIND,
                format!("open a transaction with {name}"),
            )
        })
        .collect()
}

/// Whether `proposer` may still open a transaction with `partner` this turn: they have not dealt yet
/// (94.1), or a faction module exempts the pair from the limit
/// (`hooks_cards::transaction_exempt_from_limit`, Yssaril's Deepgloom Executable).
#[must_use]
pub fn may_open_again(
    state: &GameState,
    content: &ContentStore,
    proposer: &PlayerId,
    partner: &PlayerId,
) -> bool {
    !state.transacted_with(proposer).contains(partner)
        || crate::factions::hooks_cards::transaction_exempt_from_limit(
            state, content, proposer, partner,
        )
}

/// Spend this pair's one transaction for the turn, unless a module exempts it from the limit.
/// The game's opening path calls this where it used to call `GameState::record_transaction`.
pub fn record_unless_exempt(
    state: &mut GameState,
    content: &ContentStore,
    proposer: &PlayerId,
    partner: &PlayerId,
) {
    if !crate::factions::hooks_cards::transaction_exempt_from_limit(
        state, content, proposer, partner,
    ) {
        state.record_transaction(proposer, partner);
    }
}

/// Payload of `TRANSACTION_RESOLVED`: `proposer` and `partner` (the two seats that dealt). Both
/// are public: that a deal happened is known to the table (Lie in Wait, Pillage read the event).
#[must_use]
pub fn resolved_payload(
    proposer: &PlayerId,
    partner: &PlayerId,
) -> std::collections::BTreeMap<String, serde_json::Value> {
    let mut payload = std::collections::BTreeMap::new();
    payload.insert("proposer".to_owned(), proposer.to_string().into());
    payload.insert("partner".to_owned(), partner.to_string().into());
    payload
}

/// The partner an opening option names, or `None` for any other option.
///
/// The id names the partner's *faction* — the oracle's player identity is its faction name — so
/// resolving it back to a seat needs the table: the first seat in seating order holding that
/// faction answers. Duplicate-faction tables are outside the games the oracle can express; there,
/// the earliest seat wins deterministically rather than the choice failing.
#[must_use]
pub fn opens_with(state: &GameState, option: &crate::choice::ChoiceOption) -> Option<PlayerId> {
    let named = option.id.strip_prefix(OPEN_PREFIX)?;
    state
        .seating_order
        .iter()
        .find(|seat| {
            state
                .player(seat)
                .is_some_and(|player| player.faction.as_str() == named)
        })
        .cloned()
}

/// What one side holds that a deal can be built from.
fn holdings(state: &GameState, player: &PlayerId) -> (i32, i32) {
    state
        .player(player)
        .map_or((0, 0), |seat| (seat.trade_goods, seat.commodities))
}

/// The extra shapes Black Market Dealings widens the table with: unscored secret
/// objectives and relic fragments ("relics" are the fragments, per the 5th-printing
/// clarification; full relics stay untradeable, LRR 73.4). Both take the same flat
/// one-trade-good line the action card does: the engine has no worth table for them, and
/// one flat line keeps the nets a decider compares commensurate.
fn black_market_shapes(
    state: &GameState,
    proposer: &PlayerId,
    their_goods: i32,
    shapes: &mut Vec<(String, String, BTreeMap<String, Value>)>,
) {
    if state.transient_flags.has(TransientFlags::BLACK_MARKET)
        && let Some(seat) = state.player(proposer)
    {
        for secret in &seat.secret_objectives {
            if their_goods >= 1 {
                let mut payload = BTreeMap::new();
                payload.insert("secret".to_owned(), Value::String(secret.to_string()));
                let id = format!("so{secret}:1");
                let label = format!("sell the secret objective {secret} for 1 trade good");
                shapes.push((id, label, payload));
            }
        }
        for (trait_name, count) in &seat.relic_fragments {
            if *count > 0 && their_goods >= 1 {
                let mut payload = BTreeMap::new();
                payload.insert("fragment".to_owned(), Value::String(trait_name.clone()));
                let id = format!("fr{trait_name}:1");
                let label = format!("sell a {trait_name} relic fragment for 1 trade good");
                shapes.push((id, label, payload));
            }
        }
    }
}

/// The deals this player can put on the table.
///
/// Every shape is written once. A shape written twice is drawn twice as often by a sampling
/// decider, which is how "give 1 for 1" quietly became the table's favourite deal.
#[must_use]
pub fn offer_options(
    state: &GameState,
    content: &ContentStore,
    proposer: &PlayerId,
    partner: &PlayerId,
) -> Vec<crate::choice::ChoiceOption> {
    let (mine_goods, mine_commodities) = holdings(state, proposer);
    let (their_goods, their_commodities) = holdings(state, partner);
    // Shapes first, pricing last: every offer — note sale included — is priced against the same
    // deal its id names, exactly as the oracle prices in one pass after building.
    let mut shapes: Vec<(String, String, BTreeMap<String, Value>)> = Vec::new();

    // Support for the Throne, swapped. Both sides gain a victory point, which is why it is the
    // one note worth trading rather than lending — and why the oracle records it as the deal
    // the whole subsystem exists for.
    if let (Some(mine), Some(theirs)) = (
        crate::promissory::available_support(state, proposer),
        crate::promissory::available_support(state, partner),
    ) {
        let _ = (&mine, &theirs);
        shapes.push((
            "ss".to_owned(),
            "exchange Support for the Throne notes".to_owned(),
            BTreeMap::new(),
        ));
    }

    // Any other note the proposer holds, sold for trade goods. Until these were offered the
    // only note that could change hands was Support, so every other note in the corpus was
    // unreachable at any price.
    for note in crate::promissory::available_notes(state, content, proposer) {
        if !crate::promissory::may_receive(state, partner, &note) {
            continue; // Hubris
        }
        // Each note prices itself (oracle `propose`): a Research Agreement is not on the table
        // until its partner can pay what a technology costs.
        let price = note_option_price(&note);
        // A note the partner cannot pay for is still offerable as a gift. Requiring the fixed sale
        // price for the note to appear at all meant a poor partner made every note in the corpus
        // unreachable, however much both sides wanted the deal -- and a gift is a legal
        // transaction (94.3), so its absence was a gap in the offer set rather than a policy
        // preference.
        if price > 0 && their_goods < price {
            let mut payload = BTreeMap::new();
            payload.insert("note".to_owned(), Value::String(note.clone()));
            payload.insert(
                "alias".to_owned(),
                Value::String(crate::promissory::alias_of(&note).to_owned()),
            );
            payload.insert("gift".to_owned(), Value::Bool(true));
            shapes.push((format!("pn{note}:0"), format!("give {note}"), payload));
        }
        if price > 0 && their_goods >= price {
            // The alias goes in the payload rather than the id: token matching splits an option
            // id on "|", never ":", so a `pn{alias}` suffix would silently leak the note's kind
            // into every feature bucket (oracle `_priced` keeps ids clean for exactly this).
            let mut payload = BTreeMap::new();
            payload.insert("note".to_owned(), Value::String(note.clone()));
            payload.insert(
                "alias".to_owned(),
                Value::String(crate::promissory::alias_of(&note).to_owned()),
            );
            let id = format!("pn{note}:{price}");
            let label = format!("sell {note} for {price} trade goods");
            shapes.push((id, label, payload));
        }
    }

    // After ss and notes, before the commodity shapes — the oracle's option order.
    action_card_shape(state, content, proposer, partner, their_goods, &mut shapes);

    black_market_shapes(state, proposer, their_goods, &mut shapes);

    // 21.5: a commodity becomes a trade good the moment it changes hands, so a straight swap
    // pays both sides. This is the standard Twilight Imperium deal and the reason the subsystem
    // exists — an engine that cannot propose it has a trade economy in name only.
    let swap = mine_commodities.min(their_commodities);
    if swap > 0 {
        // The oracle's label uses a plain hyphen; the em dash is not in the feature tokenizer's
        // alphabet and would split this option's text into different buckets than its twin.
        let id = format!("cc{swap}");
        let label = format!("swap {swap} commodities each -- both gain");
        shapes.push((id, label, BTreeMap::new()));
        if swap > 1 {
            shapes.push((
                "cc1".to_owned(),
                "swap 1 commodity each".to_owned(),
                BTreeMap::new(),
            ));
        }
    }
    if mine_commodities > 0 && their_goods > 0 {
        let give = mine_commodities.min(3);
        let want = their_goods.min(give);
        if want > 0 {
            shapes.push((
                format!("ct{give}:{want}"),
                format!("give {give} commodities for {want} trade goods"),
                BTreeMap::new(),
            ));
        }
    }
    if their_commodities > 0 && mine_goods > 0 {
        let want = their_commodities.min(3);
        let give = mine_goods.min(want);
        if give > 0 {
            shapes.push((
                format!("tc{give}:{want}"),
                format!("give {give} trade goods for {want} commodities"),
                BTreeMap::new(),
            ));
        }
    }
    for give in 0..=mine_goods.min(3) {
        for want in 0..=their_goods.min(3) {
            if give == 0 && want == 0 {
                continue; // nothing for nothing is not an offer
            }
            shapes.push((
                format!("{give}:{want}"),
                format!("give {give} for {want}"),
                BTreeMap::new(),
            ));
        }
    }
    if mine_commodities > 0 {
        shapes.push((
            format!("c{mine_commodities}:0"),
            format!("gift {mine_commodities} commodities"),
            BTreeMap::new(),
        ));
    }

    partner_assets(state, content, proposer, partner, &mut shapes);
    priced(state, content, proposer, partner, shapes)
}

/// The asks that take something *from* the partner, as generated by [`partner_assets`].
///
/// `None` for every id this is not, so a caller tries it in the middle of a prefix chain and falls
/// through. Note ids contain `:` themselves (`ra:jolnar`), so a price is whatever follows the
/// *last* colon; the two-asset shapes are split on `>` because neither `:` nor `|` can divide a
/// pair of note ids.
fn ask_terms(id: &str, proposer: &PlayerId, partner: &PlayerId) -> Option<Offer> {
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

    if let Some(rest) = id.strip_prefix("np") {
        // `np{note}:{price}` — note ids carry a colon, so the price is after the last one.
        let (note, price) = rest.rsplit_once(':')?;
        return deal(
            goods(price.parse().ok()?),
            Terms {
                promissory: Some(note.to_owned()),
                ..Terms::default()
            },
        );
    }
    if let Some(rest) = id.strip_prefix("cp") {
        let (note, price) = rest.rsplit_once(':')?;
        return deal(
            commodities(price.parse().ok()?),
            Terms {
                promissory: Some(note.to_owned()),
                ..Terms::default()
            },
        );
    }
    if let Some(rest) = id.strip_prefix("pc") {
        // `pc{mine}:{price}` — my note, for their commodities.
        let (note, price) = rest.rsplit_once(':')?;
        return deal(
            Terms {
                promissory: Some(note.to_owned()),
                ..Terms::default()
            },
            commodities(price.parse().ok()?),
        );
    }
    if let Some(rest) = id.strip_prefix("nn") {
        // `nn{mine}>{theirs}`. Neither half can be a `|` (option-token separator) and a `:` cannot
        // divide them, because both halves are note ids.
        let (owed, want) = rest.split_once('>')?;
        return deal(
            Terms {
                promissory: Some(owed.to_owned()),
                ..Terms::default()
            },
            Terms {
                promissory: Some(want.to_owned()),
                ..Terms::default()
            },
        );
    }
    if let Some(rest) = id.strip_prefix("cn") {
        let (card, note) = rest.split_once('>')?;
        return deal(
            Terms {
                action_card: Some(ActionCardId::new(card)),
                ..Terms::default()
            },
            Terms {
                promissory: Some(note.to_owned()),
                ..Terms::default()
            },
        );
    }
    None
}

/// The asks that take something *from* the partner, paid in goods, commodities or paper.
///
/// Every shape the generator originally produced carried something to the partner and took payment
/// back: a note sold (`pn`), a card sold under Arbiters (`ac`), commodities swapped (`cc`, `ct`,
/// `tc`), support exchanged (`ss`). Nothing in the set asked for an asset the partner was holding,
/// so "will you sell me your Research Agreement" was not a rude offer — it was unsayable. Hacan
/// made that loudest, because the note worth having is the one across the table and the only price
/// its card had was one the partner might not hold.
///
/// Prices are not invented here. Each is the posted price that the same note carries when it is the
/// thing being sold (`note_option_price`), which is what lets the payer substitute the currency they
/// actually have — that half of the report was "deals often want trade goods as payment although
/// commodities would be available".
///
/// * `np{note}:{g}` — `g` of my trade goods for their note;
/// * `cp{note}:{c}` — the same price paid in my commodities;
/// * `pc{note}:{c}` — my note for `c` of their commodities, the same ask from the other chair;
/// * `nn{mine}>{theirs}` — one of my notes for one of theirs;
/// * `cn{card}>{note}` — an action card for their note, under the same 94.3 Arbiters gate as the
///   card sale (added in [`action_card_shape`], not here).
///
/// `>` separates the halves of a two-asset id. Token matching splits an option id on `|`, and `:`
/// already carries a price — and a note id contains `:` itself (`ra:jolnar`), so neither can divide
/// `nn` without making the note unreadable. This is also why commodities-for-a-note is `cp` and not
/// `pnc`: `pnc…` would be eaten by the `pn` branch above, which is a prefix test and not a grammar.
///
/// The note pairs are capped: both chairs can hold several notes and every ordered pair buries the
/// rest of the window in near-duplicates, so the four best nets go on the table in a fixed order.
fn partner_assets(
    state: &GameState,
    content: &ContentStore,
    proposer: &PlayerId,
    partner: &PlayerId,
    shapes: &mut Vec<(String, String, BTreeMap<String, Value>)>,
) {
    let (my_goods, my_commodities) = holdings(state, proposer);
    let (_, their_commodities) = holdings(state, partner);
    // Hubris: nobody gives the Mahact player an Alliance, so such a note is not offered.
    let mine: Vec<String> = crate::promissory::available_notes(state, content, proposer)
        .into_iter()
        .filter(|note| crate::promissory::may_receive(state, partner, note))
        .collect();
    let theirs: Vec<String> = crate::promissory::available_notes(state, content, partner)
        .into_iter()
        .filter(|note| crate::promissory::may_receive(state, proposer, note))
        .collect();
    let asking = |given: Option<&str>, want: &str| {
        let mut payload = BTreeMap::new();
        if let Some(given) = given {
            payload.insert("promissory".to_owned(), Value::String(given.to_owned()));
        }
        payload.insert(
            "received_promissory".to_owned(),
            Value::String(want.to_owned()),
        );
        payload
    };
    for note in &theirs {
        let price = note_option_price(note);
        if price > 0 && my_goods >= price {
            shapes.push((
                format!("np{note}:{price}"),
                format!("pay {price} trade goods for the note {note}"),
                asking(None, note),
            ));
        }
        if price > 0 && my_commodities >= price {
            shapes.push((
                format!("cp{note}:{price}"),
                format!("pay {price} commodities for the note {note}"),
                asking(None, note),
            ));
        }
    }
    // Their commodities for my note, at the note's own posted price or whatever of it they hold.
    // This one is a function of the card I give and not of anything across the table, so it is
    // generated once per note of mine: emitting it inside the loop above wrote the same id again
    // for every note the partner held, and `no_deal_shape_is_written_twice` says one shape may not
    // appear twice in a window.
    for owed in &mine {
        let paid = their_commodities.min(note_option_price(owed).max(1));
        if paid <= 0 {
            continue;
        }
        let mut payload = BTreeMap::new();
        payload.insert("promissory".to_owned(), Value::String(owed.clone()));
        payload.insert(
            "alias".to_owned(),
            Value::String(crate::promissory::alias_of(owed).to_owned()),
        );
        shapes.push((
            format!("pc{owed}:{paid}"),
            format!("give the note {owed} for {paid} commodity"),
            payload,
        ));
    }
    // Note for note: different promises only, best net first, ties broken by the card given away so
    // the list does not depend on which chair happened to hold what.
    let mut pairs = mine
        .iter()
        .flat_map(|owed| {
            theirs.iter().filter_map(move |want| {
                (crate::promissory::alias_of(want) != crate::promissory::alias_of(owed))
                    .then_some((owed, want))
            })
        })
        .filter_map(|(owed, want)| {
            let net = note_worth(state, content, want) - note_cost(state, content, owed);
            (net >= 0.0).then_some((net, owed, want))
        })
        .collect::<Vec<(f64, &String, &String)>>();
    pairs.sort_by(|left, right| {
        right
            .0
            .total_cmp(&left.0)
            .then_with(|| left.1.cmp(right.1))
            .then_with(|| left.2.cmp(right.2))
    });
    for (_, owed, want) in pairs.into_iter().take(4) {
        shapes.push((
            format!("nn{owed}>{want}"),
            format!("give the note {owed} for the note {want}"),
            asking(Some(owed), want),
        ));
    }
}

/// Price every shape and build the option list.
///
/// Net to the proposer and, from the other chair, net to the partner: a proposal is only worth
/// anything if it gets accepted, so both sides are priced (oracle `_priced`). Unpriced before —
/// which made every deal look exactly as good as any other at feature time.
fn priced(
    state: &GameState,
    content: &ContentStore,
    proposer: &PlayerId,
    partner: &PlayerId,
    shapes: Vec<(String, String, BTreeMap<String, Value>)>,
) -> Vec<crate::choice::ChoiceOption> {
    let mut options = Vec::new();
    for (id, label, mut payload) in shapes {
        if let Some(deal) = offer_from(state, &id, proposer, partner) {
            // Net to the proposer and, from the other chair, net to the partner: a proposal is
            // only worth anything if it gets accepted, so both sides are priced (oracle
            // `_priced`). Unpriced before — which made every deal look exactly as good as any
            // other at feature time.
            payload.insert(
                "net".to_owned(),
                Value::from(
                    deal.received.worth_to_receiver(state, content)
                        - deal.given.cost_to_giver(state, content),
                ),
            );
            payload.insert(
                "their_net".to_owned(),
                Value::from(
                    deal.given.worth_to_receiver(state, content)
                        - deal.received.cost_to_giver(state, content),
                ),
            );
        }
        let mut option = crate::choice::ChoiceOption::labelled(id, OFFER_KIND, label);
        option.payload.extend(payload);
        options.push(option);
    }
    options
}

/// Split a `"{a}:{b}"` pair of numbers.
fn pair(text: &str) -> Option<(i32, i32)> {
    let (left, right) = text.split_once(':')?;
    Some((left.parse().ok()?, right.parse().ok()?))
}

/// The deal an offer option stands for, or `None` when the id is no recognised shape.
///
/// Prefixes are tested longest-first: `"c3:0"` is a gift and `"cc3"` a swap, so a plain `"c"`
/// test placed first would read every swap as a gift of three commodities for nothing. The
/// state is needed only for the Support swap: its note ids carry the players' faction names.
#[must_use]
pub fn offer_from(
    state: &GameState,
    id: &str,
    proposer: &PlayerId,
    partner: &PlayerId,
) -> Option<Offer> {
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

    if id.starts_with(FREE_FORM_PREFIX) {
        return free_form_offer(id, proposer, partner);
    }
    if id == "ss" {
        let from = crate::promissory::faction_name(state, proposer);
        let to = crate::promissory::faction_name(state, partner);
        return deal(
            Terms {
                promissory: Some(crate::promissory::support(&from)),
                ..Terms::default()
            },
            Terms {
                promissory: Some(crate::promissory::support(&to)),
                ..Terms::default()
            },
        );
    }
    if let Some(rest) = id.strip_prefix("ac") {
        // `ac{card}:{price}` — card aliases never carry colons, so this splits cleanly. An
        // unpriced form parses to no deal rather than inventing a price; the oracle would raise.
        let (card, price) = rest.split_once(':')?;
        return deal(
            Terms {
                action_card: Some(ActionCardId::new(card)),
                ..Terms::default()
            },
            goods(price.parse().ok()?),
        );
    }
    if let Some(rest) = id.strip_prefix("pn") {
        // `pn{note}:{price}` — the note id itself carries a colon (alias:faction), so the price
        // is whatever follows its *last* one. An unpriced legacy form parses to no deal rather
        // than inventing a price; the oracle would raise on it.
        let (note, price) = rest.rsplit_once(':')?;
        return deal(
            Terms {
                promissory: Some(note.to_owned()),
                ..Terms::default()
            },
            goods(price.parse().ok()?),
        );
    }
    // The asks that take something from the partner: `np`, `cp`, `pc`, `nn`, `cn`. Delegated rather
    // than inline because a shape this file cannot read has to fall through to the commodity and
    // goods branches untouched, and `offer_from` was already at the edge of being readable.
    if let Some(ask) = ask_terms(id, proposer, partner) {
        return Some(ask);
    }
    if let Some(rest) = id.strip_prefix("so") {
        // `so{secret}:{price}` — Black Market Dealings puts an unscored secret objective on
        // the table. An alias never carries a colon, so the split is clean; an unpriced form
        // parses to no deal rather than inventing a price.
        let (secret, price) = rest.split_once(':')?;
        return deal(
            Terms {
                secret: Some(SecretObjectiveId::new(secret)),
                ..Terms::default()
            },
            goods(price.parse().ok()?),
        );
    }
    if let Some(rest) = id.strip_prefix("fr") {
        // `fr{trait}:{price}` — a Black Market relic fragment, one per entry in the terms.
        let (trait_name, price) = rest.split_once(':')?;
        return deal(
            Terms {
                fragments: vec![trait_name.to_owned()],
                ..Terms::default()
            },
            goods(price.parse().ok()?),
        );
    }
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

/// The id prefix of a free-form deal: the proposer names both sides themselves instead of picking
/// one of the listed shapes (94.1: a transaction is any exchange of trade goods, commodities,
/// promissory notes and relic fragments, in any amounts the two seats hold).
pub const FREE_FORM_PREFIX: &str = "x;";

/// A side's terms as the fields of a free-form id, `{side}t=2;{side}c=1;{side}n=note;...`.
fn push_fields(out: &mut String, side: char, terms: &Terms) {
    use std::fmt::Write as _;
    if terms.trade_goods > 0 {
        let _ = write!(out, "{side}t={};", terms.trade_goods);
    }
    if terms.commodities > 0 {
        let _ = write!(out, "{side}c={};", terms.commodities);
    }
    if let Some(note) = &terms.promissory {
        let _ = write!(out, "{side}n={note};");
    }
    if let Some(card) = &terms.action_card {
        let _ = write!(out, "{side}a={card};");
    }
    if let Some(secret) = &terms.secret {
        let _ = write!(out, "{side}s={secret};");
    }
    if !terms.fragments.is_empty() {
        let _ = write!(out, "{side}f={};", terms.fragments.join(","));
    }
}

/// The free-form id of a deal: `x;` then `g…` fields for what the proposer gives and `r…` fields
/// for what they receive. The inverse of [`offer_from`] for such ids.
#[must_use]
pub fn free_form_id(given: &Terms, received: &Terms) -> String {
    let mut id = FREE_FORM_PREFIX.to_owned();
    push_fields(&mut id, 'g', given);
    push_fields(&mut id, 'r', received);
    id
}

/// Read a free-form id. `None` for anything malformed (unknown or repeated field, a count that is
/// not a plain non-negative number) rather than a guess; legality is `why_illegal`'s business.
fn free_form_offer(id: &str, proposer: &PlayerId, partner: &PlayerId) -> Option<Offer> {
    let mut given = Terms::default();
    let mut received = Terms::default();
    let mut seen = BTreeSet::new();
    for field in id.strip_prefix(FREE_FORM_PREFIX)?.split(';') {
        if field.is_empty() {
            continue;
        }
        let (key, value) = field.split_once('=')?;
        if !seen.insert(key.to_owned()) || value.is_empty() {
            return None;
        }
        let mut chars = key.chars();
        let (side, kind) = (chars.next()?, chars.next()?);
        if chars.next().is_some() {
            return None;
        }
        let terms = match side {
            'g' => &mut given,
            'r' => &mut received,
            _ => return None,
        };
        let count = || {
            // Plain digits only: "+2" and "-2" parse as numbers but are not amounts.
            (value.len() <= 4 && value.bytes().all(|b| b.is_ascii_digit()))
                .then(|| value.parse::<i32>().ok())
                .flatten()
        };
        match kind {
            't' => terms.trade_goods = count()?,
            'c' => terms.commodities = count()?,
            'n' => terms.promissory = Some(value.to_owned()),
            'a' => terms.action_card = Some(ActionCardId::new(value)),
            's' => terms.secret = Some(SecretObjectiveId::new(value)),
            'f' => terms.fragments = value.split(',').map(str::to_owned).collect(),
            _ => return None,
        }
    }
    Some(Offer {
        proposer: proposer.clone(),
        partner: partner.clone(),
        given,
        received,
    })
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
// `Answering` owns an `Offer` because the negotiation must survive across turns; windows are
// rare and short-lived, so boxing the offer for a size lint would only add allocations without
// changing behaviour.
#[expect(clippy::large_enum_variant)]
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
///
/// # Example
///
/// ```
/// use ti4_content::ContentStore;
/// use ti4_model::content_types::POK;
/// use ti4_model::id::PlayerId;
/// use ti4_engine::transactions::TradeWindow;
///
/// let players = [PlayerId::new("a"), PlayerId::new("b")];
/// let mut state =
///     ti4_engine::setup::start_game(ContentStore::embedded(), &players, POK, None).unwrap();
/// for player in &players {
///     let seat = state.player_mut(player).unwrap();
///     seat.trade_goods = 2;
///     seat.commodities = 3;
/// }
///
/// // Opening spends this pair's one transaction for the turn (94.1), whether or not a deal
/// // closes — which is also what stops the free action being taken for ever.
/// let window = TradeWindow::open(&mut state, &players[0], &players[1]);
/// assert!(state.transacted_with(&players[0]).contains(&players[1]));
///
/// let choice = window
///     .pending_choice(&state, ti4_content::ContentStore::embedded())
///     .expect("there are deals to propose");
/// assert!(choice.ids().contains(&"cc3"), "swap three commodities each");
/// ```
#[derive(Clone)]
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
        Self::open_with_content(state, ContentStore::embedded(), proposer, partner)
    }

    /// [`Self::open`] with the game's content, so a module's exemption from the per-turn limit
    /// (`hooks_cards::transaction_exempt_from_limit`) is honoured: an exempt pair is not recorded.
    #[must_use]
    pub fn open_with_content(
        state: &mut GameState,
        content: &ContentStore,
        proposer: &PlayerId,
        partner: &PlayerId,
    ) -> Self {
        record_unless_exempt(state, content, proposer, partner);
        if state.diplomacy.enabled {
            let _ = state.diplomacy.consume_initiation(proposer, partner);
        }
        Self {
            proposer: proposer.clone(),
            partner: partner.clone(),
            stage: Stage::Proposing,
            rounds_left: 2,
        }
    }

    /// The two seats: `(proposer, partner)`. For [`resolved_payload`].
    #[must_use]
    pub const fn parties(&self) -> (&PlayerId, &PlayerId) {
        (&self.proposer, &self.partner)
    }

    /// Whether negotiations have ended.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.stage == Stage::Done
    }

    /// The decision this negotiation owes, if any.
    #[must_use]
    pub fn pending_choice(
        &self,
        state: &GameState,
        content: &ContentStore,
    ) -> Option<crate::choice::Choice> {
        match &self.stage {
            Stage::Done => None,
            Stage::Proposing => {
                let mut options = offer_options(state, content, &self.proposer, &self.partner);
                if options.is_empty() {
                    return None;
                }
                let offers = options
                    .iter()
                    .filter_map(|option| {
                        let offer = offer_from(state, &option.id, &self.proposer, &self.partner)?;
                        Some((option.id.clone(), TradeDeal::of(&offer, content)))
                    })
                    .collect();
                let hints = TransactionHints {
                    proposer: self.proposer.clone(),
                    partner: self.partner.clone(),
                    offers,
                    offer: None,
                };
                options.push(crate::choice::ChoiceOption::decline());
                Some(
                    crate::choice::Choice::new(
                        self.proposer.clone(),
                        format!("transaction with {}", faction_name(state, &self.partner)),
                        options,
                    )
                    .accepting_free_form(FREE_FORM_PREFIX)
                    .hinted("transaction", &hints)
                    .contextualized(
                        DecisionContext::new(
                            self.proposer.clone(),
                            DecisionSource::Rule("60".to_owned()),
                            "propose_transaction",
                            state.phase,
                            state.round,
                        )
                        .about(DecisionTarget::Player(self.partner.clone())),
                    ),
                )
            }
            Stage::Answering(offer) => {
                // Priced from the *receiver's* side: what they are being handed against what is
                // being asked of them. Unpriced before, so a deal that gave two trade goods for
                // nothing was accepted exactly as often as one that took them.
                let accept = crate::choice::ChoiceOption::labelled("accept", ANSWER_KIND, "accept")
                    .with(
                        "net",
                        offer.given.worth_to_receiver(state, content)
                            - offer.received.cost_to_giver(state, content),
                    );
                Some(
                    crate::choice::Choice::new(
                        self.partner.clone(),
                        format!("{} -- accept?", offer.describe(state)),
                        vec![
                            accept,
                            crate::choice::ChoiceOption::labelled(
                                "refuse",
                                crate::choice::DECLINE_KIND,
                                "refuse",
                            ),
                            crate::choice::ChoiceOption::labelled(
                                "counter",
                                ANSWER_KIND,
                                "counter-offer",
                            ),
                        ],
                    )
                    .hinted(
                        "transaction",
                        &TransactionHints {
                            proposer: offer.proposer.clone(),
                            partner: offer.partner.clone(),
                            offers: BTreeMap::new(),
                            offer: Some(TradeDeal::of(offer, content)),
                        },
                    )
                    .contextualized(
                        DecisionContext::new(
                            self.partner.clone(),
                            DecisionSource::Rule("60".to_owned()),
                            "answer_transaction",
                            state.phase,
                            state.round,
                        )
                        .about(DecisionTarget::Player(self.proposer.clone())),
                    ),
                )
            }
        }
    }

    /// Apply one answer.
    pub fn resolve(
        &mut self,
        state: &mut GameState,
        content: &ContentStore,
        galaxy: &Galaxy,
        answer: &crate::choice::ChoiceOption,
    ) -> Traded {
        match std::mem::replace(&mut self.stage, Stage::Done) {
            Stage::Done => Traded::NothingOffered,
            Stage::Proposing => {
                if answer.is_decline() {
                    return Traded::NothingOffered;
                }
                let Some(offer) = offer_from(state, &answer.id, &self.proposer, &self.partner)
                else {
                    return Traded::NothingOffered;
                };
                // Legality is checked before the partner is troubled with it, so an offer that
                // could not be paid never reaches the table as though it could.
                if let Some(reason) = why_illegal(state, content, galaxy, &offer) {
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
                    let fair = (offer.given.worth_to_receiver(state, content)
                        - offer.received.worth_to_receiver(state, content))
                    .abs()
                        <= 0.5;
                    return match resolve(state, content, galaxy, &offer) {
                        Ok(()) => {
                            if fair {
                                crate::diplomacy::apply_relationship_event(
                                    state,
                                    &crate::diplomacy::RelationshipEvent::FairTransaction {
                                        a: offer.proposer.clone(),
                                        b: offer.partner.clone(),
                                    },
                                )
                                .expect("transaction participants are distinct");
                            }
                            Traded::Resolved
                        }
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
    use ti4_model::content_types::POK;
    use ti4_model::id::FactionId;

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

    /// OBS-003f: proposing and answering a transaction are typed distinctly, though both arise
    /// from the same negotiation between the same two players.
    #[test]
    fn obs003f_propose_and_answer_carry_typed_context() {
        let players = [a(), b()];
        let content = ContentStore::embedded();
        let hub = plain_hub();
        let mut state = crate::setup::start_game(content, &players, POK, None).unwrap();
        let centre = SystemId::new(hub.centre.clone());
        put(&mut state, &centre, "cruiser", &a(), 1);
        put(&mut state, &centre, "cruiser", &b(), 1);
        for player in &players {
            let seat = state.player_mut(player).unwrap();
            seat.trade_goods = 2;
            seat.commodities = 3;
        }

        let mut window = TradeWindow::open(&mut state, &a(), &b());
        let proposing = window
            .pending_choice(&state, content)
            .expect("there are deals to propose");
        let propose_context = proposing.context.as_ref().expect("typed context");
        assert_eq!(
            propose_context.source,
            DecisionSource::Rule("60".to_owned())
        );
        assert_eq!(propose_context.subtype, "propose_transaction");
        assert_eq!(propose_context.target, Some(DecisionTarget::Player(b())));

        let deal = proposing.option("cc3").expect("a swap is offered").clone();
        let outcome = window.resolve(&mut state, content, &hub.galaxy, &deal);
        assert_eq!(outcome, Traded::Offered, "the offer reached the partner");
        let answering = window
            .pending_choice(&state, content)
            .expect("the partner answers");
        let answer_context = answering.context.as_ref().expect("typed context");
        assert_eq!(answer_context.subtype, "answer_transaction");
        assert_eq!(answer_context.target, Some(DecisionTarget::Player(a())));
        assert_ne!(propose_context.subtype, answer_context.subtype);
    }

    /// A free-form deal names its own amounts: five trade goods for two commodities is not on the
    /// listed menu (which stops at three), yet it is a legal transaction and must resolve, with the
    /// commodities turning into trade goods on arrival (21.5).
    #[test]
    fn a_free_form_deal_of_unequal_amounts_resolves() {
        let players = [a(), b()];
        let content = ContentStore::embedded();
        let hub = plain_hub();
        let mut state = crate::setup::start_game(content, &players, POK, None).unwrap();
        let centre = SystemId::new(hub.centre.clone());
        put(&mut state, &centre, "cruiser", &a(), 1);
        put(&mut state, &centre, "cruiser", &b(), 1);
        state.player_mut(&a()).unwrap().trade_goods = 6;
        state.player_mut(&b()).unwrap().commodities = 4;
        state.player_mut(&b()).unwrap().trade_goods = 0;

        let mut window = TradeWindow::open(&mut state, &a(), &b());
        let choice = window.pending_choice(&state, content).unwrap();
        let id = free_form_id(&goods(5), &commodities(2));
        assert!(choice.option(&id).is_none(), "not a listed shape");
        let answer = choice.answer_for(&id).expect("free-form ids are accepted");
        crate::choice::validate(&choice, answer.clone()).expect("the table accepts it");
        assert_eq!(
            window.resolve(&mut state, content, &hub.galaxy, &answer),
            Traded::Offered
        );
        let accept = ChoiceOption::labelled("accept", ANSWER_KIND, "accept");
        assert_eq!(
            window.resolve(&mut state, content, &hub.galaxy, &accept),
            Traded::Resolved
        );
        assert_eq!(state.player(&a()).unwrap().trade_goods, 1 + 2);
        assert_eq!(state.player(&a()).unwrap().commodities, 0);
        assert_eq!(state.player(&b()).unwrap().trade_goods, 5);
        assert_eq!(state.player(&b()).unwrap().commodities, 2);
    }

    #[test]
    fn a_free_form_deal_the_proposer_cannot_pay_is_rejected() {
        let players = [a(), b()];
        let content = ContentStore::embedded();
        let hub = plain_hub();
        let mut state = crate::setup::start_game(content, &players, POK, None).unwrap();
        let centre = SystemId::new(hub.centre.clone());
        put(&mut state, &centre, "cruiser", &a(), 1);
        put(&mut state, &centre, "cruiser", &b(), 1);
        state.player_mut(&a()).unwrap().trade_goods = 2;
        let mut window = TradeWindow::open(&mut state, &a(), &b());
        let choice = window.pending_choice(&state, content).unwrap();
        let answer = choice
            .answer_for(&free_form_id(&goods(3), &Terms::default()))
            .unwrap();
        assert_eq!(
            window.resolve(&mut state, content, &hub.galaxy, &answer),
            Traded::Rejected(OfferError::CannotPay(a()))
        );
    }

    #[test]
    fn free_form_ids_round_trip_and_refuse_nonsense() {
        let state = game(&["a", "b"]);
        let given = Terms {
            trade_goods: 7,
            commodities: 1,
            promissory: Some("ra:jolnar".to_owned()),
            fragments: vec!["cultural".to_owned(), "hazardous".to_owned()],
            ..Terms::default()
        };
        let received = commodities(3);
        let id = free_form_id(&given, &received);
        let offer = offer_from(&state, &id, &a(), &b()).expect("parses");
        assert_eq!((offer.given, offer.received), (given, received));
        for bad in [
            "x;gt=-1;",
            "x;gt=2;gt=3;",
            "x;zt=1;",
            "x;gt=;",
            "x;gt=1x;",
            "x;gq=1;",
            "x;gt=99999;",
        ] {
            assert!(offer_from(&state, bad, &a(), &b()).is_none(), "{bad}");
        }
        // A bare prefix names no deal, and the choice does not accept it as an answer.
        let choice =
            crate::choice::Choice::new(a(), "q", vec![]).accepting_free_form(FREE_FORM_PREFIX);
        assert!(choice.answer_for("x;").is_none());
        assert!(choice.answer_for("y;gt=1;").is_none());
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
        resolve(&mut state, ContentStore::embedded(), &hub.galaxy, &offer).unwrap();

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
            resolve(&mut state, ContentStore::embedded(), &hub.galaxy, &offer),
            Err(OfferError::NotNeighbours(a(), b()))
        );
        assert!(state.identical(&before), "nothing changed hands");
    }

    /// Guild Ships: "You can negotiate transactions with players who are not your neighbor."
    ///
    /// The offer path honoured this (`partners`) and the legality check did not (`are_neighbours`), so a
    /// Hacan seat was offered a partner across the board and refused the instant it accepted - reported
    /// by the operator as "straight up guild ships not working", and reproducible in fifteen lines. When
    /// generation and legality disagree, what the reader experiences is a dead button.
    #[test]
    fn guild_ships_let_a_hacan_seat_deal_with_a_stranger() {
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
        state.player_mut(&a()).unwrap().faction = FactionId::new("hacan");
        assert!(
            !are_neighbours(&state, &hub.galaxy, &a(), &b()),
            "the fixture has to be strangers, or this proves nothing"
        );

        let offer = Offer {
            proposer: a(),
            partner: b(),
            given: goods(2),
            received: Terms::default(),
        };
        let outcome = resolve(&mut state, ContentStore::embedded(), &hub.galaxy, &offer);
        assert!(
            outcome.is_ok(),
            "Guild Ships reaches across the board; got {outcome:?}"
        );
        assert_eq!(state.player(&a()).unwrap().trade_goods, 3);
        assert_eq!(state.player(&b()).unwrap().trade_goods, 2);
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
            resolve(&mut state, ContentStore::embedded(), &hub.galaxy, &offer),
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
            resolve(&mut state, ContentStore::embedded(), &hub.galaxy, &offer),
            Err(OfferError::CannotPay(a())),
            "a holds nothing, so cannot give two whatever b is sending"
        );
    }

    /// Diplomacy names fragments in lower case (`cultural`, `unknown`); they are held upper case
    /// (`CULTURAL`, and `FRONTIER` for an unknown fragment). The two must meet.
    #[test]
    fn diplomacy_fragment_names_match_the_held_keys() {
        let hub = plain_hub();
        let mut state = game(&["a", "b"]);
        let centre = SystemId::new(hub.centre.clone());
        put(&mut state, &centre, "cruiser", &a(), 1);
        put(&mut state, &centre, "cruiser", &b(), 1);
        crate::exploration::gain_fragment(&mut state, &a(), "CULTURAL");
        crate::exploration::gain_fragment(&mut state, &a(), "FRONTIER");
        let offer = Offer {
            proposer: a(),
            partner: b(),
            given: Terms {
                fragments: vec!["cultural".to_owned(), "unknown".to_owned()],
                ..Terms::default()
            },
            received: Terms::default(),
        };
        assert_eq!(
            resolve(&mut state, ContentStore::embedded(), &hub.galaxy, &offer),
            Ok(())
        );
        let b_holds = &state.player(&b()).unwrap().relic_fragments;
        assert_eq!(b_holds.get("CULTURAL"), Some(&1));
        assert_eq!(b_holds.get("FRONTIER"), Some(&1));
        assert!(state.player(&a()).unwrap().relic_fragments.is_empty());
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
        resolve(&mut state, ContentStore::embedded(), &hub.galaxy, &offer).unwrap();

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
            resolve(&mut state, ContentStore::embedded(), &hub.galaxy, &offer),
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
        // Distinct factions: note and Support ids carry the faction name, so a table whose
        // seats all read "generic" is one the oracle cannot express. The setup deal ran before
        // seating with generic names, so re-deal once the seats know who they are (G1).
        state.player_mut(&a()).unwrap().faction = FactionId::new("hacan");
        state.player_mut(&b()).unwrap().faction = FactionId::new("jolnar");
        crate::promissory::deal(&mut state, ContentStore::embedded(), POK);
        (hub, state)
    }

    #[test]
    fn opening_a_transaction_spends_the_pair_for_the_turn() {
        // 94.1 is spent on opening, not on closing. If it were spent on closing, a seat that
        // keeps declining its own offer could reopen the same talks forever, and the free
        // action would never stop being offered.
        let (hub, mut state) = trading_partners();
        assert_eq!(
            available_actions(&state, ContentStore::embedded(), &hub.galaxy, &a()).len(),
            1,
            "b is a neighbour and has not been dealt with"
        );

        let mut window = TradeWindow::open(&mut state, &a(), &b());
        let choice = window
            .pending_choice(&state, ti4_content::ContentStore::embedded())
            .expect("a deal to propose");
        window.resolve(
            &mut state,
            ContentStore::embedded(),
            &hub.galaxy,
            &ChoiceOption::decline(),
        );

        assert!(
            window.is_complete(),
            "declining to offer ends the negotiation"
        );
        assert!(
            available_actions(&state, ContentStore::embedded(), &hub.galaxy, &a()).is_empty(),
            "the opportunity is gone even though no deal was struck"
        );
        assert!(choice.ids().contains(&"cc3"), "a swap was on the table");
    }

    #[test]
    fn an_exempt_pair_may_open_again_and_the_opening_is_not_recorded() {
        let (hub, mut state) = trading_partners();
        let content = ContentStore::embedded();
        let exempt = crate::factions::hooks_cards::CardHooks {
            transaction_limit_exempt: Some(|_, _, _, other| other.as_str() == "b"),
            ..crate::factions::hooks_cards::CardHooks::NONE
        };
        crate::factions::hooks_cards::with_test_hooks(exempt, || {
            let first = TradeWindow::open_with_content(&mut state, content, &a(), &b());
            assert!(
                state.transacted_with(&a()).is_empty(),
                "an exempt transaction does not count against the limit"
            );
            assert_eq!(first.parties(), (&a(), &b()));
            assert!(may_open_again(&state, content, &a(), &b()));
            assert_eq!(
                available_actions(&state, content, &hub.galaxy, &a()).len(),
                1,
                "still offered"
            );
        });
        // Without the hook the same opening is spent, and not offered again.
        let _ = TradeWindow::open_with_content(&mut state, content, &a(), &b());
        assert!(state.transacted_with(&a()).contains(&b()));
        assert!(!may_open_again(&state, content, &a(), &b()));
        assert!(available_actions(&state, content, &hub.galaxy, &a()).is_empty());
    }

    #[test]
    fn the_resolved_payload_names_both_parties() {
        let payload = resolved_payload(&a(), &b());
        assert_eq!(payload.get("proposer").and_then(Value::as_str), Some("a"));
        assert_eq!(payload.get("partner").and_then(Value::as_str), Some("b"));
        assert_eq!(payload.len(), 2);
    }

    #[test]
    fn a_commodity_swap_pays_both_sides() {
        // 21.5 is the whole economy: a commodity is worth nothing to its owner and a trade good
        // to anybody else, so the standard deal leaves both seats richer.
        let (hub, mut state) = trading_partners();
        let mut window = TradeWindow::open(&mut state, &a(), &b());

        let offered = window.resolve(
            &mut state,
            ContentStore::embedded(),
            &hub.galaxy,
            &ChoiceOption::labelled("cc3", OFFER_KIND, ""),
        );
        assert_eq!(offered, Traded::Offered);
        let answering = window
            .pending_choice(&state, ti4_content::ContentStore::embedded())
            .expect("b answers");
        assert_eq!(answering.player, b());

        let outcome = window.resolve(
            &mut state,
            ContentStore::embedded(),
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
            ContentStore::embedded(),
            &hub.galaxy,
            &ChoiceOption::labelled("cc3", OFFER_KIND, ""),
        );

        let outcome = window.resolve(
            &mut state,
            ContentStore::embedded(),
            &hub.galaxy,
            &ChoiceOption::labelled("counter", ANSWER_KIND, ""),
        );

        assert_eq!(outcome, Traded::Countered);
        let counter = window
            .pending_choice(&state, ti4_content::ContentStore::embedded())
            .expect("b now proposes");
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

        window.resolve(&mut state, ContentStore::embedded(), &hub.galaxy, &offer);
        window.resolve(&mut state, ContentStore::embedded(), &hub.galaxy, &counter);
        window.resolve(&mut state, ContentStore::embedded(), &hub.galaxy, &offer);
        let outcome = window.resolve(&mut state, ContentStore::embedded(), &hub.galaxy, &counter);

        assert_eq!(outcome, Traded::NothingOffered);
        assert!(window.is_complete(), "the second counter ends it");
    }

    #[test]
    fn swapping_support_scores_both_sides() {
        // The deal the subsystem exists for: each player receives the other's Support, and each
        // is worth a victory point.
        let (hub, mut state) = trading_partners();
        let mut window = TradeWindow::open(&mut state, &a(), &b());
        let choice = window
            .pending_choice(&state, ti4_content::ContentStore::embedded())
            .expect("deals on offer");
        assert!(choice.ids().contains(&"ss"), "the swap is offered");

        window.resolve(
            &mut state,
            ContentStore::embedded(),
            &hub.galaxy,
            &ChoiceOption::labelled("ss", OFFER_KIND, ""),
        );
        let outcome = window.resolve(
            &mut state,
            ContentStore::embedded(),
            &hub.galaxy,
            &ChoiceOption::labelled("accept", ANSWER_KIND, ""),
        );

        assert_eq!(outcome, Traded::Resolved);
        assert_eq!(state.player(&a()).unwrap().victory_points, 1);
        assert_eq!(state.player(&b()).unwrap().victory_points, 1);
        assert_eq!(state.support_holders.get(&a()), Some(&b()));
        assert_eq!(state.support_holders.get(&b()), Some(&a()));
    }

    #[test]
    fn a_note_actually_changes_hands_when_it_is_sold() {
        let (hub, mut state) = trading_partners();
        crate::promissory::deal(
            &mut state,
            ti4_content::ContentStore::embedded(),
            ti4_model::content_types::POK,
        );
        state.player_mut(&b()).unwrap().trade_goods = 5;

        let offers = offer_options(&state, ti4_content::ContentStore::embedded(), &a(), &b());
        let sale = offers
            .iter()
            .find(|option| option.id.starts_with("pn"))
            .cloned()
            .expect("a note is on the table");
        // The id carries the sale price: `pn{note}:{price}`.
        let suffix = sale.id.trim_start_matches("pn");
        let (note, _price) = suffix.rsplit_once(':').expect("priced note id");
        let note = note.to_owned();

        let mut window = TradeWindow::open(&mut state, &a(), &b());
        window.resolve(&mut state, ContentStore::embedded(), &hub.galaxy, &sale);
        let outcome = window.resolve(
            &mut state,
            ContentStore::embedded(),
            &hub.galaxy,
            &ChoiceOption::labelled("accept", ANSWER_KIND, ""),
        );

        assert_eq!(outcome, Traded::Resolved);
        assert_eq!(
            state.promissory_notes.get(&note),
            Some(&b()),
            "the card moved"
        );
        assert!(
            !crate::promissory::available_notes(&state, ContentStore::embedded(), &a(),)
                .contains(&note),
            "and is no longer a's to sell again"
        );
    }

    #[test]
    fn note_option_prices_follow_the_oracle_table() {
        // The oracle prices each sale at `int(round(_note_worth(note)))` with no game, so a
        // Trade Agreement takes the flat 2.5 row rather than its live value; both `.5` rows are
        // exact banker's-rounding cases (Python rounds half to even: round(1.5) = round(2.5) = 2).
        assert_eq!(note_option_price("cf:hacan"), 2); // 1.5 -> 2, not away-from-zero 2 by luck
        assert_eq!(note_option_price("ps:letnev"), 2);
        assert_eq!(
            note_option_price("ta:sol"),
            2,
            "flat row, not the live table"
        );
        assert_eq!(note_option_price("ra:jolnar"), 4);
        assert_eq!(note_option_price("an:xxcha"), 3);
        assert_eq!(note_option_price("convoys:hacan"), 3);
        assert_eq!(note_option_price("ce:l1z1x"), 2);
        assert_eq!(note_option_price("ms:sol"), 2);
        assert_eq!(note_option_price("favor:xxcha"), 2);
        assert_eq!(note_option_price("war_funding:letnev"), 2);
        // Support is never sold for goods in either engine; the row exists so a support id
        // priced anywhere else reads as the oracle would.
        assert_eq!(note_option_price("support:hacan"), 4);
    }

    #[test]
    fn note_sales_carry_their_own_price_in_id_and_label() {
        let (_hub, mut state) = trading_partners();
        // b can afford every price the table offers here.
        state.player_mut(&b()).unwrap().trade_goods = 6;

        // a (hacan) holds cf + ps + ta + convoys after the re-deal; `an` is withheld until its
        // commander unlocks, which this scaffolding never does. BTreeMap key order: cf, convoys,
        // ps, ta.
        let offers = offer_options(&state, ContentStore::embedded(), &a(), &b());
        let ids: Vec<&str> = offers
            .iter()
            .filter(|option| option.id.starts_with("pn"))
            .map(|option| option.id.as_str())
            .collect();
        assert_eq!(
            ids,
            vec![
                "pncf:hacan:2",
                "pnconvoys:hacan:3",
                "pnps:hacan:2",
                "pnta:hacan:2"
            ]
        );
        let cf = offers
            .iter()
            .find(|option| option.id == "pncf:hacan:2")
            .expect("the ceasefire is on the table");
        assert_eq!(cf.label, "sell cf:hacan for 2 trade goods");
    }

    /// Strike the deal the option names and say so loudly if the engine would not.
    ///
    /// Every new offer shape needs this, not just a parse test: an option that lists, parses, and
    /// then refuses at execution is the failure the operator actually experiences.
    fn struck(
        state: &mut GameState,
        hub: &crate::fixtures::Hub,
        proposer: &PlayerId,
        partner: &PlayerId,
        option: &ChoiceOption,
    ) {
        let mut window = TradeWindow::open(state, proposer, partner);
        window.resolve(state, ContentStore::embedded(), &hub.galaxy, option);
        let outcome = window.resolve(
            state,
            ContentStore::embedded(),
            &hub.galaxy,
            &ChoiceOption::labelled("accept", ANSWER_KIND, ""),
        );
        assert_eq!(
            outcome,
            Traded::Resolved,
            "{} was not settleable",
            option.id
        );
    }

    /// "cant ask for opponent held promissory notes during transactions"
    #[test]
    fn a_note_can_be_bought_off_the_partner() {
        let (hub, mut state) = trading_partners();
        // b (jolnar) buys a ceasefire note out of a's hand. Ceasefire posts at 2 and the fixture
        // seat holds 2 goods, so the ask is affordable as soon as one more arrives: an ask the
        // payer cannot cover must not be on the table either, which is the other half of this.
        state.player_mut(&b()).unwrap().trade_goods = 5;
        let offers = offer_options(&state, ContentStore::embedded(), &b(), &a());
        let buy = offers
            .iter()
            .find(|option| option.id == "npcf:hacan:2")
            .cloned()
            .expect("the partner's note is on the table");
        let deal = offer_from(&state, &buy.id, &b(), &a()).expect("the id parses back");
        assert_eq!(
            deal.given.trade_goods, 2,
            "the posted price, not an invented one"
        );
        assert_eq!(deal.received.promissory.as_deref(), Some("cf:hacan"));

        struck(&mut state, &hub, &b(), &a(), &buy);
        assert_eq!(state.promissory_notes.get("cf:hacan"), Some(&b()));
        assert_eq!(state.player(&b()).unwrap().trade_goods, 3);
        assert!(
            !crate::promissory::available_notes(&state, ContentStore::embedded(), &a())
                .contains(&"cf:hacan".to_owned()),
            "and a is left holding nothing of it",
        );
    }

    /// "deals often want tradegoods as payment although commodities would be available"
    #[test]
    fn a_note_can_be_paid_for_in_commodities() {
        let (hub, mut state) = trading_partners();
        let seat = state.player_mut(&b()).unwrap();
        seat.trade_goods = 0;
        seat.commodities = 4;
        let offers = offer_options(&state, ContentStore::embedded(), &b(), &a());
        // Ceasefire posts at 2, and b holds 4 commodities, so the paper is buyable in commodities
        // even though b cannot buy a paperclip in goods.
        let buy = offers
            .iter()
            .find(|option| option.id == "cpcf:hacan:2")
            .cloned()
            .or_else(|| {
                offers
                    .iter()
                    .find(|option| option.id.starts_with("cp"))
                    .cloned()
            })
            .expect("a commodity-priced ask is on the table");
        let deal = offer_from(&state, &buy.id, &b(), &a()).expect("the id parses back");
        assert!(deal.given.commodities > 0, "paid in commodities");
        assert_eq!(deal.given.trade_goods, 0);
        assert!(deal.received.promissory.is_some(), "and a note comes back");

        let before = state.player(&a()).unwrap().trade_goods;
        struck(&mut state, &hub, &b(), &a(), &buy);
        assert_eq!(
            state.player(&b()).unwrap().commodities,
            4 - deal.given.commodities
        );
        assert_eq!(
            state.player(&a()).unwrap().trade_goods,
            before + deal.given.commodities,
            "21.5: their commodities land as trade goods",
        );
    }

    /// BUG-03: "Note for note impossible, any faction"
    #[test]
    fn notes_trade_for_one_another() {
        let (hub, mut state) = trading_partners();
        let offers = offer_options(&state, ContentStore::embedded(), &b(), &a());
        let swap = offers
            .iter()
            .find(|option| option.id.starts_with("nn"))
            .cloned()
            .expect("a note-for-note shape is offered");
        let (owed, want) = swap.id[2..].split_once('>').expect("two note ids");
        assert_ne!(owed, want, "a promise is not traded for itself");
        let deal = offer_from(&state, &swap.id, &b(), &a()).expect("the id parses back");
        assert_eq!(deal.given.promissory.as_deref(), Some(owed));
        assert_eq!(deal.received.promissory.as_deref(), Some(want));

        struck(&mut state, &hub, &b(), &a(), &swap);
        assert_eq!(state.promissory_notes.get(want), Some(&b()));
        assert_eq!(state.promissory_notes.get(owed), Some(&a()));
    }

    /// "hacan cant sell action cards for promissory notes"
    #[test]
    fn an_action_card_buys_a_note_across_the_table() {
        let (hub, mut state) = trading_partners();
        state
            .player_mut(&a())
            .unwrap()
            .action_cards
            .push(ActionCardId::new("shutdown"));
        // A partner with no goods at all: the card's old one-good price was unreachable, so this is
        // the table where the missing shape is the only deal available.
        state.player_mut(&b()).unwrap().trade_goods = 0;
        let offers = offer_options(&state, ContentStore::embedded(), &a(), &b());
        let sell = offers
            .iter()
            .find(|option| option.id.starts_with("cn"))
            .cloned()
            .expect("a card-for-note shape is offered under Arbiters");
        let deal = offer_from(&state, &sell.id, &a(), &b()).expect("the id parses back");
        assert_eq!(
            deal.given
                .action_card
                .as_ref()
                .map(|card| card.as_str().to_owned()),
            Some("shutdown".to_owned()),
        );
        assert!(deal.received.promissory.is_some());

        struck(&mut state, &hub, &a(), &b(), &sell);
        assert!(
            !state.player(&a()).is_some_and(|seat| seat
                .action_cards
                .iter()
                .any(|card| card.as_str() == "shutdown")),
            "the card left hacan's hand",
        );
        assert!(
            state.player(&b()).is_some_and(|seat| seat
                .action_cards
                .iter()
                .any(|card| card.as_str() == "shutdown")),
            "and arrived across the table",
        );
    }

    /// Every ask the generator can name must be a deal the engine accepts, with both hands paying
    /// something. A shape that lists but cannot settle is worse than a missing shape: it reads as
    /// available right up to the moment it fails.
    #[test]
    fn every_ask_parses_into_a_deal_with_two_sides() {
        let (_hub, mut state) = trading_partners();
        state.player_mut(&b()).unwrap().trade_goods = 6;
        state.player_mut(&a()).unwrap().trade_goods = 6;
        let mut seen = 0;
        for (proposer, partner) in [(a(), b()), (b(), a())] {
            for option in offer_options(&state, ContentStore::embedded(), &proposer, &partner) {
                if !(option.id.starts_with("np")
                    || option.id.starts_with("cp")
                    || option.id.starts_with("pc")
                    || option.id.starts_with("nn")
                    || option.id.starts_with("cn"))
                {
                    continue;
                }
                seen += 1;
                let deal = offer_from(&state, &option.id, &proposer, &partner)
                    .unwrap_or_else(|| panic!("{} does not parse", option.id));
                assert!(!deal.given.is_empty(), "{} gives nothing", option.id);
                assert!(!deal.received.is_empty(), "{} takes nothing", option.id);
            }
        }
        assert!(seen > 0, "the asks were never generated at all");
    }

    #[test]
    fn note_sales_require_the_partner_to_afford_the_live_price() {
        let (_hub, mut state) = trading_partners();
        // b (jolnar) holds the Research Agreement. Its true price is 4; a flat 2 would have
        // listed it for a partner holding only three trade goods.
        state.player_mut(&a()).unwrap().trade_goods = 3;
        let offers = offer_options(&state, ContentStore::embedded(), &b(), &a());
        assert!(
            !offers.iter().any(|option| option.id.ends_with(":4")),
            "the Research Agreement costs 4 and the partner holds 3, so it is not for sale"
        );
        // It is still offerable as a gift, which is a legal transaction (94.3). Requiring the sale
        // price for the note to appear at all made a poor partner unreachable at any terms.
        assert!(
            offers.iter().any(|option| option.id == "pnra:jolnar:0"),
            "but a gift is still on the table: {:?}",
            offers.iter().map(|o| &o.id).collect::<Vec<_>>()
        );

        state.player_mut(&a()).unwrap().trade_goods = 4;
        let offers = offer_options(&state, ContentStore::embedded(), &b(), &a());
        assert!(offers.iter().any(|option| option.id == "pnra:jolnar:4"));
    }

    /// A gift offer round-trips: the id parses back into a deal that hands the note over free.
    ///
    /// Acceptance criterion 7 of the promissory-note bug -- an option that is listed but does not
    /// survive parsing is worse than one that is missing, because it looks available and fails at
    /// execution.
    #[test]
    fn a_note_gift_parses_back_into_a_free_transfer() {
        let (_hub, state) = trading_partners();
        let deal = offer_from(&state, "pnra:jolnar:0", &b(), &a()).expect("a gift is a shape");
        assert_eq!(deal.given.promissory.as_deref(), Some("ra:jolnar"));
        assert_eq!(deal.received.trade_goods, 0, "nothing comes back");
        assert!(
            deal.received.promissory.is_none(),
            "and it is a gift, not a swap"
        );
    }

    #[test]
    fn a_priced_note_id_parses_back_into_the_same_deal() {
        let (_hub, state) = trading_partners();
        for id in ["pncf:hacan:2", "pnra:jolnar:4", "pnconvoys:hacan:3"] {
            let deal = offer_from(&state, id, &a(), &b()).expect("recognised shape");
            let (note, price) = id[2..].rsplit_once(':').expect("priced note id");
            assert_eq!(deal.given.promissory.as_deref(), Some(note));
            assert_eq!(deal.received.trade_goods, price.parse::<i32>().unwrap());
        }
        // No price suffix is not a recognised shape. The oracle would raise on the parse; this
        // engine declines to guess rather than invent a deal.
        assert!(offer_from(&state, "pncf:hacan", &a(), &b()).is_none());
    }

    #[test]
    fn a_note_you_have_already_lent_out_cannot_be_sold_again() {
        // Once lent out the note is no longer in a's hands, so a cannot sell it. The oracle
        // prices and offers by holder, not by original owner (G3b).
        let (hub, mut state) = trading_partners();
        crate::promissory::deal(
            &mut state,
            ti4_content::ContentStore::embedded(),
            ti4_model::content_types::POK,
        );
        let note = "cf:hacan".to_owned();
        crate::promissory::take(
            &mut state,
            ti4_content::ContentStore::embedded(),
            &b(),
            &note,
        );

        let offer = Offer {
            proposer: a(),
            partner: b(),
            given: Terms {
                promissory: Some(note),
                ..Terms::default()
            },
            received: goods(1),
        };

        assert_eq!(
            why_illegal(&state, ContentStore::embedded(), &hub.galaxy, &offer),
            Some(OfferError::CannotPay(a()))
        );
    }

    #[test]
    fn support_is_not_offered_once_it_is_lent() {
        let (_, mut state) = trading_partners();
        assert!(
            offer_options(&state, ti4_content::ContentStore::embedded(), &a(), &b())
                .iter()
                .any(|o| o.id == "ss")
        );

        crate::promissory::receive(&mut state, &b(), &crate::promissory::support("hacan"));

        assert!(
            !offer_options(&state, ti4_content::ContentStore::embedded(), &a(), &b())
                .iter()
                .any(|o| o.id == "ss"),
            "a has nothing left to swap"
        );
    }

    #[test]
    fn a_gift_is_not_a_swap() {
        // "c3:0" gifts three commodities; "cc3" swaps three each. A plain "c" prefix tested
        // first reads every swap as a gift, and one side silently gets nothing.
        let (_, state) = trading_partners();
        let gift = offer_from(&state, "c3:0", &a(), &b()).unwrap();
        assert_eq!(gift.given.commodities, 3);
        assert!(gift.received.is_empty(), "a gift asks for nothing back");

        let swap = offer_from(&state, "cc3", &a(), &b()).unwrap();
        assert_eq!(swap.given.commodities, 3);
        assert_eq!(swap.received.commodities, 3);
    }

    #[test]
    fn every_offered_option_is_a_deal_that_can_be_paid_for() {
        // An option nobody can honour is an offer the partner is asked to accept and the engine
        // then rejects, which spends the turn's transaction on nothing.
        let (hub, mut state) = trading_partners();
        state.player_mut(&b()).unwrap().trade_goods = 0;

        for option in offer_options(&state, ti4_content::ContentStore::embedded(), &a(), &b()) {
            let offer = offer_from(&state, &option.id, &a(), &b())
                .unwrap_or_else(|| panic!("{} does not parse back into a deal", option.id));
            assert_eq!(
                why_illegal(&state, ContentStore::embedded(), &hub.galaxy, &offer),
                None,
                "{} was offered but cannot be paid for",
                option.id
            );
        }
    }

    #[test]
    fn nothing_for_nothing_is_never_offered() {
        let (hub, state) = trading_partners();
        for option in offer_options(&state, ti4_content::ContentStore::embedded(), &a(), &b()) {
            let offer = offer_from(&state, &option.id, &a(), &b()).unwrap();
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
            why_illegal(&state, ContentStore::embedded(), &hub.galaxy, &empty),
            Some(OfferError::Empty)
        );
    }

    #[test]
    fn no_deal_shape_is_written_twice() {
        // A sampling decider draws per option, so a shape written twice is twice as likely as
        // an equally good one written once.
        let (_, state) = trading_partners();
        let options = offer_options(&state, ti4_content::ContentStore::embedded(), &a(), &b());
        let mut deals: Vec<(Terms, Terms)> = options
            .iter()
            .map(|option| {
                let offer = offer_from(&state, &option.id, &a(), &b()).unwrap();
                (offer.given, offer.received)
            })
            .collect();
        let before = deals.len();
        deals.dedup_by(|left, right| left == right);
        deals.sort_by_key(|deal| format!("{deal:?}"));
        deals.dedup();

        assert_eq!(before, deals.len(), "a deal shape appears more than once");
    }

    #[test]
    fn terms_read_the_way_the_oracle_reads_them() {
        assert_eq!(Terms::default().describe(), "nothing");
        let goods = Terms {
            trade_goods: 2,
            ..Terms::default()
        };
        assert_eq!(goods.describe(), "2 trade goods");
        let full = Terms {
            trade_goods: 1,
            commodities: 3,
            fragments: vec!["engine".to_owned()],
            promissory: Some("cf:b".to_owned()),
            action_card: Some(ActionCardId::new("emergency")),
            secret: Some(SecretObjectiveId::new("sb")),
        };
        assert_eq!(
            full.describe(),
            "1 trade goods, 3 commodities, 1 relic fragments, cf:b, the action card emergency, the secret objective sb"
        );
    }

    #[test]
    fn the_offer_prompt_names_the_faction_and_every_term() {
        let (_, state) = trading_partners();
        let offer = Offer {
            proposer: a(),
            partner: b(),
            given: Terms {
                trade_goods: 1,
                ..Terms::default()
            },
            received: Terms::default(),
        };
        let name = state.player(&a()).unwrap().faction.as_str();
        assert_eq!(
            offer.describe(&state),
            format!("{name} gives 1 trade goods for nothing")
        );
    }

    #[test]
    fn answering_an_offer_offers_accept_refuse_counter_like_the_oracle() {
        let (hub, mut state) = trading_partners();
        let content = ti4_content::ContentStore::embedded();
        let mut window = TradeWindow::open(&mut state, &a(), &b());
        let proposing = window
            .pending_choice(&state, content)
            .expect("offers exist");
        assert_eq!(
            proposing.prompt,
            format!("transaction with {}", faction_name(&state, &b()))
        );

        // Offer one trade good for nothing; the receiver answers.
        let answer = proposing
            .option("1:0")
            .cloned()
            .expect("a 1-for-0 deal is on the table");
        assert_eq!(
            window.resolve(&mut state, ContentStore::embedded(), &hub.galaxy, &answer),
            Traded::Offered
        );

        let answering = window.pending_choice(&state, content).unwrap();
        let name_a = state.player(&a()).unwrap().faction.as_str();
        assert_eq!(
            answering.prompt,
            format!("{name_a} gives 1 trade goods for nothing -- accept?")
        );

        // Same ids, kinds and order as the oracle's answer tuple: accept, refuse, counter.
        let refuse = answering
            .option("refuse")
            .expect("refuse exists by that name");
        assert_eq!(refuse.kind, crate::choice::DECLINE_KIND);
        assert!(
            refuse.is_decline(),
            "the window must still treat it as a decline"
        );

        // Accept is priced from the receiver's side: handed 1 trade good, asked for nothing.
        let net = answering
            .option("accept")
            .unwrap()
            .payload
            .get("net")
            .and_then(Value::as_f64)
            .expect("the accept option is priced");
        assert!(
            (net - 1.0).abs() < f64::EPSILON,
            "handed a trade good for nothing: {net}"
        );
    }

    #[test]
    fn every_offer_option_carries_its_net_like_the_oracle() {
        let (_, state) = trading_partners();
        for option in offer_options(&state, ti4_content::ContentStore::embedded(), &a(), &b()) {
            assert!(
                option.payload.contains_key("net"),
                "{} is unpriced",
                option.id
            );
            assert!(
                option.payload.contains_key("their_net"),
                "{} is unpriced from the other chair",
                option.id
            );
        }
    }

    #[test]
    fn opening_a_transaction_names_the_partners_faction() {
        let (hub, mut state) = trading_partners();
        // Distinct factions: the id names a faction, and two "generic" seats cannot be told
        // apart by that name. Real tables seat distinct ones (the oracle's player *is* its
        // faction), so this is the path parity actually runs on.
        state.player_mut(&a()).unwrap().faction = ti4_model::id::FactionId::new("hacan");
        state.player_mut(&b()).unwrap().faction = ti4_model::id::FactionId::new("jolnar");
        let options = crate::transactions::available_actions(
            &state,
            ti4_content::ContentStore::embedded(),
            &hub.galaxy,
            &a(),
        );
        assert!(!options.is_empty());
        for option in &options {
            let named = option
                .id
                .strip_prefix(OPEN_PREFIX)
                .unwrap_or("<wrong prefix>");
            assert!(option.id.starts_with(OPEN_PREFIX), "{}", option.id);
            assert_eq!(option.label, format!("open a transaction with {named}"));
            // The id must resolve back to the seat that holds that faction.
            let seat = opens_with(&state, option).expect("the id resolves back to a seat");
            assert_eq!(faction_name(&state, &seat), named);
        }
    }

    fn card(id: &str) -> Terms {
        Terms {
            action_card: Some(ti4_model::id::ActionCardId::new(id)),
            ..Terms::default()
        }
    }

    #[test]
    fn the_arbiters_ability_resolves_through_the_faction_record() {
        let (_, state) = trading_partners(); // a = hacan, b = jolnar
        assert!(trades_action_cards(&state, ContentStore::embedded(), &a()));
        assert!(!trades_action_cards(&state, ContentStore::embedded(), &b()));
    }

    #[test]
    fn card_terms_need_arbiters_at_the_table() {
        let (hub, mut state) = trading_partners(); // a = hacan, b = jolnar
        let content = ContentStore::embedded();

        // Neither chair has the ability: a card in either leg is flatly rejected before any
        // holding question is asked.
        state.player_mut(&a()).unwrap().faction = FactionId::new("jolnar");
        state.player_mut(&b()).unwrap().faction = FactionId::new("letnev");
        for player in [a(), b()] {
            state
                .player_mut(&player)
                .unwrap()
                .action_cards
                .push(ti4_model::id::ActionCardId::new("emergency"));
        }
        let offer = Offer {
            proposer: a(),
            partner: b(),
            given: card("emergency"),
            received: goods(1),
        };
        assert_eq!(
            why_illegal(&state, content, &hub.galaxy, &offer),
            Some(OfferError::ActionCardsNotTradeable)
        );

        // One chair has it (a = hacan again): legal when the giver actually holds the card…
        state.player_mut(&a()).unwrap().faction = FactionId::new("hacan");
        assert_eq!(why_illegal(&state, content, &hub.galaxy, &offer), None);

        // …and each side must hold what its own leg hands over.
        let mut state2 = trading_partners().1;
        state2
            .player_mut(&b())
            .unwrap()
            .action_cards
            .push(ti4_model::id::ActionCardId::new("emergency"));
        // a (hacan) offers b's card: the leg names it, but a does not hold it.
        let offer = Offer {
            proposer: a(),
            partner: b(),
            given: card("emergency"),
            received: goods(1),
        };
        assert_eq!(
            why_illegal(&state2, content, &hub.galaxy, &offer),
            Some(OfferError::MissingActionCard(a(), "emergency".to_owned()))
        );
    }

    #[test]
    fn arbiters_offer_the_first_sorted_card_for_one_trade_good() {
        let (_, mut state) = trading_partners(); // a = hacan, b = jolnar, both hold 2 goods
        let content = ContentStore::embedded();

        state.player_mut(&a()).unwrap().action_cards = vec![
            ti4_model::id::ActionCardId::new("hack"),
            ti4_model::id::ActionCardId::new("emergency"),
        ];
        let options = offer_options(&state, content, &a(), &b());
        let ac: Vec<_> = options.iter().filter(|o| o.id.starts_with("ac")).collect();
        assert_eq!(ac.len(), 1, "exactly one card option: {ac:?}");
        assert_eq!(ac[0].id, "acemergency:1", "the sorted head of the hand");
        assert_eq!(
            ac[0].label,
            "sell the action card emergency for 1 trade good"
        );
        assert_eq!(
            ac[0].payload.get("action_card"),
            Some(&serde_json::Value::String("emergency".to_owned()))
        );

        // No hand, nothing to sell.
        state.player_mut(&a()).unwrap().action_cards.clear();
        let options = offer_options(&state, content, &a(), &b());
        assert!(options.iter().all(|o| !o.id.starts_with("ac")));

        // A hand but a partner who cannot pay one trade good.
        state
            .player_mut(&a())
            .unwrap()
            .action_cards
            .push(ti4_model::id::ActionCardId::new("emergency"));
        state.player_mut(&b()).unwrap().trade_goods = 0;
        let options = offer_options(&state, content, &a(), &b());
        assert!(options.iter().all(|o| !o.id.starts_with("ac")));
    }

    #[test]
    fn a_priced_card_id_parses_back_into_the_same_deal() {
        let (_, state) = trading_partners();
        for id in ["acemergency:1", "achack:1"] {
            let deal = offer_from(&state, id, &a(), &b()).expect("the id names a deal");
            // The card is whatever sits between the `ac` prefix and the price suffix.
            let expected = &id[2..id.len() - 2];
            assert_eq!(
                deal.given.action_card.as_ref().map(ActionCardId::as_str),
                Some(expected)
            );
        }
        let deal = offer_from(&state, "acemergency:1", &a(), &b()).unwrap();
        assert_eq!(deal.received.trade_goods, 1);
        // An unpriced form names no deal (the oracle would raise; declining is the safe twin).
        assert!(offer_from(&state, "acemergency", &a(), &b()).is_none());
    }

    #[test]
    // The flat one-trade-good price is a constant, not arithmetic: exact comparison is intended.
    #[allow(clippy::float_cmp)]
    fn action_cards_price_at_one_in_both_directions_and_read_their_name() {
        let (_, state) = trading_partners();
        let content = ContentStore::embedded();
        let terms = card("emergency");
        assert_eq!(terms.worth_to_receiver(&state, content), 1.0);
        assert_eq!(terms.cost_to_giver(&state, content), 1.0);
        assert!(!terms.is_empty());
        assert_eq!(terms.describe(), "the action card emergency");
    }

    #[test]
    fn an_action_card_trade_moves_the_card_and_the_goods() {
        let (hub, mut state) = trading_partners(); // a = hacan holds 2 goods; b holds 2
        let content = ContentStore::embedded();
        state
            .player_mut(&a())
            .unwrap()
            .action_cards
            .push(ti4_model::id::ActionCardId::new("emergency"));
        let offer = Offer {
            proposer: a(),
            partner: b(),
            given: card("emergency"),
            received: goods(1),
        };
        resolve(&mut state, content, &hub.galaxy, &offer).expect("legal: arbiters at the table");

        assert!(state.player(&a()).unwrap().action_cards.is_empty());
        let b_hand = state.player(&b()).unwrap().action_cards.clone();
        assert_eq!(b_hand, vec![ti4_model::id::ActionCardId::new("emergency")]);
        // The sale prices at one trade good: the seller (a) receives it, the buyer (b) pays it.
        assert_eq!(state.player(&a()).unwrap().trade_goods, 3);
        assert_eq!(state.player(&b()).unwrap().trade_goods, 1);
    }

    // -- the contents of a deal, for clients ------------------------------------------------

    /// What `offer_from` makes of an option of the window, as items.
    fn deal_of(state: &GameState, id: &str, proposer: &PlayerId, partner: &PlayerId) -> TradeDeal {
        let offer =
            offer_from(state, id, proposer, partner).unwrap_or_else(|| panic!("{id} parses"));
        TradeDeal::of(&offer, ContentStore::embedded())
    }

    fn note(card: &str, owner: &str) -> TradeItem {
        TradeItem::Note {
            card: card.to_owned(),
            owner: FactionId::new(owner),
        }
    }

    #[test]
    fn a_note_names_its_record_in_the_catalogue() {
        let content = ContentStore::embedded();
        assert_eq!(
            crate::promissory::record_id(content, "support:hacan"),
            "<color>_sftt"
        );
        assert_eq!(
            crate::promissory::record_id(content, "cf:sol"),
            "<color>_cf"
        );
        assert_eq!(
            crate::promissory::record_id(content, "ta:nomad"),
            "<color>_ta"
        );
        assert_eq!(crate::promissory::record_id(content, "ra:jolnar"), "ra");
        for record in ["<color>_sftt", "<color>_cf", "<color>_ta", "ra"] {
            assert!(
                content
                    .get(
                        ti4_model::content_types::ContentType::PromissoryNotes,
                        record
                    )
                    .is_some(),
                "{record} is a record of the catalogue"
            );
        }
    }

    /// Every offer of the proposer's question carries what it moves, as `details.transaction`:
    /// the deal that its id stands for, item by item, and nothing for the decline.
    #[test]
    fn every_offer_option_carries_its_contents_in_the_details() {
        let (_hub, mut state) = trading_partners();
        let content = ContentStore::embedded();
        // Black Market Dealings puts a secret objective and a relic fragment on the table, and
        // Hacan's Arbiters an action card.
        state.transient_flags.set(TransientFlags::BLACK_MARKET);
        let seat = state.player_mut(&a()).unwrap();
        seat.trade_goods = 4;
        seat.action_cards.push(ActionCardId::new("shutdown"));
        seat.secret_objectives.push(SecretObjectiveId::new("ans"));
        seat.relic_fragments.insert("CULTURAL".to_owned(), 1);
        let window = TradeWindow::open(&mut state, &a(), &b());
        let choice = window
            .pending_choice(&state, content)
            .expect("deals to propose");

        let hints: TransactionHints = choice.hint("transaction").expect("typed contents");
        assert_eq!((&hints.proposer, &hints.partner), (&a(), &b()));
        assert_eq!(hints.offer, None, "nothing is on the table yet");
        for option in &choice.options {
            if option.is_decline() {
                assert!(!hints.offers.contains_key(&option.id));
            } else {
                assert_eq!(
                    hints.offers.get(&option.id),
                    Some(&deal_of(&state, &option.id, &a(), &b())),
                    "{}",
                    option.id
                );
            }
        }
        assert_eq!(
            hints.offers.len(),
            choice.options.len() - 1,
            "a deal for each offer and for nothing else"
        );

        let offered = |id: &str| {
            hints
                .offers
                .get(id)
                .unwrap_or_else(|| panic!("{id} is offered"))
                .clone()
        };
        let deal = |given: Vec<TradeItem>, received: Vec<TradeItem>| TradeDeal { given, received };
        assert_eq!(
            offered("cc3"),
            deal(
                vec![TradeItem::Commodities { count: 3 }],
                vec![TradeItem::Commodities { count: 3 }]
            )
        );
        assert_eq!(
            offered("1:0"),
            deal(vec![TradeItem::TradeGoods { count: 1 }], vec![])
        );
        assert_eq!(
            offered("ss"),
            deal(
                vec![note("<color>_sftt", "hacan")],
                vec![note("<color>_sftt", "jolnar")]
            )
        );
        assert_eq!(
            offered("pncf:hacan:2"),
            deal(
                vec![note("<color>_cf", "hacan")],
                vec![TradeItem::TradeGoods { count: 2 }]
            )
        );
        assert_eq!(
            offered("npra:jolnar:4"),
            deal(
                vec![TradeItem::TradeGoods { count: 4 }],
                vec![note("ra", "jolnar")]
            )
        );
        assert_eq!(
            offered("acshutdown:1"),
            deal(
                vec![TradeItem::ActionCard {
                    card: ActionCardId::new("shutdown")
                }],
                vec![TradeItem::TradeGoods { count: 1 }]
            )
        );
        assert_eq!(
            offered("soans:1"),
            deal(
                vec![TradeItem::SecretObjective {
                    card: SecretObjectiveId::new("ans")
                }],
                vec![TradeItem::TradeGoods { count: 1 }]
            )
        );
        assert_eq!(
            offered("frCULTURAL:1"),
            deal(
                vec![TradeItem::RelicFragments {
                    fragment: "cultural".to_owned(),
                    count: 1
                }],
                vec![TradeItem::TradeGoods { count: 1 }]
            )
        );

        // The JSON that the clients read.
        let wire = &choice.details["transaction"];
        assert_eq!(wire["proposer"], "a");
        assert_eq!(wire["partner"], "b");
        assert_eq!(wire["offers"]["cc3"]["given"][0]["kind"], "commodities");
        assert_eq!(wire["offers"]["cc3"]["given"][0]["count"], 3);
        assert_eq!(wire["offers"]["ss"]["received"][0]["kind"], "note");
        assert_eq!(wire["offers"]["ss"]["received"][0]["card"], "<color>_sftt");
        assert_eq!(wire["offers"]["ss"]["received"][0]["owner"], "jolnar");
        assert_eq!(
            wire["offers"]["frCULTURAL:1"]["given"][0]["trait"],
            "cultural"
        );
        assert!(wire.get("offer").is_none());
    }

    /// The partner's question carries the deal on the table, and a counter is asked of the other
    /// chair with its seats swapped.
    #[test]
    fn the_answer_carries_the_deal_on_the_table() {
        let (hub, mut state) = trading_partners();
        let content = ContentStore::embedded();
        let mut window = TradeWindow::open(&mut state, &a(), &b());
        let proposing = window.pending_choice(&state, content).unwrap();
        let sale = proposing
            .option("pncf:hacan:2")
            .cloned()
            .expect("hacan's Ceasefire is for sale");
        assert_eq!(
            window.resolve(&mut state, content, &hub.galaxy, &sale),
            Traded::Offered
        );

        let answering = window.pending_choice(&state, content).unwrap();
        assert_eq!(answering.player, b());
        assert_eq!(
            answering.hint::<TransactionHints>("transaction"),
            Some(TransactionHints {
                proposer: a(),
                partner: b(),
                offers: BTreeMap::new(),
                offer: Some(TradeDeal {
                    given: vec![note("<color>_cf", "hacan")],
                    received: vec![TradeItem::TradeGoods { count: 2 }],
                }),
            })
        );

        let counter = answering.option("counter").cloned().unwrap();
        assert_eq!(
            window.resolve(&mut state, content, &hub.galaxy, &counter),
            Traded::Countered
        );
        let countering = window.pending_choice(&state, content).unwrap();
        let hints: TransactionHints = countering.hint("transaction").unwrap();
        assert_eq!((&hints.proposer, &hints.partner), (&b(), &a()));
        assert!(
            hints
                .offers
                .keys()
                .all(|id| countering.option(id).is_some())
        );
    }
}
