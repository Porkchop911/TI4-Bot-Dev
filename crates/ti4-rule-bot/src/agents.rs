//! The rule bot's agent layer: when to use a faction agent, and the questions that follow.
//!
//! Plan and per-faction reasoning: `crates/ti4-server/docs/FACTION_AGENTS_EARLY.md`. In short:
//!
//! - **Reactions** (`reaction_*` windows offering `leader:...:<agent>:...`): an agent whose effect
//!   only helps the player it acts for is used for the bot itself when the effect is clearly good
//!   (a free mech, two infantry, a production discount, a command token, an exploration, a
//!   cancelled hit, two fighters, a doubled trade-good gain, a raised move that lets a loaded
//!   transport reach) and never for another seat. The Cabal agent is used on a 3+ commodity
//!   replenish. A Ssruu copy follows the copied agent's rule.
//! - **Component actions** in the action-phase choice: the Arborec turn a cruiser or destroyer
//!   into a carrier in the opening while short of transports; Muaat build the opening's home
//!   transport with Umbat where the plan wants one (no token at home); Naalu take their best
//!   tactical action through Z'eu, without a command token. Instead of passing (or with no tactic
//!   token left) any of these, and the Xxcha readying a planet, is a free extra action. At the end of a turn, Nekro discard an action card for two trade goods
//!   and Hacan take two commodities.
//! - **Follow-ups**: always choose the bot itself, its own ship, its own planet, the dearest
//!   replacement or capture; never remove its own infantry; Mahact pay a secondary with the
//!   active player's token instead of a strategy token.
//!
//! Only the bound seat's view is read (public board plus its own hand); every answer is one of the
//! offered options, and the choice of one is deterministic for a given position.

use rand_chacha::ChaCha8Rng;
use ti4_engine::choice::{Choice, ChoiceOption, Observed, SeatObservation};
use ti4_engine::decision_context::DecisionTrigger;
use ti4_model::id::{PlanetId, PlayerId, SystemId};

use super::{
    ACTIVATION_THRESHOLD, CARRIER_COST, activation_value, exhausted_resources, expanding,
    free_transports_and_ground, home_build_wanted, home_system, is_ground, pick_action,
    pick_secondary, spare_resources, target_near, transport_plan, unit_type, waiting_and_lift,
};

/// Every agent this layer has a rule for (Ssruu, `yssarilagent`, carries the others' texts).
const AGENTS: &[&str] = &[
    "arborecagent",
    "argentagent",
    "cabalagent",
    "empyreanagent",
    "ghostagent",
    "hacanagent",
    "jolnaragent",
    "l1z1xagent",
    "letnevagent",
    "mahactagent",
    "mentakagent",
    "muaatagent",
    "naaluagent",
    "naaluagent-te",
    "naazagent",
    "nekroagent",
    "nomadagentartuno",
    "nomadagentmercer",
    "nomadagentthundarian",
    "saaragent",
    "sardakkagent",
    "solagent",
    "titansagent",
    "winnuagent",
    "xxchaagent",
    "yinagent",
];

/// Commodity value from which the Cabal agent's capture (a carrier or better) is worth the gift.
pub const CABAL_COMMODITIES: i32 = 3;
/// Ground forces kept at home beyond what its transports lift, while expanding.
const HOME_RESERVE: usize = 2;

/// The agent an offered ability or component option stands for: the copied agent for a Ssruu
/// offer, else the seat's own. `None` for anything that is not an agent.
#[must_use]
pub fn agent_of(id: &str) -> Option<&str> {
    let parts: Vec<&str> = if let Some(rest) = id.strip_prefix("component|leader|") {
        rest.split('|').collect()
    } else if id.starts_with("leader:") {
        id.split(':').collect()
    } else {
        return None;
    };
    parts.into_iter().rev().find(|part| AGENTS.contains(part))
}

/// The seat an event's agent would act for: the victim of hits and destroyed units, otherwise the
/// seat that acted.
fn beneficiary<'t>(trigger: Option<&'t DecisionTrigger>, agent: &str) -> Option<&'t PlayerId> {
    let trigger = trigger?;
    if matches!(agent, "titansagent" | "yinagent") {
        trigger.subject.as_ref().or(trigger.actor.as_ref())
    } else {
        trigger.actor.as_ref().or(trigger.subject.as_ref())
    }
}

/// The agent layer's answer to `choice`, or `None` to leave it to the other rules. Never the
/// action-phase choice itself (see [`action_choice`]).
#[must_use]
pub fn pick(choice: &Choice, seen: &SeatObservation<'_>, me: &PlayerId) -> Option<usize> {
    decide(choice, seen.observed(), me, seen.held_action_cards().len())
}

