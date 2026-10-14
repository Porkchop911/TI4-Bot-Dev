//! The Council Keleres, in its three Tribuni variants: Keleres–Mentak (`keleresm`), Keleres–Xxcha
//! (`keleresx`) and Keleres–Argent (`keleresa`). The content models The Tribuni's setup choice as
//! three faction records with their own home systems, heroes and Rider note ids, so this file
//! registers three modules over one shared implementation. See `plans/evidence/BF-keleres.md`.
//!
//! Split for parallel work: this file holds the abilities (The Tribuni, Council Patronage, Law's
//! Order), the technologies, the breakthrough, the commander/agent and the variant plumbing;
//! `keleres_units.rs` holds the flagship, mech, the three heroes and the Keleres Rider.
//!
//! **One module carries the hooks.** The engine calls every module's hooks for every seat, so
//! three modules sharing one hook table would run each hook three times (three copies of every
//! timing ability, three offers of every component action). [`MODULE_M`] carries the table; the
//! other two variants register their claims only. Every hook decides from the seat's own faction
//! ([`is_keleres_faction`]), never from the module it was called through.
//!
//! Card texts (latest printing, `crates/ti4-content/content/*.json`):
//!
//! * The Tribuni: "During setup, choose an unplayed faction from among the Mentak, the Xxcha and The
//!   Argent Flight; take that faction's home system, command tokens and control markers.
//!   Additionally, take the Keleres Hero that corresponds to that faction." Implemented in
//!   `seating.rs` ([`crate::seating::validate_tribuni`]): the variant's base faction must be
//!   unplayed. The home system and hero are the variant record's own; command tokens are pool counts
//!   and control is per seat, so there is nothing further to take.
//! * Council Patronage: "At the start of the strategy phase: Replenish your commodities, then gain
//!   1 trade good."
//! * Law's Order: "At the start of any player's turn: You may spend 1 trade good or 1 commodity to
//!   treat all laws as blank until the end of that turn." ([`crate::laws::blanked`].)
//! * Agency Supply Network (`asn`): "Once per action, when you resolve a unit's PRODUCTION ability,
//!   you may resolve another of your unit's PRODUCTION abilities in any system."
//!   ([`crate::production::agency_supply_network`].)
//! * I.I.H.Q. Modernization (`iihq`): "You are neighbors with all players that have units or control
//!   planets in or adjacent to the Mecatol Rex system. Gain the Custodia Vigilia planet card and its
//!   legendary planet ability card. You cannot lose these cards, and this card cannot have an X or Y
//!   assimilator token placed on it." The breakthrough `keleresbt` has the same two clauses.
//! * Executive Order (`executiveorder`): "ACTION: Exhaust this card and draw the top or bottom card
//!   of the Agenda deck. Players immediately vote on this agenda as if you were the speaker; you can
//!   spend trade goods and resources on this agenda as if they were votes."
//! * Xander Alexin Victori III (`keleresagent`): "At any time: You may exhaust this card to allow any
//!   player to spend commodities as if they were trade goods."
//! * Suffi An (`kelerescommander`): unlock "Spend 1 trade good after you play an action card that has
//!   a component action"; the effect is in `borrowed_commanders.rs`.
//!
//! **Scoping decisions** (also in the evidence file):
//!
//! * Custodia Vigilia is placed in the Keleres' home system as a planet card
//!   (`planets::place`), gained the first step after the seat holds `iihq` or `keleresbt`
//!   ([`reconcile`], called by the driver before each step: tech and breakthrough reach a seat from
//!   many places and none announces it). It holds no units. Its legendary card is the planet's
//!   standing effect (SPACE CANNON 5 and PRODUCTION 3 for Mecatol Rex, two command tokens when
//!   another player scores Imperial's Mecatol point).
//! * The agent's "at any time" is offered where a player pays ([`offer_agent`]): once at the start of
//!   each payment window of a player who holds commodities, to the Keleres holder of a readied
//!   agent. The windows are `production::pay` and its variants (every spend that goes through the
//!   shared payment loop: unit production by ability, relics, exploration, Custodians, faction
//!   abilities), the production windows (tactical action, Warfare/Construction, abilities), the
//!   Technology card's paid research and Leadership's token purchase. When used, the payer's
//!   commodities count as trade goods for that window only ([`close_agent_window`]).
//! * Executive Order exhausts the card and draws the agenda ([`perform_component`]); the driver
//!   (`Game::begin_executive_order`) then reveals it and runs the vote through the ordinary agenda
//!   machinery, with the owner seated as speaker for the vote and a `VoteWindow` that lets the
//!   owner spend trade goods and resources as votes (`VoteWindow::with_spender`).

use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{LeaderId, PlanetId, PlayerId, SystemId, TechnologyId};
use ti4_model::state::{GameState, LeaderStatus};

use super::hooks_economy::EconomyHooks;
use super::{FactionModule, Hooks};
use crate::choice::{Choice, ChoiceOption, IllegalChoice};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::timing::{Ability, Relation, TimingContext, TimingError};

/// The three variant aliases, in Tribuni order.
pub const VARIANTS: [&str; 3] = ["keleresm", "keleresx", "keleresa"];

/// Whether a content record's `faction` tag belongs to `alias`.
///
/// A record normally names its faction by alias. Keleres is the exception: its shared cards are
/// tagged with the family `keleres` (technologies, leaders, breakthrough) or with the first variant
/// `keleresm` (units shared by all three), while each variant's Rider is tagged with the variant.
/// The variant's own faction sheet still decides which leaders and units it actually has.
#[must_use]
pub fn tag_belongs_to(tag: &str, alias: &str, unit: bool) -> bool {
    tag.eq_ignore_ascii_case(alias)
        || (is_keleres_faction(alias)
            && (tag.eq_ignore_ascii_case("keleres")
                || (unit && tag.eq_ignore_ascii_case("keleresm"))))
}

/// Whether `faction` is any Keleres variant.
#[must_use]
pub fn is_keleres_faction(faction: &str) -> bool {
    VARIANTS.contains(&faction)
}

const COUNCIL_PATRONAGE: &str = "council_patronage";
const LAWS_ORDER: &str = "laws_order";
const THE_TRIBUNI: &str = "the_tribuni";
const ASN: &str = "asn";
const IIHQ: &str = "iihq";
const EXECUTIVE_ORDER: &str = "executiveorder";
const BREAKTHROUGH: &str = "keleresbt";
const AGENT: &str = "keleresagent";
const COMMANDER: &str = "kelerescommander";
/// The planet card I.I.H.Q. Modernization gives.
pub const CUSTODIA: &str = "custodiavigilia";
/// Custodian's Favour: Mecatol Rex gains SPACE CANNON 5 (one die)...
const CUSTODIAN_CANNON: (u32, usize) = (5, 1);
/// ... and PRODUCTION 3.
const CUSTODIAN_PRODUCTION: i64 = 3;

/// The component-action option id prefix (`faction|<alias>|`; the family alias is shared by all
/// three variants).
const OPTION_PREFIX: &str = "faction|keleres|";

/// `faction_marks` key: `player` used Agency Supply Network since the last action ended.
fn asn_key(player: &PlayerId) -> String {
    format!("keleres|asn_used|{player}")
}

const AGENT_KEY_PREFIX: &str = "keleres|agent_goods|";

/// `faction_marks` key: present while `player` may spend commodities as trade goods, that is inside
/// a payment window the agent was used for (value: the Keleres seat that used it).
fn agent_key(player: &PlayerId) -> String {
    format!("{AGENT_KEY_PREFIX}{player}")
}

/// The hooks, carried by exactly one module (see the module docs).
const HOOKS: Hooks = Hooks {
    timing_abilities: Some(timing_abilities),
    component_actions: Some(component_actions),
    perform_component: Some(perform_component),
    // Harka Leeds (keleres_units.rs): an ACTION leader.
    leader_action: Some(super::keleres_units::leader_action),
    use_leader: Some(super::keleres_units::use_leader),
    combat: super::keleres_units::COMBAT_HOOKS,
    economy: EconomyHooks {
        extra_production_planet: Some(extra_production_planet),
        ..EconomyHooks::NONE
    },
    ..Hooks::NONE
};

const ABILITIES: &[&str] = &[COUNCIL_PATRONAGE, LAWS_ORDER, THE_TRIBUNI];
const TECHNOLOGIES: &[&str] = &[ASN, IIHQ, EXECUTIVE_ORDER];
const BREAKTHROUGHS: &[&str] = &[BREAKTHROUGH];

/// Keleres–Mentak. Carries the shared hooks.
pub const MODULE_M: FactionModule = FactionModule {
    alias: "keleresm",
    abilities: ABILITIES,
    technologies: TECHNOLOGIES,
    units: super::keleres_units::UNITS,
    promissory: super::keleres_units::PROMISSORY_M,
    leaders: super::keleres_units::LEADERS_M,
    breakthroughs: BREAKTHROUGHS,
    hooks: HOOKS,
};

/// Keleres–Xxcha. Claims only: the hooks are [`MODULE_M`]'s.
pub const MODULE_X: FactionModule = FactionModule {
    alias: "keleresx",
    abilities: ABILITIES,
    technologies: TECHNOLOGIES,
    units: super::keleres_units::UNITS,
    promissory: super::keleres_units::PROMISSORY_X,
    leaders: super::keleres_units::LEADERS_X,
    breakthroughs: BREAKTHROUGHS,
    hooks: Hooks::NONE,
};

