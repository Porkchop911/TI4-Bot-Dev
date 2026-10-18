//! The rule bot: the random bot (`ti4_server::session::random_bot`) with an opinion on the board.
//!
//! # Plan
//!
//! A `smart` seat is played through `ti4_host::bots::choose`, on both hosts. For
//! every question it first draws the random bot's pick (so the RNG stream matches), then asks
//! [`pick`] here. `pick` returns `Some(index)` only for the questions it has a simple opinion on,
//! and `None` for everything else, which keeps the random pick. An index is always one of the
//! offered options, so the bot can never answer illegally or stall a game the random bot would
//! finish. It reads only the seat's [`SeatObservation`] (public board plus its own hand), and is
//! deterministic: the same position and seed give the same answer. The stream of a decision is
//! seeded by the game seed, the place of the seat in the seat order and the number of the
//! decision ([`random::decision_rng`]), never by the id of the seat, so a table of a seed is the
//! same game on the server and in the page.
//!
//! The opinions, deliberately few:
//!
//! - **Score** an objective whenever the engine offers one.
//! - **Action phase**: take a tactical action when some system is worth activating (below),
//!   always when a decent planet is there for the taking, otherwise sometimes the strategic
//!   action instead so the card still gets played. With nothing worth a
//!   command token, play the card or pass.
//! - **Activation**: value each system as the planets it can take (resources + influence, Mecatol
//!   Rex extra while the custodians point is there to win), counting only the planets its
//!   reachable carriers can bring enough ground forces to; a hostile fleet must be beaten clearly
//!   (fleet-strength matchup, [`ti4_engine::fleet_strength`]) or the system is avoided. A system
//!   with its space dock is worth activating to build when the seat is short of units it could
//!   build there with plenty to spend ([`PLENTY_RESOURCES`]), or to fetch an empty transport for
//!   the ground forces waiting there.
//! - **Movement**: into a fight, move the strongest ships until the matchup is clearly won; move a
//!   carrier while the target's planets still need ground forces (to a dock: an empty one for
//!   the ground forces waiting there); then stop. Never move into a
//!   fight it cannot win. **Loading**: ground forces while planets need them, fighters for a fight.
//! - **Landing**: commit ground forces to the planet still short of a winning force with the best
//!   worth per troop (so every free planet gets one), never to a planet it cannot outnumber;
//!   stop when every planet is covered.
//! - **Retreat** from a space combat it is clearly losing, otherwise stay.
//! - **Production**: buy the most useful unit per resource it can pay for (per production slot
//!   when resources are plentiful): ground forces and a carrier when it lacks them, warships
//!   otherwise, without exceeding fleet supply or capacity. Ground forces beyond what its ships
//!   can carry only to fill the dock's production limit, never into a glut.
//! - **Victory points**: score whenever offered, including Imperial's extra objective, and always
//!   pay to remove the custodians token.
//! - **Command tokens**: a new token goes to the fleet pool until it holds three (or enough for
//!   its biggest ship stack), then to the smaller of the tactic and strategy pools; a
//!   redistribution aims at the same arrangement. Buy Leadership tokens with spare influence
//!   (keeping six for the custodians only while its units are in or next to Mecatol Rex's system,
//!   or could move there this turn). Always follow Technology and Imperial.
//! - **Payment**: exhaust the planet that wastes least.
//! - **Trades**: accept an offer only when its engine-estimated value to the bot is slightly
//!   positive; propose only offers that gain for both sides; never counter.
//! - **Over fleet supply**: remove the cheapest unit.
//! - **Strategy cards**: in the opening rounds the cards that take systems (Warfare,
//!   Construction, Leadership, Technology for Gravity Drive, Diplomacy), never Trade unless Hacan
//!   (who take it first), never Imperial or Politics in round 1; Imperial when Mecatol Rex is held
//!   or about to be. Later: Imperial when it scores, else the card with most trade goods.
//! - **The opening** (rounds 1-2, [`EXPANSION_ROUNDS`]): every transport takes a system of its
//!   own, the best planet first and a system's others only with troops to spare. A token-free
//!   production at home (Warfare's free tactical action, Construction's production, Warfare's
//!   secondary) builds troops and then a third transport; otherwise Warfare is played early, so
//!   the carrier it moves (with two troops extra) can move on from a system without a token.
//!   Mid-round redistributions shrink the fleet pool to the biggest stack and make tactic tokens.
//!   Strategy tokens are kept for Warfare's secondary while it is still to come.
//! - **Research**: Gravity Drive whenever offered.
//! - **Faction plans** (from round 3, [`faction_plans`]): research
//!   follows the faction's technology order, production weighs units by its plan, and a dock is
//!   worth a token to build at until the plan's fleet size. A strategy card the rules above
//!   leave open goes to the plan's preferred card.
//!
//! - **Bot version 2** ([`tuning::BOT_VERSION`]; version 1 is [`tuning::for_version`]`(1)`):
//!   agenda votes, Politics' speaker and agenda placement, sustain damage and casualties, readying
//!   planets, retreat destinations and Trade's free replenish ([`answers`]); production weights for
//!   objectives within reach ([`planner`]); Mecatol Rex held by a rival worth more; rivals denied
//!   from six points; reworked plans for the weakest factions ([`faction_plans::PLANS_V2`]).
//!
//! - **Bot version 3** ([`tuning::BOT_VERSION`]; version 2 is [`tuning::for_version`]`(2)`): a technology gained
//!   outside a research is chosen by the seat's order and the Nekro always use Technological Singularity
//!   (`tech_pick_rule`, [`answers`]); Technology first in round 1 for the Arborec, Naalu and L1Z1X (`r1_tech`)
//!   and in every round for the Nekro (`nekro_tech`). The rejected round-2 candidates stay behind their knobs.
//!
//! - **Bot version 4** ([`tuning::BOT_VERSION`]; version 3 is [`tuning::for_version`]`(3)`): version 3 without
//!   `nekro_tech`, `planner_gap` 5 (the objective planner weighs objectives up to five steps short), and
//!   backward planning ([`backward`], knobs `bw_*`): from round 3 each seat plans backwards from the
//!   objectives it can still score and Mecatol Rex / Imperial, weighs the plans and steers activation, the
//!   Imperial draft, production and a few hooks; with the prep layer (build, Warfare move, technology range
//!   and strength), the ability, agenda, holding and Turn Their Fleets to Dust planners and the production
//!   secrets. `bw_fleet_shape` stays off (a net loss in the direct A/B).
//!
//!   Both versions also choose the starting technology of the Winnu, the Argent Flight, the Council
//!   Keleres and the Thunder's Edge factions ([`faction_plans::starting_tech_pick`]): the first
//!   technology of the faction's plan, then of the general order, that is offered
//!   (`starting_tech_rule`).
//!
//! Other technology choices, action cards, the other secondaries and every other window stay
//! random.

use std::collections::{BTreeMap, BTreeSet};

use rand::Rng;
use rand_chacha::ChaCha8Rng;
use serde_json::Value;
use ti4_content::units::UnitType;
use ti4_engine::choice::{Choice, ChoiceOption, Observed, SeatObservation};
use ti4_engine::fleet_strength::{Descriptors, descriptors, matchup};
use ti4_engine::production::Spend;
use ti4_engine::tactical::Movable;
use ti4_model::id::{PlanetId, PlayerId, SystemId};
use ti4_model::units::Unit;

// Objective layer (round 3+, and the scoring order).
pub mod objectives;

// Agent layer (when to use a faction agent).
pub mod agents;

pub mod agenda_vote;
pub mod answers;
pub mod backward;
mod decider;
pub mod faction_plans;
pub mod planner;
pub mod random;
pub mod tuning;

pub use decider::RuleBotDecider;

/// A fight is taken only when the strength matchup (`ln` of the Lanchester strength ratio) is at
/// least this: roughly a 1.6x edge, "clearly winning" rather than a coin flip.
pub const WIN_MARGIN: f64 = 0.5;
/// A space combat is retreated from when the matchup is this bad.
pub const RETREAT_MARGIN: f64 = -0.5;
/// Extra value of Mecatol Rex while its custodians token (a victory point) is still on it.
pub const MECATOL_BONUS: f64 = 6.0;
/// A system must be worth at least this to steer the action and activation choice.
pub const ACTIVATION_THRESHOLD: f64 = 1.0;
/// A system worth this much (a decent planet to take) is always activated before the strategic
/// action is played.
pub const EAGER_WORTH: f64 = 3.0;
/// An accepted trade must be worth at least this much to the bot (engine `net` estimate).
pub const ACCEPT_NET: f64 = 0.25;
/// Ground forces kept waiting at its docks even when no ship could carry them yet.
pub const MIN_IDLE_GROUND: i64 = 4;
/// Spendable resources from which production is limited by slots rather than by resources.
pub const RICH_RESOURCES: f64 = 6.0;
/// Fleet pool size the bot keeps before putting new tokens elsewhere.
pub const FLEET_TOKENS_WANTED: i32 = 3;
/// Mecatol Rex's planet id.
const MECATOL: &str = "mr";
/// Rounds in which the bot plays for planets above all: strategy cards, tokens and production
/// serve taking more systems (see [`expanding`]).
pub const EXPANSION_ROUNDS: u32 = 2;
/// Spendable resources from which a dock is worth a command token just to build there.
pub const PLENTY_RESOURCES: i64 = 6;
/// A carrier's cost: what a new transport at home needs at least.
const CARRIER_COST: i64 = 3;
/// Tactic plus strategy tokens beyond which Leadership buys no more.
pub const ENOUGH_TOKENS: i32 = 7;
/// Gravity Drive's alias.
const GRAVITY_DRIVE: &str = "gd";

/// The answer of a bot seat to its decision number `index` of the game with `game_seed`: the
/// index of one of the options, and the generator after the draws (for the host's think time).
/// `seat` is the place of the seat in the seat order of the game, from 0 (see
/// [`random::decision_rng`]). With `seen` the seat is a rule bot, without it a random bot. The
/// choice must have at least one option.
///
/// The random pick is always drawn first, so the stream is the same whether or not the
/// heuristic overrides it.
#[must_use]
pub fn choose(
    choice: &Choice,
    seen: Option<&SeatObservation<'_>>,
    game_seed: u64,
    seat: usize,
    index: usize,
) -> (usize, ChaCha8Rng) {
    let (picked, _, rng) = choose_explained(choice, seen, game_seed, seat, index);
    (picked, rng)
}

/// As [`choose`], also saying whether the rule bot's own opinion decided (`true`) or the random
/// pick stood (`false`: no opinion, or none without `seen`). Same pick, same stream.
#[must_use]
pub fn choose_explained(
    choice: &Choice,
    seen: Option<&SeatObservation<'_>>,
    game_seed: u64,
    seat: usize,
    index: usize,
) -> (usize, bool, ChaCha8Rng) {
    let mut rng = random::decision_rng(game_seed, seat, index);
    let random_pick = random::pick_index(choice, &mut rng);
    let smart = seen
        .and_then(|seen| pick(choice, seen, &mut rng))
        .filter(|&smart| smart < choice.options.len());
    (smart.unwrap_or(random_pick), smart.is_some(), rng)
}

/// The smart answer to `choice`, or `None` to keep the random pick.
#[must_use]
pub fn pick(choice: &Choice, seen: &SeatObservation<'_>, rng: &mut ChaCha8Rng) -> Option<usize> {
    let me = seen.bound_seat().clone();
    let observed = seen.observed();
    let subtype = choice.context.as_ref().map(|c| c.subtype.as_str());
    // Round 1: Imperial's immediate scoring of a spending objective (Lead from the Front's three
    // tokens, Erect a Monument's resources, ...) would spend what the expansion plan needs; the
    // objective can still be scored in the status phase with what is left.
    if subtype == Some("imperial_score_objective")
        && spending_objective_blocks_plan(choice, observed, &me)
        && let Some(index) = choice.options.iter().position(ChoiceOption::is_decline)
    {
        return Some(index);
    }
    // -- movement filters (bot version 2): no gravity-rift gamble outside a fight; never strip
    // the home system bare before a rival fleet --
    if subtype == Some("movement_step")
        && let Some(keep) = rift_keep(choice, observed, &me)
            .or_else(|| home_guard_keep(choice, observed, &me))
            .or_else(|| mecatol_guard_keep(choice, observed, &me))
    {
        let mut narrowed = choice.clone();
        narrowed.options = keep.iter().map(|&i| choice.options[i].clone()).collect();
        return pick(&narrowed, seen, rng)
            .and_then(|i| keep.get(i).copied())
            .or_else(|| choice.options.iter().position(ChoiceOption::is_decline));
    }
    // -- backward planning, holdings (backward/holdings.rs, knob bw_holdings): spend nothing a held
    // secret needs, accept a rival's Support for the Throne for Strengthen Bonds --
    if let Some(index) = backward::holdings::answer(choice, seen, &me) {
        return Some(index);
    }
    if let Some(keep) = backward::holdings::keep(choice, seen, &me) {
        let mut narrowed = choice.clone();
        narrowed.options = keep.iter().map(|&i| choice.options[i].clone()).collect();
        return pick(&narrowed, seen, rng)
            .and_then(|i| keep.get(i).copied())
            .or_else(|| choice.options.iter().position(ChoiceOption::is_decline));
    }
    // -- agenda riders (agenda_vote.rs, knob agenda_riders) --
    if let Some(index) = agenda_vote::rider_pick(choice, seen) {
        return Some(index);
    }
    // -- answers to formerly random questions (answers.rs, bot version 2) --
    if subtype != Some("ready_planet")
        && let Some(index) = answers::pick_with(
            choice,
            observed,
            &me,
            &backward::agenda::AgendaBias::of(seen),
        )
    {
        return Some(index);
    }
    // -- objective layer hook (objectives.rs) --
    if let Some(index) = objectives::pick(choice, seen, &me, rng) {
        return Some(index);
    }
    // -- end objective layer hook --
    // -- agent layer hook (agents.rs) --
    if let Some(index) = agents::pick(choice, seen, &me) {
        return Some(index);
    }
    if let Some(answer) = agents::action_choice(choice, seen, &me, rng) {
        return answer;
    }
    // -- end agent layer hook --
    if let Some(index) = choice.options.iter().position(|o| o.kind == "score") {
        return Some(index);
    }
    match subtype {
        // A victory point is never worth declining once the engine offers it. Likewise the free
        // mech Self-Assembly Routines offers after production (rules version 13): the card
        // readies again every round.
        Some("imperial_score_objective" | "remove_custodians" | "self_assembly_mech") => {
            return choice.options.iter().position(|o| !o.is_decline());
        }
        Some("activate_system") => return pick_activation(choice, observed, &me),
        Some("movement_step") => return pick_move(choice, observed, &me),
        Some("load_cargo") => return pick_load(choice, observed, &me),
        Some("commit_ground_forces") => return pick_commit(choice, observed, &me),
        Some("announce_retreat") => return pick_retreat(choice, observed, &me),
        Some("draft_strategy_card") => return pick_strategy_card(choice, observed, &me),
        // v3: the faction plan's order; v2 and older leave the pick to the seeded random one.
        Some("starting_technology") if tuning::starting_tech_rule() => {
            return faction_plans::starting_tech_pick(choice, observed, &me);
        }
        Some("gain_command_token") => return pick_token_pool(choice, observed, &me),
        Some("research_technology") => {
            return pick_research(choice, observed, &me)
                .or_else(|| faction_plans::research_pick(choice, observed, &me));
        }
        // Shuffling tokens between pools at random can empty the fleet pool: keep them.
        Some("predictive_intelligence_redistribute") => {
            return choice.options.iter().position(|o| o.id == "done");
        }
        Some("warfare_free_tactical") => return pick_free_tactical(choice, observed, &me),
        Some("construction_choose_ability") => return pick_construction(choice, observed, &me),
        Some("status_redistribute_tokens" | "warfare_redistribute_tokens") => {
            let warfare = subtype == Some("warfare_redistribute_tokens");
            return pick_redistribution(choice, observed, &me, warfare);
        }
        Some("ready_planet") => {
            return pick_ready_planet(choice, observed, &me)
                .or_else(|| answers::pick(choice, observed, &me));
        }
        // Round 1's trade goods pay for transports, not an action card.
        Some("functioning_base_choose_reward") if expanding(observed) => {
            return choice.options.iter().position(|o| o.id == "gain");
        }
        // Ambush (the Mentak): free dice before the fight, as many ships as offered.
        Some("ambush_roll") => {
            return choice
                .options
                .iter()
                .enumerate()
                .filter(|(_, o)| !o.is_decline())
                .max_by_key(|(i, o)| (o.id.matches('+').count(), std::cmp::Reverse(*i)))
                .map(|(i, _)| i);
        }
        // Pillage (the Mentak): always, and a trade good.
        Some("pillage_take") => {
            return choice
                .options
                .iter()
                .position(|o| o.id.starts_with("trade_good|"))
                .or(Some(0));
        }
        Some(sub) if sub.starts_with("reaction_after_") => {
            if let Some(index) = choice
                .options
                .iter()
                .position(|o| o.id.contains(":pillage:"))
            {
                return Some(index);
            }
        }
        Some("answer_transaction") => return pick_answer(choice),
        Some("propose_transaction") => return pick_proposal(choice),
        _ => {}
    }
    if choice
        .options
        .iter()
        .any(|o| o.kind == "action" && o.id == "tactical")
    {
        return pick_action(choice, observed, &me, rng);
    }
    if let Some(index) = pick_secondary(choice, observed, &me) {
        return Some(index);
    }
    if choice.options.iter().any(|o| o.kind == "produce") {
        return pick_produce_biased(choice, observed, &me, &planner::Bias::of(seen, &me));
    }
    if choice.options.iter().all(|o| o.kind == "pay") {
        return pick_pay(choice, observed, &me);
    }
    if choice.prompt.contains("over fleet supply")
        && choice.options.iter().all(|o| o.kind == "remove")
    {
        return pick_cheapest_removal(choice, observed);
    }
    // Bot version 2: nothing else wanted at the end of a turn, end it (a random free option
    // spends an agent or opens a trade for nothing); a legendary planet's ability is always used.
    if tuning::end_turn_rule() {
        match subtype {
            Some("end_turn") => return choice.options.iter().position(|o| o.id == "end_turn"),
            Some("legendary_end_of_turn") => {
                return choice.options.iter().position(|o| !o.is_decline());
            }
            _ => {}
        }
    }
    None
}

/// Whether every objective Imperial offers to score now spends something (tokens, resources,
/// influence, trade goods) while a round-1 plan still needs it ([`transport_plan`], the Cruiser II
/// plan's reserve).
fn spending_objective_blocks_plan(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> bool {
    const SPENDING: &[&str] = &[
        "monument",
        "lead",
        "trade_routes",
        "sway_council",
        "amass_wealth",
        "centralize_trade",
        "golden_age",
        "galvanize",
        "manipulate_law",
        "vast_reserves",
    ];
    let planning = transport_plan(seen, me).is_some() || reserved_resources(seen, me) > 0;
    let offered: Vec<&ChoiceOption> = choice.options.iter().filter(|o| !o.is_decline()).collect();
    planning
        && seen.round() <= 1
        && !offered.is_empty()
        && offered.iter().all(|o| SPENDING.contains(&o.id.as_str()))
}

// -- reading the board ---------------------------------------------------------------------------

fn faction(seen: &Observed<'_>, player: &PlayerId) -> String {
    seen.seat(player)
        .map(|seat| seat.faction.as_str().to_owned())
        .unwrap_or_default()
}

fn unit_type<'a>(seen: &Observed<'a>, id: &str) -> Option<UnitType<'a>> {
    ti4_content::units::unit_type(seen.content(), id, seen.sources())
}

fn is_ship(seen: &Observed<'_>, unit: &Unit) -> bool {
    unit_type(seen, unit.type_id.as_str()).is_some_and(|t| t.is_ship())
}

fn is_ground(seen: &Observed<'_>, unit: &Unit) -> bool {
    unit_type(seen, unit.type_id.as_str()).is_some_and(|t| t.is_ground_force())
}

/// `(unit id, count)` pairs, as [`descriptors`] takes them.
type Counts = Vec<(String, usize)>;

/// Tally units into the `(id, count)` / damaged lists [`descriptors`] takes.
fn tally<'u>(units: impl IntoIterator<Item = &'u Unit>) -> (Counts, Counts) {
    let mut all: BTreeMap<String, usize> = BTreeMap::new();
    let mut hurt: BTreeMap<String, usize> = BTreeMap::new();
    for unit in units {
        *all.entry(unit.type_id.to_string()).or_default() += 1;
        if unit.sustained_damage {
            *hurt.entry(unit.type_id.to_string()).or_default() += 1;
        }
    }
    (all.into_iter().collect(), hurt.into_iter().collect())
}

/// Combat descriptors of `units`, all owned by `owner`.
fn fleet<'u>(
    seen: &Observed<'_>,
    owner: &PlayerId,
    units: impl IntoIterator<Item = &'u Unit>,
) -> Descriptors {
    let (all, hurt) = tally(units);
    descriptors(
        seen.content(),
        seen.sources(),
        &faction(seen, owner),
        &all,
        &hurt,
    )
}

/// Sum descriptors of several owners' units (a multi-player defence is treated as one side).
fn hostile_fleet<'u>(
    seen: &Observed<'_>,
    me: &PlayerId,
    units: impl IntoIterator<Item = &'u Unit>,
) -> Descriptors {
    let mut by_owner: BTreeMap<&PlayerId, Vec<&Unit>> = BTreeMap::new();
    for unit in units {
        if &unit.owner != me {
            by_owner.entry(&unit.owner).or_default().push(unit);
        }
    }
    let mut total = Descriptors::default();
    for (owner, units) in by_owner {
        let part = fleet(seen, owner, units);
        total.firepower += part.firepower;
        total.durability += part.durability;
        total.fighters += part.fighters;
        total.barrage += part.barrage;
        total.cannon += part.cannon;
        total.capacity += part.capacity;
        total.cost += part.cost;
    }
    total
}

/// Hostile ships in a system's space area, plus the space cannon of hostile units on its planets.
fn hostile_space(seen: &Observed<'_>, me: &PlayerId, system: &SystemId) -> Descriptors {
    let board = seen.system(system);
    let mut ships = hostile_fleet(seen, me, board.units.iter().filter(|u| is_ship(seen, u)));
    let ground = hostile_fleet(seen, me, board.planet_units.values().flatten());
    ships.cannon += ground.cannon;
    ships
}

fn has_hostile_ships(seen: &Observed<'_>, me: &PlayerId, system: &SystemId) -> bool {
    seen.system(system)
        .units
        .iter()
        .any(|u| &u.owner != me && is_ship(seen, u))
}

/// What a planet is worth to take: resources plus influence, Mecatol Rex more while its custodians
/// point is still there.
fn planet_worth(seen: &Observed<'_>, planet: &str) -> f64 {
    let Some(record) = ti4_content::galaxy::planet(seen.content(), planet, seen.sources()) else {
        return 0.0;
    };
    #[expect(
        clippy::cast_precision_loss,
        reason = "printed values are single digits"
    )]
    let mut worth = (record.resources() + record.influence()) as f64 + 0.5;
    // Bot version 3: while the custodians token stands, influence is what lifts it (six to
    // remove it, the point it scores): planets that bring it are worth more early on.
    if !seen.custodians_removed() && seen.round() <= 3 {
        worth += tuning::influence_early() * record.influence() as f64;
    }
    if planet == MECATOL && !seen.custodians_removed() {
        worth += tuning::mecatol_bonus();
        // Bot version 3: Mecatol Rex is the strongest marker of a won game (analysis of 2000
        // 4p games); a push in rounds 2 and 3, when its custodians can be lifted.
        if (2..=3).contains(&seen.round()) {
            worth += tuning::mecatol_early();
        }
    }
    worth
}

/// Mecatol Rex's system on this map.
fn mecatol_system(seen: &Observed<'_>) -> Option<SystemId> {
    ti4_content::galaxy::planet(seen.content(), MECATOL, seen.sources())?
        .system_id()
        .map(SystemId::new)
}

/// Whether the seat could invade Mecatol Rex this turn or next, so six influence for the
/// custodians are worth keeping back: the custodians still stand and it has units in Mecatol's
/// system or next to it, or ships that could move there now. Without a map, assume it can.
fn mecatol_in_reach(seen: &Observed<'_>, me: &PlayerId) -> bool {
    if seen.custodians_removed() {
        return false;
    }
    let (Some(galaxy), Some(mr)) = (seen.galaxy(), mecatol_system(seen)) else {
        return true;
    };
    let has_units = |system: &SystemId| {
        let board = seen.system(system);
        board
            .units
            .iter()
            .chain(board.planet_units.values().flatten())
            .any(|u| &u.owner == me)
    };
    has_units(&mr)
        || galaxy
            .adjacent(mr.as_str())
            .into_iter()
            .any(|system| has_units(&SystemId::new(system)))
        || !seen.movable_into(me, &mr).is_empty()
}