/// [`pick`] on the public view plus the size of the bot's own hand.
#[must_use]
pub fn decide(choice: &Choice, seen: &Observed<'_>, me: &PlayerId, held: usize) -> Option<usize> {
    let subtype = choice.context.as_ref().map(|c| c.subtype.as_str());
    let by_id = |id: &str| choice.options.iter().position(|o| o.id == id);
    let decline = || choice.options.iter().position(ChoiceOption::is_decline);
    match subtype {
        Some(s) if s.starts_with("reaction_") => return reaction(choice, seen, me),
        Some("end_turn") => return end_of_turn(choice, seen, me, held),
        Some("ospha_ship") => return ospha_ship(choice, seen, me),
        Some("ospha_replace") => return ospha_replace(choice, seen, me),
        Some("umbat_player" | "malleon_player") => {
            return by_id(me.as_str()).or_else(decline);
        }
        Some("malleon_payment") => return by_id("discard").or_else(decline),
        Some("leader_hacanagent_branch") => return by_id("self"),
        Some("leader_xxchaagent_ready_planet") => return own_richest_planet(choice, seen, me),
        Some("leader_xxchaagent_remove_infantry") => return by_id("keep"),
        Some("l1z1x_agent_swap") => {
            let mine = beneficiary(choice.context.as_ref()?.trigger.as_ref(), "l1z1xagent")
                .is_some_and(|who| who == me);
            return if mine { by_id("use") } else { decline() };
        }
        Some("agent_ship") => return saar_ship(choice, seen, me).or_else(decline),
        Some("stillness_target") => return dearest_unit(choice, seen),
        Some("suffi_an") => return by_id("exhaust"),
        // Kept: an own use was measured to break the replay of later rounds (a Ssruu copy of
        // Sucaban is offered on replay that the live game did not offer; seeds 19 and 30 of the
        // turn-one survey, `FACTION_AGENTS_EARLY.md`), and its value in rounds 1-3 is low.
        Some("doctor_sucaban_exhaust" | "doctor_sucaban_borrowed_exhaust") => return decline(),
        Some("doctor_sucaban_remove_infantry") => {
            return spare_site(choice, seen, me, |id| {
                let (system, planet) = id.split_once(':')?;
                Some((
                    SystemId::new(system),
                    (!planet.is_empty()).then(|| planet.to_owned()),
                ))
            })
            .or_else(decline);
        }
        Some("mercer_take") => {
            return spare_site(choice, seen, me, |id| {
                // take|<system>|<planet or space>|<unit>|<index>
                let mut parts = id.strip_prefix("take|")?.split('|');
                let system = parts.next()?;
                let planet = parts.next()?;
                Some((
                    SystemId::new(system),
                    (planet != "space").then(|| planet.to_owned()),
                ))
            })
            .or_else(decline);
        }
        Some("leader_ssruu_round_agent_unit") => return own_round_unit(choice, seen, me),
        Some("zeu_activate_system") => return zeu_system(choice, seen, me).or_else(decline),
        _ => {}
    }
    mahact_waiver(choice, seen, me)
}

// -- reactions -----------------------------------------------------------------------------------

/// A reaction window: use the first agent worth using; with every agent on offer not worth it and
/// nothing else to choose, decline; otherwise no opinion.
fn reaction(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    let trigger = choice.context.as_ref().and_then(|c| c.trigger.as_ref());
    let mut unwanted = 0;
    for (index, option) in choice.options.iter().enumerate() {
        let Some(agent) = agent_of(&option.id) else {
            continue;
        };
        match use_in_window(agent, trigger, seen, me) {
            Some(true) => return Some(index),
            Some(false) => unwanted += 1,
            None => return None,
        }
    }
    let declinable = choice.options.iter().any(ChoiceOption::is_decline);
    (unwanted > 0 && unwanted + usize::from(declinable) == choice.options.len())
        .then(|| choice.options.iter().position(ChoiceOption::is_decline))
        .flatten()
}

/// Whether to use `agent` in the window `trigger` opened: `Some(true)` use, `Some(false)` keep it,
/// `None` no opinion.
fn use_in_window(
    agent: &str,
    trigger: Option<&DecisionTrigger>,
    seen: &Observed<'_>,
    me: &PlayerId,
) -> Option<bool> {
    let mine = beneficiary(trigger, agent).is_some_and(|who| who == me);
    let decided = match agent {
        // Another seat replenishes: capture a carrier or better from it.
        "cabalagent" => {
            let replenisher = beneficiary(trigger, agent)?;
            replenisher != me
                && seen
                    .seat(replenisher)
                    .is_some_and(|seat| seat.commodities >= CABAL_COMMODITIES)
        }
        "mentakagent" => true,
        // No dice in the window: a combat it is not in is none of its business, its own a coin.
        "nomadagentthundarian" => {
            if in_combat(seen, me) {
                return None;
            }
            false
        }
        // Free units, discounts, tokens, explorations, cancelled hits: for itself only.
        "sardakkagent" | "winnuagent" | "yinagent" | "naazagent" | "l1z1xagent"
        | "empyreanagent" | "titansagent" | "nomadagentartuno" | "naaluagent-te" => mine,
        // Its own combats: the copied extra die (Ssruu) on its own unit.
        "letnevagent" | "solagent" => in_combat(seen, me),
        // New troops belong at the dock where the transports load while expanding.
        "argentagent" => mine && !expanding(seen),
        "nomadagentmercer" => mine && spare_ground_total(seen, me) > 0,
        "saaragent" => {
            mine && trigger
                .and_then(|t| t.system.as_ref())
                .is_some_and(|system| !saar_candidates(seen, me, system).is_empty())
        }
        "ghostagent" => {
            mine && trigger
                .and_then(|t| t.system.as_ref())
                .is_some_and(|system| ships_in_other_wormhole(seen, me, system))
        }
        _ => return None,
    };
    Some(decided)
}

/// Whether the bot fights the current space combat or invasion.
fn in_combat(seen: &Observed<'_>, me: &PlayerId) -> bool {
    seen.space_battle()
        .is_some_and(|(_, a, b)| &a == me || &b == me)
        || seen.invasion().is_some_and(|invasion| {
            &invasion.invader == me || invasion.defender.as_ref() == Some(me)
        })
}

/// Whether the bot has a ship in a wormhole system other than `active` (Emissary Taivra's link
/// lets it move in from there).
fn ships_in_other_wormhole(seen: &Observed<'_>, me: &PlayerId, active: &SystemId) -> bool {
    let Some(galaxy) = seen.galaxy() else {
        return false;
    };
    galaxy
        .wormhole_systems()
        .into_iter()
        .filter(|system| *system != active.as_str())
        .any(|system| {
            seen.system(&SystemId::new(system)).units.iter().any(|u| {
                &u.owner == me && unit_type(seen, u.type_id.as_str()).is_some_and(|t| t.is_ship())
            })
        })
}