/// Keleres–Argent. Claims only: the hooks are [`MODULE_M`]'s.
pub const MODULE_A: FactionModule = FactionModule {
    alias: "keleresa",
    abilities: ABILITIES,
    technologies: TECHNOLOGIES,
    units: super::keleres_units::UNITS,
    promissory: super::keleres_units::PROMISSORY_A,
    leaders: super::keleres_units::LEADERS_A,
    breakthroughs: BREAKTHROUGHS,
    hooks: Hooks::NONE,
};

fn decision(state: &GameState, player: &PlayerId, card: &str, subtype: &str) -> DecisionContext {
    DecisionContext::new(
        player.clone(),
        DecisionSource::FactionAbility(card.to_owned()),
        subtype,
        state.phase,
        state.round,
    )
}

/// Whether `player` sits as any Keleres variant.
fn is_keleres(state: &GameState, player: &PlayerId) -> bool {
    state
        .player(player)
        .is_some_and(|seat| is_keleres_faction(seat.faction.as_str()))
}

/// Owns the technology, or the Nekro's Valefar Assimilator carries its text.
fn has_technology(state: &GameState, player: &PlayerId, alias: &str) -> bool {
    crate::technology::has_technology_text(state, player, alias)
}

fn leader_status(state: &GameState, player: &PlayerId, leader: &str) -> Option<LeaderStatus> {
    state
        .player(player)
        .and_then(|seat| seat.leaders.get(&LeaderId::new(leader)).copied())
}

/// Whether `player` holds I.I.H.Q. Modernization in either printing: the technology or the
/// breakthrough (the two carry the same text).
#[must_use]
pub fn has_iihq(state: &GameState, player: &PlayerId) -> bool {
    // The technology may be a Nekro's Valefar Assimilator carrying its text; the breakthrough is
    // the Keleres's own.
    has_technology(state, player, IIHQ)
        || (is_keleres(state, player) && crate::breakthroughs::holds(state, player, BREAKTHROUGH))
}

// -- I.I.H.Q. Modernization: neighbours ----------------------------------------------------------

/// Whether `player` has units or controls planets in or adjacent to the Mecatol Rex system.
fn near_mecatol(state: &GameState, galaxy: &Galaxy, player: &PlayerId) -> bool {
    let mecatol = crate::seating::mecatol_in_galaxy(galaxy);
    let presence = crate::transactions::presence(state, player);
    presence.contains(&SystemId::new(mecatol))
        || galaxy
            .adjacent(mecatol)
            .into_iter()
            .any(|adjacent| presence.contains(&SystemId::new(adjacent)))
}

/// "You are neighbors with all players that have units or control planets in or adjacent to the
/// Mecatol Rex system." Neighbourhood is mutual, so it holds from either side. Called from
/// `transactions::are_neighbours`, the one function every neighbour question goes through.
#[must_use]
pub(crate) fn mecatol_neighbours(
    state: &GameState,
    galaxy: &Galaxy,
    a: &PlayerId,
    b: &PlayerId,
) -> bool {
    (has_iihq(state, a) && near_mecatol(state, galaxy, b))
        || (has_iihq(state, b) && near_mecatol(state, galaxy, a))
}

// -- I.I.H.Q. Modernization: Custodia Vigilia ----------------------------------------------------

/// Whether `player` controls the Custodia Vigilia planet card.
#[must_use]
pub fn holds_custodia(state: &GameState, player: &PlayerId) -> bool {
    state
        .controlled_planets(player)
        .into_iter()
        .any(|(_, planet)| planet.as_str() == CUSTODIA)
}

/// Gain the Custodia Vigilia planet card for every Keleres seat that holds `iihq` or `keleresbt`,
/// and keep it: "You cannot lose these cards". Idempotent, so the driver calls it before every
/// step.
///
/// The card is placed in the seat's home system (`planets::place`: readied and controlled), where
/// it holds no units. If it is already on the board but somebody else controls it, control goes
/// back. Whether anything could take it is the invasion code's question; this is the backstop.
pub fn reconcile(state: &mut GameState) {
    let holders: Vec<(PlayerId, Option<SystemId>)> = state
        .players
        .iter()
        .filter(|seat| has_iihq(state, &seat.id))
        .map(|seat| (seat.id.clone(), seat.home_system.clone()))
        .collect();
    let custodia = PlanetId::new(CUSTODIA);
    for (player, home) in holders {
        match state.placed_planets.get(&custodia).cloned() {
            None => {
                if let Some(home) = home {
                    crate::planets::place(state, &home, &custodia, &player);
                }
            }
            Some(system) => {
                // Only the card's holder keeps it; a second Keleres cannot exist.
                let holder = state
                    .board
                    .get(&system)
                    .and_then(|board| board.planet_control.get(&custodia))
                    .cloned();
                if holder.as_ref() != Some(&player)
                    && let Some(board) = state.board.get_mut(&system)
                {
                    board.set_control(custodia.clone(), player.clone());
                }
            }
        }
    }
}

/// Custodian's Favour, first clause: Mecatol Rex "gains SPACE CANNON 5" for the player who controls
/// it and holds Custodia Vigilia. Read by `planets::attachment_cannons`, which space cannon offense
/// and defense already consult for a planet's "as if it were a unit" cannons.
#[must_use]
pub(crate) fn custodian_cannon(
    state: &GameState,
    planet: &PlanetId,
    owner: &PlayerId,
) -> Option<(PlayerId, u32, usize)> {
    (crate::seating::is_mecatol_planet(planet.as_str()) && holds_custodia(state, owner))
        .then(|| (owner.clone(), CUSTODIAN_CANNON.0, CUSTODIAN_CANNON.1))
}

/// Custodian's Favour, first clause: Mecatol Rex "gains ... PRODUCTION 3" for its controller while
/// they hold Custodia Vigilia. Summed into `production::capacity` as if from a unit on the planet,
/// whether or not any unit stands there.
fn extra_production_planet(
    state: &GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) -> i64 {
    let controls = state
        .board
        .get(system)
        .and_then(|board| board.planet_control.get(planet))
        == Some(player);
    if controls
        && crate::seating::is_mecatol_planet(planet.as_str())
        && holds_custodia(state, player)
    {
        CUSTODIAN_PRODUCTION
    } else {
        0
    }
}

/// Custodian's Favour, second clause: "Gain 2 command tokens when another player scores a victory
/// point with the second clause of the 'Imperial' strategy card." Called by `imperial_primary`
/// after `scorer` was given the point. The holder places the two tokens in the pools they choose.
///
/// # Errors
/// [`IllegalChoice`] when a decider answers a pool question with something not offered.
pub(crate) fn custodians_favour_tokens(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut crate::choice::Table,
    scorer: &PlayerId,
) -> Result<(), IllegalChoice> {
    let holders: Vec<PlayerId> = state
        .seating_order
        .iter()
        .filter(|player| *player != scorer && holds_custodia(state, player))
        .cloned()
        .collect();
    for holder in holders {
        crate::strategy_cards::gain_tokens(state, content, sources, galaxy, table, &holder, 2)?;
    }
    Ok(())
}

// -- Council Patronage ---------------------------------------------------------------------------

/// "At the start of the strategy phase: Replenish your commodities, then gain 1 trade good."
fn council_patronage(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("ability:{owner_name}:{COUNCIL_PATRONAGE}:STRATEGY_PHASE_BEGAN:after"),
        seat.clone(),
        "STRATEGY_PHASE_BEGAN",
        Relation::After,
        Arc::new(move |_event, resolver, context| {
            if !is_keleres(context.state, &owner) {
                return Ok(());
            }
            crate::strategy_cards::replenish(context.state, context.content, &owner);
            crate::supply::gain_trade_goods_via(context, resolver, &owner, 1, COUNCIL_PATRONAGE)?;
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        is_keleres(context.state, &condition_owner)
    }))
}

// -- Law's Order ---------------------------------------------------------------------------------

/// Whether Law's Order has anything to do for `owner`: laws in play, not already blanked, and a
/// trade good or commodity to pay with.
fn laws_order_ready(state: &GameState, owner: &PlayerId) -> bool {
    is_keleres(state, owner)
        && !crate::laws::blanked(state)
        && !state.laws.is_empty()
        && state
            .player(owner)
            .is_some_and(|seat| seat.trade_goods > 0 || seat.commodities > 0)
}

/// "At the start of any player's turn: You may spend 1 trade good or 1 commodity to treat all laws as
/// blank until the end of that turn." Offered at every turn's start while a law is in play.
fn laws_order(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("ability:{owner_name}:{LAWS_ORDER}:TURN_BEGAN:after"),
        seat.clone(),
        "TURN_BEGAN",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            if !laws_order_ready(context.state, &owner) {
                return Ok(());
            }
            let (goods, commodities) = context
                .state
                .player(&owner)
                .map_or((0, 0), |seat| (seat.trade_goods, seat.commodities));
            let pay_commodity = if goods > 0 && commodities > 0 {
                let choice = Choice::new(
                    owner.clone(),
                    "Law's Order: spend a trade good or a commodity".to_owned(),
                    vec![
                        ChoiceOption::labelled("trade_good", "payment", "spend 1 trade good"),
                        ChoiceOption::labelled("commodity", "payment", "spend 1 commodity"),
                    ],
                )
                .contextualized(decision(
                    context.state,
                    &owner,
                    LAWS_ORDER,
                    "laws_order_payment",
                ));
                let answer = context
                    .ask_seeing(&choice)
                    .map_err(TimingError::IllegalChoice)?;
                answer.id == "commodity"
            } else {
                goods == 0
            };
            if let Some(seat) = context.state.player_mut(&owner) {
                if pay_commodity {
                    seat.commodities -= 1;
                } else {
                    seat.trade_goods -= 1;
                }
            }
            crate::laws::blank_for_turn(context.state);
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        laws_order_ready(context.state, &condition_owner)
    }))
}