/// Whether the seat could take Mecatol Rex with its next activation: the custodians still stand,
/// it has the six influence for them, and carriers that can reach the system bring ground forces
/// past any defence there (no hostile fleet it cannot clearly beat).
fn mecatol_takeable(seen: &Observed<'_>, me: &PlayerId) -> bool {
    if seen.custodians_removed()
        || (!blood_ties(seen, me) && seen.available_spend(me, Spend::Influence) < 6)
    {
        return false;
    }
    let Some(mr) = mecatol_system(seen) else {
        return false;
    };
    let planet = PlanetId::new(MECATOL);
    if seen.system(&mr).planet_control.get(&planet) == Some(me) {
        return false;
    }
    activation_value(seen, me, &mr) >= tuning::mecatol_bonus()
}

/// Bot version 5 candidate ([`tuning::mecatol_imperial_priority`]): the seat holds Mecatol Rex
/// now or could take it, so Imperial's primary scores a point for it. The controller of the
/// planet is what is known at draft time (round 1 nobody holds it; the plan to take it counts
/// through [`mecatol_takeable`]).
pub(crate) fn mecatol_imperial_wanted(seen: &Observed<'_>, me: &PlayerId) -> bool {
    holds_mecatol(seen, me) || mecatol_takeable(seen, me)
}

/// Bot version 3: the Winnu's Blood Ties lift the custodians without influence.
fn blood_ties(seen: &Observed<'_>, me: &PlayerId) -> bool {
    tuning::winnu_mecatol() && faction(seen, me) == "winnu"
}

/// Whether the seat holds Mecatol Rex (Imperial's point).
fn holds_mecatol(seen: &Observed<'_>, me: &PlayerId) -> bool {
    seen.controlled_planets(me)
        .into_iter()
        .any(|(_, planet)| planet.as_str() == MECATOL)
}

/// Ground forces needed to take `planet`: one more than the hit points defending it.
fn ground_needed(
    seen: &Observed<'_>,
    me: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) -> usize {
    let board = seen.system(system);
    let defenders: usize = board
        .planet_units
        .get(planet)
        .into_iter()
        .flatten()
        .filter(|u| &u.owner != me && is_ground(seen, u))
        .map(|u| {
            let sustains = !u.sustained_damage
                && unit_type(seen, u.type_id.as_str()).is_some_and(|t| t.sustain_damage());
            1 + usize::from(sustains)
        })
        .sum();
    // Bot version 3: a margin over the defenders (a ground combat is two dice rolls a round).
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "small counts"
    )]
    let margin = (tuning::ground_margin() * defenders as f64) as usize;
    defenders + 1 + margin
}

/// Planets in `system` worth invading, best value per ground force first: (planet, worth, ground
/// needed). Planets the
/// seat already holds, and Mecatol Rex while it cannot pay to lift the custodians, are left out.
fn targets(seen: &Observed<'_>, me: &PlayerId, system: &SystemId) -> Vec<(PlanetId, f64, usize)> {
    let board = seen.system(system);
    let mut out: Vec<(PlanetId, f64, usize)> =
        ti4_content::galaxy::planets_in(seen.content(), system.as_str(), seen.sources())
            .into_iter()
            .map(|record| PlanetId::new(record.id()))
            .filter(|planet| board.planet_control.get(planet) != Some(me))
            .filter(|planet| {
                planet.as_str() != MECATOL
                    || seen.custodians_removed()
                    || blood_ties(seen, me)
                    || seen.available_spend(me, Spend::Influence) >= 6
            })
            .map(|planet| {
                let mut worth = planet_worth(seen, planet.as_str());
                // Bot version 2: Mecatol Rex in a rival's hands scores them Imperial's point.
                if planet.as_str() == MECATOL && board.planet_control.contains_key(&planet) {
                    worth += tuning::mecatol_rival_bonus();
                }
                // Reclamation: a free dock and PDS there.
                if planet.as_str() == MECATOL && blood_ties(seen, me) {
                    worth += 2.0;
                }
                let need = ground_needed(seen, me, system, &planet);
                (planet, worth, need)
            })
            .collect();
    out.sort_by(|a, b| {
        per_troop(b)
            .total_cmp(&per_troop(a))
            .then_with(|| a.0.cmp(&b.0))
    });
    out
}

/// A target's worth per ground force it takes to win it: free planets first, then the richest.
#[expect(clippy::cast_precision_loss, reason = "small counts")]
fn per_troop(target: &(PlanetId, f64, usize)) -> f64 {
    target.1 / target.2.max(1) as f64
}

/// The value of taking as many of `targets` as `ground` ground forces can, greedily by
/// [`per_troop`] (the order [`targets`] returns).
fn takeable_value(targets: &[(PlanetId, f64, usize)], mut ground: usize) -> f64 {
    let mut value = 0.0;
    for (_, worth, need) in targets {
        if *need <= ground {
            ground -= need;
            value += worth;
        }
    }
    value
}

/// Count this seat's units.
fn own_counts(seen: &Observed<'_>, me: &PlayerId) -> (usize, usize) {
    let (mut ships, mut ground) = (0, 0);
    for system in seen.board().values() {
        for unit in system
            .units
            .iter()
            .chain(system.planet_units.values().flatten())
        {
            if &unit.owner != me {
                continue;
            }
            let Some(kind) = unit_type(seen, unit.type_id.as_str()) else {
                continue;
            };
            if kind.is_ship() && !kind.is_fighter() {
                ships += 1;
            } else if kind.is_ground_force() {
                ground += 1;
            }
        }
    }
    (ships, ground)
}

/// Whether the seat is short of units it could build in `system`: ships while it has few and the
/// system's fleet supply has room, ground forces while few wait at its docks for transport.
fn wants_to_build(seen: &Observed<'_>, me: &PlayerId, system: &SystemId) -> bool {
    let (ships, _) = own_counts(seen, me);
    let here = seen
        .system(system)
        .units
        .iter()
        .filter(|u| &u.owner == me)
        .filter(|u| {
            unit_type(seen, u.type_id.as_str()).is_some_and(|t| t.is_ship() && !t.is_fighter())
        })
        .count();
    let fleet = seen
        .seat(me)
        .map_or(0, |seat| usize::try_from(seat.fleet_tokens).unwrap_or(0));
    let wanted = faction_plans::ships_wanted(seen, me);
    (ships < wanted && here < fleet) || !enough_ground(seen, me)
}

/// Whether as many ground forces wait at its docks as its ships can carry (at least
/// [`MIN_IDLE_GROUND`]), so more would only sit there.
fn enough_ground(seen: &Observed<'_>, me: &PlayerId) -> bool {
    let (idle, lift) = idle_ground_and_lift(seen, me);
    idle >= usize::try_from(lift.max(tuning::min_idle_ground())).unwrap_or(0)
}

fn has_own_production(seen: &Observed<'_>, me: &PlayerId, system: &SystemId) -> bool {
    let board = seen.system(system);
    board
        .units
        .iter()
        .chain(board.planet_units.values().flatten())
        .any(|u| {
            &u.owner == me
                && unit_type(seen, u.type_id.as_str()).is_some_and(|t| t.has_production())
        })
}

/// What activating `system` is worth to `me`, from the public board.
///
/// Negative when the fight there would be lost; zero when the bot has no reason to go.
#[must_use]
pub fn activation_value(seen: &Observed<'_>, me: &PlayerId, system: &SystemId) -> f64 {
    let movable = movable_for(seen, me, system);
    let board = seen.system(system);
    let present: Vec<&Unit> = board
        .units
        .iter()
        .filter(|u| &u.owner == me && is_ship(seen, u))
        .collect();

    // Space: everything that could come, plus what is already there.
    let arriving: Vec<&Unit> = movable.iter().map(|m| &m.unit).collect();
    let own = fleet(
        seen,
        me,
        present.iter().copied().chain(arriving.iter().copied()),
    );
    let enemy = hostile_space(seen, me, system);
    let contested = has_hostile_ships(seen, me, system);
    let mut value = 0.0;
    if contested {
        if movable.is_empty() || matchup(&own, &enemy) < fight_margin(seen, system) {
            return -10.0;
        }
        value += 1.0 + 0.5 * enemy.cost;
        if tuning::tech_hunt() > 0.0 && faction(seen, me) == "nekro" {
            value += tuning::tech_hunt() * technologies_to_take(seen, me, system);
        }
    }

    // Ground: what carriers coming from the reachable origins could bring. Only ships: a unit
    // that moves without being one (the Saar's Floating Factory) cannot hold the space, so the
    // invasion step never opens for what it carries alone.
    let carriers: Vec<_> = movable
        .iter()
        .filter(|m| m.capacity > 0 && is_ship(seen, &m.unit))
        .collect();
    let capacity: i64 = carriers.iter().map(|m| m.capacity).sum();
    let origins: BTreeSet<&SystemId> = carriers.iter().map(|m| &m.origin).collect();
    let mut loadable_ground = 0usize;
    for origin in origins {
        loadable_ground += seen
            .loadable(me, origin)
            .iter()
            .filter(|cargo| is_ground(seen, &cargo.unit))
            .count();
    }
    let already_there = board
        .units
        .iter()
        .filter(|u| &u.owner == me && is_ground(seen, u))
        .count();
    let ground = loadable_ground.min(usize::try_from(capacity).unwrap_or(0)) + already_there;
    let planets = targets(seen, me, system);
    if !movable.is_empty() || already_there > 0 {
        let taken = takeable_value(&planets, ground);
        // Bot version 2: a fight that takes no planet (no troops to land), next to nothing of
        // its own, wins nothing (fb_1791896411); from round 3 the objective layer says the same.
        if contested
            && tuning::empty_fight_rule()
            && taken <= 0.0
            && !board.planet_control.values().any(|owner| owner == me)
            && !home_system(seen, me).is_some_and(|home| {
                &home == system
                    || seen.galaxy().is_some_and(|g| {
                        g.adjacent(home.as_str())
                            .iter()
                            .any(|s| *s == system.as_str())
                    })
            })
        {
            value = 0.0;
        }
        value += taken;
        if taken > 0.0 && tuning::adjacent_bonus() > 0.0 {
            value += tuning::adjacent_bonus() * own_neighbours(seen, me, system);
        }
    }

    // Production at home when short of units and able to pay, or an empty transport to fetch the
    // ground forces waiting there.
    if !contested && has_own_production(seen, me, system) {
        // A command token only to build is worth it with plenty to spend; otherwise Warfare's
        // secondary or Construction builds without one.
        // (Not in round 1 while a token-free production at home is still to come for the
        // transport plan: the token would keep the new transports home, and that production
        // could not build there either.)
        let waiting_build = seen.round() <= 1
            && transport_plan(seen, me).is_some()
            && home_builder_ahead(seen, me)
            && home_system(seen, me).as_ref() == Some(system);
        let build = !waiting_build
            && spare_resources(seen, me) >= tuning::plenty_resources()
            && wants_to_build(seen, me, system);
        let ferry = needs_transport(seen, me, system)
            && movable.iter().any(|m| {
                m.capacity > 0
                    && !seen
                        .loadable(me, &m.origin)
                        .iter()
                        .any(|cargo| is_ground(seen, &cargo.unit))
            });
        if build || ferry {
            value = value.max(2.0);
        }
    }
    value
}

/// Bot version 3: how many systems next to `system` hold a planet of the seat (at most three).
fn own_neighbours(seen: &Observed<'_>, me: &PlayerId, system: &SystemId) -> f64 {
    let Some(galaxy) = seen.galaxy() else {
        return 0.0;
    };
    let n = galaxy
        .adjacent(system.as_str())
        .into_iter()
        .filter(|s| {
            seen.system(&SystemId::new(*s))
                .planet_control
                .values()
                .any(|owner| owner == me)
        })
        .count();
    #[expect(clippy::cast_precision_loss, reason = "tiny count")]
    {
        n.min(3) as f64
    }
}

/// Bot version 3, the Nekro: technologies of the rivals in `system` the seat lacks, each one a
/// Technological Singularity pick (a destroyed unit is enough).
fn technologies_to_take(seen: &Observed<'_>, me: &PlayerId, system: &SystemId) -> f64 {
    let Some(mine) = seen.seat(me) else {
        return 0.0;
    };
    let mut owners: BTreeSet<&PlayerId> = BTreeSet::new();
    let board = seen.system(system);
    for unit in &board.units {
        if &unit.owner != me && is_ship(seen, unit) {
            owners.insert(&unit.owner);
        }
    }
    let mut lacking: BTreeSet<&str> = BTreeSet::new();
    for owner in owners {
        if let Some(seat) = seen.seat(owner) {
            for tech in seat.technologies {
                if !mine.technologies.contains(tech) {
                    lacking.insert(tech.as_str());
                }
            }
        }
    }
    #[expect(clippy::cast_precision_loss, reason = "tiny count")]
    {
        lacking.len().min(4) as f64
    }
}

// -- the expansion plan --------------------------------------------------------------------------

/// Whether this is one of the opening rounds, when planets come first.
fn expanding(seen: &Observed<'_>) -> bool {
    seen.round() <= tuning::expansion_rounds()
}

/// The seat's home system: where its home planets are.
fn home_system(seen: &Observed<'_>, me: &PlayerId) -> Option<SystemId> {
    seen.controlled_planets(me)
        .into_iter()
        .find(|(_, planet)| {
            ti4_content::galaxy::planet(seen.content(), planet.as_str(), seen.sources())
                .is_some_and(|record| record.homeworld_of().is_some())
        })
        .map(|(system, _)| system.clone())
}

fn card_is(seen: &Observed<'_>, card: &str, name: &str) -> bool {
    ti4_engine::strategy_cards::card_name(seen.content(), card).as_deref() == Some(name)
}

/// The seat's strategy card called `name`, while it is still unplayed.
fn ready_card(seen: &Observed<'_>, me: &PlayerId, name: &str) -> Option<String> {
    let seat = seen.seat(me)?;
    seat.strategy_cards
        .iter()
        .find(|card| {
            !seat.exhausted_strategy_cards.contains(*card) && card_is(seen, card.as_str(), name)
        })
        .map(ToString::to_string)
}

/// Whether another seat holds the card called `name` and has not played it yet: its secondary
/// is still to come this round.
fn secondary_to_come(seen: &Observed<'_>, me: &PlayerId, name: &str) -> bool {
    seen.players()
        .into_iter()
        .filter(|player| *player != me)
        .any(|player| ready_card(seen, player, name).is_some())
}

/// Strategy tokens kept back for a secondary worth more than the one on offer: in the opening
/// rounds one for Warfare's home production while another seat has Warfare still to play, and
/// one for Diplomacy's readying while the transport plan has a home build to pay for.
fn strategy_reserve(seen: &Observed<'_>, me: &PlayerId) -> i32 {
    let warfare = expanding(seen) && secondary_to_come(seen, me, "Warfare");
    // Short of transports: one more for Diplomacy's readying, the home build's resources.
    let diplomacy = short_of_transports(seen, me)
        && home_builder_ahead(seen, me)
        && secondary_to_come(seen, me, "Diplomacy");
    i32::from(warfare) + i32::from(diplomacy)
}

/// Own non-fighter ships in `system`'s space area, and their capacity.
fn ships_and_lift(seen: &Observed<'_>, me: &PlayerId, system: &SystemId) -> (usize, i64) {
    seen.system(system)
        .units
        .iter()
        .filter(|u| &u.owner == me)
        .filter_map(|u| unit_type(seen, u.type_id.as_str()))
        .filter(|t| t.is_ship() && !t.is_fighter())
        .fold((0, 0), |(n, lift), t| (n + 1, lift + t.capacity()))
}

/// Whether a planet it could take lies within one move of `origin` (a carrier's reach), with no
/// hostile fleet in the way.
fn target_near(seen: &Observed<'_>, me: &PlayerId, origin: &SystemId) -> bool {
    let Some(galaxy) = seen.galaxy() else {
        return false;
    };
    let tokens = seen.systems_with_token(me);
    galaxy
        .system_ids()
        .into_iter()
        .map(SystemId::new)
        .filter(|system| system != origin && !tokens.contains(system))
        .filter(|system| !has_hostile_ships(seen, me, system))
        .filter(|system| !targets(seen, me, system).is_empty())
        .any(|system| seen.can_reach(me, origin, &system, carrier_reach(seen, me)))
}

/// A new carrier's movement: one, plus Gravity Drive, plus the Creuss' Slipstream (they start in
/// a wormhole system).
fn carrier_reach(seen: &Observed<'_>, me: &PlayerId) -> i32 {
    let gravity = seen.seat(me).is_some_and(|seat| {
        seat.technologies
            .iter()
            .any(|t| t.as_str() == GRAVITY_DRIVE)
    });
    1 + i32::from(gravity) + i32::from(faction(seen, me) == "ghost")
}

/// The opening plan's third system: the transports have left home, a planet is still in reach of
/// it, home is not activated (so a new carrier can leave), a tactic token is left to send it, and
/// the seat can pay for a carrier (and two infantry when none wait at home). Then a production at
/// home that places no token there (Warfare, Construction) is worth a planet.
fn wants_home_transport(seen: &Observed<'_>, me: &PlayerId) -> bool {
    if !expanding(seen) {
        return false;
    }
    let Some(home) = home_system(seen, me) else {
        return false;
    };
    if seen.systems_with_token(me).contains(&home)
        || seen.seat(me).is_none_or(|seat| seat.tactic_tokens <= 0)
    {
        return false;
    }
    let (_, lift) = ships_and_lift(seen, me, &home);
    if lift >= 2 || home_fleet_room(seen, me, &home) <= 0 {
        return false;
    }
    let (waiting, _) = waiting_and_lift(seen, me, &home);
    let need = CARRIER_COST + if waiting == 0 { 2 } else { 0 };
    spare_resources(seen, me) >= need && target_near(seen, me, &home)
}

/// Whether a token-free production at home (Warfare's free tactical action, Construction's
/// production) is the seat's best next step in the opening: a new transport once the others have
/// left ([`wants_home_transport`]), or, before they leave, a transport and troops for a third
/// system when it has fewer than three transports and can pay for a carrier and two infantry.
fn home_build_wanted(seen: &Observed<'_>, me: &PlayerId) -> bool {
    if wants_home_transport(seen, me) {
        return true;
    }
    if !expanding(seen) {
        return false;
    }
    let Some(home) = home_system(seen, me) else {
        return false;
    };
    let (transports, ground) = free_transports_and_ground(seen, me);
    // Two infantry (one resource) only when the new transport would have no troop of its own.
    let troops = i64::from(ground <= transports);
    !seen.systems_with_token(me).contains(&home)
        && transports < 3
        && home_fleet_room(seen, me, &home) > 0
        && spare_resources(seen, me) >= CARRIER_COST + troops
        && target_near(seen, me, &home)
}

/// How many more non-fighter ships fleet supply lets the seat build in `home` now.
fn home_fleet_room(seen: &Observed<'_>, me: &PlayerId, home: &SystemId) -> i64 {
    let (ships, _) = ships_and_lift(seen, me, home);
    let fleet = seen.seat(me).map_or(0, |seat| i64::from(seat.fleet_tokens));
    fleet - i64::try_from(ships).unwrap_or(i64::MAX)
}

/// The round-1 transport plan of a seat with fewer transports than tactic tokens: how many it is
/// short (at most three systems' worth) and what building them and a troop each at home costs.
/// `None` when it has enough, or outside round 1.
fn transport_plan(seen: &Observed<'_>, me: &PlayerId) -> Option<(i64, i64)> {
    if seen.round() > 1 || (cruiser_plan(seen, me) && ready_card(seen, me, "Technology").is_some())
    {
        return None;
    }
    // Three systems this round, less those already taken; as many as the tactic tokens (one
    // more from Warfare's redistribution while it is still to play) can reach.
    let tokens = seen
        .seat(me)
        .map_or(0, |seat| i64::from(seat.tactic_tokens))
        + i64::from(ready_card(seen, me, "Warfare").is_some());
    let wanted = (3 - systems_taken_this_round(seen, me)).min(tokens);
    let (transports, ground) = free_transports_and_ground(seen, me);
    let missing = wanted - transports;
    if missing <= 0 {
        return None;
    }
    // A troop for every transport (the first one leaves two at home for the new ones, see
    // `ground_shortfall`); two infantry cost one; Sarween Tools takes one off the build.
    let troops = (missing + transports - ground).max(0);
    let discount = i64::from(has_technology(seen, me, "st"));
    Some((
        missing,
        CARRIER_COST * missing + (troops + 1) / 2 - discount,
    ))
}

/// Whether the seat owns technology `alias`.
fn has_technology(seen: &Observed<'_>, me: &PlayerId, alias: &str) -> bool {
    seen.seat(me)
        .is_some_and(|seat| seat.technologies.iter().any(|t| t.as_str() == alias))
}

/// Systems activated this round (they hold its command token) where it now controls a planet,
/// home aside: the systems the round has taken so far.
fn systems_taken_this_round(seen: &Observed<'_>, me: &PlayerId) -> i64 {
    let home = home_system(seen, me);
    let taken = seen
        .systems_with_token(me)
        .into_iter()
        .filter(|system| Some(*system) != home.as_ref())
        .filter(|system| {
            seen.system(system)
                .planet_control
                .values()
                .any(|owner| owner == me)
        })
        .count();
    i64::try_from(taken).unwrap_or(i64::MAX)
}

/// The Creuss: their home lies beyond the wormhole everyone else uses too, so their warships
/// have nowhere to wait out of the others' way (the Gate itself is their road); they escort the
/// first carrier instead of a separate clearing move.
fn gate_bound(seen: &Observed<'_>, me: &PlayerId) -> bool {
    faction(seen, me) == "ghost"
}

/// Whether the seat should clear its home before Warfare's home build: a tactical action that
/// only moves its warships without capacity out of home, so fleet supply lets the build place a
/// transport for every remaining token. The loaded transport has already left; Warfare's
/// redistribution gives the token back (the spread-out fleet needs fewer fleet tokens), so the
/// tokens still send every new transport. It also waits a turn for Diplomacy's readying.
fn wait_move_pays(seen: &Observed<'_>, me: &PlayerId) -> bool {
    if gate_bound(seen, me) {
        return false;
    }
    let Some((missing, _)) = transport_plan(seen, me) else {
        return false;
    };
    let Some(home) = home_system(seen, me) else {
        return false;
    };
    let tokens = seen
        .seat(me)
        .map_or(0, |seat| i64::from(seat.tactic_tokens));
    let warships_home = seen
        .system(&home)
        .units
        .iter()
        .filter(|u| &u.owner == me)
        .filter_map(|u| unit_type(seen, u.type_id.as_str()))
        .any(|t| t.is_ship() && !t.is_fighter() && t.capacity() == 0);
    let (_, lift) = waiting_and_lift(seen, me, &home);
    ready_card(seen, me, "Warfare").is_some()
        && !ground_produces(seen, me)
        && missing >= 2
        && warships_home
        && lift == 0
        && home_fleet_room(seen, me, &home) < missing
        && !seen.systems_with_token(me).contains(&home)
        && tokens >= missing
        && !wait_systems(seen, me).is_empty()
}

/// The waiting move's system ([`wait_move_pays`]): one the home warships can reach, without a
/// hostile fleet, best without planets to take (nothing is landed).
fn pick_wait_activation(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    let wanted = wait_systems(seen, me);
    choice
        .options
        .iter()
        .enumerate()
        .filter_map(|(i, o)| {
            wanted
                .iter()
                .find(|(system, _)| system.as_str() == o.id)
                .map(|(_, crowd)| (i, *crowd))
        })
        .min_by_key(|&(i, crowd)| (crowd, i))
        .map(|(i, _)| i)
}

/// Systems the waiting move may go to: no hostile ships, not home, reachable by a home warship
/// without capacity, and either without planets or with no other seat's units next to it (they
/// would want its planets); each scored by the neighbouring systems holding another seat's units,
/// planet systems after the planet-less ones (lower is better: the warships stay out of the
/// others' way).
fn wait_systems(seen: &Observed<'_>, me: &PlayerId) -> Vec<(SystemId, usize)> {
    let (Some(galaxy), Some(home)) = (seen.galaxy(), home_system(seen, me)) else {
        return Vec::new();
    };
    let tokens = seen.systems_with_token(me);
    galaxy
        .system_ids()
        .into_iter()
        .map(SystemId::new)
        .filter(|system| system != &home && !tokens.contains(system))
        // No wormhole or anomaly: a wormhole is everyone's road (the Creuss Gate), and warships
        // parked there block the others' moves through it.
        .filter(|system| {
            ti4_content::galaxy::system(seen.content(), system.as_str(), seen.sources())
                .is_none_or(|record| record.wormholes().is_empty() && !record.is_anomaly())
        })
        .filter(|system| !has_hostile_ships(seen, me, system))
        .filter(|system| {
            seen.movable_into(me, system)
                .iter()
                .any(|m| m.origin == home && m.capacity == 0 && is_ship(seen, &m.unit))
        })
        .map(|system| {
            // Other seats' ships within two moves: the warships would sit in their way.
            let mut ring: BTreeSet<&str> = galaxy.adjacent(system.as_str());
            for near in ring.clone() {
                ring.extend(galaxy.adjacent(near));
            }
            let crowd = ring
                .into_iter()
                .filter(|near| {
                    seen.system(&SystemId::new(*near))
                        .units
                        .iter()
                        .any(|u| &u.owner != me && is_ship(seen, u))
                })
                .count();
            let planets =
                !ti4_content::galaxy::planets_in(seen.content(), system.as_str(), seen.sources())
                    .is_empty();
            // A system with planets only where no other seat is near (it would want them).
            (system, crowd + 10 * usize::from(planets), planets)
        })
        // Never next to another seat's units: warships there would block its moves.
        .filter(|(_, crowd, planets)| *crowd == 10 * usize::from(*planets))
        .map(|(system, crowd, _)| (system, crowd))
        .collect()
}