// -- Captain Mendosa (Saar) ----------------------------------------------------------------------

/// The fastest printed move of any ship on the board.
fn top_move(seen: &Observed<'_>) -> i64 {
    seen.board()
        .values()
        .flat_map(|s| s.units.iter())
        .filter_map(|u| unit_type(seen, u.type_id.as_str()))
        .filter(ti4_content::units::UnitType::is_ship)
        .map(|t| t.move_value())
        .max()
        .unwrap_or(0)
}

/// Own transports with ground forces to load that cannot reach `active` on their own move but can
/// on the board's fastest: `(origin, unit type)`.
fn saar_candidates(
    seen: &Observed<'_>,
    me: &PlayerId,
    active: &SystemId,
) -> Vec<(SystemId, String)> {
    let top = top_move(seen);
    let gravity = i64::from(seen.seat(me).is_some_and(|seat| {
        seat.technologies
            .iter()
            .any(|t| t.as_str() == super::GRAVITY_DRIVE)
    }));
    let mut found = Vec::new();
    for (origin, system) in seen.board() {
        if origin == active {
            continue;
        }
        let loadable = seen
            .loadable(me, origin)
            .iter()
            .any(|cargo| is_ground(seen, &cargo.unit));
        if !loadable {
            continue;
        }
        for unit in system.units.iter().filter(|u| &u.owner == me) {
            let Some(kind) = unit_type(seen, unit.type_id.as_str()) else {
                continue;
            };
            if !kind.is_ship() || kind.capacity() <= 0 || kind.move_value() >= top {
                continue;
            }
            let own = i32::try_from(kind.move_value() + gravity).unwrap_or(0);
            let raised = i32::try_from(top + gravity).unwrap_or(0);
            if !seen.can_reach(me, origin, active, own)
                && seen.can_reach(me, origin, active, raised)
            {
                found.push((origin.clone(), unit.type_id.to_string()));
            }
        }
    }
    found
}

/// `agent_ship`: the option naming a candidate transport (`<unit> in <system> (move a to b)`).
fn saar_ship(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    let active = seen.active_system()?;
    let candidates = saar_candidates(seen, me, active);
    choice.options.iter().position(|o| {
        candidates
            .iter()
            .any(|(origin, unit)| o.label.starts_with(&format!("{unit} in {origin} (")))
    })
}

// -- spare ground forces (Doctor Sucaban, Mercer) ------------------------------------------------

/// Ground forces of the bot's that no plan needs where they stand: all but one on a planet away
/// from home; at home, those beyond what its transports there lift plus a small reserve while
/// expanding, beyond the reserve later.
fn spare_at(seen: &Observed<'_>, me: &PlayerId, system: &SystemId, planet: Option<&str>) -> usize {
    let board = seen.system(system);
    let home = home_system(seen, me).is_some_and(|h| &h == system);
    let here = |p: &PlanetId| {
        board.planet_units.get(p).map_or(0, |units| {
            units
                .iter()
                .filter(|u| &u.owner == me && is_ground(seen, u))
                .count()
        })
    };
    if home {
        let (waiting, lift) = waiting_and_lift(seen, me, system);
        let keep = if expanding(seen) {
            usize::try_from(lift).unwrap_or(0) + HOME_RESERVE
        } else {
            HOME_RESERVE
        };
        return waiting.saturating_sub(keep);
    }
    match planet {
        Some(planet) => here(&PlanetId::new(planet)).saturating_sub(1),
        None => 0,
    }
}

/// Spare ground forces across the board.
fn spare_ground_total(seen: &Observed<'_>, me: &PlayerId) -> usize {
    let home = home_system(seen, me);
    let mut total = home.as_ref().map_or(0, |h| spare_at(seen, me, h, None));
    for (system, planet) in seen.controlled_planets(me) {
        if home.as_ref() == Some(system) {
            continue;
        }
        total += spare_at(seen, me, system, Some(planet.as_str()));
    }
    total
}

/// The first offered site holding a spare ground force, `parse` reading an option id as
/// `(system, planet)`.
fn spare_site(
    choice: &Choice,
    seen: &Observed<'_>,
    me: &PlayerId,
    parse: impl Fn(&str) -> Option<(SystemId, Option<String>)>,
) -> Option<usize> {
    choice.options.iter().position(|o| {
        parse(&o.id).is_some_and(|(system, planet)| {
            seen.active_system().is_none_or(|active| active != &system)
                && spare_at(seen, me, &system, planet.as_deref()) > 0
        })
    })
}

// -- follow-ups ----------------------------------------------------------------------------------

/// `ospha_ship` (`<owner>'s <unit>[ (damaged)] in <system>`): the bot's own cheapest non-carrier
/// ship (a cruiser or destroyer becomes a carrier), outside its tokened systems first.
fn ospha_ship(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    let prefix = format!("{me}'s ");
    let tokens = seen.systems_with_token(me);
    choice
        .options
        .iter()
        .enumerate()
        .filter_map(|(i, o)| {
            let rest = o.label.strip_prefix(&prefix)?;
            let (unit, system) = rest.split_once(" in ")?;
            let unit = unit.trim_end_matches(" (damaged)");
            let kind = unit_type(seen, unit)?;
            let tokened = tokens.contains(&SystemId::new(system));
            let carrier = kind.capacity() >= 4;
            // Lowest is best: not a carrier, not tokened, cheapest.
            #[expect(clippy::cast_possible_truncation, reason = "unit costs are small")]
            let cost = (kind.cost() * 2.0) as i64;
            Some((i, (carrier, tokened, cost)))
        })
        .min_by_key(|&(i, key)| (key, i))
        .map(|(i, _)| i)
}