// -- Agency Supply Network -----------------------------------------------------------------------

/// Whether `player` may resolve another PRODUCTION now: they hold `asn` and have not used it this
/// action.
#[must_use]
pub(crate) fn asn_ready(state: &GameState, player: &PlayerId) -> bool {
    has_technology(state, player, ASN) && !state.faction_marks.contains_key(&asn_key(player))
}

/// Record the use, until the action ends.
pub(crate) fn asn_mark(state: &mut GameState, player: &PlayerId) {
    state.faction_marks.insert(asn_key(player), "1".to_owned());
}

/// "Once per action": the mark goes when any action completes. The resolver's own `OncePerTurn`
/// bookkeeping is not saved with the game, so the limit lives in state.
fn asn_clear(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("technology:{owner_name}:{ASN}_clear:ACTION_COMPLETED:after"),
        seat.clone(),
        "ACTION_COMPLETED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            context.state.faction_marks.remove(&asn_key(&owner));
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        context
            .state
            .faction_marks
            .contains_key(&asn_key(&condition_owner))
    }))
}

// -- Xander Alexin Victori III -------------------------------------------------------------------

/// Safety net: a payment window that ended abnormally (a decider's illegal answer aborted it) must
/// not leave commodities spendable. Every window closes itself; this clears what an abort left once
/// the action is over.
fn agent_clear(owner_name: &str, seat: &PlayerId) -> Ability {
    Ability::stateful(
        format!("leader:{owner_name}:{AGENT}_clear:ACTION_COMPLETED:after"),
        seat.clone(),
        "ACTION_COMPLETED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            context
                .state
                .faction_marks
                .retain(|key, _| !key.starts_with(AGENT_KEY_PREFIX));
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        context
            .state
            .faction_marks
            .keys()
            .any(|key| key.starts_with(AGENT_KEY_PREFIX))
    }))
}

/// Commodities `player` may spend as if they were trade goods right now: all of them inside a
/// payment window the agent was used for ([`offer_agent`]), else none.
#[must_use]
pub(crate) fn spendable_commodities(state: &GameState, player: &PlayerId) -> i64 {
    if state
        .faction_marks
        .get(&agent_key(player))
        .is_some_and(|mark| mark != WINDOW_DECLINED)
    {
        state
            .player(player)
            .map_or(0, |seat| i64::from(seat.commodities.max(0)))
    } else {
        0
    }
}

/// The Keleres seats that could use the agent for `payer` now: readied, `payer` has commodities,
/// and no agent window is open for them yet.
fn agent_holders(state: &GameState, payer: &PlayerId) -> Vec<PlayerId> {
    if state.faction_marks.contains_key(&agent_key(payer))
        || state.player(payer).is_none_or(|seat| seat.commodities <= 0)
    {
        return Vec::new();
    }
    state
        .players
        .iter()
        .filter(|seat| {
            is_keleres_faction(seat.faction.as_str())
                && leader_status(state, &seat.id, AGENT) == Some(LeaderStatus::Readied)
        })
        .map(|seat| seat.id.clone())
        .collect()
}

/// Whether a readied Keleres agent could still be offered to `payer`: the up-front gate of an
/// option whose trade-good cost the agent's commodities could pay.
#[must_use]
pub(crate) fn agent_could_grant(state: &GameState, payer: &PlayerId) -> bool {
    !agent_holders(state, payer).is_empty()
}

/// "At any time: You may exhaust this card to allow any player to spend commodities as if they were
/// trade goods." Offered to each Keleres holder of a readied agent at the start of a payment window
/// of `payer` (the shared seams: [`crate::production::pay`] and its variants, production windows,
/// the Technology card's paid research, Leadership's token purchase) when `payer` holds commodities.
///
/// Returns `true` if it opened the window, which the caller must then close with
/// [`close_agent_window`]: if the agent was used, `payer`'s commodities count as trade goods until
/// then; if every holder declined, the window stays open without them, so nothing is offered again
/// inside it. It returns `false`, and the caller closes nothing, when no holder could be asked or a
/// window is already open (a payment inside a larger window is the same window).
///
/// # Errors
/// [`IllegalChoice`] when a decider answers something not offered.
pub(crate) fn offer_agent(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut crate::choice::Table,
    payer: &PlayerId,
) -> Result<bool, IllegalChoice> {
    let holders = agent_holders(state, payer);
    if holders.is_empty() {
        return Ok(false);
    }
    for holder in holders {
        let choice = Choice::new(
            holder.clone(),
            format!("Xander Alexin Victori III: let {payer} spend commodities as trade goods"),
            vec![
                ChoiceOption::labelled(
                    "use",
                    "leader_ability",
                    format!("exhaust the agent: {payer}'s commodities count as trade goods"),
                ),
                ChoiceOption::decline(),
            ],
        )
        .contextualized(decision(state, &holder, AGENT, "keleres_agent_commodities"));
        let answer = table.ask_seeing(
            &choice,
            &crate::choice::Observed::new(state, content, sources, galaxy),
        )?;
        if answer.is_decline() {
            continue;
        }
        if !crate::leaders::exhaust(state, &holder, &LeaderId::new(AGENT)) {
            continue;
        }
        state
            .faction_marks
            .insert(agent_key(payer), holder.to_string());
        return Ok(true);
    }
    state
        .faction_marks
        .insert(agent_key(payer), WINDOW_DECLINED.to_owned());
    Ok(true)
}

/// The mark's value for a window whose offer was declined.
const WINDOW_DECLINED: &str = "declined";

/// End `payer`'s agent window: their commodities are not trade goods any more.
pub(crate) fn close_agent_window(state: &mut GameState, payer: &PlayerId) {
    state.faction_marks.remove(&agent_key(payer));
}

/// Run `probe` on a copy of the position in which `payer` has the agent's permission, if a Keleres
/// holder could grant it; `None` when it could not. Lets an eligibility gate that precedes the
/// payment window ("can this player afford it at all?") see what the agent would add.
pub(crate) fn with_agent_granted<T>(
    state: &GameState,
    payer: &PlayerId,
    probe: impl FnOnce(&GameState) -> T,
) -> Option<T> {
    if agent_holders(state, payer).is_empty() {
        return None;
    }
    let mut granted = state.clone();
    granted
        .faction_marks
        .insert(agent_key(payer), "probe".to_owned());
    Some(probe(&granted))
}

// -- Suffi An: unlock ----------------------------------------------------------------------------

/// Whether the played card is one with a component action (its window is "Action").
fn has_component_action(content: &ContentStore, card: &str) -> bool {
    content
        .get(ti4_model::content_types::ContentType::ActionCards, card)
        .and_then(|record| record.text("window"))
        .is_some_and(|window| window.eq_ignore_ascii_case("action"))
}

fn commander_unlock_ready(
    state: &GameState,
    content: &ContentStore,
    owner: &PlayerId,
    event: &crate::event::Event,
) -> bool {
    event.text("player") == Some(owner.as_str())
        && is_keleres(state, owner)
        && leader_status(state, owner, COMMANDER) == Some(LeaderStatus::Locked)
        && crate::supply::potential_goods(state, owner) >= 1
        && event
            .text("card")
            .is_some_and(|card| has_component_action(content, card))
}

/// Suffi An's unlock: "Spend 1 trade good after you play an action card that has a component
/// action." Offered after each such play while the commander is still locked.
fn commander_unlock(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("leader:{owner_name}:{COMMANDER}_unlock:ACTION_CARD_PLAYED:after"),
        seat.clone(),
        "ACTION_CARD_PLAYED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            if !commander_unlock_ready(context.state, context.content, &owner, event) {
                return Ok(());
            }
            // The agent may let a commodity pay the trade good (the holder may be this very seat).
            let paid = crate::supply::with_goods_window(context, &owner, 1, |context| {
                crate::supply::spend_goods(context.state, &owner, 1)
            });
            if paid == Some(true)
                && let Some(seat) = context.state.player_mut(&owner)
            {
                seat.leaders
                    .insert(LeaderId::new(COMMANDER), LeaderStatus::Unlocked);
            }
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        commander_unlock_ready(context.state, context.content, &condition_owner, event)
    }))
}

// -- Executive Order -----------------------------------------------------------------------------

fn executive_order_ready(state: &GameState, player: &PlayerId) -> bool {
    let id = TechnologyId::new(EXECUTIVE_ORDER);
    is_keleres(state, player)
        && state.player(player).is_some_and(|seat| {
            seat.technologies.contains(&id) && !seat.exhausted_technologies.contains(&id)
        })
        && !state.agenda_deck.is_empty()
}

/// Executive Order's ACTION, once for the top card and once for the bottom (a deck of one has only
/// the one). Offered while the technology is ready and the deck holds a card.
pub fn component_actions(
    state: &GameState,
    _content: &ContentStore,
    player: &PlayerId,
) -> Vec<ChoiceOption> {
    if !executive_order_ready(state, player) {
        return Vec::new();
    }
    let mut options = vec![ChoiceOption::labelled(
        format!("{OPTION_PREFIX}{EXECUTIVE_ORDER}|top"),
        "component_action",
        "Executive Order: draw the top agenda and vote on it now",
    )];
    if state.agenda_deck.len() > 1 {
        options.push(ChoiceOption::labelled(
            format!("{OPTION_PREFIX}{EXECUTIVE_ORDER}|bottom"),
            "component_action",
            "Executive Order: draw the bottom agenda and vote on it now",
        ));
    }
    options
}

/// `faction_marks` key: the agenda Executive Order just drew, as `"<owner>|<agenda>"`, until the
/// driver takes it to put it to a vote ([`take_executive_order`]).
const PENDING_KEY: &str = "keleres|executive_order_pending";