/// Whether the seat's ground forces have Production (the Arborec): a transport can be built where
/// they land.
fn ground_produces(seen: &Observed<'_>, me: &PlayerId) -> bool {
    seen.board()
        .values()
        .flat_map(|s| s.units.iter().chain(s.planet_units.values().flatten()))
        .filter(|u| &u.owner == me)
        .filter_map(|u| unit_type(seen, u.type_id.as_str()))
        .any(|t| t.is_ground_force() && t.has_production())
}

/// Whether the transport plan needs two or more new transports: a single transport, whose
/// Warfare route (a free activation, then a tactical one) reaches two systems at most.
fn short_of_transports(seen: &Observed<'_>, me: &PlayerId) -> bool {
    transport_plan(seen, me).is_some_and(|(missing, _)| missing >= 2)
}

/// Whether a token-free production at home is still to come this round: Construction or Warfare
/// held unplayed, or Warfare's secondary while another seat has it to play and a strategy token
/// is left.
fn home_builder_ahead(seen: &Observed<'_>, me: &PlayerId) -> bool {
    ready_card(seen, me, "Construction").is_some()
        || ready_card(seen, me, "Warfare").is_some()
        || (secondary_to_come(seen, me, "Warfare")
            && seen.seat(me).is_some_and(|seat| seat.strategic_tokens > 0))
}

/// Whether the seat's home is full for fleet supply while a transport with troops waits there:
/// it leaves first, or nothing could be built.
fn loaded_transport_blocks_home(seen: &Observed<'_>, me: &PlayerId) -> bool {
    home_system(seen, me).is_some_and(|home| {
        let (waiting, lift) = waiting_and_lift(seen, me, &home);
        home_fleet_room(seen, me, &home) <= 0 && lift > 0 && waiting > 0
    })
}

/// Own transports (non-fighter ships with capacity) and ground forces outside the
/// systems holding its command token: what can still move this round.
fn free_transports_and_ground(seen: &Observed<'_>, me: &PlayerId) -> (i64, i64) {
    let tokens = seen.systems_with_token(me);
    let (mut transports, mut ground) = (0, 0);
    for (id, system) in seen.board() {
        if tokens.contains(id) {
            continue;
        }
        for unit in system
            .units
            .iter()
            .chain(system.planet_units.values().flatten())
            .filter(|u| &u.owner == me)
        {
            let Some(kind) = unit_type(seen, unit.type_id.as_str()) else {
                continue;
            };
            if kind.is_ground_force() {
                ground += 1;
            } else if kind.is_ship() && !kind.is_fighter() && kind.capacity() >= 1 {
                transports += 1;
            }
        }
    }
    (transports, ground)
}

/// Whether the seat could research Gravity Drive: it needs one propulsion technology.
fn gravity_drive_open(seen: &Observed<'_>, me: &PlayerId) -> bool {
    let Some(seat) = seen.seat(me) else {
        return false;
    };
    if seat
        .technologies
        .iter()
        .any(|t| t.as_str() == GRAVITY_DRIVE)
    {
        return false;
    }
    seat.technologies.iter().any(|tech| {
        seen.content()
            .get(
                ti4_model::content_types::ContentType::Technologies,
                tech.as_str(),
            )
            .is_some_and(|record| record.strings("types").contains(&"PROPULSION"))
    })
}

// -- faction openings ----------------------------------------------------------------------------

/// Cruiser II's alias: capacity 1 and move 3 for every cruiser the seat owns.
const CRUISER_II: &str = "cr2";
/// Trade goods the Trade primary gives.
const TRADE_GOODS: i64 = 3;
/// What the Technology primary's second research costs.
const SECOND_RESEARCH: i64 = 6;

/// A technology's colour letter (`G`, `Y`, `R`, `B`), if it has one.
fn tech_colour(seen: &Observed<'_>, tech: &str) -> Option<char> {
    let record = seen
        .content()
        .get(ti4_model::content_types::ContentType::Technologies, tech)?;
    let types = record.strings("types");
    [
        ("BIOTIC", 'G'),
        ("CYBERNETIC", 'Y'),
        ("WARFARE", 'R'),
        ("PROPULSION", 'B'),
    ]
    .into_iter()
    .find(|(name, _)| types.contains(name))
    .map(|(_, letter)| letter)
}

/// The prerequisite colours `tech` needs that the seat's technologies do not cover yet.
fn missing_colours(seen: &Observed<'_>, me: &PlayerId, tech: &str) -> Vec<char> {
    let Some(record) = seen
        .content()
        .get(ti4_model::content_types::ContentType::Technologies, tech)
    else {
        return Vec::new();
    };
    let mut owned: Vec<char> = seen.seat(me).map_or_else(Vec::new, |seat| {
        seat.technologies
            .iter()
            .filter_map(|t| tech_colour(seen, t.as_str()))
            .collect()
    });
    let mut missing = Vec::new();
    for letter in record.text("requirements").unwrap_or("").chars() {
        if let Some(i) = owned.iter().position(|c| *c == letter) {
            owned.remove(i);
        } else if "GYRB".contains(letter) {
            missing.push(letter);
        }
    }
    missing
}

/// Own units of type `id` on the board.
fn own_units(seen: &Observed<'_>, me: &PlayerId, id: &str) -> usize {
    seen.board()
        .values()
        .flat_map(|s| s.units.iter())
        .filter(|u| &u.owner == me && u.type_id.as_str() == id)
        .count()
}

/// Resources on the seat's planets (ready or not): what it could spend in a round.
fn planet_resources(seen: &Observed<'_>, me: &PlayerId) -> i64 {
    seen.controlled_planets(me)
        .into_iter()
        .filter_map(|(_, planet)| {
            ti4_content::galaxy::planet(seen.content(), planet.as_str(), seen.sources())
        })
        .map(|record| record.resources())
        .sum()
}

/// The Cruiser II opening (the Mentak, with two cruisers and a red and a yellow technology): in
/// round 1 Trade's goods and Technology's primary research the missing colour for free and then
/// Cruiser II for six, so both cruisers carry a ground force (capacity 1, move 3) beside the
/// carrier: three transports for the three tactic tokens without building anything at home.
fn cruiser_plan(seen: &Observed<'_>, me: &PlayerId) -> bool {
    seen.round() <= 1
        && own_units(seen, me, "cruiser") >= 2
        && missing_colours(seen, me, CRUISER_II).len() == 1
        && planet_resources(seen, me) + TRADE_GOODS >= SECOND_RESEARCH
}

/// Draft picks the other seats make before this seat's next one, in the snake draft (1..n, n..1),
/// while it still has none: `None` once it holds a card or the draft is not a two-pass one.
fn picks_before_next(seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    let players = seen.players();
    let n = players.len();
    let made: usize = players
        .iter()
        .filter_map(|p| seen.seat(p))
        .map(|seat| seat.strategy_cards.len())
        .sum();
    let mine = seen.seat(me)?.strategy_cards.len();
    if mine > 0 || made >= n || n > 4 {
        return None;
    }
    let position = made + 1;
    Some(2 * (n - position))
}

/// Whether `seat` takes Trade early in round 1: the Hacan, the Cruiser II plan, and a seat short
/// of transports and of the resources to build them.
fn wants_trade(seen: &Observed<'_>, seat: &PlayerId) -> bool {
    faction(seen, seat) == "hacan"
        || cruiser_plan(seen, seat)
        || transport_plan(seen, seat).is_some_and(|(_, cost)| planet_resources(seen, seat) < cost)
}

/// Whether Trade would still be there at this seat's next draft pick: the other seats take every
/// card but Imperial before Trade in round 1 (unless one loves Trade).
fn trade_survives(choice: &Choice, seen: &Observed<'_>, me: &PlayerId, taking: &str) -> bool {
    let Some(between) = picks_before_next(seen, me) else {
        return false;
    };
    let offered = |name: &str| choice.options.iter().any(|o| card_is(seen, &o.id, name));
    if !offered("Trade") {
        return false;
    }
    // The seats still without a card pick before this seat's next pick.
    let lover = seen
        .players()
        .into_iter()
        .filter(|p| *p != me)
        .filter(|p| {
            seen.seat(p)
                .is_some_and(|seat| seat.strategy_cards.is_empty())
        })
        .any(|p| wants_trade(seen, p));
    let before_trade = choice
        .options
        .iter()
        .filter(|o| o.id != taking)
        .filter(|o| !card_is(seen, &o.id, "Trade") && !card_is(seen, &o.id, "Imperial"))
        .count();
    !lover && before_trade >= between
}

/// A faction opening's draft score for `card` in round 1, overriding the default, or `None`.
fn faction_card_score(
    choice: &Choice,
    seen: &Observed<'_>,
    me: &PlayerId,
    card: &str,
) -> Option<f64> {
    if seen.round() > 1 {
        return None;
    }
    let holds = |name: &str| {
        seen.seat(me).is_some_and(|seat| {
            seat.strategy_cards
                .iter()
                .any(|held| card_is(seen, held.as_str(), name))
        })
    };
    let name = ti4_engine::strategy_cards::card_name(seen.content(), card).unwrap_or_default();
    if cruiser_plan(seen, me) {
        // Diplomacy readies the planets the carrier takes first: the six resources without Trade.
        let next_pick_now = picks_before_next(seen, me) == Some(0);
        let offered = |name: &str| choice.options.iter().any(|o| card_is(seen, &o.id, name));
        match name.as_str() {
            "Trade" if holds("Technology") => return Some(30.0),
            "Diplomacy" if holds("Technology") => return Some(28.0),
            "Technology" if holds("Trade") || holds("Diplomacy") => return Some(30.0),
            "Technology" if trade_survives(choice, seen, me, card) => return Some(30.0),
            "Technology" if next_pick_now && offered("Diplomacy") => return Some(30.0),
            _ => {}
        }
    }
    // Short of transports and of the resources for them: Trade's goods for the home build, with
    // Construction or Warfare when the seat has one (or takes one first when Trade will still be
    // there), else beside Warfare's secondary; and then Diplomacy, to act early.
    if let Some((_, cost)) = transport_plan(seen, me)
        && planet_resources(seen, me) < cost
    {
        let builder = holds("Construction") || holds("Warfare");
        let builder_offered = choice
            .options
            .iter()
            .find(|o| card_is(seen, &o.id, "Construction") || card_is(seen, &o.id, "Warfare"));
        match name.as_str() {
            "Trade" if builder => return Some(25.0),
            "Trade"
                if builder_offered
                    .is_none_or(|taken| !trade_survives(choice, seen, me, &taken.id)) =>
            {
                return Some(9.0);
            }
            "Diplomacy" if holds("Trade") => return Some(7.5),
            _ => {}
        }
    }
    None
}

/// Resources a faction opening keeps for a strategic action still to come: the Cruiser II plan's
/// six for Technology's second research.
fn reserved_resources(seen: &Observed<'_>, me: &PlayerId) -> i64 {
    if cruiser_plan(seen, me) && ready_card(seen, me, "Technology").is_some() {
        SECOND_RESEARCH
    } else {
        0
    }
}

/// Resources free for building: what the seat can spend less what its opening keeps back.
fn spare_resources(seen: &Observed<'_>, me: &PlayerId) -> i64 {
    seen.available_spend(me, Spend::Resources) - reserved_resources(seen, me)
}

/// A faction opening's next strategic action in round 1, before any tactical action: Trade first
/// when the transport plan cannot pay for its home build yet; the Cruiser II plan plays Trade, then
/// Technology once six resources are there.
fn faction_card_play(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    let play = |name: &str| {
        ready_card(seen, me, name).and_then(|card| strategic_option(choice, seen, &card))
    };
    // Short of transports and of the resources to build them: Trade's goods first, before the
    // token-free production at home (unless the home must first be emptied for fleet supply).
    if let Some((_, cost)) = transport_plan(seen, me)
        && ready_card(seen, me, "Trade").is_some()
        && home_builder_ahead(seen, me)
        && spare_resources(seen, me) < cost
        && !loaded_transport_blocks_home(seen, me)
    {
        return play("Trade");
    }
    if short_of_transports(seen, me)
        && let Some((_, cost)) = transport_plan(seen, me)
        && spare_resources(seen, me) < cost
        && (ready_card(seen, me, "Warfare").is_some()
            || ready_card(seen, me, "Construction").is_some())
    {
        // Diplomacy of its own readies the planets just taken: before the build.
        if best_two_exhausted(seen, me) > 0
            && let Some(index) = play("Diplomacy")
        {
            return Some(index);
        }
        // Diplomacy (its own or another seat's) or Trade still to come: the loaded transport
        // leaves first, and the build waits for their resources.
        let more = ready_card(seen, me, "Diplomacy").is_some()
            || ready_card(seen, me, "Trade").is_some()
            || secondary_to_come(seen, me, "Diplomacy");
        let eager_warfare = ground_produces(seen, me) && ready_card(seen, me, "Warfare").is_some();
        if more
            && !eager_warfare
            && seen.seat(me).is_some_and(|seat| seat.tactic_tokens > 1)
            && home_system(seen, me).is_some_and(|home| {
                let (waiting, lift) = waiting_and_lift(seen, me, &home);
                lift > 0 && waiting > 0 && !seen.systems_with_token(me).contains(&home)
            })
        {
            return choice.options.iter().position(|o| o.id == "tactical");
        }
    }
    // The home build cannot pay for the transports yet, and Diplomacy's secondary (readying the
    // planets just taken) is still to come: another card first, to wait for it.
    if let Some((_, cost)) = transport_plan(seen, me)
        && (ready_card(seen, me, "Warfare").is_some()
            || ready_card(seen, me, "Construction").is_some())
        && spare_resources(seen, me) < cost
        && secondary_to_come(seen, me, "Diplomacy")
        && exhausted_resources(seen, me) > 0
        && seen.seat(me).is_some_and(|seat| seat.strategic_tokens > 0)
    {
        for name in ["Leadership", "Politics", "Imperial", "Technology"] {
            if let Some(index) = play(name) {
                return Some(index);
            }
        }
    }
    // No card left to wait with: a tactical action that moves the home warships out instead.
    if wait_move_pays(seen, me) {
        return choice.options.iter().position(|o| o.id == "tactical");
    }
    if cruiser_plan(seen, me) && ready_card(seen, me, "Technology").is_some() {
        if let Some(index) = play("Trade") {
            return Some(index);
        }
        let resources = seen.available_spend(me, Spend::Resources);
        if resources >= SECOND_RESEARCH {
            return play("Technology");
        }
        // Without Trade: Diplomacy readies the planets the carrier took, once that pays for it.
        if resources + best_two_exhausted(seen, me) >= SECOND_RESEARCH
            && let Some(index) = play("Diplomacy")
        {
            return Some(index);
        }
    }
    None
}

/// The fleet pool the bot needs during the action phase of the opening rounds, once its ships are
/// spread out: its biggest stack (and room at home for what a planned build adds), at least one.
/// Every token above that is a tactic token for one more system.
fn fleet_lean(seen: &Observed<'_>, me: &PlayerId) -> i32 {
    let stack = biggest_stack(seen, me);
    let home_need = home_system(seen, me)
        .filter(|_| home_build_wanted(seen, me))
        .map_or(0, |home| ships_and_lift(seen, me, &home).0 + 1);
    i32::try_from(stack.max(home_need).max(1)).unwrap_or(i32::MAX)
}

fn biggest_stack(seen: &Observed<'_>, me: &PlayerId) -> usize {
    seen.board()
        .keys()
        .map(|system| ships_and_lift(seen, me, system).0)
        .max()
        .unwrap_or(0)
}

// -- the questions -------------------------------------------------------------------------------

/// Best activation among the offered systems; avoid losing fights; random among the rest.
fn pick_activation(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    seen.galaxy()?;
    if wait_move_pays(seen, me)
        && let Some(index) = pick_wait_activation(choice, seen, me)
    {
        return Some(index);
    }
    let scored: Vec<(usize, f64)> = choice
        .options
        .iter()
        .enumerate()
        .filter(|(_, o)| o.kind == "activate")
        .map(|(i, o)| (i, activation_value(seen, me, &SystemId::new(&o.id))))
        .collect();
    best_above(&scored, tuning::activation_threshold())
}

/// Highest score at or above `threshold`, first on a tie.
fn best_above(scored: &[(usize, f64)], threshold: f64) -> Option<usize> {
    scored
        .iter()
        .filter(|(_, s)| *s >= threshold)
        .fold(None, |best: Option<(usize, f64)>, &(i, s)| match best {
            Some((_, b)) if b >= s => best,
            _ => Some((i, s)),
        })
        .map(|(i, _)| i)
}

/// Action phase: tactical when a system is worth it (now and then the strategic action instead,
/// so the card is played); otherwise no opinion.
fn pick_action(
    choice: &Choice,
    seen: &Observed<'_>,
    me: &PlayerId,
    rng: &mut ChaCha8Rng,
) -> Option<usize> {
    let galaxy = seen.galaxy()?;
    let tactical = choice.options.iter().position(|o| o.id == "tactical")?;
    let out_of_tokens = seen.seat(me).is_some_and(|seat| seat.tactic_tokens <= 0);
    let play = |name: &str| {
        ready_card(seen, me, name).and_then(|card| strategic_option(choice, seen, &card))
    };
    if tuning::mecatol_imperial_priority()
        && holds_mecatol(seen, me)
        && let Some(index) = play("Imperial")
    {
        return Some(index);
    }
    let tokens = seen.systems_with_token(me);
    let worth = galaxy
        .system_ids()
        .into_iter()
        .map(SystemId::new)
        .filter(|system| !tokens.contains(system))
        .map(|system| activation_value(seen, me, &system))
        .fold(f64::NEG_INFINITY, f64::max);
    // Imperial waits while Mecatol Rex could still be taken: played after landing there, it
    // scores the point.
    let hold_imperial = !holds_mecatol(seen, me) && mecatol_in_reach(seen, me);
    let strategic = choice
        .options
        .iter()
        .enumerate()
        .filter(|(_, o)| o.kind == "action" && o.id.starts_with("strategic"))
        .min_by_key(|(i, o)| {
            let imperial =
                o.id.strip_prefix("strategic|")
                    .is_some_and(|card| card_is(seen, card, "Imperial"));
            (hold_imperial && imperial, *i)
        })
        .map(|(i, _)| i);
    if expanding(seen)
        && let Some(index) = faction_card_play(choice, seen, me)
    {
        return Some(index);
    }
    // A new transport and troops at home without placing a token there (Warfare's free tactical
    // action, Construction's production): the third system.
    // The Arborec's Warfare builds where their troops land instead (see `warfare_builds`).
    let landing_build = short_of_transports(seen, me) && ground_produces(seen, me);
    if home_build_wanted(seen, me) {
        for name in ["Warfare", "Construction"] {
            if name == "Warfare" && landing_build {
                continue;
            }
            if let Some(index) = play(name) {
                return Some(index);
            }
        }
    }
    // Warfare while there is a planet to take (or no tactic token left for it): its tactical
    // action places no token, so the carrier that lands there can move on again this round.
    // Short of transports, Warfare is the home build: the transports leave by tactical actions
    // first, so the build has fleet supply and the free production comes after.
    // (Not when its ground forces produce: the Arborec build the transport where they land.)
    let warfare_builds =
        short_of_transports(seen, me) && !out_of_tokens && !ground_produces(seen, me);
    if expanding(seen)
        && !warfare_builds
        && (worth >= tuning::eager_worth()
            || (out_of_tokens && worth >= tuning::activation_threshold()))
        && let Some(index) = play("Warfare")
    {
        return Some(index);
    }
    if out_of_tokens {
        return None;
    }
    if worth < tuning::activation_threshold() {
        // Nothing worth a command token: play the card or pass rather than waste one. But not
        // pass while Warfare's secondary would still build the transport its tokens could send:
        // a token on an empty system keeps the seat in the round.
        return strategic.or_else(|| choice.options.iter().position(|o| o.id == "pass"));
    }
    // A planet worth having is there for the taking: always go (the card waits for a quieter
    // turn). Otherwise now and then play the card first.
    match strategic {
        Some(index) if worth < tuning::eager_worth() && rng.random_bool(0.25) => Some(index),
        _ => Some(tactical),
    }
}

/// The action-phase option that plays `card`: `strategic|card`, or the bare `strategic` while it
/// is the only card left to play.
fn strategic_option(choice: &Choice, seen: &Observed<'_>, card: &str) -> Option<usize> {
    choice
        .options
        .iter()
        .position(|o| o.kind == "action" && o.id == format!("strategic|{card}"))
        .or_else(|| {
            let bare = choice
                .options
                .iter()
                .position(|o| o.kind == "action" && o.id == "strategic")?;
            let me = &choice.player;
            let seat = seen.seat(me)?;
            let unplayed: Vec<_> = seat
                .strategy_cards
                .iter()
                .filter(|c| !seat.exhausted_strategy_cards.contains(*c))
                .collect();
            (unplayed.len() == 1 && unplayed[0].as_str() == card).then_some(bare)
        })
}

/// Warfare's free tactical action: home, to build a transport there without a token, when the
/// opening plan wants one; otherwise the system most worth activating (one with its own token
/// too, as no token is placed).
fn pick_free_tactical(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    let landing_build = short_of_transports(seen, me) && ground_produces(seen, me);
    if !landing_build
        && home_build_wanted(seen, me)
        && let Some(home) = home_system(seen, me)
        && let Some(index) = choice.options.iter().position(|o| o.id == home.as_str())
    {
        return Some(index);
    }
    let home = home_system(seen, me);
    let scored: Vec<(usize, f64)> = choice
        .options
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let system = SystemId::new(&o.id);
            let mut value = activation_value(seen, me, &system);
            // No token is placed, so building at home costs nothing but resources.
            if home.as_ref() == Some(&system)
                && spare_resources(seen, me) >= CARRIER_COST
                && wants_to_build(seen, me, &system)
            {
                value = value.max(2.0);
            }
            (i, value)
        })
        .collect();
    best_above(&scored, tuning::activation_threshold())
}

/// Construction's primary: a production at home when the opening plan wants a new transport there,
/// else a structure.
fn pick_construction(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    let structure = choice.options.iter().position(|o| o.id == "structure");
    if home_build_wanted(seen, me)
        && let Some(home) = home_system(seen, me)
        && let Some(index) = choice
            .options
            .iter()
            .position(|o| o.id == format!("produce|{home}"))
    {
        return Some(index);
    }
    structure
}

/// Research: Gravity Drive whenever it is on offer (one more hex for a ship each activation);
/// otherwise no opinion.
fn pick_research(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    let offered = |id: &str| choice.options.iter().position(|o| o.id == id);
    // The Cruiser II plan: Cruiser II as soon as it is open, before it the missing colour (a
    // technology of that colour without prerequisites).
    if own_units(seen, me, "cruiser") >= 2 && seen.round() <= 1 {
        if let Some(index) = offered(CRUISER_II) {
            return Some(index);
        }
        if let [colour] = missing_colours(seen, me, CRUISER_II)[..]
            && let Some(index) = choice.options.iter().position(|o| {
                tech_colour(seen, &o.id) == Some(colour)
                    && missing_colours(seen, me, &o.id).is_empty()
                    && seen
                        .content()
                        .get(ti4_model::content_types::ContentType::Technologies, &o.id)
                        .is_some_and(|r| {
                            !r.text("requirements")
                                .unwrap_or("")
                                .chars()
                                .any(|c| "GYRB".contains(c))
                        })
            })
        {
            return Some(index);
        }
    }
    offered(GRAVITY_DRIVE)
}

fn payload_str<'o>(option: &'o ChoiceOption, key: &str) -> Option<&'o str> {
    option.payload.get(key).and_then(Value::as_str)
}

fn payload_f64(option: &ChoiceOption, key: &str) -> Option<f64> {
    option.payload.get(key).and_then(Value::as_f64)
}

/// Ground forces the active system's planets still need beyond what is already in its space.
fn ground_shortfall(seen: &Observed<'_>, me: &PlayerId, system: &SystemId, extra: usize) -> usize {
    let carried = seen
        .system(system)
        .units
        .iter()
        .filter(|u| &u.owner == me && is_ground(seen, u))
        .count()
        + extra;
    // Only the planets worth having count; an unreachable Mecatol is already filtered out.
    let wanted = targets(seen, me, system);
    let mut need: usize = wanted.iter().map(|(_, _, n)| n).sum();
    if expanding(seen) && wanted.len() > 1 {
        // Troops are scarce in the opening: the best planet first, the system's others only with
        // a troop left over for each other transport, which can take a system of its own.
        let (transports, ground) = free_transports_and_ground(seen, me);
        let mut others = (transports - 1).max(0);
        // Round 1 with a home build still to come: a troop stays behind for each transport the
        // remaining tactic tokens will send, built ones included.
        if seen.round() <= 1 && home_builder_ahead(seen, me) {
            let tokens = seen
                .seat(me)
                .map_or(0, |seat| i64::from(seat.tactic_tokens.min(2)));
            others = others.max(tokens);
        }
        let mut spare = ground - others;
        need = 0;
        for (i, (_, _, n)) in wanted.iter().enumerate() {
            let n = i64::try_from(*n).unwrap_or(i64::MAX);
            if i == 0 || spare >= n {
                spare -= n;
                need += usize::try_from(n).unwrap_or(0);
            }
        }
    }
    need.saturating_sub(carried)
}