/// `ospha_replace`: a carrier while the opening is short of transports, else the dearest ship.
fn ospha_replace(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    let (transports, _) = free_transports_and_ground(seen, me);
    let carrier = choice
        .options
        .iter()
        .position(|o| o.id == "replace|carrier");
    if expanding(seen) && transports < 3 && carrier.is_some() {
        return carrier;
    }
    choice
        .options
        .iter()
        .enumerate()
        .filter_map(|(i, o)| {
            let base = o.id.strip_prefix("replace|")?;
            let kind = unit_type(seen, base)?;
            #[expect(clippy::cast_possible_truncation, reason = "unit costs are small")]
            Some((i, (kind.cost() * 2.0) as i64))
        })
        .max_by_key(|&(i, cost)| (cost, std::cmp::Reverse(i)))
        .map(|(i, _)| i)
        .or(carrier)
}

/// `stillness_target`: the dearest unit, a carrier on a tie.
fn dearest_unit(choice: &Choice, seen: &Observed<'_>) -> Option<usize> {
    choice
        .options
        .iter()
        .enumerate()
        .filter_map(|(i, o)| {
            let kind = unit_type(seen, &o.id)?;
            #[expect(clippy::cast_possible_truncation, reason = "unit costs are small")]
            Some((i, ((kind.cost() * 2.0) as i64, kind.capacity())))
        })
        .max_by_key(|&(i, key)| (key, std::cmp::Reverse(i)))
        .map(|(i, _)| i)
}

/// `leader_xxchaagent_ready_planet`: the bot's own planet with the most resources (then
/// influence); no opinion when none of its planets is offered.
fn own_richest_planet(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    let own: Vec<PlanetId> = seen
        .controlled_planets(me)
        .into_iter()
        .map(|(_, p)| p.clone())
        .collect();
    choice
        .options
        .iter()
        .enumerate()
        .filter(|(_, o)| own.iter().any(|p| p.as_str() == o.id))
        .filter_map(|(i, o)| {
            let record = ti4_content::galaxy::planet(seen.content(), &o.id, seen.sources())?;
            Some((i, (record.resources(), record.influence())))
        })
        .max_by_key(|&(i, key)| (key, std::cmp::Reverse(i)))
        .map(|(i, _)| i)
}

/// `leader_ssruu_round_agent_unit` (`<owner>|<planet>|<unit>|<index>|<marks>`): the bot's own
/// dearest unit.
fn own_round_unit(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    choice
        .options
        .iter()
        .enumerate()
        .filter_map(|(i, o)| {
            let mut parts = o.id.split('|');
            if parts.next()? != me.as_str() {
                return None;
            }
            let kind = unit_type(seen, parts.nth(1)?)?;
            #[expect(clippy::cast_possible_truncation, reason = "unit costs are small")]
            Some((i, (kind.cost() * 2.0) as i64))
        })
        .max_by_key(|&(i, cost)| (cost, std::cmp::Reverse(i)))
        .map(|(i, _)| i)
}

/// `zeu_activate_system`: the best system by activation value, if it is worth a tactical action.
fn zeu_system(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    seen.galaxy()?;
    choice
        .options
        .iter()
        .enumerate()
        .filter(|(_, o)| !o.is_decline())
        .map(|(i, o)| (i, activation_value(seen, me, &SystemId::new(&o.id))))
        .filter(|(_, worth)| *worth >= ACTIVATION_THRESHOLD)
        .fold(None, |best: Option<(usize, f64)>, (i, w)| match best {
            Some((_, b)) if b >= w => best,
            _ => Some((i, w)),
        })
        .map(|(i, _)| i)
}

/// Jae Mir Kan inside a secondary prompt (`follow|waived|...|agent|<system>`): follow through the
/// agent whenever the bot would follow anyway, and for the secondaries that cost nothing but the
/// token. The token comes from the system with the fewest units in it.
fn mahact_waiver(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    let waivers: Vec<(usize, &str)> = choice
        .options
        .iter()
        .enumerate()
        .filter_map(|(i, o)| {
            o.id.starts_with("follow|waived|")
                .then(|| o.id.rsplit_once("|agent|").map(|(_, system)| (i, system)))
                .flatten()
        })
        .collect();
    if waivers.is_empty() {
        return None;
    }
    let prompt = choice.prompt.as_str();
    let probe = Choice::new(
        me.clone(),
        prompt,
        vec![
            ChoiceOption::new("yes", "follow"),
            ChoiceOption::new("no", "follow"),
        ],
    );
    let follow = pick_secondary(&probe, seen, me);
    let cheap = prompt.contains("draw a secret objective")
        || prompt.contains("draw two action cards")
        || prompt.contains("replenish commodities")
        || (prompt.contains("ready two planets") && exhausted_resources(seen, me) > 0);
    if follow == Some(0) || cheap {
        return waivers
            .iter()
            .min_by_key(|(i, system)| (seen.system(&SystemId::new(*system)).units.len(), *i))
            .map(|(i, _)| *i);
    }
    match follow {
        Some(_) => choice.options.iter().position(|o| o.id == "no"),
        None => None,
    }
}

// -- the action phase ----------------------------------------------------------------------------

/// The index of the component option for `agent`: the seat's own, or a Ssruu copy of an agent
/// whose effect is the bot's own whatever its board (a copied Umbat or Ospha needs a war sun or a
/// fitting ship the copier rarely has, and a copied use that resolves nothing is worse than none:
/// see `FACTION_AGENTS_EARLY.md`, replay divergences).
fn component(choice: &Choice, agent: &str) -> Option<usize> {
    let own = format!("component|leader|{agent}");
    let copy = format!("component|leader|yssarilagent|{agent}");
    let copyable = matches!(agent, "xxchaagent" | "nekroagent" | "hacanagent");
    choice
        .options
        .iter()
        .position(|o| o.id == own || (copyable && o.id == copy))
}