/// Perform Executive Order's cost and draw; `true` if the option was this module's and resolved.
///
/// The vote is not resolved here. The action exhausts the card and draws the top or bottom agenda;
/// `Game::apply_choice` then takes the drawn agenda ([`take_executive_order`]) and runs it through
/// the ordinary agenda machinery (reveal windows, `VoteWindow`, `VOTES_CAST`, `AGENDA_RESOLVED`,
/// predictions, the agenda's effect) with the owner as the speaker for that vote.
pub fn perform_component(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    option: &ChoiceOption,
) -> bool {
    let Some(rest) = option.id.strip_prefix(OPTION_PREFIX) else {
        return false;
    };
    let from_top = match rest.strip_prefix(EXECUTIVE_ORDER) {
        Some("|top") => true,
        Some("|bottom") => false,
        _ => return false,
    };
    if !executive_order_ready(context.state, player) {
        return false;
    }
    let drawn = if from_top {
        ti4_model::deck_reserve::take_top(
            &mut context.state.deck_reserve,
            "agenda",
            &mut context.state.agenda_deck,
            Some(player.as_str()),
        )
    } else {
        ti4_model::deck_reserve::take_bottom(
            &mut context.state.deck_reserve,
            "agenda",
            &mut context.state.agenda_deck,
            Some(player.as_str()),
        )
    };
    let Some(alias) = drawn else {
        return false;
    };
    if let Some(seat) = context.state.player_mut(player) {
        seat.exhausted_technologies
            .insert(TechnologyId::new(EXECUTIVE_ORDER));
    }
    context
        .state
        .faction_marks
        .insert(PENDING_KEY.to_owned(), format!("{player}|{alias}"));
    true
}

/// The agenda an Executive Order action just drew and who drew it, if one is waiting for its vote.
/// Taken (cleared) by the driver straight after the action resolves.
pub fn take_executive_order(state: &mut GameState) -> Option<(PlayerId, String)> {
    let pending = state.faction_marks.remove(PENDING_KEY)?;
    let (owner, alias) = pending.split_once('|')?;
    Some((PlayerId::new(owner), alias.to_owned()))
}

// -- timing registration -------------------------------------------------------------------------