/// Movement: reinforce a fight until it is clearly won, bring a carrier while planets need ground
/// forces, then finish. Never into a fight that cannot be won.
fn pick_move(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    let active = seen.active_system()?.clone();
    let done = choice.options.iter().position(ChoiceOption::is_decline)?;
    let moves: Vec<(usize, &ChoiceOption, UnitType<'_>)> = choice
        .options
        .iter()
        .enumerate()
        .filter(|(_, o)| o.kind == "move")
        .filter_map(|(i, o)| Some((i, o, unit_type(seen, payload_str(o, "unit")?)?)))
        .collect();
    let board = seen.system(&active);
    let present: Vec<&Unit> = board
        .units
        .iter()
        .filter(|u| &u.owner == me && is_ship(seen, u))
        .collect();

    if has_hostile_ships(seen, me, &active) {
        let enemy = hostile_space(seen, me, &active);
        let now = fleet(seen, me, present.iter().copied());
        if matchup(&now, &enemy) >= fight_margin(seen, &active) + 0.3 {
            // Already winning: only a carrier for the invasion is still useful.
            return carrier_for_ground(&moves, seen, me, &active).or(Some(done));
        }
        let everything = fleet(
            seen,
            me,
            present
                .iter()
                .copied()
                .chain(seen.movable_into(me, &active).iter().map(|m| &m.unit)),
        );
        if matchup(&everything, &enemy) < fight_margin(seen, &active) {
            return Some(done); // a losing fight: stay out
        }
        // Strongest warship first.
        return moves
            .iter()
            .filter(|(_, _, t)| t.combat_dice() > 0)
            .max_by(|a, b| {
                strength(a.2)
                    .total_cmp(&strength(b.2))
                    .then_with(|| b.0.cmp(&a.0))
            })
            .map(|(i, _, _)| *i)
            .or(Some(done));
    }

    if has_own_production(seen, me, &active) && targets(seen, me, &active).is_empty() {
        // A build activation: keep the fleet where it is, but fetch an empty transport for the
        // ground forces waiting here.
        return empty_transport_home(&moves, seen, me, &active).or(Some(done));
    }
    if let Some(index) = carrier_for_ground(&moves, seen, me, &active) {
        return Some(index);
    }
    if let Some(index) = escort_from_home(&moves, seen, me, &active) {
        return Some(index);
    }
    if let Some(index) = waiting_escort(&moves, seen, me, &active) {
        return Some(index);
    }
    if !targets(seen, me, &active).is_empty() || !present.is_empty() {
        return Some(done);
    }
    None
}

/// The matchup a fight in `system` must reach: [`tuning::win_margin`], more in a nebula, where the
/// defender rolls one better (bot version 2, [`tuning::nebula_margin`]).
fn fight_margin(seen: &Observed<'_>, system: &SystemId) -> f64 {
    let nebula = tuning::nebula_margin() > 0.0
        && ti4_content::galaxy::system(seen.content(), system.as_str(), seen.sources())
            .is_some_and(|s| s.is_nebula());
    // Bot version 3: a bigger edge for a fight in the first two rounds (analysis: bad seats lose
    // the fleet in an early fight and never catch up).
    let early = if seen.round() <= 2 {
        tuning::early_margin()
    } else {
        0.0
    };
    tuning::win_margin() + early + if nebula { tuning::nebula_margin() } else { 0.0 }
}

/// Bot version 2 ([`tuning::home_guard`]): the move options to keep when a ship leaving its home
/// system would leave it to a fleet a rival could bring there now ([`Observed::movable_into`],
/// which follows the map's anomalies, wormholes and hyperlanes); `None` when nothing is held back.
fn home_guard_keep(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<Vec<usize>> {
    if !(tuning::home_guard() || tuning::home_last_ship()) || seen.round() < 2 {
        return None;
    }
    let home = home_system(seen, me)?;
    let active = seen.active_system()?;
    if active == &home {
        return None;
    }
    let leaving = |o: &ChoiceOption| {
        o.kind == "move"
            && payload_str(o, "origin") == Some(home.as_str())
            && payload_str(o, "unit")
                .and_then(|u| unit_type(seen, u))
                .is_some_and(|t| t.is_ship() && !t.is_fighter())
    };
    if !choice.options.iter().any(leaving) {
        return None;
    }
    let threat = seen
        .players()
        .into_iter()
        .filter(|p| *p != me)
        .map(|rival| {
            let movable = seen.movable_into(rival, &home);
            fleet(
                seen,
                rival,
                movable.iter().map(|m| &m.unit).filter(|u| is_ship(seen, u)),
            )
        })
        .max_by(|a, b| a.firepower.total_cmp(&b.firepower))?;
    if threat.firepower <= 0.0 {
        return None;
    }
    let board = seen.system(&home);
    let ships: Vec<&Unit> = board
        .units
        .iter()
        .filter(|u| &u.owner == me && is_ship(seen, u))
        .collect();
    // Bot version 3 (narrow guard): only the last warship stays, and only from round 3.
    if !tuning::home_guard() {
        let warships = ships
            .iter()
            .filter(|u| unit_type(seen, u.type_id.as_str()).is_some_and(|t| !t.is_fighter()))
            .count();
        if warships > 1 || seen.round() < 3 {
            return None;
        }
        let keep: Vec<usize> = choice
            .options
            .iter()
            .enumerate()
            .filter(|(_, o)| !leaving(o))
            .map(|(i, _)| i)
            .collect();
        return (keep.len() < choice.options.len()).then_some(keep);
    }
    let mut guard = fleet(seen, me, ships.iter().copied());
    guard.cannon += hostile_fleet_cannon(seen, me, &home);
    // Leaving one warship: does the home still hold?
    let without = |unit: &str| {
        let mut rest: Vec<&Unit> = ships.clone();
        if let Some(pos) = rest.iter().position(|u| u.type_id.as_str() == unit) {
            rest.remove(pos);
        }
        let mut d = fleet(seen, me, rest);
        d.cannon = guard.cannon;
        matchup(&d, &threat)
    };
    if matchup(&guard, &threat) < 0.0 {
        // Already too weak to hold: staying does not save it.
        return None;
    }
    let keep: Vec<usize> = choice
        .options
        .iter()
        .enumerate()
        .filter(|(_, o)| !leaving(o) || payload_str(o, "unit").is_some_and(|u| without(u) >= 0.0))
        .map(|(i, _)| i)
        .collect();
    (keep.len() < choice.options.len()).then_some(keep)
}

/// Bot version 5 candidate ([`tuning::mecatol_guard`]): the warships on Mecatol Rex that may
/// leave, by unit type and count; `None` when nothing is held back (knob off, planet not held,
/// no rival fleet in reach, or the ships there would not deter it anyway, in which case staying
/// would not save the planet).
///
/// The garrison is the smallest set of ships (with the seat's space cannon on the planet) that
/// still has `matchup >= -win_margin` against the strongest fleet a rival can bring now, the
/// matchup a rule-bot attacker needs. Ships are given up one at a time, always the one whose
/// leaving costs the least matchup, while the rest still deters; those are the surplus that may
/// move out for tactical actions. Fighters always stay. Feedback fb_1792225201_7d70d5bd: the
/// Muaat warsun, the only ship on Mecatol Rex, left it for Thibah and a rival took the planet.
fn mecatol_surplus(seen: &Observed<'_>, me: &PlayerId) -> Option<BTreeMap<String, usize>> {
    if !tuning::mecatol_guard() || seen.round() < 2 || !holds_mecatol(seen, me) {
        return None;
    }
    let mecatol = mecatol_system(seen)?;
    let board = seen.system(&mecatol);
    let ships: Vec<&Unit> = board
        .units
        .iter()
        .filter(|u| &u.owner == me && is_ship(seen, u))
        .collect();
    let removable = |u: &&Unit| {
        unit_type(seen, u.type_id.as_str()).is_some_and(|t| t.is_ship() && !t.is_fighter())
    };
    if !ships.iter().any(removable) {
        return None;
    }
    let threat = seen
        .players()
        .into_iter()
        .filter(|p| *p != me)
        .map(|rival| {
            let movable = seen.movable_into(rival, &mecatol);
            fleet(
                seen,
                rival,
                movable.iter().map(|m| &m.unit).filter(|u| is_ship(seen, u)),
            )
        })
        .max_by(|a, b| a.firepower.total_cmp(&b.firepower))
        .filter(|t| t.firepower > 0.0)?;
    let cannon = hostile_fleet_cannon(seen, me, &mecatol);
    let value = |set: &[&Unit]| {
        let mut guard = fleet(seen, me, set.iter().copied());
        guard.cannon += cannon;
        matchup(&guard, &threat)
    };
    let floor = -tuning::win_margin();
    let mut stay = ships.clone();
    if value(&stay) < floor {
        return None;
    }
    let mut surplus: BTreeMap<String, usize> = BTreeMap::new();
    loop {
        let best = (0..stay.len())
            .filter(|&i| removable(&stay[i]))
            .map(|i| {
                let mut rest = stay.clone();
                rest.remove(i);
                (i, value(&rest))
            })
            .filter(|(_, v)| *v >= floor)
            .max_by(|a, b| a.1.total_cmp(&b.1));
        let Some((i, _)) = best else { break };
        let gone = stay.remove(i);
        *surplus.entry(gone.type_id.as_str().to_owned()).or_default() += 1;
    }
    Some(surplus)
}

/// The ships that could move into `system` ([`Observed::movable_into`]), less the Mecatol Rex
/// garrison ([`mecatol_surplus`]).
pub(crate) fn movable_for(seen: &Observed<'_>, me: &PlayerId, system: &SystemId) -> Vec<Movable> {
    let movable = seen.movable_into(me, system);
    if !tuning::mecatol_guard() {
        return movable;
    }
    let Some(mecatol) = mecatol_system(seen) else {
        return movable;
    };
    if system == &mecatol || !movable.iter().any(|m| m.origin == mecatol) {
        return movable;
    }
    let Some(mut surplus) = mecatol_surplus(seen, me) else {
        return movable;
    };
    movable
        .into_iter()
        .filter(|m| {
            if !garrison_ship(seen, &mecatol, &m.origin, m.unit.type_id.as_str()) {
                return true;
            }
            take_one(&mut surplus, m.unit.type_id.as_str())
        })
        .collect()
}

/// Whether a unit of `me` at `origin` is a warship (not a fighter) of the Mecatol Rex system.
fn garrison_ship(seen: &Observed<'_>, mecatol: &SystemId, origin: &SystemId, unit: &str) -> bool {
    origin == mecatol && unit_type(seen, unit).is_some_and(|t| t.is_ship() && !t.is_fighter())
}

/// Use up one unit of `kind` from `surplus`; false when none is left.
fn take_one(surplus: &mut BTreeMap<String, usize>, kind: &str) -> bool {
    match surplus.get_mut(kind) {
        Some(n) if *n > 0 => {
            *n -= 1;
            true
        }
        _ => false,
    }
}

/// Bot version 5 candidate ([`tuning::mecatol_guard`]): the move options that do not take more
/// ships from Mecatol Rex than [`mecatol_surplus`] allows; `None` when none is dropped.
fn mecatol_guard_keep(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<Vec<usize>> {
    if !tuning::mecatol_guard() {
        return None;
    }
    let mecatol = mecatol_system(seen)?;
    if seen.active_system()? == &mecatol {
        return None;
    }
    let mut surplus = mecatol_surplus(seen, me)?;
    let keep: Vec<usize> = choice
        .options
        .iter()
        .enumerate()
        .filter(|(_, o)| {
            if o.kind != "move" || payload_str(o, "origin") != Some(mecatol.as_str()) {
                return true;
            }
            match payload_str(o, "unit") {
                Some(u) if garrison_ship(seen, &mecatol, &mecatol, u) => take_one(&mut surplus, u),
                _ => true,
            }
        })
        .map(|(i, _)| i)
        .collect();
    (keep.len() < choice.options.len()).then_some(keep)
}

/// Bot version 2 ([`tuning::rift_caution`]): outside a fight, the move options that do not
/// leave a gravity rift (41.2: a ship rolls for each rift it exits, and dies on 1-3 with its
/// cargo; the engine marks such a move's preview as unresolved); `None` when none is dropped.
fn rift_keep(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<Vec<usize>> {
    if !tuning::rift_caution() {
        return None;
    }
    let active = seen.active_system()?;
    if has_hostile_ships(seen, me, active) {
        return None;
    }
    let risky = |o: &ChoiceOption| {
        o.kind == "move"
            && matches!(
                o.preview.as_ref().map(|p| &p.outcome),
                Some(ti4_engine::preview::Outcome::Unknown { reason }) if reason.contains("gravity-rift")
            )
    };
    let keep: Vec<usize> = choice
        .options
        .iter()
        .enumerate()
        .filter(|(_, o)| !risky(o))
        .map(|(i, _)| i)
        .collect();
    (keep.len() < choice.options.len() && !keep.is_empty()).then_some(keep)
}

/// Space cannon of the seat's own units on the planets of `system`.
fn hostile_fleet_cannon(seen: &Observed<'_>, me: &PlayerId, system: &SystemId) -> f64 {
    let board = seen.system(system);
    fleet(
        seen,
        me,
        board
            .planet_units
            .values()
            .flatten()
            .filter(|u| &u.owner == me),
    )
    .cannon
}

fn strength(kind: UnitType<'_>) -> f64 {
    #[expect(clippy::cast_precision_loss, reason = "dice counts are small")]
    let dice = kind.combat_dice() as f64;
    let hits = kind
        .combat_hits_on()
        .map_or(0.0, |on| ti4_engine::fleet_strength::hit_chance(on, 0));
    dice * hits * (1.0 + f64::from(u8::from(kind.sustain_damage())))
}

/// A ship with capacity whose origin has ground forces, while the active system needs more.
fn carrier_for_ground(
    moves: &[(usize, &ChoiceOption, UnitType<'_>)],
    seen: &Observed<'_>,
    me: &PlayerId,
    active: &SystemId,
) -> Option<usize> {
    if ground_shortfall(seen, me, active, 0) == 0 {
        return None;
    }
    // In the opening each transport takes a system of its own: one that already brought troops
    // here is enough.
    if expanding(seen)
        && seen
            .system(active)
            .units
            .iter()
            .any(|u| &u.owner == me && is_ground(seen, u))
    {
        return None;
    }
    moves
        .iter()
        .filter(|(_, o, t)| {
            t.capacity() > 0
                && t.is_ship()
                && payload_str(o, "origin").is_some_and(|origin| {
                    seen.loadable(me, &SystemId::new(origin))
                        .iter()
                        .any(|cargo| is_ground(seen, &cargo.unit))
                })
        })
        .max_by_key(|(i, _, t)| (t.capacity(), std::cmp::Reverse(*i)))
        .map(|(i, _, _)| *i)
}

/// A warship without capacity leaving home beside the transport, while home is full for fleet
/// supply and the transport plan still wants new transports built there: each one that leaves
/// is room for one more.
fn escort_from_home(
    moves: &[(usize, &ChoiceOption, UnitType<'_>)],
    seen: &Observed<'_>,
    me: &PlayerId,
    active: &SystemId,
) -> Option<usize> {
    let home = home_system(seen, me)?;
    let (missing, _) = transport_plan(seen, me)?;
    // Warfare's own build: the warships leave by a move of their own later
    // ([`wait_move_pays`]), so this transport's system keeps a small fleet.
    if ready_card(seen, me, "Warfare").is_some()
        && !ground_produces(seen, me)
        && !gate_bound(seen, me)
    {
        return None;
    }
    if !home_builder_ahead(seen, me)
        || home_fleet_room(seen, me, &home) >= missing
        || home_fleet_room(seen, me, active) <= 0
    {
        return None;
    }
    let (_, lift) = ships_and_lift(seen, me, active);
    if lift == 0 {
        return None;
    }
    moves
        .iter()
        .filter(|(_, o, t)| {
            t.is_ship()
                && !t.is_fighter()
                && t.capacity() == 0
                && payload_str(o, "origin") == Some(home.as_str())
        })
        .min_by_key(|(i, _, _)| *i)
        .map(|(i, _, _)| *i)
}

/// The waiting move ([`wait_move_pays`]) in round 1: into a system no transport comes to, the
/// home warships without capacity go, so the home build has fleet supply.
fn waiting_escort(
    moves: &[(usize, &ChoiceOption, UnitType<'_>)],
    seen: &Observed<'_>,
    me: &PlayerId,
    active: &SystemId,
) -> Option<usize> {
    let home = home_system(seen, me)?;
    if seen.round() > 1
        || !short_of_transports(seen, me)
        || ships_and_lift(seen, me, active).1 > 0
        || ships_and_lift(seen, me, &home).1 > 0
        || moves
            .iter()
            .any(|(_, _, t)| t.capacity() > 0 && t.is_ship())
        || active == &home
        || has_hostile_ships(seen, me, active)
        || home_fleet_room(seen, me, active) <= 0
    {
        return None;
    }
    moves
        .iter()
        .filter(|(_, o, t)| {
            t.is_ship()
                && !t.is_fighter()
                && t.capacity() == 0
                && payload_str(o, "origin") == Some(home.as_str())
        })
        .min_by_key(|(i, _, _)| *i)
        .map(|(i, _, _)| *i)
}

/// Own ground forces waiting on the planets of `system`, and own capacity in its space area.
fn waiting_and_lift(seen: &Observed<'_>, me: &PlayerId, system: &SystemId) -> (usize, i64) {
    let board = seen.system(system);
    let waiting = board
        .planet_units
        .values()
        .flatten()
        .filter(|u| &u.owner == me && is_ground(seen, u))
        .count();
    let lift = board
        .units
        .iter()
        .filter(|u| &u.owner == me)
        .filter_map(|u| unit_type(seen, u.type_id.as_str()))
        .map(|t| t.capacity())
        .sum();
    (waiting, lift)
}

/// Whether ground forces wait at `system` with no ship there to carry them.
fn needs_transport(seen: &Observed<'_>, me: &PlayerId, system: &SystemId) -> bool {
    let (waiting, lift) = waiting_and_lift(seen, me, system);
    waiting >= 2 && lift == 0
}

/// A ship with capacity and nothing to load where it stands, sent to a dock where ground forces
/// wait without a ride.
fn empty_transport_home(
    moves: &[(usize, &ChoiceOption, UnitType<'_>)],
    seen: &Observed<'_>,
    me: &PlayerId,
    active: &SystemId,
) -> Option<usize> {
    let (waiting, lift) = waiting_and_lift(seen, me, active);
    if waiting < 2 || lift >= i64::try_from(waiting).unwrap_or(i64::MAX) {
        return None;
    }
    moves
        .iter()
        .filter(|(_, o, t)| {
            t.capacity() > 0
                && payload_str(o, "origin").is_some_and(|origin| {
                    !seen
                        .loadable(me, &SystemId::new(origin))
                        .iter()
                        .any(|cargo| is_ground(seen, &cargo.unit))
                })
        })
        .max_by_key(|(i, _, t)| (t.capacity(), std::cmp::Reverse(*i)))
        .map(|(i, _, _)| *i)
}

/// Loading: ground forces while the target needs them, fighters for a fight, else stop.
fn pick_load(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    let active = seen.active_system()?.clone();
    let done = choice.options.iter().position(ChoiceOption::is_decline)?;
    let loaded = choice
        .options
        .iter()
        .find_map(|o| payload_f64(o, "loaded_ground"))
        .unwrap_or(0.0);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "small count"
    )]
    let loaded = loaded.max(0.0) as usize;
    let load_of = |ground: bool| {
        choice.options.iter().position(|o| {
            o.kind == "load"
                && payload_str(o, "unit")
                    .and_then(|id| unit_type(seen, id))
                    .is_some_and(|t| {
                        if ground {
                            t.is_ground_force()
                        } else {
                            t.is_fighter()
                        }
                    })
        })
    };
    // An activation that placed no token (Warfare) leaves the carrier free to move on this round:
    // bring two more ground forces for the next system.
    // Not at the cost of the other transports, which want a troop each.
    let short = ground_shortfall(seen, me, &active, 0);
    let extra = if expanding(seen) && !seen.systems_with_token(me).contains(&active) {
        let (transports, ground) = free_transports_and_ground(seen, me);
        let spare = ground - i64::try_from(short).unwrap_or(i64::MAX) - (transports - 1).max(0);
        usize::try_from(spare.clamp(0, 2)).unwrap_or(0)
    } else {
        0
    };
    if short > 0
        && short + extra > loaded
        && let Some(index) = load_of(true)
    {
        return Some(index);
    }
    if has_hostile_ships(seen, me, &active)
        && let Some(index) = load_of(false)
    {
        return Some(index);
    }
    Some(done)
}

/// Landing: the planet still short of a winning force with the best worth per troop it still
/// needs, never one the troops in space cannot win; stop when every planet is covered.
fn pick_commit(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    let done = choice.options.iter().position(ChoiceOption::is_decline)?;
    let system = seen
        .active_system()
        .cloned()
        .or_else(|| seen.invasion().map(|invasion| invasion.system))?;
    // Identical units are offered once, so count the troops from the options and from space.
    let troops: BTreeSet<&str> = choice
        .options
        .iter()
        .filter(|o| o.kind == "commit")
        .filter_map(|o| o.id.split('|').nth(1))
        .collect();
    let board = seen.system(&system);
    let in_space = board
        .units
        .iter()
        .filter(|u| &u.owner == me && is_ground(seen, u))
        .count();
    let available = troops.len().max(in_space);
    let mut best: Option<(usize, f64)> = None;
    for (index, option) in choice.options.iter().enumerate() {
        if option.kind != "commit" {
            continue;
        }
        let Some(planet) = payload_str(option, "planet") else {
            continue;
        };
        let planet = PlanetId::new(planet);
        let mine = board
            .planet_units
            .get(&planet)
            .into_iter()
            .flatten()
            .filter(|u| &u.owner == me && is_ground(seen, u))
            .count();
        let need = ground_needed(seen, me, &system, &planet);
        let held = board.planet_control.get(&planet) == Some(me) && need == 1;
        if held || mine >= need || mine + available < need {
            continue;
        }
        // Value per troop still to land: every free planet gets one before a defended one.
        #[expect(clippy::cast_precision_loss, reason = "small counts")]
        let worth = planet_worth(seen, planet.as_str()) / (need - mine) as f64;
        if best.is_none_or(|(_, b)| worth > b) {
            best = Some((index, worth));
        }
    }
    if best.is_none() && tuning::land_all() {
        return Some(land_leftover(choice, seen, me, &system).unwrap_or(done));
    }
    Some(best.map_or(done, |(index, _)| index))
}

/// Knob [`tuning::land_all`]: the ground forces that [`pick_commit`]'s winning-force logic leaves
/// aboard land anyway, on a planet that cannot cost them a fight: one the seat holds or already
/// has troops on, else one without defending ground forces. The worthier planet first, a planet
/// not yet held slightly ahead. `None` when every planet is defended (the troops stay aboard).
fn land_leftover(
    choice: &Choice,
    seen: &Observed<'_>,
    me: &PlayerId,
    system: &SystemId,
) -> Option<usize> {
    let board = seen.system(system);
    let mut best: Option<(usize, f64)> = None;
    for (index, option) in choice.options.iter().enumerate() {
        if option.kind != "commit" {
            continue;
        }
        let Some(planet) = payload_str(option, "planet") else {
            continue;
        };
        let planet = PlanetId::new(planet);
        let units = board.planet_units.get(&planet).into_iter().flatten();
        let mine = units
            .clone()
            .filter(|u| &u.owner == me && is_ground(seen, u))
            .count();
        let defended = units.clone().any(|u| &u.owner != me && is_ground(seen, u));
        let held = board.planet_control.get(&planet) == Some(me);
        if defended && mine == 0 && !held {
            continue;
        }
        let worth = planet_worth(seen, planet.as_str()) + if held { 0.0 } else { 0.25 };
        if best.is_none_or(|(_, b)| worth > b) {
            best = Some((index, worth));
        }
    }
    best.map(|(index, _)| index)
}

/// Retreat when clearly losing the space combat, else stay.
fn pick_retreat(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    let system = choice
        .options
        .iter()
        .find_map(|o| payload_str(o, "system"))
        .map(SystemId::new)?;
    let stay = choice.options.iter().position(|o| o.id == "stay")?;
    let retreat = choice.options.iter().position(|o| o.id == "retreat")?;
    let board = seen.system(&system);
    let own = fleet(
        seen,
        me,
        board
            .units
            .iter()
            .filter(|u| &u.owner == me && is_ship(seen, u)),
    );
    let enemy = hostile_space(seen, me, &system);
    Some(if matchup(&own, &enemy) < tuning::retreat_margin() {
        retreat
    } else {
        stay
    })
}

/// Ground forces waiting in systems with own production (where new ones appear), and the
/// capacity of every own ship that could carry them away.
fn idle_ground_and_lift(seen: &Observed<'_>, me: &PlayerId) -> (usize, i64) {
    if tuning::ground_rule() {
        return idle_ground_and_free_lift(seen, me);
    }
    let (mut idle, mut lift) = (0, 0);
    for (id, system) in seen.board() {
        let at_dock = has_own_production(seen, me, id);
        for unit in system
            .units
            .iter()
            .chain(system.planet_units.values().flatten())
        {
            if &unit.owner != me {
                continue;
            }
            let Some(kind) = unit_type(seen, unit.type_id.as_str()) else {
                continue;
            };
            if kind.is_ship() {
                lift += kind.capacity();
            } else if at_dock && kind.is_ground_force() {
                idle += 1;
            }
        }
    }
    (idle, lift)
}

/// Bot version 2 ([`tuning::ground_rule`]): ground forces at its docks, and the room its ships
/// still have (capacity less the fighters and ground forces already in their systems' space), so
/// the docks do not keep filling with troops that no free ship can carry (fb_1791902555,
/// fb_1792038973).
fn idle_ground_and_free_lift(seen: &Observed<'_>, me: &PlayerId) -> (usize, i64) {
    let (mut idle, mut lift) = (0usize, 0i64);
    for (id, system) in seen.board() {
        let at_dock = has_own_production(seen, me, id);
        let (mut room, mut used) = (0i64, 0i64);
        for unit in &system.units {
            if &unit.owner != me {
                continue;
            }
            let Some(kind) = unit_type(seen, unit.type_id.as_str()) else {
                continue;
            };
            if kind.is_ship() && !kind.is_fighter() {
                room += kind.capacity();
            } else if kind.is_fighter() || kind.is_ground_force() {
                used += 1;
            }
        }
        lift += (room - used).max(0);
        if at_dock {
            idle += system
                .planet_units
                .values()
                .flatten()
                .filter(|u| &u.owner == me && is_ground(seen, u))
                .count();
        }
    }
    (idle, lift)
}

/// Round-1 production while short of transports (see [`pick_produce`]): declined where a command
/// token keeps the new units this round while a home build is still to come, otherwise a
/// transport whenever its troop is there or affordable with it, then troops for every transport.
fn opening_production(
    choice: &Choice,
    seen: &Observed<'_>,
    me: &PlayerId,
    free_transports: i64,
    free_ground: i64,
) -> Option<usize> {
    // Round 1, short of transports: nothing where its command token keeps the new units this
    // round (the Arborec's troops produce where they land) while a home build is still to come.
    let producing_in = choice
        .options
        .iter()
        .find_map(|o| payload_str(o, "system"))
        .map(SystemId::new);
    if seen.round() <= 1
        && transport_plan(seen, me).is_some()
        && home_builder_ahead(seen, me)
        && producing_in.is_some_and(|system| seen.systems_with_token(me).contains(&system))
    {
        return choice.options.iter().position(ChoiceOption::is_decline);
    }
    // Round 1, short of transports: a transport whenever its troop is there or affordable with
    // it, then troops for every transport.
    if seen.round() <= 1 && expanding(seen) && free_transports < 3 {
        let option_of = |want: &dyn Fn(UnitType<'_>) -> bool| {
            choice.options.iter().position(|o| {
                o.kind == "produce"
                    && payload_str(o, "unit")
                        .and_then(|id| unit_type(seen, id))
                        .is_some_and(want)
                    && payload_f64(o, "fleet_excess_after").unwrap_or(0.0) <= 0.0
                    && payload_f64(o, "owed").unwrap_or(0.0)
                        <= payload_f64(o, "available_resources").unwrap_or(f64::MAX)
            })
        };
        let money = choice
            .options
            .iter()
            .find_map(|o| payload_f64(o, "available_resources"))
            .unwrap_or(0.0);
        let troop_short = free_ground <= free_transports;
        #[expect(clippy::cast_precision_loss, reason = "small cost")]
        let carrier_and_troops = (CARRIER_COST + 1) as f64;
        let transport =
            option_of(&|k| k.is_ship() && !k.is_fighter() && k.capacity() >= 2 && k.cost() <= 4.0);
        let troops = option_of(&|k| k.is_ground_force() && k.cost() <= 1.0);
        if let Some(index) = transport
            && (!troop_short || money >= carrier_and_troops)
        {
            return Some(index);
        }
        if troop_short && let Some(index) = troops {
            return Some(index);
        }
    }
    None
}

/// Production: the most useful affordable unit per resource (per production slot when resources
/// are plentiful), within fleet supply and capacity. Ground forces only while fewer are waiting
/// at its docks than its ships can carry; when nothing useful is left, stop.
#[cfg(test)]
fn pick_produce(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    pick_produce_biased(choice, seen, me, &planner::Bias::default())
}

/// [`pick_produce`] with the objective planner's unit weights ([`planner::Bias`]).
fn pick_produce_biased(
    choice: &Choice,
    seen: &Observed<'_>,
    me: &PlayerId,
    bias: &planner::Bias,
) -> Option<usize> {
    let (ships, ground) = own_counts(seen, me);
    let enough_ground = enough_ground(seen, me);
    let carriers = seen
        .board()
        .values()
        .flat_map(|s| s.units.iter())
        .filter(|u| &u.owner == me)
        .filter(|u| unit_type(seen, u.type_id.as_str()).is_some_and(|t| t.capacity() >= 4))
        .count();
    let mut scored = Vec::new();
    // Ground forces that only fill the production limit, cheapest first.
    let mut filler: Vec<(usize, f64)> = Vec::new();
    let (idle, lift) = idle_ground_and_lift(seen, me);
    let ground_glut = if tuning::ground_rule() {
        idle >= usize::try_from(lift.max(tuning::min_idle_ground()) + 2).unwrap_or(0)
    } else {
        idle >= usize::try_from(2 * lift.max(tuning::min_idle_ground())).unwrap_or(0)
    };
    // The opening: transports and troops free to move this round (outside activated systems).
    // Troops first while there are fewer than two per transport, then another transport while
    // there are fewer than three.
    let (free_transports, free_ground) = free_transports_and_ground(seen, me);
    let need_ground = free_ground < 2 * free_transports.max(1);
    let need_transport = free_transports < 3 && !need_ground;
    if let Some(index) = opening_production(choice, seen, me, free_transports, free_ground) {
        return Some(index);
    }
    for (index, option) in choice.options.iter().enumerate() {
        if option.kind != "produce" {
            continue;
        }
        let Some(kind) = payload_str(option, "unit").and_then(|id| unit_type(seen, id)) else {
            continue;
        };
        let cost = payload_f64(option, "owed")
            .or_else(|| payload_f64(option, "cost"))
            .unwrap_or_else(|| kind.cost());
        if payload_f64(option, "available_resources").is_some_and(|available| cost > available) {
            continue;
        }
        if payload_f64(option, "fleet_excess_after").unwrap_or(0.0) > 0.0
            || payload_f64(option, "capacity_excess_after").unwrap_or(0.0) > 0.0
        {
            continue;
        }
        let count = payload_f64(option, "count").unwrap_or(1.0);
        if kind.is_ground_force() && enough_ground {
            // Only to fill the dock's production limit when nothing better is left, and never
            // into a heap no ship could ever lift.
            if !ground_glut {
                filler.push((index, -cost));
            }
            continue;
        }
        let mut worth =
            count * ti4_engine::valuation::unit_value(seen.content(), seen.sources(), kind.id());
        if kind.is_ground_force() && ground < 6 {
            worth *= 1.6;
        }
        if kind.capacity() >= 4 && carriers < 2 {
            worth *= 2.0;
        }
        // In the opening a dock whose ground forces outnumber the lift there builds a transport.
        // The opening's needs come first: troops, then a transport.
        if kind.capacity() >= 2
            && kind.is_ship()
            && kind.cost() <= 4.0
            && expanding(seen)
            && need_transport
        {
            worth *= 10.0;
        }
        if kind.is_ground_force() && expanding(seen) && need_ground {
            worth *= 5.0;
        }
        if kind.is_ship() && !kind.is_fighter() && ships < 6 {
            worth *= 1.3;
        }
        if kind.is_fighter() {
            worth *= 0.6;
        }
        // The faction's plan from round 3 on (1.0 in the opening).
        worth *= faction_plans::unit_weight(seen, me, kind);
        // Objectives within reach that more of this unit would meet (bot version 2).
        worth *= bias.weight(kind);
        // Bot version 3: ground forces idle at the docks that no ship lifts want a transport.
        if tuning::lift_rule() > 0.0
            && kind.is_ship()
            && !kind.is_fighter()
            && kind.capacity() >= 2
            && lift < i64::try_from(idle).unwrap_or(i64::MAX)
        {
            #[expect(clippy::cast_precision_loss, reason = "small counts")]
            let over = ((idle as f64 - lift as f64) / 6.0).min(1.0);
            worth *= 1.0 + tuning::lift_rule() * over;
        }
        // With resources to spare, production slots are what runs out: value per slot.
        let rich =
            payload_f64(option, "available_resources").unwrap_or(0.0) >= tuning::rich_resources();
        let price = if rich {
            payload_f64(option, "production_spent")
                .unwrap_or(count)
                .max(1.0)
        } else {
            cost.max(0.5)
        };
        scored.push((index, worth / price));
    }
    if scored.is_empty() {
        scored = filler;
    }
    if scored.is_empty() {
        return choice.options.iter().position(ChoiceOption::is_decline);
    }
    best_above(&scored, f64::NEG_INFINITY)
}

/// Payment: the source that wastes least of what it is worth, counting what else it could have
/// paid for: a planet's other value (influence when paying resources, and the reverse), and a
/// trade good's whole worth. The influence counts heavily while Mecatol Rex's custodians are in
/// reach (six are needed to land there).
fn pick_pay(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    let resources = choice
        .options
        .iter()
        .find_map(|o| payload_str(o, "kind"))
        .is_none_or(|kind| kind == "resources");
    let weight = if resources && mecatol_in_reach(seen, me) {
        0.6
    } else {
        0.2
    };
    choice
        .options
        .iter()
        .enumerate()
        .filter_map(|(i, o)| {
            let worth = payload_f64(o, "worth")?;
            let owed = payload_f64(o, "owed")?;
            let other = match o.id.strip_prefix("exhaust|") {
                Some(rest) => {
                    let planet = rest.split('|').next().unwrap_or(rest);
                    ti4_content::galaxy::planet(seen.content(), planet, seen.sources()).map_or(
                        0.0,
                        |record| {
                            #[expect(clippy::cast_precision_loss, reason = "single digits")]
                            let value = if resources {
                                record.influence()
                            } else {
                                record.resources()
                            } as f64;
                            value
                        },
                    )
                }
                None => worth,
            };
            Some((i, (worth - owed).max(0.0) + weight * other, worth))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1).then(b.2.total_cmp(&a.2)))
        .map(|(i, _, _)| i)
}

/// Remove the cheapest unit when over fleet supply.
fn pick_cheapest_removal(choice: &Choice, seen: &Observed<'_>) -> Option<usize> {
    choice
        .options
        .iter()
        .enumerate()
        .filter_map(|(i, o)| {
            let unit = o.label.strip_prefix("remove ")?;
            Some((
                i,
                ti4_engine::valuation::unit_value(seen.content(), seen.sources(), unit),
            ))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// Draft. In the opening rounds ([`expanding`]) the cards that take more systems: Warfare (a
/// tactical action without a token, and a token-free production at home), Construction (a
/// production without activating), Leadership (tokens), Technology (Gravity Drive), then
/// Diplomacy; never Trade unless the seat is the Hacan (who take it first), and never Imperial or
/// Politics in round 1 (an agenda peek wins nothing yet). In round 2 Imperial comes first when
/// Mecatol Rex can be taken this round. Later: Imperial, else the card with most trade goods.
/// Trade goods on a card add to its score throughout.
fn pick_strategy_card(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    if !expanding(seen) {
        // Imperial when it scores: Mecatol Rex held or about to be.
        let imperial = choice
            .options
            .iter()
            .position(|o| card_is(seen, &o.id, "Imperial"))
            .filter(|_| holds_mecatol(seen, me) || mecatol_takeable(seen, me));
        let nekro_tech = (tuning::nekro_tech() && faction(seen, me) == "nekro")
            .then(|| {
                choice
                    .options
                    .iter()
                    .position(|o| card_is(seen, &o.id, "Technology"))
            })
            .flatten();
        return imperial
            .or(nekro_tech)
            .or_else(|| {
                let scored: Vec<(usize, f64)> = choice
                    .options
                    .iter()
                    .enumerate()
                    .map(|(i, o)| (i, f64::from(card_goods(seen, &o.id))))
                    .collect();
                best_above(&scored, 1.0)
            })
            .or_else(|| faction_plans::preferred_card(choice, seen, me));
    }
    let scored: Vec<(usize, f64)> = choice
        .options
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let score = faction_card_score(choice, seen, me, &o.id)
                .unwrap_or_else(|| opening_card_score(seen, me, &o.id));
            (i, score)
        })
        .collect();
    best_above(&scored, f64::NEG_INFINITY)
}

fn card_goods(seen: &Observed<'_>, card: &str) -> i32 {
    seen.strategy_card_goods(&ti4_model::id::StrategyCardId::new(card))
}

/// Bot version 3: the first-round card order of the community guides for the factions whose
/// openings differ from the generic one (`crates/ti4-server/docs/COMMUNITY_FACTION_STRATEGIES.md`).
fn r1_card(faction: &str, card: &str) -> Option<f64> {
    match (faction, card) {
        // Trade first: Nekro, Ghosts, Cabal, Empyrean, Titans ("Trade in round 1 and play it early").
        ("nekro" | "ghost" | "cabal" | "empyrean" | "titans", "Trade") if tuning::r1_trade() => {
            Some(12.0)
        }
        // Technology first for the Arborec (Sarween Tools), Naalu and L1Z1X (the 5-resource home).
        ("arborec" | "naalu" | "l1z1x", "Technology") if tuning::r1_tech() => Some(11.0),
        // Factions that start without a technology here: Gravity Drive via the first card.
        ("winnu" | "sardakk" | "argent", "Technology") if tuning::r1_tech2() => Some(11.0),
        ("muaat" | "mahact" | "cabal", "Leadership") if tuning::r1_leadership() => Some(9.0),
        // The Nekro's research is three command tokens (Propagation): Technology is a free
        // Leadership.
        ("nekro", "Technology") if tuning::nekro_tech() => Some(11.0),
        // Peace Accords after Diplomacy readies the planets the carrier took.
        ("xxcha", "Diplomacy") if tuning::r1_diplomacy() => Some(9.0),
        _ => None,
    }
}

/// What a strategy card is worth to the seat in the opening rounds (see [`pick_strategy_card`]).
fn opening_card_score(seen: &Observed<'_>, me: &PlayerId, card: &str) -> f64 {
    let hacan = faction(seen, me) == "hacan";
    let first_round = seen.round() <= 1;
    let holds = |name: &str| {
        seen.seat(me).is_some_and(|seat| {
            seat.strategy_cards
                .iter()
                .any(|held| card_is(seen, held.as_str(), name))
        })
    };
    let name = ti4_engine::strategy_cards::card_name(seen.content(), card).unwrap_or_default();
    if first_round && let Some(score) = r1_card(&faction(seen, me), &name) {
        return score + f64::from(card_goods(seen, card));
    }
    let base = match name.as_str() {
        "Trade" if hacan => 20.0,
        "Trade" if first_round => -10.0,
        "Trade" => 4.0,
        "Imperial" if first_round => -20.0,
        "Imperial" if holds_mecatol(seen, me) || mecatol_takeable(seen, me) => 30.0,
        "Imperial" => 3.0,
        "Warfare" => 10.0,
        "Construction" => 8.0,
        "Leadership" => 7.0,
        // Gravity Drive; with Warfare it is the way to Mecatol Rex.
        "Technology" if gravity_drive_open(seen, me) && holds("Warfare") => 9.0,
        "Technology" if gravity_drive_open(seen, me) => 6.5,
        "Technology" => 5.0,
        // Readies planets: the influence for the custodians after paying for research.
        "Diplomacy" if holds("Warfare") && holds("Technology") => 7.5,
        "Diplomacy" => 4.5,
        "Politics" if first_round => -5.0,
        "Politics" => 2.0,
        _ => 0.0,
    };
    base + f64::from(card_goods(seen, card))
}

/// A new command token: the fleet pool until it holds a few, then whichever of the tactic and
/// strategy pools is smaller (tactic on a tie, as tactical actions are what the bot mostly takes).
fn pick_token_pool(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    let seat = seen.seat(me)?;
    let pool = if let Some((fleet, strategy)) = opening_pools(seen, me) {
        // In the opening a tactic token is one more system.
        if seat.fleet_tokens < fleet {
            "fleet_tokens"
        } else if seat.strategic_tokens < strategy {
            "strategic_tokens"
        } else {
            "tactic_tokens"
        }
    } else if seat.fleet_tokens < fleet_wanted(seen, me) {
        "fleet_tokens"
    } else if seat.tactic_tokens <= seat.strategic_tokens {
        "tactic_tokens"
    } else {
        "strategic_tokens"
    };
    choice.options.iter().position(|o| o.id == pool)
}

/// Whether the seat's round-1 transports need every token: it is short of them (the plan builds
/// more).
fn transports_need_tokens(seen: &Observed<'_>, me: &PlayerId) -> bool {
    short_of_transports(seen, me)
}

/// Round 1, Warfare's redistribution around its home build: as many transports wait at home to
/// move as there are tactic tokens (or more), so no token stays in the strategy pool unless a
/// secondary worth one is still to come.
fn warfare_build_needs_tokens(seen: &Observed<'_>, me: &PlayerId) -> bool {
    let (transports, _) = free_transports_and_ground(seen, me);
    seen.round() <= 1
        && strategy_reserve(seen, me) == 0
        && seen
            .seat(me)
            .is_some_and(|seat| transports >= i64::from(seat.tactic_tokens))
}

/// The fleet and strategy pools the opening wants, every other token going to tactics: mid-round
/// the fleet as spread out now ([`fleet_lean`]) and one strategy token; in the status phase before
/// round 2 a fleet of two (or the biggest stack) for the builds to come and one strategy token
/// (Warfare's secondary). `None` outside the opening.
fn opening_pools(seen: &Observed<'_>, me: &PlayerId) -> Option<(i32, i32)> {
    match seen.phase() {
        ti4_model::state::Phase::Action if expanding(seen) => {
            // Round 1: a strategy token only while a secondary worth one is still to come; the
            // rest send transports.
            let strategy = if seen.round() <= 1 && transports_need_tokens(seen, me) {
                strategy_reserve(seen, me).min(1)
            } else {
                1
            };
            Some((fleet_lean(seen, me), strategy))
        }
        ti4_model::state::Phase::Status if seen.round() < tuning::expansion_rounds() => {
            let stack = i32::try_from(biggest_stack(seen, me)).unwrap_or(i32::MAX);
            Some((stack.max(2), 1))
        }
        _ => None,
    }
}

/// The fleet pool the bot wants: a few tokens, or enough for its biggest stack of non-fighter
/// ships in one system.
fn fleet_wanted(seen: &Observed<'_>, me: &PlayerId) -> i32 {
    let stack = seen
        .board()
        .values()
        .map(|system| {
            system
                .units
                .iter()
                .filter(|u| &u.owner == me)
                .filter(|u| {
                    unit_type(seen, u.type_id.as_str())
                        .is_some_and(|t| t.is_ship() && !t.is_fighter())
                })
                .count()
        })
        .max()
        .unwrap_or(0);
    // Bot version 3: the Letnev's Armada of Fire holds two more ships than tokens.
    let armada = if tuning::letnev_armada() && faction(seen, me) == "letnev" {
        2
    } else {
        0
    };
    tuning::fleet_tokens_wanted().max(i32::try_from(stack).unwrap_or(i32::MAX) - armada)
}

/// Redistribution ("tactic|fleet|strategy"): the fleet pool at [`fleet_wanted`], the rest split
/// evenly between tactic and strategy, an odd token to tactic.
///
/// In the opening ([`opening_pools`]) the fleet pool shrinks to what the spread-out fleet needs and
/// every spare token goes to tactics: each tactic token is another system.
fn pick_redistribution(
    choice: &Choice,
    seen: &Observed<'_>,
    me: &PlayerId,
    warfare: bool,
) -> Option<usize> {
    let opening = opening_pools(seen, me).map(|(fleet, strategy)| {
        let zero = warfare && warfare_build_needs_tokens(seen, me);
        (fleet, if zero { 0 } else { strategy })
    });
    let fleet = opening.map_or_else(|| fleet_wanted(seen, me), |(fleet, _)| fleet);
    choice
        .options
        .iter()
        .enumerate()
        .filter_map(|(i, o)| {
            let mut parts = o.id.split('|').map(|p| p.parse::<i32>().ok());
            let (t, f, s) = (parts.next()??, parts.next()??, parts.next()??);
            let rest = match opening {
                Some((_, strategy)) => 3 * (s - strategy).abs() - t,
                None => (t - s - 1).abs(),
            };
            Some((i, 10 * (f - fleet).abs() + rest))
        })
        .min_by_key(|&(i, cost)| (cost, i))
        .map(|(i, _)| i)
}

/// Strategy-card follow-ups worth a strategy token whenever offered: research and a secret
/// objective (Leadership's token purchase is handled the same way). Others stay random.
fn pick_secondary(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    let yes = choice.options.iter().position(|o| o.id == "yes")?;
    let no = choice.options.iter().position(|o| o.id == "no")?;
    let prompt = choice.prompt.as_str();
    let tokens = seen.seat(me).map_or(0, |seat| seat.strategic_tokens);
    // A strategy token spent here would be missed by a better secondary still to come.
    let spare_token = tokens > strategy_reserve(seen, me);
    let answer = |follow: bool| Some(if follow { yes } else { no });
    if prompt.starts_with("spend a strategy token to produce at home") {
        // Warfare: a production at home without a token there, the opening plan's new transport.
        let resources = spare_resources(seen, me);
        // In the opening any token-free production at home is another transport or more
        // ground forces for the planets still in reach.
        let useful = wants_home_transport(seen, me)
            || home_system(seen, me).is_some_and(|home| {
                wants_to_build(seen, me, &home) || (expanding(seen) && target_near(seen, me, &home))
            });
        return answer(resources >= CARRIER_COST && useful);
    }
    if prompt.starts_with("spend a strategy token and 4 resources to research") {
        // In the opening only for Gravity Drive (the resources build transports otherwise).
        return answer(if expanding(seen) {
            spare_token && gravity_drive_open(seen, me)
        } else {
            true
        });
    }
    if prompt.starts_with("spend a strategy token to ready two planets") {
        // The transport plan's home build is still to come: the planets just taken (and home)
        // are its resources.
        // Not with the last token while another seat's Warfare (the build itself) is to come.
        let keep_for_warfare = i32::from(expanding(seen) && secondary_to_come(seen, me, "Warfare"));
        if readying_pays(seen, me) && exhausted_resources(seen, me) > 0 && tokens > keep_for_warfare
        {
            return answer(true);
        }
        // Diplomacy: the influence back for the custodians when Mecatol Rex is in reach.
        if mecatol_in_reach(seen, me)
            && seen.available_spend(me, Spend::Influence) < 6
            && exhausted_influence(seen, me) > 0
            && (seen.round() > 1 || tokens > keep_for_warfare)
        {
            return answer(true);
        }
        return if expanding(seen) && !spare_token {
            answer(false)
        } else {
            None
        };
    }
    if prompt.starts_with("spend a strategy token to draw a secret objective") {
        return answer(!expanding(seen) || spare_token);
    }
    if expanding(seen)
        && (prompt.starts_with("spend a strategy token to draw two action cards")
            || prompt.starts_with("spend a strategy token to place a structure")
            || prompt.starts_with("spend a strategy token to build a structure")
            || prompt.starts_with("spend a strategy token to replenish commodities"))
        && !spare_token
    {
        return answer(false);
    }
    if prompt.starts_with("spend a strategy token to draw two action cards") && seen.round() <= 1 {
        return answer(false);
    }
    // Bot version 2 ([`tuning::secondary_rule`]): from round 3 the other secondaries only with
    // a spare strategy token, and only where they pay: two exhausted planets worth readying
    // (Diplomacy), commodities to fill (Trade); a structure (Construction) and two action cards
    // (Politics) whenever the token is spare.
    if tuning::secondary_rule() && !expanding(seen) {
        if prompt.starts_with("spend a strategy token to ready two planets") {
            return answer(spare_token && best_two_exhausted(seen, me) >= 4);
        }
        if prompt.starts_with("spend a strategy token to replenish commodities") {
            let room = seen
                .seat(me)
                .map_or(0, |seat| faction_commodities(seen, me) - seat.commodities);
            return answer(spare_token && room >= 2);
        }
        if prompt.starts_with("spend a strategy token to draw two action cards")
            || prompt.starts_with("spend a strategy token to place a structure")
            || prompt.starts_with("spend a strategy token to build a structure")
        {
            return answer(spare_token);
        }
    }
    if prompt.contains("influence for a command token") {
        // Keep six influence for the custodians only while Mecatol Rex is within reach, and stop
        // once the pools are full enough to use.
        let spare = seen.available_spend(me, Spend::Influence)
            - ti4_engine::strategy_cards::INFLUENCE_PER_TOKEN;
        let pools = seen
            .seat(me)
            .map_or(0, |seat| seat.tactic_tokens + seat.strategic_tokens);
        // Nor while a round-1 plan still needs its planets' resources (they pay influence too).
        // (Paying influence exhausts planets whose resources the build needs, so any purchase
        // can spoil it while the build is still to come.)
        let saving = reserved_resources(seen, me) > 0
            || (transport_plan(seen, me).is_some() && home_builder_ahead(seen, me))
            // Another seat's Warfare secondary still to come in round 1: its home build.
            || (seen.round() <= 1
                && secondary_to_come(seen, me, "Warfare")
                && seen.seat(me).is_some_and(|seat| seat.strategic_tokens > 0));
        return answer(
            pools < tuning::enough_tokens()
                && !saving
                && (!mecatol_in_reach(seen, me) || spare >= 6),
        );
    }
    None
}

/// The seat's printed commodity limit (0 when unknown).
fn faction_commodities(seen: &Observed<'_>, me: &PlayerId) -> i32 {
    let faction = faction(seen, me);
    seen.content()
        .get(ti4_model::content_types::ContentType::Factions, &faction)
        .and_then(|record| record.int("commodities"))
        .and_then(|n| i32::try_from(n).ok())
        .unwrap_or(0)
}

/// Resources on the seat's exhausted planets: what readying them could give back.
fn exhausted_resources(seen: &Observed<'_>, me: &PlayerId) -> i64 {
    seen.controlled_planets(me)
        .into_iter()
        .filter(|(_, planet)| !seen.planet_is_ready(planet))
        .filter_map(|(_, planet)| {
            ti4_content::galaxy::planet(seen.content(), planet.as_str(), seen.sources())
        })
        .map(|record| record.resources())
        .sum()
}

/// Resources the two richest exhausted planets would give back when readied (Diplomacy).
fn best_two_exhausted(seen: &Observed<'_>, me: &PlayerId) -> i64 {
    let mut values: Vec<i64> = seen
        .controlled_planets(me)
        .into_iter()
        .filter(|(_, planet)| !seen.planet_is_ready(planet))
        .filter_map(|(_, planet)| {
            ti4_content::galaxy::planet(seen.content(), planet.as_str(), seen.sources())
        })
        .map(|record| record.resources())
        .collect();
    values.sort_unstable_by(|a, b| b.cmp(a));
    values.into_iter().take(2).sum()
}

/// Whether readying planets now pays for a round-1 plan: the transport plan's home build or the
/// Cruiser II research still to come.
fn readying_pays(seen: &Observed<'_>, me: &PlayerId) -> bool {
    (home_builder_ahead(seen, me)
        && transport_plan(seen, me).is_some_and(|(_, cost)| spare_resources(seen, me) < cost))
        || (cruiser_plan(seen, me) && ready_card(seen, me, "Technology").is_some())
}

/// Which planet to ready: while a round-1 plan has resources to find ([`readying_pays`]), the one
/// with the most resources; otherwise no opinion.
fn pick_ready_planet(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    if !readying_pays(seen, me) {
        return None;
    }
    choice
        .options
        .iter()
        .enumerate()
        .filter_map(|(i, o)| {
            let record = ti4_content::galaxy::planet(seen.content(), &o.id, seen.sources())?;
            Some((i, record.resources()))
        })
        .max_by_key(|&(i, r)| (r, std::cmp::Reverse(i)))
        .map(|(i, _)| i)
}

/// Influence on the seat's exhausted planets: what readying them could give back.
fn exhausted_influence(seen: &Observed<'_>, me: &PlayerId) -> i64 {
    seen.controlled_planets(me)
        .into_iter()
        .filter(|(_, planet)| !seen.planet_is_ready(planet))
        .filter_map(|(_, planet)| {
            ti4_content::galaxy::planet(seen.content(), planet.as_str(), seen.sources())
        })
        .map(|record| record.influence())
        .sum()
}

/// Accept only a trade that is slightly in the bot's favour; otherwise refuse, never counter.
fn pick_answer(choice: &Choice) -> Option<usize> {
    let accept = choice.options.iter().position(|o| o.id == "accept");
    let refuse = choice.options.iter().position(ChoiceOption::is_decline);
    // Bot version 2: a Support for the Throne is a victory point to whoever holds it; the bot
    // never hands one over (the prompt reads "<proposer> gives X for <what we give>").
    if tuning::sftt_rule()
        && choice
            .prompt
            .split(" for ")
            .skip(1)
            .any(|part| part.contains(ti4_engine::promissory::SUPPORT_PREFIX))
    {
        return refuse;
    }
    let net = accept.and_then(|i| payload_f64(&choice.options[i], "net"));
    match (accept, net) {
        (Some(index), Some(net)) if net >= tuning::accept_net() => Some(index),
        _ => refuse,
    }
}

/// Whether a proposal gives away a Support for the Throne (its label: "give/sell X for Y").
fn gives_support(option: &ChoiceOption) -> bool {
    option
        .label
        .split(" for ")
        .next()
        .is_some_and(|given| given.contains(ti4_engine::promissory::SUPPORT_PREFIX))
}

/// Propose the offer that gains most for the bot among those that also gain for the other side.
fn pick_proposal(choice: &Choice) -> Option<usize> {
    let scored: Vec<(usize, f64)> = choice
        .options
        .iter()
        .enumerate()
        .filter(|(_, o)| o.kind == "offer")
        .filter(|(_, o)| !(tuning::sftt_rule() && gives_support(o)))
        .filter_map(|(i, o)| {
            let net = payload_f64(o, "net")?;
            let theirs = payload_f64(o, "their_net")?;
            (net > 0.0 && theirs >= tuning::accept_net()).then_some((i, net))
        })
        .collect();
    best_above(&scored, f64::NEG_INFINITY)
        .or_else(|| choice.options.iter().position(ChoiceOption::is_decline))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tuning::Tuning;
    use rand::SeedableRng;
    use ti4_content::ContentStore;
    use ti4_engine::choice::{Decider, IllegalChoice};
    use ti4_engine::fixtures::{
        Hub, game, hub_from, hub_with_centre, plain_systems, put, put_on_planet,
    };
    use ti4_model::POK;
    use ti4_model::state::GameState;

    fn me() -> PlayerId {
        PlayerId::new("a")
    }

    fn foe() -> PlayerId {
        PlayerId::new("b")
    }

    fn planets(system: &str) -> Vec<String> {
        ti4_content::galaxy::planets_in(ContentStore::embedded(), system, POK)
            .iter()
            .map(|p| p.id().to_owned())
            .collect()
    }

    /// A one-ring map whose centre has one planet and whose first two outer systems are empty.
    fn board() -> (GameState, Hub, SystemId, PlanetId) {
        let plain = plain_systems(60);
        let with_planet = plain
            .iter()
            .find(|id| {
                let p = planets(id);
                p.len() == 1
                    && p[0] != MECATOL
                    && ti4_content::galaxy::planet(ContentStore::embedded(), &p[0], POK)
                        .is_some_and(|r| r.homeworld_of().is_none() && r.resources() > 0)
            })
            .expect("a one-planet system")
            .clone();
        let mut ids = vec![with_planet.clone()];
        ids.extend(
            plain
                .iter()
                .filter(|id| planets(id).is_empty())
                .take(6)
                .cloned(),
        );
        let hub = hub_from(&ids);
        let state = game(&["a", "b"]);
        let planet = PlanetId::new(planets(&with_planet)[0].clone());
        (state, hub, SystemId::new(with_planet), planet)
    }

    fn observe<'a>(state: &'a GameState, hub: &'a Hub) -> Observed<'a> {
        Observed::new(state, ContentStore::embedded(), POK, Some(&hub.galaxy))
    }

    fn outer(hub: &Hub, i: usize) -> SystemId {
        SystemId::new(hub.outer[i].clone())
    }

    fn activation_choice(systems: &[&SystemId]) -> Choice {
        Choice::new(
            me(),
            "activate a system",
            systems
                .iter()
                .map(|s| ChoiceOption::labelled(s.as_str(), "activate", format!("activate {s}")))
                .collect(),
        )
    }

    #[test]
    fn activation_prefers_the_system_with_a_planet_it_can_take() {
        let (mut state, hub, centre, _) = board();
        let home = outer(&hub, 0);
        put(&mut state, &home, "carrier", &me(), 1);
        put(&mut state, &home, "infantry", &me(), 2);
        let seen = observe(&state, &hub);
        assert!(activation_value(&seen, &me(), &centre) >= ACTIVATION_THRESHOLD);
        assert!(activation_value(&seen, &me(), &outer(&hub, 1)) < ACTIVATION_THRESHOLD);
        let empty = outer(&hub, 1);
        let choice = activation_choice(&[&empty, &centre]);
        assert_eq!(pick_activation(&choice, &seen, &me()), Some(1));
    }

    #[test]
    fn a_carrier_without_ground_forces_takes_nothing() {
        let (mut state, hub, centre, _) = board();
        put(&mut state, &outer(&hub, 0), "carrier", &me(), 1);
        let seen = observe(&state, &hub);
        assert!(activation_value(&seen, &me(), &centre) < ACTIVATION_THRESHOLD);
    }

    #[test]
    fn a_fight_it_would_lose_is_avoided_and_one_it_wins_is_taken() {
        let (mut state, hub, centre, _) = board();
        let home = outer(&hub, 0);
        put(&mut state, &home, "carrier", &me(), 1);
        put(&mut state, &home, "infantry", &me(), 2);
        put(&mut state, &home, "destroyer", &me(), 1);
        put(&mut state, &centre, "dreadnought", &foe(), 2);
        let seen = observe(&state, &hub);
        assert!(activation_value(&seen, &me(), &centre) < 0.0);
        assert_eq!(
            pick_activation(&activation_choice(&[&centre]), &seen, &me()),
            None
        );

        let (mut state, hub, centre, _) = board();
        let home = outer(&hub, 0);
        put(&mut state, &home, "carrier", &me(), 1);
        put(&mut state, &home, "infantry", &me(), 2);
        put(&mut state, &home, "dreadnought", &me(), 3);
        put(&mut state, &centre, "destroyer", &foe(), 1);
        let seen = observe(&state, &hub);
        assert!(activation_value(&seen, &me(), &centre) >= ACTIVATION_THRESHOLD);
    }

    #[test]
    fn mecatol_rex_is_worth_more_than_its_printed_values_until_the_custodians_fall() {
        let mut state = game(&["a"]);
        let hub = hub_from(&["18".to_owned()]);
        let seen = observe(&state, &hub);
        assert!(planet_worth(&seen, MECATOL) > 7.0 + MECATOL_BONUS - 0.1);
        state.custodians_removed = true;
        let seen = observe(&state, &hub);
        assert!(planet_worth(&seen, MECATOL) < 8.0);
    }

    #[test]
    fn mecatol_rex_is_invaded_only_when_the_custodians_can_be_paid_for() {
        let mut state = game(&["a"]);
        let hub = hub_from(&["18".to_owned()]);
        let mr = SystemId::new("18");
        state.player_mut(&me()).expect("seat").trade_goods = 0;
        let seen = observe(&state, &hub);
        assert!(
            targets(&seen, &me(), &mr).is_empty(),
            "six influence needed"
        );
        state.player_mut(&me()).expect("seat").trade_goods = 6;
        let seen = observe(&state, &hub);
        let found = targets(&seen, &me(), &mr);
        assert_eq!(found.len(), 1);
        assert!(
            found[0].1 > 10.0,
            "the custodians point makes it the best target"
        );
    }

    fn move_choice(origin: &SystemId, units: &[(&str, i64)]) -> Choice {
        let mut options: Vec<ChoiceOption> = units
            .iter()
            .enumerate()
            .map(|(i, (unit, capacity))| {
                ChoiceOption::labelled(format!("move|{origin}|{i}"), "move", format!("move {unit}"))
                    .with("origin", origin.to_string())
                    .with("unit", *unit)
                    .with("capacity", *capacity)
            })
            .collect();
        options.push(ChoiceOption::labelled(
            "done_moving",
            "decline",
            "finish movement",
        ));
        Choice::new(me(), "movement", options)
    }

    #[test]
    fn movement_brings_a_carrier_for_the_planet_then_stops() {
        let (mut state, hub, centre, _) = board();
        let home = outer(&hub, 0);
        put(&mut state, &home, "cruiser", &me(), 1);
        put(&mut state, &home, "carrier", &me(), 1);
        put(&mut state, &home, "infantry", &me(), 2);
        state.active_system = Some(centre.clone());
        let choice = move_choice(&home, &[("cruiser", 0), ("carrier", 4)]);
        let seen = observe(&state, &hub);
        assert_eq!(pick_move(&choice, &seen, &me()), Some(1), "the carrier");

        // Once the ground forces are in the active system, finish.
        put(&mut state, &centre, "carrier", &me(), 1);
        put(&mut state, &centre, "infantry", &me(), 1);
        let choice = move_choice(&home, &[("cruiser", 0)]);
        let seen = observe(&state, &hub);
        assert_eq!(pick_move(&choice, &seen, &me()), Some(1), "done");
    }

    #[test]
    fn movement_stays_out_of_a_losing_fight_and_reinforces_a_winnable_one() {
        let (mut state, hub, centre, _) = board();
        let home = outer(&hub, 0);
        put(&mut state, &home, "destroyer", &me(), 1);
        put(&mut state, &centre, "dreadnought", &foe(), 2);
        state.active_system = Some(centre.clone());
        let seen = observe(&state, &hub);
        let choice = move_choice(&home, &[("destroyer", 0)]);
        assert_eq!(pick_move(&choice, &seen, &me()), Some(1), "finish, not die");

        let (mut state, hub, centre, _) = board();
        let home = outer(&hub, 0);
        put(&mut state, &home, "destroyer", &me(), 1);
        put(&mut state, &home, "dreadnought", &me(), 2);
        put(&mut state, &centre, "destroyer", &foe(), 1);
        state.active_system = Some(centre.clone());
        let seen = observe(&state, &hub);
        let choice = move_choice(&home, &[("destroyer", 0), ("dreadnought", 0)]);
        assert_eq!(
            pick_move(&choice, &seen, &me()),
            Some(1),
            "the dreadnought first"
        );
    }

    fn mecatol_hub(foe_dreadnoughts: usize) -> (GameState, Hub, SystemId) {
        let hub = hub_with_centre("18");
        let mut state = game(&["a", "b"]);
        let mr = SystemId::new("18");
        state
            .system_mut(&mr)
            .planet_control
            .insert(PlanetId::new(MECATOL), me());
        put(&mut state, &mr, "dreadnought", &me(), 2);
        put(
            &mut state,
            &outer(&hub, 2),
            "dreadnought",
            &foe(),
            foe_dreadnoughts,
        );
        state.round = 3;
        state.active_system = Some(outer(&hub, 0));
        (state, hub, mr)
    }

    #[test]
    fn only_the_surplus_of_the_mecatol_garrison_may_leave_and_only_with_the_knob() {
        let on = Tuning {
            mecatol_guard: true,
            ..Tuning::default()
        };
        let (state, hub, mr) = mecatol_hub(1);
        let seen = observe(&state, &hub);
        let choice = move_choice(&mr, &[("dreadnought", 0), ("dreadnought", 0)]);
        let all = seen.movable_into(&me(), &outer(&hub, 0));
        assert!(all.iter().any(|m| m.origin == mr));

        // Bot version 4 (knob off): nothing is held back.
        assert_eq!(mecatol_surplus(&seen, &me()), None);
        assert_eq!(mecatol_guard_keep(&choice, &seen, &me()), None);
        // Knob on: two dreadnoughts against one; one of them is surplus, one stays.
        tuning::with(on, || {
            assert_eq!(
                mecatol_surplus(&seen, &me()),
                Some(BTreeMap::from([("dreadnought".to_owned(), 1)]))
            );
            assert_eq!(mecatol_guard_keep(&choice, &seen, &me()), Some(vec![0, 2]));
            let left = movable_for(&seen, &me(), &outer(&hub, 0));
            assert_eq!(
                left.iter().filter(|m| m.origin == mr).count(),
                1,
                "one leaves, one stays"
            );
            // Activating Mecatol Rex itself is not affected.
            assert_eq!(
                movable_for(&seen, &me(), &mr).len(),
                seen.movable_into(&me(), &mr).len()
            );
        });

        // Two against two: nothing to spare.
        let (state, hub, mr) = mecatol_hub(2);
        let seen = observe(&state, &hub);
        let choice = move_choice(&mr, &[("dreadnought", 0), ("dreadnought", 0)]);
        tuning::with(on, || {
            assert_eq!(mecatol_surplus(&seen, &me()), Some(BTreeMap::new()));
            assert_eq!(mecatol_guard_keep(&choice, &seen, &me()), Some(vec![2]));
        });

        // A fleet the garrison cannot deter: staying would not save the planet.
        let (state, hub, mr) = mecatol_hub(5);
        let seen = observe(&state, &hub);
        let choice = move_choice(&mr, &[("dreadnought", 0), ("dreadnought", 0)]);
        tuning::with(on, || {
            assert_eq!(mecatol_surplus(&seen, &me()), None);
            assert_eq!(mecatol_guard_keep(&choice, &seen, &me()), None);
        });

        // No rival fleet in reach: free to go.
        let (state, hub, _) = mecatol_hub(0);
        let seen = observe(&state, &hub);
        tuning::with(on, || assert_eq!(mecatol_surplus(&seen, &me()), None));
    }

    #[test]
    fn a_warsun_with_two_cruisers_keeps_the_warsun_and_frees_the_cruisers() {
        let on = Tuning {
            mecatol_guard: true,
            ..Tuning::default()
        };
        let (mut state, hub, mr) = mecatol_hub(1);
        state.system_mut(&mr).units.clear();
        put(&mut state, &mr, "muaat_warsun", &me(), 1);
        put(&mut state, &mr, "cruiser", &me(), 2);
        let seen = observe(&state, &hub);
        let choice = move_choice(&mr, &[("muaat_warsun", 6), ("cruiser", 0), ("cruiser", 0)]);
        tuning::with(on, || {
            let surplus = mecatol_surplus(&seen, &me()).expect("a garrison");
            assert!(!surplus.contains_key("muaat_warsun"), "{surplus:?}");
            assert_eq!(surplus.get("cruiser"), Some(&2), "{surplus:?}");
            assert_eq!(
                mecatol_guard_keep(&choice, &seen, &me()),
                Some(vec![1, 2, 3])
            );
            let left = movable_for(&seen, &me(), &outer(&hub, 0));
            assert!(
                left.iter()
                    .all(|m| m.unit.type_id.as_str() != "muaat_warsun" || m.origin != mr)
            );
        });
    }

    fn load_choice(loaded_ground: i64) -> Choice {
        let load = |id: &str, unit: &str| {
            ChoiceOption::labelled(id, "load", format!("load {unit}"))
                .with("unit", unit)
                .with("loaded_ground", loaded_ground)
        };
        Choice::new(
            me(),
            "load carrier",
            vec![
                load("load|0", "fighter"),
                load("load|1", "infantry"),
                ChoiceOption::labelled("done_loading", "decline", "carry nothing further"),
            ],
        )
    }

    #[test]
    fn loading_takes_ground_forces_the_target_needs_and_no_more() {
        let (mut state, hub, centre, planet) = board();
        state.active_system = Some(centre.clone());
        put_on_planet(&mut state, &centre, &planet, "infantry", &foe(), 1);
        let seen = observe(&state, &hub);
        assert_eq!(pick_load(&load_choice(0), &seen, &me()), Some(1));
        assert_eq!(
            pick_load(&load_choice(1), &seen, &me()),
            Some(1),
            "two needed"
        );
        assert_eq!(pick_load(&load_choice(2), &seen, &me()), Some(2), "enough");
    }

    fn commit_choice(planet: &PlanetId, troops: usize) -> Choice {
        let mut options: Vec<ChoiceOption> = (0..troops)
            .map(|i| {
                ChoiceOption::labelled(format!("commit|{i}|{planet}"), "commit", "land infantry")
                    .with("planet", planet.to_string())
                    .with("unit", "infantry")
            })
            .collect();
        options.push(ChoiceOption::labelled(
            "done_committing",
            "decline",
            "commit no more ground forces",
        ));
        Choice::new(me(), "commit ground forces", options)
    }

    #[test]
    fn landing_commits_until_the_planet_is_outnumbered_and_not_into_a_lost_cause() {
        tuning::set(tuning::for_version(4));
        let (mut state, hub, centre, planet) = board();
        state.active_system = Some(centre.clone());
        put_on_planet(&mut state, &centre, &planet, "infantry", &foe(), 2);
        let seen = observe(&state, &hub);
        assert_eq!(
            pick_commit(&commit_choice(&planet, 2), &seen, &me()),
            Some(2),
            "2 < 3"
        );
        assert_eq!(
            pick_commit(&commit_choice(&planet, 3), &seen, &me()),
            Some(0)
        );

        put_on_planet(&mut state, &centre, &planet, "infantry", &me(), 3);
        let seen = observe(&state, &hub);
        assert_eq!(
            pick_commit(&commit_choice(&planet, 2), &seen, &me()),
            Some(2),
            "covered"
        );
    }

    #[test]
    fn land_all_lands_what_the_winning_force_leaves_aboard() {
        tuning::set(tuning::for_version(4));
        let (mut state, hub, centre, planet) = board();
        state.active_system = Some(centre.clone());
        // Held and undefended: the old logic leaves the troops aboard, the knob lands them.
        state
            .system_mut(&centre)
            .planet_control
            .insert(planet.clone(), me());
        let seen = observe(&state, &hub);
        let choice = commit_choice(&planet, 2);
        assert_eq!(pick_commit(&choice, &seen, &me()), Some(2), "left aboard");
        let mut on = tuning::Tuning::default();
        on.land_all = true;
        tuning::with(on, || {
            assert_eq!(pick_commit(&choice, &seen, &me()), Some(0), "landed");
        });

        // Covered against two defenders: the surplus lands on the planet it is winning.
        let mut state = state;
        put_on_planet(&mut state, &centre, &planet, "infantry", &foe(), 2);
        put_on_planet(&mut state, &centre, &planet, "infantry", &me(), 3);
        let seen = observe(&state, &hub);
        assert_eq!(pick_commit(&choice, &seen, &me()), Some(2));
        tuning::with(on, || {
            assert_eq!(pick_commit(&choice, &seen, &me()), Some(0));
        });

        // A defended planet without any own troops (hopeless) stays untouched.
        let (mut state, hub, centre, planet) = board();
        state.active_system = Some(centre.clone());
        put_on_planet(&mut state, &centre, &planet, "infantry", &foe(), 5);
        let seen = observe(&state, &hub);
        let choice = commit_choice(&planet, 2);
        tuning::with(on, || {
            assert_eq!(pick_commit(&choice, &seen, &me()), Some(2));
        });
    }

    #[test]
    fn a_clearly_lost_space_combat_is_retreated_from() {
        let (mut state, hub, centre, _) = board();
        put(&mut state, &centre, "destroyer", &me(), 1);
        put(&mut state, &centre, "dreadnought", &foe(), 2);
        let choice = Choice::new(
            me(),
            "announce a retreat",
            vec![
                ChoiceOption::labelled("stay", "retreat", "stay")
                    .with("system", centre.to_string()),
                ChoiceOption::labelled("retreat", "retreat", "go")
                    .with("system", centre.to_string()),
            ],
        );
        let seen = observe(&state, &hub);
        assert_eq!(pick_retreat(&choice, &seen, &me()), Some(1));
        put(&mut state, &centre, "dreadnought", &me(), 4);
        let seen = observe(&state, &hub);
        assert_eq!(pick_retreat(&choice, &seen, &me()), Some(0));
    }

    fn build(unit: &str, count: i64, owed: i64, available: i64) -> ChoiceOption {
        ChoiceOption::labelled(format!("build|{unit}|{count}"), "produce", unit)
            .with("unit", unit)
            .with("count", count)
            .with("owed", owed)
            .with("available_resources", available)
            .with("fleet_excess_after", 0)
            .with("capacity_excess_after", 0)
    }

    #[test]
    fn production_buys_a_carrier_when_it_has_none_and_never_what_it_cannot_pay() {
        let (state, hub, _, _) = board();
        let seen = observe(&state, &hub);
        let choice = Choice::new(
            me(),
            "produce",
            vec![
                build("fighter", 2, 1, 3),
                build("carrier", 1, 3, 3),
                build("dreadnought", 1, 4, 3),
                ChoiceOption::labelled("done_producing", "decline", "done"),
            ],
        );
        assert_eq!(pick_produce(&choice, &seen, &me()), Some(1));
        let broke = Choice::new(
            me(),
            "produce",
            vec![
                build("dreadnought", 1, 4, 3),
                ChoiceOption::labelled("done_producing", "decline", "done"),
            ],
        );
        assert_eq!(pick_produce(&broke, &seen, &me()), Some(1), "decline");
    }

    #[test]
    fn production_stops_buying_ground_forces_its_ships_cannot_carry() {
        let (mut state, hub, centre, planet) = board();
        state.round = 3;
        put_on_planet(&mut state, &centre, &planet, "spacedock", &me(), 1);
        put_on_planet(&mut state, &centre, &planet, "infantry", &me(), 2);
        let choice = || {
            Choice::new(
                me(),
                "produce",
                vec![
                    build("infantry", 2, 1, 3),
                    build("destroyer", 1, 1, 3),
                    ChoiceOption::labelled("done_producing", "decline", "done"),
                ],
            )
        };
        let seen = observe(&state, &hub);
        assert_eq!(
            pick_produce(&choice(), &seen, &me()),
            Some(0),
            "few waiting"
        );

        put_on_planet(&mut state, &centre, &planet, "infantry", &me(), 2);
        let seen = observe(&state, &hub);
        assert_eq!(
            pick_produce(&choice(), &seen, &me()),
            Some(1),
            "four waiting, nothing to carry them: a warship instead"
        );

        // A carrier's worth of lift makes room for more.
        put(&mut state, &centre, "carrier", &me(), 2);
        let seen = observe(&state, &hub);
        assert_eq!(pick_produce(&choice(), &seen, &me()), Some(0));

        // Nothing but ground forces on offer: they fill the dock's production limit, until twice
        // what any ship could lift waits there.
        let (mut state, hub, centre, planet) = board();
        state.round = 3;
        put_on_planet(&mut state, &centre, &planet, "spacedock", &me(), 1);
        put_on_planet(&mut state, &centre, &planet, "infantry", &me(), 6);
        let only_ground = Choice::new(
            me(),
            "produce",
            vec![
                build("infantry", 2, 1, 3),
                ChoiceOption::labelled("done_producing", "decline", "done"),
            ],
        );
        let seen = observe(&state, &hub);
        assert_eq!(pick_produce(&only_ground, &seen, &me()), Some(0), "fill");
        put_on_planet(&mut state, &centre, &planet, "infantry", &me(), 2);
        let seen = observe(&state, &hub);
        assert_eq!(pick_produce(&only_ground, &seen, &me()), Some(1), "a glut");
    }

    #[test]
    fn production_with_resources_to_spare_buys_the_most_per_slot() {
        let (state, hub, _, _) = board();
        let seen = observe(&state, &hub);
        let slot = |o: ChoiceOption| o.with("production_spent", 1);
        let choice = |available: i64| {
            Choice::new(
                me(),
                "produce",
                vec![
                    slot(build("destroyer", 1, 1, available)),
                    slot(build("dreadnought", 1, 4, available)),
                    ChoiceOption::labelled("done_producing", "decline", "done"),
                ],
            )
        };
        assert_eq!(
            pick_produce(&choice(4), &seen, &me()),
            Some(0),
            "per resource"
        );
        assert_eq!(pick_produce(&choice(10), &seen, &me()), Some(1), "per slot");
    }

    fn contextual(choice: Choice, subtype: &str) -> Choice {
        choice.contextualized(ti4_engine::decision_context::DecisionContext::new(
            me(),
            ti4_engine::decision_context::DecisionSource::Rule("test".to_owned()),
            subtype,
            ti4_model::state::Phase::Action,
            1,
        ))
    }

    /// What [`pick`] answers, through the engine's private ask.
    fn picked(state: &GameState, hub: &Hub, choice: &Choice) -> Option<usize> {
        let mut probe = Probe(None);
        ti4_engine::choice::ask_private(
            choice,
            state,
            ContentStore::embedded(),
            POK,
            Some(&hub.galaxy),
            &mut probe,
        )
        .expect("legal");
        probe.0
    }

    #[test]
    fn a_victory_point_is_never_declined() {
        let (state, hub, _, _) = board();
        let imperial = contextual(
            Choice::new(
                me(),
                "score a public objective with Imperial",
                vec![
                    ChoiceOption::labelled("corner", "objective", "corner"),
                    ChoiceOption::decline(),
                ],
            ),
            "imperial_score_objective",
        );
        assert_eq!(picked(&state, &hub, &imperial), Some(0));
        let custodians = contextual(
            Choice::new(
                me(),
                "spend 6 influence to remove the custodians token",
                vec![
                    ChoiceOption::labelled("no", "decline", "leave it"),
                    ChoiceOption::labelled("yes", "custodians", "remove it"),
                ],
            ),
            "remove_custodians",
        );
        assert_eq!(picked(&state, &hub, &custodians), Some(1));
    }

    #[test]
    fn command_tokens_fill_the_fleet_pool_first_then_the_smaller_pool() {
        let (mut state, hub, _, _) = board();
        let choice = contextual(
            Choice::new(
                me(),
                "gain a command token into which pool",
                ["tactic_tokens", "fleet_tokens", "strategic_tokens"]
                    .into_iter()
                    .map(|id| ChoiceOption::labelled(id, "pool", id))
                    .collect(),
            ),
            "gain_command_token",
        );
        let pools = |state: &mut GameState, t: i32, f: i32, s: i32| {
            let seat = state.player_mut(&me()).expect("seat");
            (seat.tactic_tokens, seat.fleet_tokens, seat.strategic_tokens) = (t, f, s);
        };
        pools(&mut state, 3, 2, 2);
        assert_eq!(picked(&state, &hub, &choice), Some(1), "fleet below three");
        pools(&mut state, 2, 3, 2);
        assert_eq!(picked(&state, &hub, &choice), Some(0), "tactic on a tie");
        pools(&mut state, 3, 3, 2);
        assert_eq!(picked(&state, &hub, &choice), Some(2));
        // A stack of four ships needs four fleet tokens.
        put(&mut state, &outer(&hub, 0), "cruiser", &me(), 4);
        assert_eq!(picked(&state, &hub, &choice), Some(1));
    }

    #[test]
    fn with_nothing_worth_a_token_it_plays_the_card_or_passes() {
        let (state, hub, _, _) = board();
        let seen = observe(&state, &hub);
        let action = |first: &str| {
            Choice::new(
                me(),
                "action phase",
                vec![
                    ChoiceOption::labelled(first, "action", first),
                    ChoiceOption::labelled("tactical", "action", "tactical"),
                ],
            )
        };
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        assert_eq!(
            pick_action(&action("strategic"), &seen, &me(), &mut rng),
            Some(0)
        );
        assert_eq!(
            pick_action(&action("pass"), &seen, &me(), &mut rng),
            Some(0)
        );
    }

    #[test]
    fn a_dock_is_activated_to_build_only_when_something_could_be_built_there() {
        let (mut state, hub, centre, planet) = board();
        put_on_planet(&mut state, &centre, &planet, "spacedock", &me(), 1);
        put_on_planet(&mut state, &centre, &planet, "infantry", &me(), 4);
        // Plenty to spend: a token only to build is worth it.
        state.player_mut(&me()).expect("seat").trade_goods = 6;
        state.player_mut(&me()).expect("seat").fleet_tokens = 3;
        let seen = observe(&state, &hub);
        assert!(activation_value(&seen, &me(), &centre) >= ACTIVATION_THRESHOLD);
        // Fleet supply full there and enough infantry waiting: nothing to build.
        put(&mut state, &centre, "destroyer", &me(), 3);
        let seen = observe(&state, &hub);
        assert!(activation_value(&seen, &me(), &centre) < ACTIVATION_THRESHOLD);
    }

    fn draft(cards: &[&str]) -> Choice {
        contextual(
            Choice::new(
                me(),
                "choose a strategy card",
                cards
                    .iter()
                    .map(|id| ChoiceOption::labelled(*id, "strategy_card", *id))
                    .collect(),
            ),
            "draft_strategy_card",
        )
    }

    #[test]
    fn later_drafts_take_imperial_only_with_mecatol_rex_else_the_most_trade_goods() {
        let (mut state, hub, _, _) = board();
        state.round = 3;
        let all = draft(&["pok1leadership", "pok5trade", "pok8imperial"]);
        assert_eq!(picked(&state, &hub, &all), None, "no Mecatol, no goods");
        state
            .system_mut(&SystemId::new("18"))
            .planet_control
            .insert(PlanetId::new(MECATOL), me());
        assert_eq!(picked(&state, &hub, &all), Some(2), "Imperial scores");
        let rest = draft(&["pok1leadership", "pok5trade", "pok7technology"]);
        assert_eq!(picked(&state, &hub, &rest), None, "no goods: no opinion");
        state
            .strategy_card_goods
            .insert(ti4_model::id::StrategyCardId::new("pok5trade"), 2);
        assert_eq!(picked(&state, &hub, &rest), Some(1));
    }

    #[test]
    fn the_first_draft_takes_expansion_cards_never_trade_imperial_or_politics() {
        let (mut state, hub, _, _) = board();
        assert_eq!(state.round, 1);
        // Transports enough for the three tactic tokens: no transport plan of its own (that one
        // takes Trade; see the next test).
        let spare = outer(&hub, 0);
        put(&mut state, &spare, "carrier", &me(), 3);
        let all = [
            "pok1leadership",
            "pok3politics",
            "te4construction",
            "pok5trade",
            "te6warfare",
            "pok7technology",
            "pok8imperial",
        ];
        assert_eq!(picked(&state, &hub, &draft(&all)), Some(4), "Warfare");
        assert_eq!(
            picked(&state, &hub, &draft(&all[..4])),
            Some(2),
            "Construction"
        );
        // Trade goods on Trade do not make it worth taking in round 1 ...
        state
            .strategy_card_goods
            .insert(ti4_model::id::StrategyCardId::new("pok5trade"), 3);
        let poor = draft(&["pok1leadership", "pok5trade", "pok8imperial"]);
        assert_eq!(picked(&state, &hub, &poor), Some(0));
        let left = draft(&["pok3politics", "pok5trade", "pok8imperial"]);
        assert_eq!(
            picked(&state, &hub, &left),
            Some(0),
            "even Politics (whose primary is then not played for its agenda peek) before Trade"
        );
        // ... unless the seat is the Hacan, who take it first.
        state.player_mut(&me()).expect("seat").faction = ti4_model::id::FactionId::new("hacan");
        assert_eq!(picked(&state, &hub, &draft(&all)), Some(3), "Hacan: Trade");
    }

    #[test]
    fn short_of_transports_and_resources_the_draft_takes_trade_once_no_builder_is_left() {
        let (mut state, hub, home, _) = opening_board();
        state.phase = ti4_model::state::Phase::Strategy;
        put(&mut state, &home, "carrier", &me(), 1);
        put_on_planet(
            &mut state,
            &home,
            &PlanetId::new("jord"),
            "infantry",
            &me(),
            4,
        );
        let seat = state.player_mut(&me()).expect("seat");
        seat.trade_goods = 0;
        seat.tactic_tokens = 3;
        let seen = observe(&state, &hub);
        let (_, cost) = transport_plan(&seen, &me()).expect("one transport, three tokens");
        assert!(
            planet_resources(&seen, &me()) < cost,
            "four resources, two carriers"
        );
        // Construction and Warfare gone: Trade's goods for Warfare's secondary.
        let late = draft(&[
            "pok1leadership",
            "pok2diplomacy",
            "pok5trade",
            "pok7technology",
        ]);
        assert_eq!(picked(&state, &hub, &late), Some(2), "Trade");
        // Holding Construction: Trade with it.
        state.player_mut(&me()).expect("seat").strategy_cards =
            vec![ti4_model::id::StrategyCardId::new("te4construction")];
        assert_eq!(
            picked(&state, &hub, &late),
            Some(2),
            "Trade beside Construction"
        );
    }

    #[test]
    fn with_warfare_held_technology_for_gravity_drive_comes_before_construction() {
        let (mut state, hub, _, _) = board();
        let seat = state.player_mut(&me()).expect("seat");
        seat.strategy_cards
            .push(ti4_model::id::StrategyCardId::new("te6warfare"));
        seat.technologies.clear();
        let choice = draft(&["te4construction", "pok7technology"]);
        assert_eq!(picked(&state, &hub, &choice), Some(0), "no propulsion yet");
        state
            .player_mut(&me())
            .expect("seat")
            .technologies
            .insert(ti4_model::id::TechnologyId::new("amd"));
        assert_eq!(
            picked(&state, &hub, &choice),
            Some(1),
            "Gravity Drive opens"
        );
    }

    #[test]
    fn research_takes_gravity_drive_when_offered() {
        let research = |ids: &[&str]| {
            Choice::new(
                me(),
                "research a technology",
                ids.iter()
                    .map(|id| ChoiceOption::labelled(*id, "research", *id))
                    .collect(),
            )
        };
        let (state, hub, _, _) = board();
        let seen = observe(&state, &hub);
        assert_eq!(
            pick_research(&research(&["amd", "gd", "sar"]), &seen, &me()),
            Some(1)
        );
        assert_eq!(
            pick_research(&research(&["amd", "sar"]), &seen, &me()),
            None
        );
    }

    #[test]
    fn mid_round_in_the_opening_the_spread_fleet_gives_its_tokens_to_tactics() {
        let (mut state, hub, _, _) = board();
        state.phase = ti4_model::state::Phase::Action;
        // Two ships in two systems: one fleet token is all the fleet needs now.
        put(&mut state, &outer(&hub, 0), "carrier", &me(), 1);
        put(&mut state, &outer(&hub, 1), "destroyer", &me(), 1);
        let ids = ["3|3|2", "4|2|2", "5|1|2", "6|1|1", "7|1|0", "4|1|3"];
        let choice = contextual(
            Choice::new(
                me(),
                "Warfare: redistribute your command tokens",
                ids.iter()
                    .map(|id| ChoiceOption::labelled(*id, "redistribute", *id))
                    .collect(),
            ),
            "warfare_redistribute_tokens",
        );
        // Round 1 with no secondary worth a strategy token still to come: every token but the
        // fleet's one goes to tactics.
        assert_eq!(picked(&state, &hub, &choice), Some(4), "7|1|0");
        // Another seat still to play Warfare: one strategy token kept for its secondary.
        state.player_mut(&foe()).expect("seat").strategy_cards =
            vec![ti4_model::id::StrategyCardId::new("te6warfare")];
        assert_eq!(picked(&state, &hub, &choice), Some(3), "6|1|1");
        // Outside the opening the fleet keeps its usual three.
        state.round = 3;
        assert_eq!(picked(&state, &hub, &choice), Some(0), "3|3|2");
    }

    /// A hub with Sol's home system (Jord) at the centre and a one-planet system beside it, the
    /// seat holding only Jord: an opening position.
    fn opening_board() -> (GameState, Hub, SystemId, SystemId) {
        let target = plain_systems(60)
            .into_iter()
            .find(|id| {
                let p = planets(id);
                p.len() == 1
                    && p[0] != MECATOL
                    && ti4_content::galaxy::planet(ContentStore::embedded(), &p[0], POK)
                        .is_some_and(|r| r.homeworld_of().is_none())
            })
            .expect("a one-planet system");
        let mut ids = vec!["01".to_owned(), target.clone()];
        ids.extend(
            plain_systems(60)
                .into_iter()
                .filter(|id| planets(id).is_empty())
                .take(5),
        );
        let hub = hub_from(&ids);
        let mut state = game(&["a", "b"]);
        for system in state.board.values_mut() {
            system.planet_control.retain(|_, owner| owner != &me());
            system.units.retain(|u| u.owner != me());
            system.planet_units.clear();
        }
        let home = SystemId::new("01");
        state
            .system_mut(&home)
            .planet_control
            .insert(PlanetId::new("jord"), me());
        let seat = state.player_mut(&me()).expect("seat");
        seat.trade_goods = 5;
        seat.tactic_tokens = 2;
        state.phase = ti4_model::state::Phase::Action;
        (state, hub, home, SystemId::new(target))
    }

    #[test]
    fn the_transports_gone_a_token_free_production_builds_one_at_home() {
        let (mut state, hub, home, _) = opening_board();
        put_on_planet(
            &mut state,
            &home,
            &PlanetId::new("jord"),
            "infantry",
            &me(),
            2,
        );
        put_on_planet(
            &mut state,
            &home,
            &PlanetId::new("jord"),
            "spacedock",
            &me(),
            1,
        );
        let seen = observe(&state, &hub);
        assert!(wants_home_transport(&seen, &me()));
        let free = Choice::new(
            me(),
            "Warfare: a tactical action without a command token",
            [hub.outer[0].as_str(), "01"]
                .iter()
                .map(|id| ChoiceOption::labelled(*id, "activate", *id))
                .collect(),
        );
        assert_eq!(pick_free_tactical(&free, &seen, &me()), Some(1), "home");
        let construction = Choice::new(
            me(),
            "Construction: a structure or a production",
            vec![
                ChoiceOption::labelled("structure", "build", "structure"),
                ChoiceOption::labelled("produce|01", "build", "produce"),
            ],
        );
        assert_eq!(pick_construction(&construction, &seen, &me()), Some(1));

        // Home already activated: whatever is built there cannot leave this round.
        state.system_mut(&home).command_tokens.insert(me());
        let seen = observe(&state, &hub);
        assert!(!home_build_wanted(&seen, &me()));
        assert_eq!(pick_construction(&construction, &seen, &me()), Some(0));
        // Three transports already: the structure.
        state.system_mut(&home).command_tokens.remove(&me());
        put(&mut state, &home, "carrier", &me(), 3);
        let seen = observe(&state, &hub);
        assert!(!home_build_wanted(&seen, &me()));
        assert_eq!(pick_construction(&construction, &seen, &me()), Some(0));
    }

    #[test]
    fn warfare_is_played_early_while_a_planet_is_there_for_the_taking() {
        let (mut state, hub, home, _) = opening_board();
        state
            .player_mut(&me())
            .expect("seat")
            .strategy_cards
            .push(ti4_model::id::StrategyCardId::new("te6warfare"));
        // Three transports and their troops at home: no new one wanted, a planet next door.
        put(&mut state, &home, "carrier", &me(), 3);
        put(&mut state, &home, "infantry", &me(), 4);
        let seen = observe(&state, &hub);
        let action = Choice::new(
            me(),
            "action phase",
            vec![
                ChoiceOption::labelled("tactical", "action", "tactical"),
                ChoiceOption::labelled("strategic|te6warfare", "action", "Warfare"),
            ],
        );
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        assert_eq!(pick_action(&action, &seen, &me(), &mut rng), Some(1));
        // Later in the game the planet is simply taken with a token.
        state.round = 3;
        let seen = observe(&state, &hub);
        assert_eq!(pick_action(&action, &seen, &me(), &mut rng), Some(0));
    }

    #[test]
    fn opening_troops_take_one_planet_per_system_unless_there_are_troops_to_spare() {
        let two = plain_systems(80)
            .into_iter()
            .find(|id| {
                let p = planets(id);
                p.len() == 2
                    && p.iter().all(|planet| {
                        ti4_content::galaxy::planet(ContentStore::embedded(), planet, POK)
                            .is_some_and(|r| r.homeworld_of().is_none())
                    })
            })
            .expect("a two-planet system");
        let mut ids = vec![two.clone()];
        ids.extend(
            plain_systems(60)
                .into_iter()
                .filter(|id| planets(id).is_empty())
                .take(6),
        );
        let hub = hub_from(&ids);
        let system = SystemId::new(two);
        let mut state = game(&["a", "b"]);
        let origin = outer(&hub, 0);
        // Two transports, two troops: one each.
        put(&mut state, &origin, "carrier", &me(), 2);
        put(&mut state, &origin, "infantry", &me(), 2);
        let seen = observe(&state, &hub);
        assert_eq!(ground_shortfall(&seen, &me(), &system, 0), 1);
        // A troop to spare: both planets.
        put(&mut state, &origin, "infantry", &me(), 1);
        let seen = observe(&state, &hub);
        assert_eq!(ground_shortfall(&seen, &me(), &system, 0), 2);
        // Later rounds take every planet.
        state.round = 3;
        put(&mut state, &origin, "carrier", &me(), 2);
        let seen = observe(&state, &hub);
        assert_eq!(ground_shortfall(&seen, &me(), &system, 0), 2);
    }

    /// The Mentak start on the opening board: Moll Primus-like home (four resources), a carrier,
    /// two cruisers and four infantry, Plasma Scoring and Sarween Tools, no trade goods.
    fn mentak_board() -> (GameState, Hub, SystemId, SystemId) {
        let (mut state, hub, home, target) = opening_board();
        put(&mut state, &home, "carrier", &me(), 1);
        put(&mut state, &home, "cruiser", &me(), 2);
        put_on_planet(
            &mut state,
            &home,
            &PlanetId::new("jord"),
            "infantry",
            &me(),
            4,
        );
        put_on_planet(
            &mut state,
            &home,
            &PlanetId::new("jord"),
            "spacedock",
            &me(),
            1,
        );
        let seat = state.player_mut(&me()).expect("seat");
        seat.faction = ti4_model::id::FactionId::new("mentak");
        seat.trade_goods = 0;
        seat.tactic_tokens = 3;
        seat.technologies = ["ps", "st"]
            .into_iter()
            .map(ti4_model::id::TechnologyId::new)
            .collect();
        (state, hub, home, target)
    }

    fn action_choice(cards: &[&str]) -> Choice {
        Choice::new(
            me(),
            "action phase",
            cards
                .iter()
                .map(|card| ChoiceOption::labelled(format!("strategic|{card}"), "action", *card))
                .chain(std::iter::once(ChoiceOption::labelled(
                    "tactical", "action", "tactical",
                )))
                .collect(),
        )
    }

    #[test]
    fn the_cruiser_two_plan_drafts_trade_and_technology_then_plays_them_in_that_order() {
        let (mut state, hub, _, _) = mentak_board();
        state.phase = ti4_model::state::Phase::Strategy;
        let seen = observe(&state, &hub);
        assert!(
            cruiser_plan(&seen, &me()),
            "two cruisers, one colour short of Cruiser II"
        );
        assert_eq!(missing_colours(&seen, &me(), CRUISER_II), vec!['G']);
        // Holding Trade, Technology is the second pick, ahead of Warfare.
        let trade = ti4_model::id::StrategyCardId::new("pok5trade");
        state.player_mut(&me()).expect("seat").strategy_cards = vec![trade.clone()];
        let rest = draft(&["te6warfare", "pok7technology", "pok8imperial"]);
        assert_eq!(picked(&state, &hub, &rest), Some(1), "Technology");

        // In the action phase: Trade first (four resources are not the six Cruiser II costs) ...
        state.phase = ti4_model::state::Phase::Action;
        let technology = ti4_model::id::StrategyCardId::new("pok7technology");
        state.player_mut(&me()).expect("seat").strategy_cards =
            vec![trade.clone(), technology.clone()];
        let both = action_choice(&["pok5trade", "pok7technology"]);
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let seen = observe(&state, &hub);
        assert_eq!(reserved_resources(&seen, &me()), SECOND_RESEARCH);
        assert_eq!(pick_action(&both, &seen, &me(), &mut rng), Some(0), "Trade");
        // ... then Technology with its goods.
        let seat = state.player_mut(&me()).expect("seat");
        seat.exhausted_strategy_cards.insert(trade);
        seat.trade_goods = 3;
        let seen = observe(&state, &hub);
        let tech_only = action_choice(&["pok7technology"]);
        assert_eq!(
            pick_action(&tech_only, &seen, &me(), &mut rng),
            Some(0),
            "Technology"
        );
    }

    #[test]
    fn the_cruiser_two_plan_researches_the_missing_colour_then_cruiser_two() {
        let (state, hub, _, _) = mentak_board();
        let seen = observe(&state, &hub);
        let research = |ids: &[&str]| {
            Choice::new(
                me(),
                "research a technology",
                ids.iter()
                    .map(|id| ChoiceOption::labelled(*id, "research", *id))
                    .collect(),
            )
        };
        // Neural Motivator: green without prerequisites (Dacxive Animators needs a green).
        assert_eq!(
            pick_research(&research(&["amd", "dxa", "nm", "gd"]), &seen, &me()),
            Some(2)
        );
        assert_eq!(
            pick_research(&research(&["amd", "cr2", "gd"]), &seen, &me()),
            Some(1)
        );
    }

    #[test]
    fn short_of_transports_trade_comes_before_the_home_build_and_the_build_is_carriers_first() {
        let (mut state, hub, home, _) = opening_board();
        put(&mut state, &home, "carrier", &me(), 1);
        put_on_planet(
            &mut state,
            &home,
            &PlanetId::new("jord"),
            "infantry",
            &me(),
            4,
        );
        put_on_planet(
            &mut state,
            &home,
            &PlanetId::new("jord"),
            "spacedock",
            &me(),
            1,
        );
        let seat = state.player_mut(&me()).expect("seat");
        seat.trade_goods = 0;
        seat.tactic_tokens = 3;
        seat.strategy_cards = vec![
            ti4_model::id::StrategyCardId::new("te4construction"),
            ti4_model::id::StrategyCardId::new("pok5trade"),
        ];
        let seen = observe(&state, &hub);
        // One transport for three tactic tokens: two carriers to build, troops enough.
        assert_eq!(transport_plan(&seen, &me()), Some((2, 2 * CARRIER_COST)));
        assert!(short_of_transports(&seen, &me()));
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let cards = action_choice(&["te4construction", "pok5trade"]);
        assert_eq!(
            pick_action(&cards, &seen, &me(), &mut rng),
            Some(1),
            "Trade first"
        );

        // At the build: a carrier while its troop waits at home.
        let build = |unit: &str, cost: f64| {
            ChoiceOption::labelled(format!("build|{unit}|1"), "produce", unit)
                .with("unit", unit)
                .with("cost", cost)
                .with("owed", cost)
                .with("available_resources", 7.0)
                .with("system", home.as_str())
        };
        let produce = Choice::new(
            me(),
            "produce in 01",
            vec![
                build("infantry", 1.0),
                build("fighter", 1.0),
                build("carrier", 3.0),
                ChoiceOption::decline(),
            ],
        );
        assert_eq!(pick_produce(&produce, &seen, &me()), Some(2), "carrier");
        // Nothing is built where its command token would keep it this round.
        state.system_mut(&home).command_tokens.insert(me());
        let seen = observe(&state, &hub);
        assert_eq!(pick_produce(&produce, &seen, &me()), Some(3), "declined");
    }

    /// The Mentak in the speaker seat after its first tactical action: the carrier holds the
    /// target system (its token there, its planet taken), the two cruisers, the dock and two
    /// infantry wait at home, Warfare and Imperial in hand.
    fn speaker_mentak_after_the_first_system() -> (GameState, Hub, SystemId, SystemId) {
        let (mut state, hub, home, target) = opening_board();
        put(&mut state, &home, "cruiser", &me(), 2);
        put_on_planet(
            &mut state,
            &home,
            &PlanetId::new("jord"),
            "infantry",
            &me(),
            2,
        );
        put_on_planet(
            &mut state,
            &home,
            &PlanetId::new("jord"),
            "spacedock",
            &me(),
            1,
        );
        put(&mut state, &target, "carrier", &me(), 1);
        let planet = PlanetId::new(planets(target.as_str())[0].clone());
        put_on_planet(&mut state, &target, &planet, "infantry", &me(), 1);
        let board = state.system_mut(&target);
        board.planet_control.insert(planet.clone(), me());
        board.command_tokens.insert(me());
        state.exhausted_planets.insert(planet);
        let seat = state.player_mut(&me()).expect("seat");
        seat.faction = ti4_model::id::FactionId::new("mentak");
        seat.trade_goods = 0;
        seat.tactic_tokens = 2;
        seat.fleet_tokens = 3;
        seat.strategic_tokens = 2;
        seat.technologies = ["ps", "st"]
            .into_iter()
            .map(ti4_model::id::TechnologyId::new)
            .collect();
        seat.strategy_cards = vec![
            ti4_model::id::StrategyCardId::new("te6warfare"),
            ti4_model::id::StrategyCardId::new("pok8imperial"),
        ];
        (state, hub, home, target)
    }

    #[test]
    fn the_speaker_with_warfare_clears_home_before_the_build_and_sarween_cuts_its_cost() {
        let (mut state, hub, _, _) = speaker_mentak_after_the_first_system();
        let seen = observe(&state, &hub);
        assert_eq!(systems_taken_this_round(&seen, &me()), 1);
        // Two more systems (two tactic tokens and Warfare's redistribution), no transport left:
        // two carriers, troops at home already, Sarween Tools takes one off.
        assert_eq!(
            transport_plan(&seen, &me()),
            Some((2, 2 * CARRIER_COST - 1))
        );
        // The cruisers fill the fleet pool at home: they leave first.
        assert!(wait_move_pays(&seen, &me()));
        // The Creuss have nowhere out of the others' way to wait: they escort instead.
        state.player_mut(&me()).expect("seat").faction = ti4_model::id::FactionId::new("ghost");
        let seen = observe(&state, &hub);
        assert!(!wait_move_pays(&seen, &me()));
        state.player_mut(&me()).expect("seat").faction = ti4_model::id::FactionId::new("mentak");
        // With room at home (a bigger fleet pool) there is nothing to clear.
        state.player_mut(&me()).expect("seat").fleet_tokens = 4;
        let seen = observe(&state, &hub);
        assert!(!wait_move_pays(&seen, &me()));
    }

    #[test]
    fn round_one_imperial_does_not_spend_the_plan_on_a_spending_objective() {
        let (mut state, hub, _, _) = speaker_mentak_after_the_first_system();
        let score = |ids: &[&str]| {
            contextual(
                Choice::new(
                    me(),
                    "score a public objective with Imperial",
                    ids.iter()
                        .map(|id| ChoiceOption::labelled(*id, "objective", *id))
                        .chain(std::iter::once(ChoiceOption::decline()))
                        .collect(),
                ),
                "imperial_score_objective",
            )
        };
        let seen = observe(&state, &hub);
        assert!(spending_objective_blocks_plan(
            &score(&["lead"]),
            &seen,
            &me()
        ));
        assert!(!spending_objective_blocks_plan(
            &score(&["lead", "expand"]),
            &seen,
            &me()
        ));
        assert_eq!(picked(&state, &hub, &score(&["lead"])), Some(1), "declined");
        // From round 2 the point is taken.
        state.round = 2;
        let seen = observe(&state, &hub);
        assert!(!spending_objective_blocks_plan(
            &score(&["lead"]),
            &seen,
            &me()
        ));
    }

    #[test]
    fn pillage_and_ambush_are_always_used() {
        let (state, hub, _, _) = opening_board();
        let pillage = contextual(
            Choice::new(
                me(),
                "after TRADE_GOODS_GAINED",
                ["ability:mentak:pillage:TRADE_GOODS_GAINED:after", "decline"]
                    .iter()
                    .map(|id| ChoiceOption::labelled(*id, "reaction", *id))
                    .collect(),
            ),
            "reaction_after_TRADE_GOODS_GAINED",
        );
        assert_eq!(picked(&state, &hub, &pillage), Some(0));
        let take = contextual(
            Choice::new(
                me(),
                "Pillage: take 1 trade good or commodity",
                ["commodity|b", "trade_good|b"]
                    .iter()
                    .map(|id| ChoiceOption::labelled(*id, "pillage", *id))
                    .collect(),
            ),
            "pillage_take",
        );
        assert_eq!(picked(&state, &hub, &take), Some(1), "the trade good");
        let ambush = contextual(
            Choice::new(
                me(),
                "Ambush: choose up to 2 cruisers or destroyers to roll",
                ["cruiser", "cruiser+cruiser"]
                    .iter()
                    .map(|id| ChoiceOption::labelled(*id, "ambush", *id))
                    .chain(std::iter::once(ChoiceOption::decline()))
                    .collect(),
            ),
            "ambush_roll",
        );
        assert_eq!(picked(&state, &hub, &ambush), Some(1), "both cruisers");
    }

    #[test]
    fn predictive_intelligence_keeps_the_pools_as_they_are() {
        let (state, hub, _, _) = opening_board();
        let choice = contextual(
            Choice::new(
                me(),
                "Predictive Intelligence: redistribute command tokens",
                ["tactic|fleet", "fleet|strategy", "done"]
                    .iter()
                    .map(|id| ChoiceOption::labelled(*id, "redistribute", *id))
                    .collect(),
            ),
            "predictive_intelligence_redistribute",
        );
        assert_eq!(picked(&state, &hub, &choice), Some(2));
    }

    #[test]
    fn the_status_phase_before_round_two_keeps_a_lean_fleet_and_one_strategy_token() {
        let (mut state, hub, _, _) = board();
        state.phase = ti4_model::state::Phase::Status;
        put(&mut state, &outer(&hub, 0), "carrier", &me(), 1);
        let ids = ["3|3|2", "5|2|1", "4|2|2", "6|1|1"];
        let choice = contextual(
            Choice::new(
                me(),
                "redistribute your command tokens",
                ids.iter()
                    .map(|id| ChoiceOption::labelled(*id, "redistribute", *id))
                    .collect(),
            ),
            "status_redistribute_tokens",
        );
        assert_eq!(picked(&state, &hub, &choice), Some(1), "5|2|1");
        state.round = 3;
        assert_eq!(picked(&state, &hub, &choice), Some(0), "3|3|2 later");
    }

    #[test]
    fn the_second_draft_takes_imperial_when_mecatol_rex_is_held() {
        let (mut state, hub, _, _) = board();
        state.round = 2;
        let choice = draft(&["te6warfare", "pok8imperial"]);
        assert_eq!(picked(&state, &hub, &choice), Some(0), "Warfare");
        state
            .system_mut(&SystemId::new("18"))
            .planet_control
            .insert(PlanetId::new(MECATOL), me());
        assert_eq!(picked(&state, &hub, &choice), Some(1), "Imperial scores");
    }

    #[test]
    fn payment_near_mecatol_rex_keeps_influence() {
        let mut state = game(&["a", "b"]);
        let hub = hub_with_centre("18");
        put(&mut state, &outer(&hub, 0), "carrier", &me(), 1);
        // Two planets worth the same resources, one with much more influence.
        let pay = |planet: &str| {
            ChoiceOption::labelled(format!("exhaust|{planet}"), "pay", planet)
                .with("worth", 2)
                .with("owed", 2)
                .with("kind", "resources")
        };
        let rich = [
            "mehar", "wellon", "lodor", "quann", "abyz", "fria", "lazar", "sakulag",
        ]
        .into_iter()
        .filter_map(|id| {
            let record = ti4_content::galaxy::planet(ContentStore::embedded(), id, POK)?;
            Some((id, record.resources(), record.influence()))
        })
        .collect::<Vec<_>>();
        let (a, b) = rich
            .iter()
            .flat_map(|x| rich.iter().map(move |y| (x, y)))
            .find(|(x, y)| x.1 == y.1 && x.2 < y.2)
            .expect("two planets alike in resources, not influence");
        let choice = Choice::new(me(), "pay 2", vec![pay(b.0), pay(a.0)]);
        let seen = observe(&state, &hub);
        assert!(mecatol_in_reach(&seen, &me()));
        assert_eq!(
            pick_pay(&choice, &seen, &me()),
            Some(1),
            "the low-influence one"
        );
    }

    #[test]
    fn redistribution_keeps_three_fleet_tokens_and_splits_the_rest() {
        let (state, hub, _, _) = board();
        let ids = ["0|8|0", "0|2|6", "4|2|2", "2|3|3", "3|3|2", "5|3|0"];
        let choice = contextual(
            Choice::new(
                me(),
                "redistribute your command tokens",
                ids.iter()
                    .map(|id| ChoiceOption::labelled(*id, "redistribute", *id))
                    .collect(),
            ),
            "status_redistribute_tokens",
        );
        assert_eq!(picked(&state, &hub, &choice), Some(4), "3|3|2");
    }

    #[test]
    fn research_and_secret_objectives_are_always_followed() {
        let (mut state, hub, _, _) = board();
        state.round = 3;
        let yes_no = |prompt: &str| {
            Choice::new(
                me(),
                prompt,
                vec![
                    ChoiceOption::labelled("no", "strategy", "no"),
                    ChoiceOption::labelled("yes", "strategy", "yes"),
                ],
            )
        };
        for prompt in [
            "spend a strategy token and 4 resources to research",
            "spend a strategy token to draw a secret objective",
        ] {
            assert_eq!(picked(&state, &hub, &yes_no(prompt)), Some(1), "{prompt}");
        }
        assert_eq!(
            picked(
                &state,
                &hub,
                &yes_no("spend a strategy token to draw two action cards")
            ),
            None,
            "no opinion"
        );
    }

    fn leadership() -> Choice {
        Choice::new(
            me(),
            "spend 3 influence for a command token",
            vec![
                ChoiceOption::labelled("no", "strategy", "no"),
                ChoiceOption::labelled("yes", "strategy", "yes"),
            ],
        )
    }

    #[test]
    fn leadership_keeps_six_influence_back_only_while_mecatol_rex_is_in_reach() {
        let mut state = game(&["a", "b"]);
        let hub = hub_with_centre("18");
        let mr = SystemId::new("18");
        state.player_mut(&me()).expect("seat").trade_goods = 4;
        assert_eq!(
            picked(&state, &hub, &leadership()),
            Some(1),
            "nothing near Mecatol Rex: spend freely"
        );

        put(&mut state, &outer(&hub, 0), "destroyer", &me(), 1);
        let seen = observe(&state, &hub);
        assert!(mecatol_in_reach(&seen, &me()));
        state.player_mut(&me()).expect("seat").trade_goods = 8;
        assert_eq!(
            picked(&state, &hub, &leadership()),
            Some(0),
            "six kept back"
        );
        state.player_mut(&me()).expect("seat").trade_goods = 9;
        assert_eq!(picked(&state, &hub, &leadership()), Some(1));

        // Ground forces already on Mecatol Rex count too; once the custodians are gone, spend.
        let (mut state, hub) = (game(&["a", "b"]), hub_with_centre("18"));
        put_on_planet(
            &mut state,
            &mr,
            &PlanetId::new(MECATOL),
            "infantry",
            &me(),
            1,
        );
        state.player_mut(&me()).expect("seat").trade_goods = 4;
        assert_eq!(picked(&state, &hub, &leadership()), Some(0));
        state.custodians_removed = true;
        assert_eq!(picked(&state, &hub, &leadership()), Some(1));
    }

    #[test]
    fn mecatol_rex_out_of_reach_keeps_no_reserve() {
        // A ship too far from Mecatol Rex to arrive this turn or next leaves it out of reach.
        let plain: Vec<String> = plain_systems(60)
            .into_iter()
            .filter(|id| planets(id).is_empty())
            .take(6)
            .collect();
        let mut ids = vec![plain[0].clone(), "18".to_owned()];
        ids.extend(plain[1..].iter().cloned());
        let hub = hub_from(&ids);
        let mut state = game(&["a", "b"]);
        // The Mecatol tile sits on the ring; the seat's ship is across the board from it.
        let far = SystemId::new(hub.across("18"));
        put(&mut state, &far, "dreadnought", &me(), 1);
        let seen = observe(&state, &hub);
        assert!(
            !mecatol_in_reach(&seen, &me()),
            "dreadnoughts move 1, two hexes away"
        );
        put(&mut state, &far, "cruiser", &me(), 1);
        let seen = observe(&state, &hub);
        assert!(mecatol_in_reach(&seen, &me()), "a cruiser moves 2");
    }

    #[test]
    fn a_free_planet_is_taken_eagerly_and_every_free_planet_gets_a_troop() {
        // A system with two planets, neither a homeworld nor Mecatol Rex.
        let two = plain_systems(80)
            .into_iter()
            .find(|id| {
                let p = planets(id);
                p.len() == 2
                    && p.iter().all(|planet| {
                        ti4_content::galaxy::planet(ContentStore::embedded(), planet, POK)
                            .is_some_and(|r| r.homeworld_of().is_none())
                    })
            })
            .expect("a two-planet system");
        let mut ids = vec![two.clone()];
        ids.extend(
            plain_systems(60)
                .into_iter()
                .filter(|id| planets(id).is_empty())
                .take(6),
        );
        let hub = hub_from(&ids);
        let system = SystemId::new(two.clone());
        let (p0, p1) = {
            let p = planets(&two);
            (PlanetId::new(p[0].clone()), PlanetId::new(p[1].clone()))
        };
        let mut state = game(&["a", "b"]);
        let home = outer(&hub, 0);
        put(&mut state, &home, "carrier", &me(), 1);
        put(&mut state, &home, "infantry", &me(), 2);
        let seen = observe(&state, &hub);
        assert!(activation_value(&seen, &me(), &system) >= EAGER_WORTH);
        let mut rng = ChaCha8Rng::seed_from_u64(3);
        let action = Choice::new(
            me(),
            "action phase",
            vec![
                ChoiceOption::labelled("strategic", "action", "strategic"),
                ChoiceOption::labelled("tactical", "action", "tactical"),
            ],
        );
        for _ in 0..20 {
            assert_eq!(
                pick_action(&action, &seen, &me(), &mut rng),
                Some(1),
                "a planet to take beats the card"
            );
        }

        // Landing: identical infantry are offered once per planet; both planets get one.
        state.system_mut(&home).units.clear();
        put(&mut state, &system, "carrier", &me(), 1);
        put(&mut state, &system, "infantry", &me(), 2);
        state.active_system = Some(system.clone());
        let landing = |planets: &[&PlanetId]| {
            let mut options: Vec<ChoiceOption> = planets
                .iter()
                .map(|planet| {
                    ChoiceOption::labelled(format!("commit|0|{planet}"), "commit", "land infantry")
                        .with("planet", planet.to_string())
                        .with("unit", "infantry")
                })
                .collect();
            options.push(ChoiceOption::labelled("done_committing", "decline", "done"));
            Choice::new(me(), "commit ground forces", options)
        };
        let seen = observe(&state, &hub);
        let first = pick_commit(&landing(&[&p0, &p1]), &seen, &me()).expect("an answer");
        assert!(first < 2, "lands");
        let (landed, other) = if first == 0 { (&p0, &p1) } else { (&p1, &p0) };
        let space = &mut state.system_mut(&system).units;
        let at = space
            .iter()
            .position(|u| u.type_id.as_str() == "infantry")
            .expect("one");
        space.remove(at);
        put_on_planet(&mut state, &system, landed, "infantry", &me(), 1);
        let seen = observe(&state, &hub);
        let choice = landing(&[&p0, &p1]);
        let second = pick_commit(&choice, &seen, &me()).expect("an answer");
        assert_eq!(
            choice.options[second]
                .payload
                .get("planet")
                .and_then(Value::as_str),
            Some(other.as_str()),
            "the other free planet"
        );
    }

    #[test]
    fn landing_counts_identical_troops_offered_once() {
        // Three infantry in space are one option; the defended planet needs two and gets them.
        let (mut state, hub, centre, planet) = board();
        state.active_system = Some(centre.clone());
        put_on_planet(&mut state, &centre, &planet, "infantry", &foe(), 1);
        put(&mut state, &centre, "carrier", &me(), 1);
        put(&mut state, &centre, "infantry", &me(), 3);
        let seen = observe(&state, &hub);
        assert_eq!(
            pick_commit(&commit_choice(&planet, 1), &seen, &me()),
            Some(0)
        );
    }

    #[test]
    fn a_dock_with_ground_forces_waiting_fetches_an_empty_transport() {
        let (mut state, hub, centre, planet) = board();
        put_on_planet(&mut state, &centre, &planet, "spacedock", &me(), 1);
        put_on_planet(&mut state, &centre, &planet, "infantry", &me(), 4);
        state
            .system_mut(&centre)
            .planet_control
            .insert(planet.clone(), me());
        put(&mut state, &centre, "destroyer", &me(), 3);
        state.player_mut(&me()).expect("seat").fleet_tokens = 3;
        let away = outer(&hub, 0);
        put(&mut state, &away, "carrier", &me(), 1);
        let seen = observe(&state, &hub);
        assert!(activation_value(&seen, &me(), &centre) >= ACTIVATION_THRESHOLD);
        state.active_system = Some(centre.clone());
        let seen = observe(&state, &hub);
        let choice = move_choice(&away, &[("carrier", 4)]);
        assert_eq!(
            pick_move(&choice, &seen, &me()),
            Some(0),
            "the carrier comes home"
        );
        // With the transport there, nothing more.
        put(&mut state, &centre, "carrier", &me(), 1);
        let seen = observe(&state, &hub);
        assert_eq!(pick_move(&choice, &seen, &me()), Some(1));
    }

    #[test]
    fn payment_wastes_least() {
        let pay = |id: &str, worth: i64| {
            ChoiceOption::labelled(id, "pay", id)
                .with("worth", worth)
                .with("owed", 1)
        };
        let choice = Choice::new(
            me(),
            "pay 1",
            vec![pay("big", 3), pay("small", 1), pay("mid", 2)],
        );
        let (state, hub, _, _) = board();
        let seen = observe(&state, &hub);
        assert_eq!(pick_pay(&choice, &seen, &me()), Some(1));
    }

    /// The Technology secondary's price is asked as an ordinary `pay_resources` choice (rules
    /// version `RULES_TECH_PAYMENT_CHOICE`): its options are `pay` options, so the bot's payment
    /// logic answers it and spends the least wasteful face on the 4-resource bill.
    #[test]
    fn a_technology_payment_is_answered_like_any_resource_payment() {
        let pay = |id: &str, worth: i64, owed: i64| {
            ChoiceOption::labelled(id, "pay", id)
                .with("worth", worth)
                .with("owed", owed)
                .with("kind", "resources")
        };
        let choice = Choice::new(
            me(),
            "pay 4 more resources",
            vec![
                pay("trade_good", 1, 4),
                pay("exhaust|big", 5, 4),
                pay("exhaust|fit", 4, 4),
            ],
        );
        assert!(choice.options.iter().all(|o| o.kind == "pay"));
        let (state, hub, _, _) = board();
        let seen = observe(&state, &hub);
        let picked = pick_pay(&choice, &seen, &me()).expect("an answer");
        assert_ne!(picked, 1, "the five-resource face wastes one");
    }

    #[test]
    fn trades_are_taken_only_slightly_in_its_favour_and_proposed_only_when_both_gain() {
        let answer = |net: f64| {
            Choice::new(
                me(),
                "accept?",
                vec![
                    ChoiceOption::labelled("accept", "transaction", "accept").with("net", net),
                    ChoiceOption::labelled("refuse", "decline", "refuse"),
                    ChoiceOption::labelled("counter", "transaction", "counter-offer"),
                ],
            )
        };
        assert_eq!(pick_answer(&answer(1.0)), Some(0));
        assert_eq!(pick_answer(&answer(0.1)), Some(1));
        assert_eq!(pick_answer(&answer(-3.0)), Some(1));

        let offer = |id: &str, net: f64, theirs: f64| {
            ChoiceOption::labelled(id, "offer", id)
                .with("net", net)
                .with("their_net", theirs)
        };
        let propose = Choice::new(
            me(),
            "transaction",
            vec![
                offer("greedy", 3.0, -1.0),
                offer("fair", 1.0, 1.0),
                offer("fairer", 0.5, 2.0),
                ChoiceOption::decline(),
            ],
        );
        assert_eq!(pick_proposal(&propose), Some(1));
        let none = Choice::new(
            me(),
            "transaction",
            vec![offer("greedy", 3.0, -1.0), ChoiceOption::decline()],
        );
        assert_eq!(pick_proposal(&none), Some(1));
    }

    #[test]
    fn over_fleet_supply_the_cheapest_unit_goes() {
        let (state, hub, _, _) = board();
        let seen = observe(&state, &hub);
        let choice = Choice::new(
            me(),
            "remove a unit: over fleet supply in 01",
            vec![
                ChoiceOption::labelled("remove|0", "remove", "remove carrier"),
                ChoiceOption::labelled("remove|1", "remove", "remove destroyer"),
            ],
        );
        assert_eq!(pick_cheapest_removal(&choice, &seen), Some(1));
    }

    /// The seat-bound entry point, through the engine's own private ask.
    struct Smart;
    impl Decider for Smart {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            Ok(choice.options[0].clone())
        }
        fn choose_seeing(
            &mut self,
            choice: &Choice,
            seen: &SeatObservation<'_>,
        ) -> Result<ChoiceOption, IllegalChoice> {
            let mut rng = ChaCha8Rng::seed_from_u64(1);
            let index = pick(choice, seen, &mut rng).unwrap_or(0);
            Ok(choice.options[index].clone())
        }
    }

    struct Probe(Option<usize>);
    impl Decider for Probe {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            Ok(choice.options[0].clone())
        }
        fn choose_seeing(
            &mut self,
            choice: &Choice,
            seen: &SeatObservation<'_>,
        ) -> Result<ChoiceOption, IllegalChoice> {
            self.0 = pick(choice, seen, &mut ChaCha8Rng::seed_from_u64(1));
            Ok(choice.options[0].clone())
        }
    }

    #[test]
    fn pick_always_scores_and_leaves_questions_without_an_opinion_to_the_random_pick() {
        let (state, hub, _, _) = board();
        let ask = |choice: &Choice| {
            ti4_engine::choice::ask_private(
                choice,
                &state,
                ContentStore::embedded(),
                POK,
                Some(&hub.galaxy),
                &mut Smart,
            )
            .expect("legal")
            .id
        };
        let score = Choice::new(
            me(),
            "score",
            vec![
                ChoiceOption::decline(),
                ChoiceOption::labelled("x", "score", "score x"),
            ],
        );
        assert_eq!(ask(&score), "x");

        let research = Choice::new(
            me(),
            "research",
            vec![
                ChoiceOption::labelled("t1", "research", "t1"),
                ChoiceOption::labelled("t2", "research", "t2"),
            ],
        );
        let mut probe = Probe(Some(99));
        ti4_engine::choice::ask_private(
            &research,
            &state,
            ContentStore::embedded(),
            POK,
            Some(&hub.galaxy),
            &mut probe,
        )
        .expect("legal");
        assert_eq!(probe.0, None, "no opinion on research");
    }
}