/// Whether the bot has a ship of its own outside its tokened systems that Letani Ospha could make a
/// carrier of: a non-fighter ship without capacity for four.
fn ospha_target(seen: &Observed<'_>, me: &PlayerId) -> bool {
    let tokens = seen.systems_with_token(me);
    seen.board().iter().any(|(id, system)| {
        !tokens.contains(id)
            && system.units.iter().any(|u| {
                &u.owner == me
                    && unit_type(seen, u.type_id.as_str()).is_some_and(|t| {
                        t.is_ship() && !t.is_fighter() && t.capacity() < 4 && t.cost() <= 2.0
                    })
            })
    })
}

/// Where Umbat would build for the bot: a system with its war sun or flagship and no token of its,
/// with a planet still in reach of a new carrier.
fn umbat_useful(seen: &Observed<'_>, me: &PlayerId) -> bool {
    let tokens = seen.systems_with_token(me);
    seen.board().iter().any(|(id, system)| {
        !tokens.contains(id)
            && system.units.iter().any(|u| {
                &u.owner == me
                    && unit_type(seen, u.type_id.as_str())
                        .is_some_and(|t| matches!(t.base_type(), "warsun" | "flagship"))
            })
            && target_near(seen, me, id)
    })
}

/// The opening's transport agents: the Arborec cruiser into a carrier and Muaat's Umbat build,
/// while the round-1 plan is short of transports and a tactic token is left to send them.
fn opening_agent(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    if !expanding(seen) || seen.seat(me).is_none_or(|seat| seat.tactic_tokens <= 0) {
        return None;
    }
    let (transports, _) = free_transports_and_ground(seen, me);
    let short = transport_plan(seen, me).is_some() || transports < 2;
    if !short {
        return None;
    }
    if let Some(index) = component(choice, "arborecagent")
        && ospha_target(seen, me)
    {
        return Some(index);
    }
    None
}

/// Umbat as the opening's token-free home build: where the plan wants a new transport at home
/// (`home_build_wanted`, the step Warfare's or Construction's production would otherwise take),
/// the war sun's system builds it without a command token. Measured: used as the very first
/// action instead, it cost Muaat a system (the resources and the turn went before the
/// transports had left).
fn umbat_home_build(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    let index = component(choice, "muaatagent")?;
    (expanding(seen)
        && seen.seat(me).is_some_and(|seat| seat.tactic_tokens > 0)
        && home_build_wanted(seen, me)
        && spare_resources(seen, me) >= CARRIER_COST
        && umbat_useful(seen, me))
    .then_some(index)
}

/// A free action instead of passing (or with no tactic token left): whichever agent still has
/// something to do.
fn stalling_agent(
    choice: &Choice,
    seen: &Observed<'_>,
    me: &PlayerId,
    held: usize,
    passing: bool,
) -> Option<usize> {
    // The ones that do not end the turn first.
    if let Some(index) = component(choice, "nekroagent")
        && held > 0
    {
        return Some(index);
    }
    if let Some(index) = component(choice, "hacanagent")
        && hacan_short(seen, me)
    {
        return Some(index);
    }
    // Only instead of an actual pass: with no tactic token left, the cards still to play come
    // first (measured: readying a planet then cost the Xxcha systems).
    if let Some(index) = component(choice, "xxchaagent")
        && passing
        && own_exhausted_value(seen, me) > 0
    {
        return Some(index);
    }
    if let Some(index) = component(choice, "arborecagent")
        && ospha_target(seen, me)
    {
        return Some(index);
    }
    if let Some(index) = component(choice, "muaatagent")
        && spare_resources(seen, me) > CARRIER_COST
    {
        return Some(index);
    }
    None
}

/// Resources plus influence on the bot's exhausted planets.
fn own_exhausted_value(seen: &Observed<'_>, me: &PlayerId) -> i64 {
    seen.controlled_planets(me)
        .into_iter()
        .filter(|(_, planet)| !seen.planet_is_ready(planet))
        .filter_map(|(_, planet)| {
            ti4_content::galaxy::planet(seen.content(), planet.as_str(), seen.sources())
        })
        .map(|record| record.resources() + record.influence())
        .sum()
}

/// Whether Hacan are two or more below their commodity value.
fn hacan_short(seen: &Observed<'_>, me: &PlayerId) -> bool {
    let Some(seat) = seen.seat(me) else {
        return false;
    };
    ti4_content::factions::get(seen.content(), seat.faction.as_str())
        .is_some_and(|faction| seat.commodities + 2 <= faction.commodities())
}

/// After the turn's action: the agents that do not take an action.
fn end_of_turn(choice: &Choice, seen: &Observed<'_>, me: &PlayerId, held: usize) -> Option<usize> {
    if let Some(index) = component(choice, "nekroagent")
        && held > 0
    {
        return Some(index);
    }
    if let Some(index) = component(choice, "hacanagent")
        && hacan_short(seen, me)
    {
        return Some(index);
    }
    None
}

/// The action-phase choice when an agent is on offer: `Some(answer)` replaces the ordinary action
/// rule, `None` leaves the choice to it (no agent offered, or not the action choice).
#[must_use]
pub fn action_choice(
    choice: &Choice,
    seen: &SeatObservation<'_>,
    me: &PlayerId,
    rng: &mut ChaCha8Rng,
) -> Option<Option<usize>> {
    action_decision(
        choice,
        seen.observed(),
        me,
        seen.held_action_cards().len(),
        rng,
    )
}