fn timing_abilities(state: &GameState, owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    let mut abilities = vec![
        council_patronage(owner_name, seat),
        laws_order(owner_name, seat),
        asn_clear(owner_name, seat),
        agent_clear(owner_name, seat),
        commander_unlock(owner_name, seat),
    ];
    abilities.extend(super::keleres_units::timing_abilities(
        state, owner_name, seat,
    ));
    abilities
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use ti4_model::content_types::DEFAULT;
    use ti4_model::id::FactionId;

    use crate::choice::{Decider, Scripted, Table};
    use crate::production::Spend;

    fn a() -> PlayerId {
        PlayerId::new("a")
    }
    fn b() -> PlayerId {
        PlayerId::new("b")
    }
    fn content() -> &'static ContentStore {
        ContentStore::embedded()
    }
    fn game(variant: &str) -> GameState {
        crate::fixtures::seated_game(&[("a", variant), ("b", "sol"), ("c", "hacan")], DEFAULT)
    }
    fn scripted(answers: &[&str]) -> Table {
        Table::with_default(Box::new(Scripted::new(
            answers.iter().map(|s| (*s).to_owned()),
        )))
    }
    fn payload(pairs: &[(&str, &str)]) -> BTreeMap<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), serde_json::Value::String((*v).to_owned())))
            .collect()
    }
    /// Emit one typed event through the armed resolver; returns the prompts offered.
    fn emit(
        state: &mut GameState,
        table: &mut Table,
        event_type: &str,
        mut data: BTreeMap<String, serde_json::Value>,
    ) -> Vec<String> {
        let mut resolver = crate::fixtures::armed_resolver(state);
        crate::fixtures::with_context(state, DEFAULT, None, table, |ctx| {
            let event = ctx
                .event_sequence
                .next(event_type, std::mem::take(&mut data))
                .expect("an event id");
            resolver
                .emit_with_context(ctx, event, |_, _| {})
                .expect("the window resolves");
        });
        table
            .log
            .records
            .iter()
            .flat_map(|record| record.offered.iter().cloned())
            .collect()
    }

    // -- The Tribuni -----------------------------------------------------------------------------

    #[test]
    fn the_tribuni_needs_an_unplayed_base_faction_whatever_the_seating_order() {
        let ids = [a(), b()];
        let fresh = || crate::setup::start_game_seeded(content(), &ids, DEFAULT, None, 0).unwrap();
        for (variant, base) in [
            ("keleresm", "mentak"),
            ("keleresx", "xxcha"),
            ("keleresa", "argent"),
        ] {
            let mut state = fresh();
            crate::seating::deploy(&mut state, content(), &b(), &FactionId::new(base), DEFAULT)
                .unwrap();
            let refused = crate::seating::deploy(
                &mut state,
                content(),
                &a(),
                &FactionId::new(variant),
                DEFAULT,
            );
            assert!(matches!(
                refused,
                Err(crate::seating::SeatingError::TribuniFactionPlayed { .. })
            ));
            assert_eq!(state.player(&a()).unwrap().faction.as_str(), "generic");
            let mut state = fresh();
            crate::seating::deploy(
                &mut state,
                content(),
                &a(),
                &FactionId::new(variant),
                DEFAULT,
            )
            .unwrap();
            assert!(
                crate::seating::deploy(&mut state, content(), &b(), &FactionId::new(base), DEFAULT)
                    .is_err()
            );
            let state = crate::fixtures::seated_game(&[("a", variant), ("b", "sol")], DEFAULT);
            let seat = state.player(&a()).unwrap();
            assert!(seat.home_system.is_some());
            assert_eq!(seat.leaders.len(), 3);
        }
        let mut assignments = BTreeMap::new();
        assignments.insert(a(), FactionId::new("keleresx"));
        assignments.insert(b(), FactionId::new("xxcha"));
        assert!(matches!(
            crate::seating::build_board(content(), &assignments, &[], DEFAULT),
            Err(crate::seating::SeatingError::TribuniFactionPlayed { .. })
        ));
        assert_eq!(
            crate::seating::tribuni_variants_available(["sol", "xxcha"]),
            ["keleresm", "keleresa"]
        );
        assert!(matches!(
            crate::seating::validate_tribuni(["keleresm", "keleresa"]),
            Err(crate::seating::SeatingError::KeleresSeatedTwice)
        ));
    }

    // -- Council Patronage -----------------------------------------------------------------------

    #[test]
    fn council_patronage_replenishes_then_gains_a_trade_good_for_every_variant() {
        for variant in VARIANTS {
            let mut state = game(variant);
            let sol_before = state.player(&b()).unwrap().clone();
            let mut table = scripted(&[]);
            emit(
                &mut state,
                &mut table,
                "STRATEGY_PHASE_BEGAN",
                BTreeMap::new(),
            );
            let seat = state.player(&a()).unwrap();
            assert_eq!(seat.commodities, 2, "{variant}");
            assert_eq!(seat.trade_goods, 1, "{variant}");
            let sol = state.player(&b()).unwrap();
            assert_eq!(sol.commodities, sol_before.commodities);
            assert_eq!(sol.trade_goods, sol_before.trade_goods);
        }
    }

    // -- Law's Order -----------------------------------------------------------------------------

    #[test]
    fn laws_order_blanks_every_law_until_the_turn_ends() {
        let mut state = game("keleresx");
        state.phase = ti4_model::state::Phase::Action;
        state.turn_seq = 3;
        state.laws.insert("sanctions".to_owned(), "for".to_owned());
        state.player_mut(&a()).unwrap().trade_goods = 1;
        let id = "ability:keleresx:laws_order:TURN_BEGAN:after";
        emit(
            &mut state,
            &mut scripted(&["decline"]),
            "TURN_BEGAN",
            payload(&[("player", "b")]),
        );
        assert_eq!(crate::laws::action_card_limit(&state, 7), 3);
        emit(
            &mut state,
            &mut scripted(&[id]),
            "TURN_BEGAN",
            payload(&[("player", "b")]),
        );
        assert_eq!(crate::laws::action_card_limit(&state, 7), 7, "blank");
        assert!(crate::laws::in_play(&state).is_empty());
        assert_eq!(state.player(&a()).unwrap().trade_goods, 0, "paid");
        assert!(state.laws.contains_key("sanctions"), "not repealed");
        state.turn_seq = 4;
        assert_eq!(
            crate::laws::action_card_limit(&state, 7),
            3,
            "the turn ended"
        );
    }

    #[test]
    fn laws_order_is_not_offered_without_a_law_or_a_resource_and_never_to_others() {
        let mut state = game("keleresm");
        state.phase = ti4_model::state::Phase::Action;
        state.player_mut(&a()).unwrap().commodities = 2;
        let asked = emit(
            &mut state,
            &mut scripted(&[]),
            "TURN_BEGAN",
            payload(&[("player", "b")]),
        );
        assert!(asked.iter().all(|p| !p.contains("laws_order")), "no law");
        state.laws.insert("sanctions".to_owned(), "for".to_owned());
        let id = "ability:keleresm:laws_order:TURN_BEGAN:after";
        emit(
            &mut state,
            &mut scripted(&[id]),
            "TURN_BEGAN",
            payload(&[("player", "a")]),
        );
        assert_eq!(
            state.player(&a()).unwrap().commodities,
            1,
            "a commodity pays"
        );
        let mut plain = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        plain.phase = ti4_model::state::Phase::Action;
        plain.laws.insert("sanctions".to_owned(), "for".to_owned());
        let before = plain.clone();
        emit(
            &mut plain,
            &mut scripted(&[]),
            "TURN_BEGAN",
            payload(&[("player", "a")]),
        );
        assert!(plain == before);
    }

    // -- I.I.H.Q. Modernization ------------------------------------------------------------------

    #[test]
    fn iihq_makes_neighbours_of_everyone_near_mecatol_and_only_with_the_card() {
        let hub = crate::fixtures::hub_with_centre("18");
        let mut state = game("keleresm");
        for sys in state.board.keys().cloned().collect::<Vec<_>>() {
            let board = state.system_mut(&sys);
            board.units.clear();
            board.planet_units.clear();
            board.planet_control.clear();
        }
        crate::fixtures::put(
            &mut state,
            &SystemId::new(hub.outer[0].as_str()),
            "carrier",
            &a(),
            1,
        );
        crate::fixtures::put(
            &mut state,
            &SystemId::new(hub.outer[3].as_str()),
            "carrier",
            &b(),
            1,
        );
        assert!(!crate::transactions::are_neighbours(
            &state,
            &hub.galaxy,
            &a(),
            &b()
        ));
        state
            .player_mut(&a())
            .unwrap()
            .technologies
            .insert(TechnologyId::new(IIHQ));
        assert!(crate::transactions::are_neighbours(
            &state,
            &hub.galaxy,
            &a(),
            &b()
        ));
        assert!(
            crate::transactions::are_neighbours(&state, &hub.galaxy, &b(), &a()),
            "mutual"
        );
        state
            .system_mut(&SystemId::new(hub.outer[3].as_str()))
            .units
            .clear();
        let far = crate::fixtures::plain_systems(20).pop().unwrap();
        crate::fixtures::put(&mut state, &SystemId::new(far), "carrier", &b(), 1);
        assert!(!crate::transactions::are_neighbours(
            &state,
            &hub.galaxy,
            &a(),
            &b()
        ));
        let mut bt = game("keleresa");
        assert!(!has_iihq(&bt, &a()));
        bt.player_mut(&a()).unwrap().breakthrough =
            Some(ti4_model::id::BreakthroughId::new(BREAKTHROUGH));
        assert!(has_iihq(&bt, &a()));
    }

    #[test]
    fn custodia_vigilia_arrives_stays_and_gives_mecatol_its_cannon_and_production() {
        let mut state = game("keleresm");
        reconcile(&mut state);
        assert!(!holds_custodia(&state, &a()), "no card, no planet");
        state
            .player_mut(&a())
            .unwrap()
            .technologies
            .insert(TechnologyId::new(IIHQ));
        reconcile(&mut state);
        assert!(holds_custodia(&state, &a()));
        let home = state.player(&a()).unwrap().home_system.clone().unwrap();
        state
            .system_mut(&home)
            .set_control(PlanetId::new(CUSTODIA), b());
        reconcile(&mut state);
        assert!(holds_custodia(&state, &a()), "cannot be lost");
        let mecatol = SystemId::new("18");
        let mr = PlanetId::new("mr");
        assert!(crate::planets::attachment_cannons(&state, content(), &mecatol, &mr).is_empty());
        assert_eq!(
            crate::production::capacity(&state, content(), DEFAULT, &a(), &mecatol),
            0
        );
        state.system_mut(&mecatol).set_control(mr.clone(), a());
        assert_eq!(
            crate::planets::attachment_cannons(&state, content(), &mecatol, &mr),
            vec![(a(), 5, 1)]
        );
        assert_eq!(
            crate::production::capacity(&state, content(), DEFAULT, &a(), &mecatol),
            3
        );
        state.system_mut(&mecatol).set_control(mr.clone(), b());
        assert!(
            crate::planets::attachment_cannons(&state, content(), &mecatol, &mr)
                .iter()
                .all(|(owner, ..)| *owner != a())
        );
        assert_eq!(
            crate::production::capacity(&state, content(), DEFAULT, &b(), &mecatol),
            0
        );
    }

    #[test]
    fn custodian_favour_pays_two_command_tokens_when_another_player_scores_imperial() {
        let mut state = game("keleresm");
        state
            .player_mut(&a())
            .unwrap()
            .technologies
            .insert(TechnologyId::new(IIHQ));
        reconcile(&mut state);
        let total = |s: &GameState| {
            let seat = s.player(&a()).unwrap();
            seat.tactic_tokens + seat.fleet_tokens + seat.strategic_tokens
        };
        let tokens = total(&state);
        let mut table = scripted(&[]);
        custodians_favour_tokens(&mut state, content(), DEFAULT, None, &mut table, &b()).unwrap();
        assert_eq!(total(&state), tokens + 2);
        let mut table = scripted(&[]);
        custodians_favour_tokens(&mut state, content(), DEFAULT, None, &mut table, &a()).unwrap();
        assert_eq!(total(&state), tokens + 2, "not for the Keleres' own point");
    }

    // -- Agent and commander ---------------------------------------------------------------------

    /// A table where the Keleres seat `a` answers `agent` to the agent's offer and `b` answers
    /// `payer` (the paying seat's script).
    fn agent_table(agent: &[&str], payer: &[&str]) -> Table {
        let mut table = Table::default();
        table.seat(a(), Box::new(Scripted::new(agent.iter().copied())));
        table.seat(b(), Box::new(Scripted::new(payer.iter().copied())));
        table
    }

    fn offers_to_keleres(table: &Table) -> usize {
        table
            .log
            .records
            .iter()
            .filter(|record| record.prompt.starts_with("Xander Alexin Victori III"))
            .count()
    }

    /// Every planet `b` controls, exhausted: nothing to pay with but goods.
    fn exhaust_planets_of_b(state: &mut GameState) {
        let planets: Vec<_> = state
            .controlled_planets(&b())
            .into_iter()
            .map(|(_, planet)| planet.clone())
            .collect();
        for planet in planets {
            state.exhaust_planet(planet);
        }
    }

    fn card_id(name: &str) -> String {
        content()
            .records(ti4_model::content_types::ContentType::StrategyCards)
            .iter()
            .find(|record| {
                record.text("name") == Some(name)
                    && record.text("id").is_some_and(|id| id.starts_with("pok"))
            })
            .and_then(|record| record.text("id"))
            .unwrap_or_else(|| panic!("missing {name}"))
            .to_owned()
    }

    #[test]
    fn the_agent_is_offered_at_a_payment_and_its_commodities_pay_for_that_payment_only() {
        let mut state = game("keleresx");
        state.player_mut(&b()).unwrap().commodities = 3;
        assert_eq!(spendable_commodities(&state, &b()), 0);
        let mut table = agent_table(&["use"], &["commodity", "commodity"]);
        let paid = crate::production::pay(
            &mut state,
            content(),
            DEFAULT,
            &mut table,
            &b(),
            2,
            Spend::Resources,
        )
        .unwrap();
        assert!(paid);
        assert_eq!(offers_to_keleres(&table), 1);
        assert_eq!(state.player(&b()).unwrap().commodities, 1, "two were spent");
        assert_eq!(
            leader_status(&state, &a(), AGENT),
            Some(LeaderStatus::Exhausted)
        );
        assert_eq!(
            spendable_commodities(&state, &b()),
            0,
            "the window closed with the payment"
        );
        // A second payment: the agent is exhausted, nothing is offered, commodities are no goods.
        let mut table = agent_table(&[], &[]);
        let before =
            crate::production::available(&state, content(), DEFAULT, &b(), Spend::Resources);
        state.player_mut(&b()).unwrap().commodities = 0;
        let after =
            crate::production::available(&state, content(), DEFAULT, &b(), Spend::Resources);
        assert_eq!(before, after, "commodities are not goods outside a window");
        crate::production::pay(
            &mut state,
            content(),
            DEFAULT,
            &mut table,
            &b(),
            1,
            Spend::Resources,
        )
        .unwrap();
        assert_eq!(offers_to_keleres(&table), 0);
    }

    #[test]
    fn the_agent_is_offered_when_a_production_window_opens() {
        let mut state = game("keleresm");
        let (system, planet) = crate::fixtures::a_placed_planet();
        state.system_mut(&system).set_control(planet.clone(), b());
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "spacedock", &b(), 1);
        exhaust_planets_of_b(&mut state);
        let seat = state.player_mut(&b()).unwrap();
        seat.trade_goods = 0;
        seat.commodities = 1;
        let destroyers = |s: &GameState| {
            s.system_state(&system)
                .units
                .iter()
                .filter(|u| u.owner == b() && u.type_id.as_str() == "destroyer")
                .count()
        };
        let mut built = false;
        let mut table = Table::default();
        table.seat(a(), Box::new(Scripted::new(["use"])));
        table.seat(
            b(),
            Box::new(Pick(move |choice: &crate::choice::Choice| {
                if !built
                    && let Some(option) = choice
                        .options
                        .iter()
                        .find(|o| o.id.starts_with("build|destroyer"))
                {
                    built = true;
                    return Some(option.clone());
                }
                if let Some(option) = choice.option("commodity") {
                    return Some(option.clone());
                }
                choice.options.iter().find(|o| o.is_decline()).cloned()
            })),
        );
        let report = crate::production::resolve(
            &mut state,
            content(),
            DEFAULT,
            None,
            &mut table,
            &b(),
            &system,
        );
        assert!(report.is_ok(), "{report:?}");
        assert_eq!(offers_to_keleres(&table), 1, "once as the window opened");
        assert_eq!(destroyers(&state), 1, "paid for with the commodity");
        assert_eq!(state.player(&b()).unwrap().commodities, 0);
        assert_eq!(spendable_commodities(&state, &b()), 0, "window closed");
    }

    #[test]
    fn declining_the_agent_keeps_commodities_out_of_the_payment() {
        let mut state = game("keleresm");
        state.player_mut(&b()).unwrap().commodities = 3;
        let mut table = agent_table(&["decline"], &[]);
        crate::production::pay(
            &mut state,
            content(),
            DEFAULT,
            &mut table,
            &b(),
            1,
            Spend::Resources,
        )
        .unwrap();
        assert_eq!(offers_to_keleres(&table), 1);
        assert_eq!(state.player(&b()).unwrap().commodities, 3);
        assert_eq!(
            leader_status(&state, &a(), AGENT),
            Some(LeaderStatus::Readied)
        );
        assert!(
            table
                .log
                .records
                .iter()
                .filter(|record| record.player == b())
                .all(|record| record.chosen != "commodity")
        );
    }

    #[test]
    fn the_agent_opens_a_research_that_only_its_commodities_can_pay_for() {
        let research = |agent_answer: &str| {
            let mut state = game("keleresx");
            exhaust_planets_of_b(&mut state);
            let seat = state.player_mut(&b()).unwrap();
            seat.trade_goods = 0;
            seat.commodities =
                i32::try_from(crate::strategy_cards::TECHNOLOGY_SECONDARY_COST).unwrap();
            let owned = seat.technologies.len();
            let mut table = Table::default();
            table.seat(a(), Box::new(Scripted::new([agent_answer])));
            // b takes the first technology offered.
            crate::strategy_cards::secondary(
                &mut state,
                content(),
                DEFAULT,
                None,
                &mut table,
                &b(),
                &card_id("Technology"),
            )
            .unwrap();
            let seat = state.player(&b()).unwrap();
            let gained = seat.technologies.len() - owned;
            (state.clone(), table, gained)
        };
        let (state, table, gained) = research("use");
        assert_eq!(offers_to_keleres(&table), 1);
        assert_eq!(gained, 1, "researched with commodities");
        assert_eq!(state.player(&b()).unwrap().commodities, 0);
        assert_eq!(
            leader_status(&state, &a(), AGENT),
            Some(LeaderStatus::Exhausted)
        );
        assert_eq!(spendable_commodities(&state, &b()), 0, "window closed");
        let (state, table, gained) = research("decline");
        assert_eq!(offers_to_keleres(&table), 1);
        assert_eq!(gained, 0, "declined: 4 commodities pay nothing");
        assert_eq!(
            state.player(&b()).unwrap().commodities,
            i32::try_from(crate::strategy_cards::TECHNOLOGY_SECONDARY_COST).unwrap()
        );
    }

    #[test]
    fn the_agent_opens_a_leadership_token_purchase_for_its_commodities() {
        let leadership = |agent_answer: &str| {
            let mut state = game("keleresa");
            exhaust_planets_of_b(&mut state);
            let seat = state.player_mut(&b()).unwrap();
            seat.trade_goods = 0;
            seat.commodities = 3;
            let tokens = |s: &GameState| {
                let seat = s.player(&b()).unwrap();
                seat.tactic_tokens + seat.fleet_tokens + seat.strategic_tokens
            };
            let before = tokens(&state);
            let mut table = Table::default();
            table.seat(a(), Box::new(Scripted::new([agent_answer])));
            assert_eq!(
                crate::strategy_cards::leadership_influence_eligible(
                    &state,
                    content(),
                    DEFAULT,
                    &b()
                ),
                true,
                "the follower window opens: the agent could pay"
            );
            crate::strategy_cards::secondary(
                &mut state,
                content(),
                DEFAULT,
                None,
                &mut table,
                &b(),
                &card_id("Leadership"),
            )
            .unwrap();
            let gained = tokens(&state) - before;
            (state.clone(), table, gained)
        };
        let (state, table, gained) = leadership("use");
        assert_eq!(offers_to_keleres(&table), 1, "once for the whole purchase");
        assert_eq!(gained, 1, "three commodities, one token");
        assert_eq!(state.player(&b()).unwrap().commodities, 0);
        assert_eq!(spendable_commodities(&state, &b()), 0, "window closed");
        let (state, _, gained) = leadership("decline");
        assert_eq!(gained, 0);
        assert_eq!(state.player(&b()).unwrap().commodities, 3);
    }

    #[test]
    fn the_agent_is_neutral_without_a_ready_agent_or_commodities_or_keleres() {
        let research_offers = |state: &mut GameState| {
            let mut table = Table::default();
            let _ = crate::production::pay(
                state,
                content(),
                DEFAULT,
                &mut table,
                &b(),
                1,
                Spend::Resources,
            );
            let _ = crate::strategy_cards::secondary(
                state,
                content(),
                DEFAULT,
                None,
                &mut table,
                &b(),
                &card_id("Technology"),
            );
            offers_to_keleres(&table)
        };
        // Exhausted agent.
        let mut state = game("keleresm");
        state.player_mut(&b()).unwrap().commodities = 3;
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new(AGENT), LeaderStatus::Exhausted);
        assert_eq!(research_offers(&mut state), 0);
        // No commodities to turn into goods.
        let mut state = game("keleresm");
        state.player_mut(&b()).unwrap().commodities = 0;
        assert_eq!(research_offers(&mut state), 0);
        // No Keleres at the table: identical positions, identical payments.
        let plain = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        let mut with_commodities = plain.clone();
        with_commodities.player_mut(&b()).unwrap().commodities = 3;
        assert_eq!(
            crate::production::available(&plain, content(), DEFAULT, &b(), Spend::Resources),
            crate::production::available(
                &with_commodities,
                content(),
                DEFAULT,
                &b(),
                Spend::Resources
            ) - 0,
            "commodities are never goods without the agent"
        );
        assert_eq!(research_offers(&mut with_commodities), 0);
        assert!(with_agent_granted(&with_commodities, &b(), |_| ()).is_none());
    }

    #[test]
    fn the_commander_unlocks_by_spending_a_trade_good_after_a_component_action_card() {
        let mut state = game("keleresm");
        state.player_mut(&a()).unwrap().trade_goods = 1;
        let id = "leader:keleresm:kelerescommander_unlock:ACTION_CARD_PLAYED:after";
        let asked = emit(
            &mut state,
            &mut scripted(&[]),
            "ACTION_CARD_PLAYED",
            payload(&[("player", "a"), ("card", "mb1")]),
        );
        assert!(asked.iter().all(|p| !p.contains("kelerescommander")));
        assert_eq!(
            leader_status(&state, &a(), COMMANDER),
            Some(LeaderStatus::Locked)
        );
        emit(
            &mut state,
            &mut scripted(&[id]),
            "ACTION_CARD_PLAYED",
            payload(&[("player", "a"), ("card", "mining_initiative")]),
        );
        assert_eq!(
            leader_status(&state, &a(), COMMANDER),
            Some(LeaderStatus::Unlocked)
        );
        assert_eq!(state.player(&a()).unwrap().trade_goods, 0);
    }

    // -- Executive Order -------------------------------------------------------------------------

    const EO_TOP: &str = "faction|keleres|executiveorder|top";
    const RESOURCES_FACE: &str = "|resources";

    /// A Keleres action phase: `a` (Keleres, the active seat), `b` (Sol), `c` (Hacan), `speaker`
    /// the real speaker, `sanctions` (a For/Against law) on top of the agenda deck, `mutiny` below.
    fn eo_game(speaker: &str) -> GameState {
        let mut state = game("keleresm");
        state.phase = ti4_model::state::Phase::Action;
        state.active = Some(a());
        state.speaker = PlayerId::new(speaker);
        state
            .player_mut(&a())
            .unwrap()
            .technologies
            .insert(TechnologyId::new(EXECUTIVE_ORDER));
        state.agenda_deck = vec!["sanctions".to_owned(), "mutiny".to_owned()];
        state
    }

    /// One seat's answers through an Executive Order vote.
    #[derive(Clone, Default)]
    struct Voter {
        vote: Option<&'static str>,
        /// The planet option to exhaust first, if offered.
        planet: Option<String>,
        goods: i32,
        tiebreak: &'static str,
        /// Play any card offered in an `AGENDA_REVEALED` window.
        react: bool,
    }

    fn voter(plan: Voter) -> Box<dyn Decider> {
        let mut planet_taken = false;
        Box::new(Pick(move |choice: &crate::choice::Choice| {
            let has = |kind: &str| choice.options.iter().any(|o| o.kind == kind);
            let decline = || choice.options.iter().find(|o| o.is_decline()).cloned();
            if let Some(option) = choice.option(EO_TOP) {
                return Some(option.clone());
            }
            if plan.react
                && let Some(option) = choice
                    .options
                    .iter()
                    .find(|o| o.id.contains("AGENDA_REVEALED"))
            {
                return Some(option.clone());
            }
            if has(crate::vote::VOTE_KIND) {
                return plan
                    .vote
                    .and_then(|vote| choice.option(vote).cloned())
                    .or_else(decline);
            }
            if has(crate::vote::VOTE_PLANET_KIND) {
                if !planet_taken
                    && let Some(option) = plan.planet.as_deref().and_then(|id| choice.option(id))
                {
                    planet_taken = true;
                    return Some(option.clone());
                }
                return decline();
            }
            if has(crate::vote::VOTE_TRADE_GOODS_KIND) {
                return choice
                    .option(&format!("spend|{}", plan.goods))
                    .cloned()
                    .or_else(decline);
            }
            if has(crate::vote::TIEBREAK_KIND) {
                return choice.option(plan.tiebreak).cloned();
            }
            None
        }))
    }

    fn eo_table(seats: [(&PlayerId, Box<dyn Decider>); 3]) -> Table {
        let mut table = Table::default();
        for (player, decider) in seats {
            table.seat(player.clone(), decider);
        }
        table
    }

    /// Drive the game until Executive Order's action resolves, with every step clean.
    fn run_eo(game: &mut crate::game::Game<'_>) {
        for _ in 0..200 {
            let step = game.step();
            assert_eq!(step.error, None, "log {:?}", game.events);
            if game.events.iter().any(|e| e == "COMPONENT_ACTION_RESOLVED") {
                return;
            }
        }
        panic!("Executive Order never resolved: {:?}", game.events);
    }

    fn position(events: &[String], wanted: &str) -> usize {
        events
            .iter()
            .position(|event| event == wanted)
            .unwrap_or_else(|| panic!("no {wanted} in {events:?}"))
    }

    fn vote_order(table: &Table) -> Vec<String> {
        table
            .log
            .records
            .iter()
            .filter(|record| record.prompt == "vote for which outcome")
            .map(|record| record.player.to_string())
            .collect()
    }

    #[test]
    fn executive_order_runs_through_the_agenda_flow_and_enacts_the_law() {
        let mut state = eo_game("c");
        state.player_mut(&a()).unwrap().trade_goods = 3;
        // b holds Hack Election: "After an agenda is revealed ... you vote last".
        state.player_mut(&b()).unwrap().action_cards =
            vec![ti4_model::id::ActionCardId::new("hack")];
        let table = eo_table([
            (
                &a(),
                voter(Voter {
                    vote: Some("for"),
                    goods: 2,
                    ..Voter::default()
                }),
            ),
            (
                &b(),
                voter(Voter {
                    react: true,
                    ..Voter::default()
                }),
            ),
            (&PlayerId::new("c"), voter(Voter::default())),
        ]);
        let mut game = crate::game::Game::with_table(state, content(), table);
        run_eo(&mut game);

        let events = &game.events;
        assert!(events.iter().any(|e| e == "AGENDA_REVEALED:sanctions"));
        assert!(
            game.table
                .log
                .records
                .iter()
                .any(|record| record.player == b()
                    && record
                        .offered
                        .iter()
                        .any(|id| id.contains("AGENDA_REVEALED"))),
            "the reveal window was offered to the holder of a card for it"
        );
        assert!(
            game.state.player(&b()).unwrap().action_cards.is_empty(),
            "Hack Election was played in it"
        );
        assert!(
            events.iter().any(|e| e.starts_with("VOTES_CAST")),
            "{events:?}"
        );
        let resolved = position(events, "AGENDA_RESOLVED:sanctions:for");
        let enacted = position(events, "LAW_ENACTED:sanctions:for");
        assert!(resolved < enacted);
        assert!(
            enacted < position(events, "COMPONENT_ACTION_RESOLVED"),
            "the action resolves after its vote"
        );
        assert_eq!(
            game.state.laws.get("sanctions").map(String::as_str),
            Some("for")
        );
        // Speaker a votes after b is moved last by Hack Election: c, a, b; the real speaker (c)
        // would have voted a, c, b.
        assert_eq!(vote_order(&game.table), ["c", "a", "b"]);
        let seat = game.state.player(&a()).unwrap();
        assert_eq!(seat.trade_goods, 1, "two goods were two votes");
        assert!(
            seat.exhausted_technologies
                .contains(&TechnologyId::new(EXECUTIVE_ORDER))
        );
        assert_eq!(game.state.agenda_deck, ["mutiny"], "the top card was drawn");
        assert_eq!(
            game.state.speaker,
            PlayerId::new("c"),
            "speakership restored"
        );
        assert!(game.state.agenda_choices.is_empty());
        assert!(game.state.agenda_votes.is_empty());
    }

    #[test]
    fn executive_order_votes_in_speaker_order_and_the_owner_breaks_the_tie() {
        let mut state = eo_game("c");
        let planet = crate::vote::votable_planets(&state, content(), DEFAULT, &b())
            .into_iter()
            .next()
            .expect("Sol has a planet with influence");
        let influence = crate::vote::influence_of(&state, content(), DEFAULT, &planet);
        state.player_mut(&a()).unwrap().trade_goods = i32::try_from(influence).unwrap();
        let table = eo_table([
            (
                &a(),
                voter(Voter {
                    vote: Some("for"),
                    goods: i32::try_from(influence).unwrap(),
                    tiebreak: "against",
                    ..Voter::default()
                }),
            ),
            (
                &b(),
                voter(Voter {
                    vote: Some("against"),
                    planet: Some(planet.to_string()),
                    ..Voter::default()
                }),
            ),
            (&PlayerId::new("c"), voter(Voter::default())),
        ]);
        let mut game = crate::game::Game::with_table(state, content(), table);
        run_eo(&mut game);

        // The owner is the speaker for the vote: last, though c really holds the speakership.
        assert_eq!(vote_order(&game.table), ["b", "c", "a"]);
        assert!(game.state.exhausted_planets.contains(&planet));
        let tie = game
            .table
            .log
            .records
            .iter()
            .find(|record| record.prompt == "speaker breaks the tie")
            .expect("the votes tied: one trade good per vote");
        assert_eq!(tie.player, a(), "the Keleres player breaks the tie");
        assert_eq!(tie.chosen, "against");
        assert!(
            game.events
                .iter()
                .any(|e| e == "AGENDA_RESOLVED:sanctions:against")
        );
        assert!(game.state.laws.is_empty());
        assert_eq!(game.state.speaker, PlayerId::new("c"));
    }

    #[test]
    fn executive_order_may_exhaust_planets_for_resources_only_for_its_owner() {
        let mut state = eo_game("b");
        let planet = state.player(&a()).unwrap().home_planets[0].clone();
        let face = format!("{planet}{RESOURCES_FACE}");
        let b_planet = crate::vote::votable_planets(&state, content(), DEFAULT, &b())
            .into_iter()
            .next()
            .expect("Sol has a planet with influence")
            .to_string();
        state.player_mut(&a()).unwrap().trade_goods = 0;
        let table = eo_table([
            (
                &a(),
                voter(Voter {
                    vote: Some("for"),
                    planet: Some(face.clone()),
                    ..Voter::default()
                }),
            ),
            (
                &b(),
                voter(Voter {
                    vote: Some("against"),
                    planet: Some(b_planet),
                    ..Voter::default()
                }),
            ),
            (&PlayerId::new("c"), voter(Voter::default())),
        ]);
        let mut game = crate::game::Game::with_table(state, content(), table);
        run_eo(&mut game);

        let planet_offers = |who: &PlayerId| -> Vec<String> {
            game.table
                .log
                .records
                .iter()
                .filter(|record| {
                    record.player == *who && record.prompt.starts_with("exhaust a planet")
                })
                .flat_map(|record| record.offered.clone())
                .collect()
        };
        assert!(
            planet_offers(&a()).contains(&face),
            "the owner may exhaust {planet} for resources: {:?}",
            planet_offers(&a())
        );
        assert!(
            planet_offers(&b())
                .iter()
                .all(|id| !id.ends_with(RESOURCES_FACE)),
            "nobody else may"
        );
        assert!(game.state.exhausted_planets.contains(&planet));
        assert_eq!(
            vote_order(&game.table),
            ["b", "c", "a"],
            "a is the speaker for the vote and votes last; the real speaker b would have voted last"
        );
    }

    #[test]
    fn an_illegal_executive_order_vote_answer_changes_nothing_and_can_be_retried() {
        let state = eo_game("c");
        let mut table = Table::default();
        table.seat(a(), voter(Voter::default()));
        // b votes first and answers something that was not offered, once.
        table.seat(b(), Box::new(Scripted::new(["not-an-outcome"])));
        let mut game = crate::game::Game::with_table(state, content(), table);
        // Step one: the action itself, up to the first vote question.
        assert_eq!(game.step().error, None);
        let before = game.state.clone();
        let question = game.legal_options().expect("b is asked to vote");
        assert_eq!(question.player, b());
        let refused = game.step();
        assert!(refused.error.is_some(), "an unoffered answer is refused");
        assert!(game.state == before, "nothing changed");
        assert_eq!(
            game.legal_options().map(|choice| choice.prompt),
            Some(question.prompt),
            "the same question is still owed"
        );
        run_eo(&mut game);
        assert!(game.events.iter().any(|e| e == "COMPONENT_ACTION_RESOLVED"));
    }

    #[test]
    fn executive_order_is_not_offered_without_the_card_or_an_agenda() {
        let mut state = eo_game("a");
        state.agenda_deck.clear();
        assert!(crate::factions::component_actions(&state, content(), &a()).is_empty());
        let plain = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        assert!(crate::factions::component_actions(&plain, content(), &a()).is_empty());
    }

    // -- Agency Supply Network -------------------------------------------------------------------

    struct Pick<F>(F);
    impl<F: FnMut(&crate::choice::Choice) -> Option<ChoiceOption>> Decider for Pick<F> {
        fn choose(
            &mut self,
            choice: &crate::choice::Choice,
        ) -> Result<ChoiceOption, IllegalChoice> {
            (self.0)(choice)
                .or_else(|| choice.options.first().cloned())
                .ok_or_else(|| IllegalChoice::NoOptions {
                    player: choice.player.clone(),
                    prompt: choice.prompt.clone(),
                })
        }
    }

    #[test]
    fn asn_resolves_a_second_systems_production_once_per_action() {
        let mut state = game("keleresm");
        state
            .player_mut(&a())
            .unwrap()
            .technologies
            .insert(TechnologyId::new(ASN));
        state.player_mut(&a()).unwrap().trade_goods = 20;
        let home = state.player(&a()).unwrap().home_system.clone().unwrap();
        let (other, planet) = crate::fixtures::a_placed_planet();
        state.system_mut(&other).set_control(planet.clone(), a());
        crate::fixtures::put_on_planet(&mut state, &other, &planet, "spacedock", &a(), 1);
        let ships = |s: &GameState, sys: &SystemId| {
            s.system_state(sys)
                .units
                .iter()
                .filter(|u| u.owner == a() && u.type_id.as_str() == "destroyer")
                .count()
        };
        let home_before = ships(&state, &home);
        let target = other.to_string();
        let mut built = false;
        let mut table = Table::default();
        table.seat(
            a(),
            Box::new(Pick(move |choice: &crate::choice::Choice| {
                if choice.prompt.starts_with("Agency Supply Network") {
                    return choice.option(&target).cloned();
                }
                if !built
                    && let Some(o) = choice
                        .options
                        .iter()
                        .find(|o| o.id.starts_with("build|destroyer"))
                {
                    built = true;
                    return Some(o.clone());
                }
                if let Some(o) = choice.option("trade_good") {
                    return Some(o.clone());
                }
                choice.options.iter().find(|o| o.is_decline()).cloned()
            })),
        );
        let report = crate::production::resolve(
            &mut state,
            content(),
            DEFAULT,
            None,
            &mut table,
            &a(),
            &home,
        );
        assert!(report.is_ok(), "{report:?}");
        assert_eq!(ships(&state, &other), 1, "built where ASN pointed");
        assert_eq!(ships(&state, &home), home_before);
        assert!(!asn_ready(&state, &a()), "once per action");
        emit(
            &mut state,
            &mut scripted(&[]),
            "ACTION_COMPLETED",
            payload(&[("player", "a")]),
        );
        assert!(asn_ready(&state, &a()));
    }

    // -- The agent at fixed trade-good spends ----------------------------------------------------

    /// `b` (Sol, 3 commodities, no trade goods) plays Bribery; the Keleres `a` answers the agent
    /// offer with `agent_answer`, then `b` spends `spend` goods.
    fn bribery_with(agent_answer: &str, spend: &str) -> (GameState, Table) {
        let mut state = game("keleresm");
        let seat = state.player_mut(&b()).unwrap();
        seat.trade_goods = 0;
        seat.commodities = 3;
        let mut table = Table::default();
        table.seat(a(), Box::new(Scripted::new([agent_answer])));
        table.seat(b(), Box::new(Scripted::new([spend])));
        let effect =
            crate::action_cards::effect_for(&ti4_model::id::ActionCardId::new("bribery")).unwrap();
        crate::fixtures::with_context(&mut state, DEFAULT, None, &mut table, |context| {
            effect(context, &b());
        });
        (state, table)
    }

    #[test]
    fn bribery_is_paid_with_commodities_through_the_agent() {
        let (state, table) = bribery_with("use", "2");
        assert_eq!(offers_to_keleres(&table), 1);
        let seat = state.player(&b()).unwrap();
        assert_eq!((seat.trade_goods, seat.commodities), (0, 1), "spent first");
        assert_eq!(crate::vote::extra_votes(&state, &b()), 2);
        assert_eq!(
            leader_status(&state, &a(), AGENT),
            Some(LeaderStatus::Exhausted)
        );
        assert_eq!(spendable_commodities(&state, &b()), 0, "window closed");
        assert!(
            !state
                .faction_marks
                .keys()
                .any(|key| key.starts_with(AGENT_KEY_PREFIX))
        );
    }

    #[test]
    fn declining_the_agent_leaves_a_fixed_spend_unchanged() {
        let (state, table) = bribery_with("decline", "2");
        assert_eq!(offers_to_keleres(&table), 1, "asked once, not again");
        let seat = state.player(&b()).unwrap();
        assert_eq!((seat.trade_goods, seat.commodities), (0, 3));
        assert_eq!(crate::vote::extra_votes(&state, &b()), 0);
        assert_eq!(
            leader_status(&state, &a(), AGENT),
            Some(LeaderStatus::Readied)
        );
        assert!(
            !state
                .faction_marks
                .keys()
                .any(|key| key.starts_with(AGENT_KEY_PREFIX))
        );
    }

    #[test]
    fn a_fixed_spend_without_keleres_is_never_offered_the_agent() {
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        let seat = state.player_mut(&b()).unwrap();
        seat.trade_goods = 0;
        seat.commodities = 3;
        let before = state.clone();
        let mut table = Table::default();
        let effect =
            crate::action_cards::effect_for(&ti4_model::id::ActionCardId::new("bribery")).unwrap();
        crate::fixtures::with_context(&mut state, DEFAULT, None, &mut table, |context| {
            effect(context, &b());
        });
        assert!(table.log.records.is_empty(), "nothing was asked");
        assert!(state == before);
        // The helper itself: plain goods only.
        assert_eq!(crate::supply::potential_goods(&state, &b()), 0);
        assert!(!crate::supply::spend_goods(&mut state, &b(), 1));
        assert!(state == before);
    }

    #[test]
    fn a_fixed_spend_only_reaches_what_the_window_allows_and_is_atomic() {
        let mut state = game("keleresm");
        let seat = state.player_mut(&b()).unwrap();
        seat.trade_goods = 1;
        seat.commodities = 2;
        assert_eq!(crate::supply::spendable_goods(&state, &b()), 1);
        assert_eq!(crate::supply::potential_goods(&state, &b()), 3);
        let before = state.clone();
        assert!(
            !crate::supply::spend_goods(&mut state, &b(), 2),
            "no window"
        );
        assert!(state == before);
        state.faction_marks.insert(agent_key(&b()), "a".to_owned());
        assert!(!crate::supply::spend_goods(&mut state, &b(), 4), "too dear");
        assert!(crate::supply::spend_goods(&mut state, &b(), 3));
        let seat = state.player(&b()).unwrap();
        assert_eq!((seat.trade_goods, seat.commodities), (0, 0));
    }

    /// An Executive Order vote where `a` (Keleres, the spender) holds `commodities` and no trade
    /// goods and answers the agent offer with `agent_answer`; returns the finished game.
    fn eo_vote_with_commodities(
        agent_answer: &'static str,
        spent: i32,
    ) -> crate::game::Game<'static> {
        let mut state = eo_game("c");
        let seat = state.player_mut(&a()).unwrap();
        seat.trade_goods = 0;
        seat.commodities = 2;
        let mut inner = voter(Voter {
            vote: Some("for"),
            goods: spent,
            ..Voter::default()
        });
        let a_seat: Box<dyn Decider> = Box::new(Pick(move |choice: &crate::choice::Choice| {
            if choice.prompt.starts_with("Xander Alexin Victori III") {
                return choice.option(agent_answer).cloned();
            }
            inner.choose(choice).ok()
        }));
        let table = eo_table([
            (&a(), a_seat),
            (&b(), voter(Voter::default())),
            (&PlayerId::new("c"), voter(Voter::default())),
        ]);
        let mut game = crate::game::Game::with_table(state, content(), table);
        run_eo(&mut game);
        game
    }

    #[test]
    fn a_vote_with_trade_goods_is_paid_by_commodities_through_the_agent() {
        let game = eo_vote_with_commodities("use", 2);
        let seat = game.state.player(&a()).unwrap();
        assert_eq!((seat.trade_goods, seat.commodities), (0, 0));
        assert_eq!(
            leader_status(&game.state, &a(), AGENT),
            Some(LeaderStatus::Exhausted)
        );
        assert!(
            game.events
                .iter()
                .any(|e| e == "AGENDA_RESOLVED:sanctions:for"),
            "two votes for, nobody against: {:?}",
            game.events
        );
        assert_eq!(spendable_commodities(&game.state, &a()), 0, "window closed");
    }

    #[test]
    fn declining_the_agent_leaves_the_vote_without_a_trade_good_stage() {
        let game = eo_vote_with_commodities("decline", 2);
        let seat = game.state.player(&a()).unwrap();
        assert_eq!((seat.trade_goods, seat.commodities), (0, 2));
        assert_eq!(
            leader_status(&game.state, &a(), AGENT),
            Some(LeaderStatus::Readied)
        );
        assert!(
            game.table
                .log
                .records
                .iter()
                .all(|record| !record.prompt.starts_with("spend trade goods for votes")),
            "nothing to spend, so nothing asked"
        );
        assert_eq!(offers_to_keleres(&game.table), 1);
    }

    // -- Neutrality ------------------------------------------------------------------------------

    #[test]
    fn a_table_without_keleres_sees_nothing_of_it() {
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        state.laws.insert("sanctions".to_owned(), "for".to_owned());
        let before = state.clone();
        for (event, who) in [
            ("STRATEGY_PHASE_BEGAN", vec![]),
            ("TURN_BEGAN", vec![("player", "a")]),
            ("PRODUCTION_USED", vec![("player", "a"), ("system", "18")]),
            (
                "ACTION_CARD_PLAYED",
                vec![("player", "a"), ("card", "mining_initiative")],
            ),
            ("ACTION_COMPLETED", vec![("player", "a")]),
        ] {
            let asked = emit(&mut state, &mut scripted(&[]), event, payload(&who));
            assert!(
                asked.iter().all(|p| !p.contains("keleres")),
                "{event}: {asked:?}"
            );
        }
        assert!(state == before);
        reconcile(&mut state);
        assert!(state == before);
        assert!(!crate::laws::blanked(&state));
        assert!(crate::factions::component_actions(&state, content(), &a()).is_empty());
    }
}
