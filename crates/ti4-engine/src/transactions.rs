//! Transactions between neighbours (LRR 60, and 21.5 for commodities).
//!
//! Ported from the oracle's `engine/transactions.py`: `_presence`, `are_neighbours`,
//! `_holdings`, `_can_pay`, `why_illegal`, `_take`, `_give` and `resolve`.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use ti4_content::ContentStore;
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
    /// A promissory note, by id.
    ///
    /// A note is a loan rather than a sale — every one of them says "then, return this card" —
    /// which is why what it costs to part with is not what it is worth to receive.
    pub promissory: Option<String>,
}

impl Terms {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.trade_goods == 0
            && self.commodities == 0
            && self.fragments.is_empty()
            && self.promissory.is_none()
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
        f64::from(self.trade_goods)
            + f64::from(self.commodities)
            + self.fragments.len() as f64
            + note
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
        f64::from(self.trade_goods)
            + 0.2 * f64::from(self.commodities)
            + self.fragments.len() as f64
            + note
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
    if !are_neighbours(state, galaxy, &offer.proposer, &offer.partner) {
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
    if let Some(note) = terms.promissory.clone() {
        // Support is worth a victory point the moment it arrives, which is the whole reason the
        // note is worth trading for; every other note simply changes hands.
        if note.starts_with(crate::promissory::SUPPORT_PREFIX) {
            crate::promissory::receive(state, player, &note);
        } else {
            crate::promissory::take(state, player, &note);
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

/// The prefix of an option that opens a transaction. Component actions carry the full
/// `component|` prefix in both engines; trade was the one Rust shape missing it, which made its
/// id tokenize differently at every action-phase decision.
const OPEN_PREFIX: &str = "component|trade|";

/// What a note sells for.
///
/// One price for every note, which is a simplification the oracle prices per card. Recorded as
/// one rather than presented as the rule: a flat price makes a Research Agreement cost what a
/// Ceasefire does.
const NOTE_PRICE: i32 = 2;

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
#[must_use]
pub fn available_actions(
    state: &GameState,
    content: &ContentStore,
    galaxy: &Galaxy,
    player: &PlayerId,
) -> Vec<crate::choice::ChoiceOption> {
    let already = state.transacted_with(player);
    partners(state, content, galaxy, player)
        .into_iter()
        .filter(|other| !already.contains(other))
        .map(|other| {
            let name = faction_name(state, &other);
            crate::choice::ChoiceOption::labelled(
                format!("{OPEN_PREFIX}{name}"),
                OPEN_KIND,
                format!("open a transaction with {name}"),
            )
        })
        .collect()
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
        if their_goods >= NOTE_PRICE {
            // The alias goes in the payload rather than the id: token matching splits an option
            // id on "|", never ":", so a `pn{alias}` suffix would silently leak the note's kind
            // into every feature bucket (oracle `_priced` keeps ids clean for exactly this).
            let mut payload = BTreeMap::new();
            payload.insert("note".to_owned(), Value::String(note.clone()));
            payload.insert(
                "alias".to_owned(),
                Value::String(crate::promissory::alias_of(&note).to_owned()),
            );
            let id = format!("pn{note}");
            let label = format!("sell {note} for {NOTE_PRICE} trade goods");
            shapes.push((id, label, payload));
        }
    }

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

    priced(state, content, proposer, partner, shapes)
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
    if let Some(note) = id.strip_prefix("pn") {
        return deal(
            Terms {
                promissory: Some(note.to_owned()),
                ..Terms::default()
            },
            goods(NOTE_PRICE),
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
                options.push(crate::choice::ChoiceOption::decline());
                Some(crate::choice::Choice::new(
                    self.proposer.clone(),
                    format!("transaction with {}", faction_name(state, &self.partner)),
                    options,
                ))
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
                Some(crate::choice::Choice::new(
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
                ))
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
                    return match resolve(state, content, galaxy, &offer) {
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
        let note = sale.id.trim_start_matches("pn").to_owned();

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
        crate::promissory::take(&mut state, &b(), &note);

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
        };
        assert_eq!(
            full.describe(),
            "1 trade goods, 3 commodities, 1 relic fragments, cf:b"
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
}