/// [`action_choice`] on the public view plus the size of the bot's own hand.
#[must_use]
pub fn action_decision(
    choice: &Choice,
    seen: &Observed<'_>,
    me: &PlayerId,
    held: usize,
    rng: &mut ChaCha8Rng,
) -> Option<Option<usize>> {
    let is_action = choice.options.iter().any(|o| {
        o.kind == "action"
            && (o.id == "tactical" || o.id == "pass" || o.id.starts_with("strategic"))
    });
    let agents = choice
        .options
        .iter()
        .any(|o| o.id.starts_with("component|leader|") && agent_of(&o.id).is_some());
    if !is_action || !agents {
        return None;
    }
    let tactical = choice.options.iter().position(|o| o.id == "tactical");
    let base = if tactical.is_some() {
        pick_action(choice, seen, me, rng)
    } else {
        None
    };
    if let Some(index) = opening_agent(choice, seen, me) {
        return Some(Some(index));
    }
    if let Some(index) = umbat_home_build(choice, seen, me) {
        return Some(Some(index));
    }
    // Z'eu: when the rule takes a tactical action, take it without a command token in the best
    // system Z'eu may activate (not home, no token of its own) if that one is worth a tactical
    // action; a home build, if that was the best, keeps the token for the next turn.
    if let (Some(t), Some(index)) = (tactical, component(choice, "naaluagent"))
        && base == Some(t)
        && zeu_best_is_away(seen, me)
    {
        return Some(Some(index));
    }
    let passing = base.is_some_and(|i| choice.options[i].id == "pass");
    if (passing || base.is_none())
        && let Some(index) = stalling_agent(choice, seen, me, held, passing)
    {
        return Some(Some(index));
    }
    // Left to chance, the random pick could take a Ssruu copy of an action agent that resolves
    // nothing for the copier (a copied Umbat without a war sun); such a failed use was measured to
    // break the game's replay (`FACTION_AGENTS_EARLY.md`). Play the card or pass instead.
    if base.is_none()
        && choice
            .options
            .iter()
            .any(|o| o.id.starts_with("component|leader|yssarilagent|"))
    {
        return Some(
            choice
                .options
                .iter()
                .position(|o| o.kind == "action" && o.id.starts_with("strategic"))
                .or_else(|| choice.options.iter().position(|o| o.id == "pass")),
        );
    }
    Some(base)
}

/// Whether some system Z'eu may activate (away from home, without the bot's token) is worth a
/// tactical action.
fn zeu_best_is_away(seen: &Observed<'_>, me: &PlayerId) -> bool {
    let Some(galaxy) = seen.galaxy() else {
        return false;
    };
    let tokens = seen.systems_with_token(me);
    let homes: Vec<SystemId> = seen
        .players()
        .into_iter()
        .filter_map(|player| home_system(seen, player))
        .collect();
    galaxy
        .system_ids()
        .into_iter()
        .map(SystemId::new)
        .filter(|system| !tokens.contains(system) && !homes.contains(system))
        .any(|system| activation_value(seen, me, &system) >= ACTIVATION_THRESHOLD)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use ti4_content::ContentStore;
    use ti4_engine::decision_context::{DecisionContext, DecisionSource, TriggerKind};
    use ti4_engine::fixtures::{Hub, game, hub_from, plain_systems, put};
    use ti4_model::POK;
    use ti4_model::state::{GameState, Phase};

    fn me() -> PlayerId {
        PlayerId::new("a")
    }

    fn foe() -> PlayerId {
        PlayerId::new("b")
    }

    fn hub() -> Hub {
        hub_from(&plain_systems(60).into_iter().take(7).collect::<Vec<_>>())
    }

    fn observe<'a>(state: &'a GameState, hub: &'a Hub) -> Observed<'a> {
        Observed::new(state, ContentStore::embedded(), POK, Some(&hub.galaxy))
    }

    fn trigger(actor: &PlayerId, kind: TriggerKind, event: &str) -> DecisionTrigger {
        DecisionTrigger {
            kind,
            event_type: event.to_owned(),
            event_id: 1,
            relation: "after".to_owned(),
            actor: Some(actor.clone()),
            subject: None,
            card: None,
            agenda: None,
            system: None,
            planet: None,
            units: Vec::new(),
            hits: None,
            chain: Vec::new(),
        }
    }

    fn window(ability: &str, event: &str, actor: &PlayerId, kind: TriggerKind) -> Choice {
        Choice::new(
            me(),
            format!("after {event}"),
            vec![
                ChoiceOption::labelled(ability, "ability", ability),
                ChoiceOption::decline(),
            ],
        )
        .contextualized(
            DecisionContext::new(
                me(),
                DecisionSource::Reaction(event.to_owned()),
                format!("reaction_after_{event}"),
                Phase::Action,
                1,
            )
            .with_trigger(trigger(actor, kind, event)),
        )
    }

    fn contextual(choice: Choice, subtype: &str) -> Choice {
        choice.contextualized(DecisionContext::new(
            me(),
            DecisionSource::Rule("test".to_owned()),
            subtype,
            Phase::Action,
            1,
        ))
    }

    #[test]
    fn agent_ids_are_read_from_own_and_copied_offers() {
        assert_eq!(
            agent_of("leader:sardakk:sardakkagent:ACTION_COMPLETED:after"),
            Some("sardakkagent")
        );
        assert_eq!(
            agent_of("leader:saar:p1:yssarilagent:p2:saaragent:SYSTEM_ACTIVATED:after"),
            Some("saaragent")
        );
        assert_eq!(
            agent_of("component|leader|yssarilagent|muaatagent"),
            Some("muaatagent")
        );
        assert_eq!(
            agent_of("component|leader|arborecagent"),
            Some("arborecagent")
        );
        assert_eq!(
            agent_of("leader:sardakk:sardakkagent_mark:SYSTEM_ACTIVATED:after"),
            None
        );
        assert_eq!(agent_of("reaction:sabotage"), None);
    }

    #[test]
    fn a_self_only_agent_is_used_for_the_bot_and_never_for_another_seat() {
        let hub = hub();
        let state = game(&["a", "b"]);
        let seen = observe(&state, &hub);
        for (ability, event) in [
            (
                "leader:sardakk:sardakkagent:ACTION_COMPLETED:after",
                "TACTICAL_ACTION_ENDED",
            ),
            (
                "leader:winnu:winnuagent:PRODUCTION_USED:after",
                "PRODUCTION_USED",
            ),
            ("leader:naaz:naazagent:TURN_PASSED:after", "TURN_PASSED"),
            (
                "leader:l1z1x:l1z1xagent:SYSTEM_ACTIVATED:after",
                "SYSTEM_ACTIVATED",
            ),
        ] {
            let own = window(ability, event, &me(), TriggerKind::Other);
            assert_eq!(
                decide(&own, &seen, &me(), 0),
                Some(0),
                "{ability} for itself"
            );
            let theirs = window(ability, event, &foe(), TriggerKind::Other);
            assert_eq!(
                decide(&theirs, &seen, &me(), 0),
                Some(1),
                "{ability} for another"
            );
        }
    }

    #[test]
    fn the_yin_and_titans_agents_follow_the_victim_of_the_hit() {
        let hub = hub();
        let state = game(&["a", "b"]);
        let seen = observe(&state, &hub);
        let mut own = window(
            "leader:yin:yinagent:SHIP_DESTROYED:after",
            "SHIP_DESTROYED",
            &foe(),
            TriggerKind::ShipDestroyed,
        );
        if let Some(context) = own.context.as_mut()
            && let Some(trigger) = context.trigger.as_mut()
        {
            trigger.actor = None;
            trigger.subject = Some(me());
        }
        assert_eq!(decide(&own, &seen, &me(), 0), Some(0));
        let mut theirs = own.clone();
        if let Some(context) = theirs.context.as_mut()
            && let Some(trigger) = context.trigger.as_mut()
        {
            trigger.subject = Some(foe());
        }
        assert_eq!(decide(&theirs, &seen, &me(), 0), Some(1));
    }

    #[test]
    fn the_cabal_captures_from_a_rich_replenish_and_takes_the_dearest_unit() {
        let hub = hub();
        let mut state = game(&["a", "b"]);
        state.player_mut(&foe()).unwrap().commodities = 3;
        let seen = observe(&state, &hub);
        let ability = "leader:cabal:cabalagent:COMMODITIES_REPLENISHED:after";
        let rich = window(
            ability,
            "COMMODITIES_REPLENISHED",
            &foe(),
            TriggerKind::Other,
        );
        assert_eq!(decide(&rich, &seen, &me(), 0), Some(0));
        let mut poor_state = state.clone();
        poor_state.player_mut(&foe()).unwrap().commodities = 2;
        let poor = observe(&poor_state, &hub);
        assert_eq!(decide(&rich, &poor, &me(), 0), Some(1));
        let target = contextual(
            Choice::new(
                me(),
                "capture",
                ["fighter", "carrier", "cruiser", "destroyer", "infantry"]
                    .iter()
                    .map(|u| ChoiceOption::new(*u, "unit"))
                    .collect(),
            ),
            "stillness_target",
        );
        assert_eq!(decide(&target, &seen, &me(), 0), Some(1), "the carrier");
    }

    #[test]
    fn follow_ups_choose_the_bot_and_never_burn_its_tokens() {
        let hub = hub();
        let state = game(&["a", "b"]);
        let seen = observe(&state, &hub);
        let players = |subtype: &str| {
            contextual(
                Choice::new(
                    me(),
                    "choose a player",
                    vec![
                        ChoiceOption::new("b", "player"),
                        ChoiceOption::new("a", "player"),
                        ChoiceOption::decline(),
                    ],
                ),
                subtype,
            )
        };
        assert_eq!(decide(&players("umbat_player"), &seen, &me(), 0), Some(1));
        assert_eq!(decide(&players("malleon_player"), &seen, &me(), 0), Some(1));
        let pay = contextual(
            Choice::new(
                me(),
                "pay",
                vec![
                    ChoiceOption::new("token|tactic", "nekro_malleon"),
                    ChoiceOption::new("discard", "nekro_malleon"),
                    ChoiceOption::decline(),
                ],
            ),
            "malleon_payment",
        );
        assert_eq!(decide(&pay, &seen, &me(), 1), Some(1));
        let tokens_only = contextual(
            Choice::new(
                me(),
                "pay",
                vec![
                    ChoiceOption::new("token|tactic", "nekro_malleon"),
                    ChoiceOption::decline(),
                ],
            ),
            "malleon_payment",
        );
        assert_eq!(decide(&tokens_only, &seen, &me(), 0), Some(1));
        let branch = contextual(
            Choice::new(
                me(),
                "branch",
                vec![
                    ChoiceOption::new("b", "player"),
                    ChoiceOption::new("self", "player"),
                ],
            ),
            "leader_hacanagent_branch",
        );
        assert_eq!(decide(&branch, &seen, &me(), 0), Some(1));
    }

    #[test]
    fn letani_ospha_turns_the_bots_cruiser_into_a_carrier() {
        let hub = hub();
        let mut state = game(&["a", "b"]);
        let system = SystemId::new(hub.outer[0].clone());
        put(&mut state, &system, "carrier", &me(), 1);
        put(&mut state, &system, "cruiser", &me(), 1);
        put(&mut state, &system, "cruiser", &foe(), 1);
        let seen = observe(&state, &hub);
        let ship = contextual(
            Choice::new(
                me(),
                "Letani Ospha: choose a non-fighter ship",
                vec![
                    ChoiceOption::labelled("ship|0", "ship", format!("a's carrier in {system}")),
                    ChoiceOption::labelled("ship|1", "ship", format!("b's cruiser in {system}")),
                    ChoiceOption::labelled("ship|2", "ship", format!("a's cruiser in {system}")),
                ],
            ),
            "ospha_ship",
        );
        assert_eq!(decide(&ship, &seen, &me(), 0), Some(2));
        let replace = contextual(
            Choice::new(
                me(),
                "replace",
                vec![
                    ChoiceOption::new("replace|destroyer", "ship"),
                    ChoiceOption::new("replace|carrier", "ship"),
                    ChoiceOption::new("replace|dreadnought", "ship"),
                    ChoiceOption::decline(),
                ],
            ),
            "ospha_replace",
        );
        assert_eq!(
            decide(&replace, &seen, &me(), 0),
            Some(1),
            "short of transports"
        );
        let mut later = state.clone();
        later.round = 3;
        assert_eq!(
            decide(&replace, &observe(&later, &hub), &me(), 0),
            Some(2),
            "the dearest"
        );
    }

    #[test]
    fn mahact_follows_through_the_agent_instead_of_a_strategy_token() {
        let hub = hub();
        let state = game(&["a", "b"]);
        let seen = observe(&state, &hub);
        let secret = Choice::new(
            me(),
            "spend a strategy token to draw a secret objective",
            vec![
                ChoiceOption::new("no", "follow"),
                ChoiceOption::new("yes", "follow"),
                ChoiceOption::new("follow|waived|0|agent|20", "follow"),
            ],
        );
        assert_eq!(decide(&secret, &seen, &me(), 0), Some(2));
    }

    #[test]
    fn another_seats_activation_gets_no_raised_move_and_sucaban_is_kept() {
        let hub = hub();
        let state = game(&["a", "b"]);
        let seen = observe(&state, &hub);
        let saar = window(
            "leader:saar:saaragent:SYSTEM_ACTIVATED:after",
            "SYSTEM_ACTIVATED",
            &foe(),
            TriggerKind::SystemActivated,
        );
        assert_eq!(decide(&saar, &seen, &me(), 0), Some(1));
        // Its own activation with no transport that only the raised move brings: kept too.
        let own = window(
            "leader:saar:saaragent:SYSTEM_ACTIVATED:after",
            "SYSTEM_ACTIVATED",
            &me(),
            TriggerKind::SystemActivated,
        );
        assert_eq!(decide(&own, &seen, &me(), 0), Some(1));
        let sucaban = contextual(
            Choice::new(
                me(),
                "Doctor Sucaban: exhaust to let a trade infantry for research",
                vec![ChoiceOption::new("yes", "leader"), ChoiceOption::decline()],
            ),
            "doctor_sucaban_exhaust",
        );
        assert_eq!(decide(&sucaban, &seen, &me(), 0), Some(1));
    }

    #[test]
    fn the_xxcha_agent_readies_the_bots_richest_planet() {
        let hub = hub();
        let mut state = game(&["a", "b"]);
        // Three planets from the plain systems: two for the bot, one for the foe.
        let mut placed: Vec<(SystemId, PlanetId)> = plain_systems(60)
            .into_iter()
            .flat_map(|system| {
                ti4_content::galaxy::planets_in(ContentStore::embedded(), &system, POK)
                    .iter()
                    .map(|p| (SystemId::new(&system), PlanetId::new(p.id())))
                    .collect::<Vec<_>>()
            })
            .take(3)
            .collect();
        let (their_system, theirs) = placed.pop().expect("three planets");
        state
            .system_mut(&their_system)
            .set_control(theirs.clone(), foe());
        for (system, planet) in &placed {
            state.system_mut(system).set_control(planet.clone(), me());
        }
        let home: Vec<PlanetId> = placed.iter().map(|(_, p)| p.clone()).collect();
        let richest = home
            .iter()
            .max_by_key(|p| {
                ti4_content::galaxy::planet(ContentStore::embedded(), p.as_str(), POK)
                    .map_or((0, 0), |r| (r.resources(), r.influence()))
            })
            .expect("a home planet")
            .clone();
        for p in home.iter().chain(std::iter::once(&theirs)) {
            state.exhausted_planets.insert(p.clone());
        }
        let seen = observe(&state, &hub);
        let mut options = vec![ChoiceOption::new(theirs.as_str(), "planet")];
        options.extend(home.iter().map(|p| ChoiceOption::new(p.as_str(), "planet")));
        let ready = contextual(
            Choice::new(me(), "Leader: ready which planet", options),
            "leader_xxchaagent_ready_planet",
        );
        let picked = decide(&ready, &seen, &me(), 0).expect("an own planet");
        assert_eq!(ready.options[picked].id, richest.as_str());
    }

    fn action_menu(extra: &[&str]) -> Choice {
        let mut options = vec![
            ChoiceOption::new("tactical", "action"),
            ChoiceOption::new("pass", "action"),
        ];
        options.extend(extra.iter().map(|id| ChoiceOption::new(*id, "component")));
        Choice::new(me(), "action phase", options)
    }

    #[test]
    fn the_action_choice_is_left_alone_without_an_agent_and_nekro_discards_before_passing() {
        let hub = hub();
        let state = game(&["a", "b"]);
        let seen = observe(&state, &hub);
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        assert_eq!(
            action_decision(&action_menu(&[]), &seen, &me(), 0, &mut rng),
            None
        );
        // Nothing on this board is worth a token: the rule would pass; Malleon first, with a card.
        let menu = action_menu(&["component|leader|nekroagent"]);
        assert_eq!(
            action_decision(&menu, &seen, &me(), 1, &mut rng),
            Some(Some(2)),
            "a card to discard"
        );
        assert_eq!(
            action_decision(&menu, &seen, &me(), 0, &mut rng),
            Some(Some(1)),
            "no card: pass"
        );
    }
}
