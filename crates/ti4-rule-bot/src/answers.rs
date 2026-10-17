//! Rules for questions the rule bot used to answer at random (bot version 2).
//!
//! Each family sits behind its own switch in [`crate::tuning`] (`vote_rule`, `casualty_rule`,
//! `misc_rule`); with a switch off the question keeps the random pick, as in bot version 1.
//!
//! - **Agenda phase** (`vote_rule`): the outcome that helps the seat (an election of itself for
//!   a card or a point, its own planet for an attachment, a rival leader's planet or the rival
//!   leader for a harmful one, the points agendas by the standings), cast with every planet
//!   (planets are readied at the end of the agenda phase, so a vote costs nothing); no opinion:
//!   abstain. Trade goods are never spent on votes. Politics: the seat makes itself speaker, and
//!   puts an agenda it would like on top of the deck, one it would not at the bottom.
//! - **Combat** (`casualty_rule`): a hit is sustained whenever a unit can (the cheapest such
//!   unit first, keeping a flagship or war sun out of Direct Hit's way); a casualty is the least
//!   valuable unit, a damaged one before a fresh one of the same kind.
//! - **Small windows** (`misc_rule`): readying the most valuable planet, retreating to the
//!   system with most of its own, and Trade's free replenish for nobody else.

use ti4_engine::choice::{Choice, ChoiceOption, Observed};
use ti4_model::id::{PlanetId, PlayerId, SystemId};

use crate::backward::agenda::{AgendaBias, agenda_kind};
use crate::{is_ship, payload_str, planet_worth, tuning};

/// The answer for one of the questions above, or `None` to keep the random pick.
pub fn pick(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    pick_with(choice, seen, me, &AgendaBias::default())
}

/// [`pick`] with the agenda secrets' vote bias (`backward/agenda.rs`).
pub fn pick_with(
    choice: &Choice,
    seen: &Observed<'_>,
    me: &PlayerId,
    bias: &AgendaBias,
) -> Option<usize> {
    let subtype = choice.context.as_ref().map(|c| c.subtype.as_str())?;
    match subtype {
        "cast_vote" | "vote_tiebreak" if tuning::vote_rule() => cast_vote(choice, seen, me, bias),
        "vote_exhaust_planet" if tuning::vote_rule() => exhaust_for_vote(choice, seen, me, bias),
        "vote_spend_trade_goods" if tuning::vote_rule() => {
            choice.options.iter().position(ChoiceOption::is_decline)
        }
        "politics_choose_speaker" if tuning::vote_rule() => {
            own_seat_option(choice, me).or_else(|| weakest_seat_option(choice, seen))
        }
        "politics_place_agenda" if tuning::vote_rule() => place_agenda(choice, seen, me),
        "sustain_damage" if tuning::casualty_rule() => sustain(choice, seen),
        "assign_casualty" if tuning::casualty_rule() => casualty(choice, seen),
        // Munitions Reserves after the roll (rules version 2; the question shows the dice, which
        // the legacy blind offer does not, so this never reaches a legacy game): 2 trade goods are
        // worth it for two or more misses -- about one hit regained on average -- and a chosen
        // die is always a re-roll, the question only asks about misses.
        "munitions_reserves_reroll" if choice.details.contains_key("dice") => {
            let misses = choice
                .details
                .get("misses")
                .and_then(serde_json::Value::as_u64);
            if misses.is_some_and(|misses| misses >= 2) {
                choice.options.iter().position(|o| o.id == "munitions")
            } else {
                choice.options.iter().position(ChoiceOption::is_decline)
            }
        }
        "munitions_reroll_die" => {
            // Only a die that misses is worth redrawing; a hit can only be lost.
            let misses = |o: &ChoiceOption| {
                let (Some(face), Some(on)) = (
                    o.payload.get("face").and_then(serde_json::Value::as_i64),
                    o.payload.get("hits_on").and_then(serde_json::Value::as_i64),
                ) else {
                    return false;
                };
                face < on
            };
            choice
                .options
                .iter()
                .position(|o| o.kind == "reroll_die" && misses(o))
                .or_else(|| choice.options.iter().position(ChoiceOption::is_decline))
        }
        "ready_planet" if tuning::misc_rule() => ready_best_planet(choice, seen),
        "retreat_to" if tuning::misc_rule() => retreat_to(choice, seen, me),
        "trade_choose_replenish" if tuning::misc_rule() => {
            choice.options.iter().position(|o| o.id == "done")
        }
        "assimilate" if tuning::tech_pick_rule() => assimilate(choice, seen, me),
        sub if tuning::tech_pick_rule()
            && (sub.ends_with("_pick_technology")
                || sub.ends_with("_choose_technology")
                || sub == "flayesh_technology") =>
        {
            let candidates: Vec<(usize, &str)> = choice
                .options
                .iter()
                .enumerate()
                .filter(|(_, o)| !o.is_decline())
                .map(|(i, o)| (i, o.id.rsplit('|').next().unwrap_or(&o.id)))
                .collect();
            crate::faction_plans::best_technology(seen, me, &candidates)
        }
        sub if tuning::empyrean_rule()
            && sub.starts_with("reaction_after_SYSTEM_ACTIVATED")
            && choice
                .options
                .iter()
                .any(|o| o.id.starts_with("ability:empyrean:aetherpassage")) =>
        {
            // Only for itself: a rival's ships through its fleets are no gift to hand out.
            let own = seen.active_player().is_some_and(|p| p == me);
            let ability = choice
                .options
                .iter()
                .position(|o| o.id.starts_with("ability:empyrean:aetherpassage"));
            if own {
                ability
            } else {
                choice.options.iter().position(ChoiceOption::is_decline)
            }
        }
        sub if tuning::tech_pick_rule() && sub.starts_with("reaction_") => choice
            .options
            .iter()
            .position(|o| o.id.starts_with("ability:nekro:technological_singularity")),
        _ => None,
    }
}

/// Technological Singularity's gain (bot version 3): the best technology by the seat's order;
/// the Valefar and Z-token choices keep the random pick when no technology can be gained.
fn assimilate(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    let candidates: Vec<(usize, &str)> = choice
        .options
        .iter()
        .enumerate()
        .filter_map(|(i, o)| {
            let mut parts = o.id.splitn(3, '|');
            (parts.next()? == "gain").then_some(())?;
            Some((i, parts.nth(1)?))
        })
        .collect();
    crate::faction_plans::best_technology(seen, me, &candidates)
}

// -- agenda phase ---------------------------------------------------------------------------------

/// The agenda a vote or a Politics placement is about (`details.agenda.id`).
fn agenda_of(choice: &Choice) -> Option<&str> {
    choice.details.get("agenda")?.get("id")?.as_str()
}

/// Elections whose winner gains something.
const GOOD_FOR_ELECTED: &[&str] = &[
    "secret",
    "committee",
    "arbiter",
    "minister_commerce",
    "minister_exploration",
    "minister_industry",
    "minister_peace",
    "minister_policy",
    "minister_sciences",
    "minister_war",
    "minister_antiquities",
    "prophecy",
    "shard_of_the_throne",
    "crown_of_emphidia",
    "crown_of_thalnos",
    "standardization",
    "censure",
    "grant_reallocation",
    "warrant",
];
/// Elections whose winner loses something.
const BAD_FOR_ELECTED: &[&str] = &["execution"];
/// Planet elections that make the planet better for its owner.
const GOOD_PLANET: &[&str] = &[
    "core_mining",
    "holy_planet_of_ixth",
    "rt_biotic",
    "rt_cybernetic",
    "rt_propulsion",
    "rt_warfare",
    "senate_sanctuary",
    "terraforming_initiative",
];
/// Planet elections that hurt the planet's owner.
const BAD_PLANET: &[&str] = &["redistribution", "disarmament", "demilitarized_zone"];

fn points(seen: &Observed<'_>, player: &PlayerId) -> i32 {
    seen.seat(player).map_or(0, |seat| seat.victory_points)
}

/// The rival with most points (ties: seating order).
fn rival_leader(seen: &Observed<'_>, me: &PlayerId) -> Option<PlayerId> {
    seen.players()
        .into_iter()
        .filter(|p| *p != me)
        .max_by_key(|p| (points(seen, p), std::cmp::Reverse((*p).clone())))
        .cloned()
}

fn planet_owner(seen: &Observed<'_>, planet: &str) -> Option<PlayerId> {
    let id = PlanetId::new(planet);
    seen.board()
        .values()
        .find_map(|system| system.planet_control.get(&id).cloned())
}

/// The outcome the seat wants on `agenda`, among `outcomes`; `None`: it does not care.
fn preferred_outcome(
    agenda: &str,
    outcomes: &[&str],
    seen: &Observed<'_>,
    me: &PlayerId,
    bias: &AgendaBias,
) -> Option<String> {
    let offered = |id: &str| outcomes.contains(&id).then(|| id.to_owned());
    if let Some(want) = secret_outcome(agenda, outcomes, seen, me, bias) {
        return Some(want);
    }
    if GOOD_FOR_ELECTED.contains(&agenda) {
        return offered(me.as_str());
    }
    if BAD_FOR_ELECTED.contains(&agenda) {
        return offered(rival_leader(seen, me)?.as_str());
    }
    if GOOD_PLANET.contains(&agenda) || BAD_PLANET.contains(&agenda) {
        let good = GOOD_PLANET.contains(&agenda);
        let leader = rival_leader(seen, me);
        return outcomes
            .iter()
            .filter(|o| **o != "for" && **o != "against")
            .filter(|o| {
                let owner = planet_owner(seen, o);
                if good {
                    owner.as_ref() == Some(me)
                } else {
                    owner.is_some() && owner.as_ref() == leader.as_ref()
                }
            })
            .max_by(|a, b| {
                planet_worth(seen, a)
                    .total_cmp(&planet_worth(seen, b))
                    .then_with(|| b.cmp(a))
            })
            .map(|o| (*o).to_owned());
    }
    let mine = points(seen, me);
    let all: Vec<i32> = seen.players().iter().map(|p| points(seen, p)).collect();
    let top = all.iter().copied().max().unwrap_or(0);
    let side = match agenda {
        // A point for each "For" voter if it passes.
        "mutiny" | "unconventional" | "articles_war" | "incentive" => "for",
        // The leader or the last gains a point: never hand a rival the lead or the win.
        "seed_empire" => {
            if mine >= top {
                "for"
            } else {
                "against"
            }
        }
        "economic_equality" => {
            let goods = seen.seat(me).map_or(0, |seat| seat.trade_goods);
            if goods < 5 { "for" } else { "against" }
        }
        _ => return None,
    };
    offered(side)
}

/// The outcome the old fixed table prefers for `who` (what a rule bot of versions 2 to 4 would
/// vote for), used to forecast the others' votes.
pub(crate) fn old_preference(
    agenda: &str,
    outcomes: &[&str],
    seen: &Observed<'_>,
    who: &PlayerId,
) -> Option<String> {
    preferred_outcome(agenda, outcomes, seen, who, &AgendaBias::default())
}

/// What the seat's agenda secrets want on `agenda`: Dictate Policy For on a law that is no
/// election (backward/agenda.rs). Drive the Debate needs nothing here: the vote rule above already
/// elects the seat or its planet in every election that does not harm the elected.
fn secret_outcome(
    agenda: &str,
    outcomes: &[&str],
    seen: &Observed<'_>,
    _me: &PlayerId,
    bias: &AgendaBias,
) -> Option<String> {
    if !bias.dp {
        return None;
    }
    let (kind, target) = agenda_kind(seen, agenda)?;
    if bias.dp && kind == "Law" && target == "For/Against" && outcomes.contains(&"for") {
        return Some("for".to_owned());
    }
    None
}

fn outcome_ids(choice: &Choice) -> Vec<&str> {
    choice
        .options
        .iter()
        .filter(|o| !o.is_decline())
        .map(|o| o.id.as_str())
        .collect()
}

/// Vote for the preferred outcome, else abstain (a tie-break, which cannot abstain, keeps the
/// random pick when the seat does not care).
fn cast_vote(
    choice: &Choice,
    seen: &Observed<'_>,
    me: &PlayerId,
    bias: &AgendaBias,
) -> Option<usize> {
    let agenda = agenda_of(choice)?;
    let outcomes = outcome_ids(choice);
    let old = preferred_outcome(agenda, &outcomes, seen, me, bias);
    let tiebreak = choice
        .context
        .as_ref()
        .is_some_and(|c| c.subtype == "vote_tiebreak");
    // The agenda secrets' bias (Dictate Policy) is honoured as before.
    let biased = secret_outcome(agenda, &outcomes, seen, me, bias).is_some();
    if !biased && !tiebreak {
        if let Some(cast) = crate::agenda_vote::plan_vote(choice, seen, me, agenda, old.clone()) {
            return Some(cast.index);
        }
    }
    if tiebreak
        && tuning::agenda_value()
        && let Some(verdict) = crate::agenda_vote::evaluate(agenda, &outcomes, seen, me)
        && let Some((best, _)) = verdict.best()
    {
        return choice.options.iter().position(|o| o.id == best);
    }
    if let Some(want) = old {
        return choice.options.iter().position(|o| o.id == want);
    }
    choice.options.iter().position(ChoiceOption::is_decline)
}

/// Votes a planet option casts (its label: "exhaust X for N votes").
fn votes_of(option: &ChoiceOption) -> i64 {
    option
        .label
        .split(" for ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

/// Every planet for the outcome it wants (they are readied after the agenda phase), the biggest
/// first; none for any other outcome.
fn exhaust_for_vote(
    choice: &Choice,
    seen: &Observed<'_>,
    me: &PlayerId,
    bias: &AgendaBias,
) -> Option<usize> {
    let outcome = choice
        .prompt
        .strip_prefix("exhaust a planet to vote ")?
        .trim()
        .to_owned();
    let agenda = agenda_of(choice)?;
    if tuning::agenda_value() || tuning::agenda_budget() {
        if let Some(index) = crate::agenda_vote::pick_planet(choice, me, &outcome) {
            return Some(index);
        }
        if crate::agenda_vote::has_plan(me) {
            // The plan is for another outcome, or its votes are all cast.
            return choice.options.iter().position(ChoiceOption::is_decline);
        }
    }
    // The outcomes are not listed here; the preference only needs the one being voted.
    let outcomes = [outcome.as_str()];
    let wanted = preferred_outcome(agenda, &outcomes, seen, me, bias).is_some_and(|w| w == outcome);
    if !wanted {
        return choice.options.iter().position(ChoiceOption::is_decline);
    }
    choice
        .options
        .iter()
        .enumerate()
        .filter(|(_, o)| !o.is_decline())
        .max_by_key(|(i, o)| (votes_of(o), std::cmp::Reverse(*i)))
        .map(|(i, _)| i)
}

/// The option that names the seat itself (`details.seats`: name -> seat).
fn own_seat_option(choice: &Choice, me: &PlayerId) -> Option<usize> {
    let seats = choice.details.get("seats")?.as_object()?;
    choice.options.iter().position(|o| {
        seats
            .get(&o.id)
            .and_then(|seat| seat.as_str())
            .is_some_and(|seat| seat == me.as_str())
    })
}

/// Already the speaker: the seat with fewest points gets the token (the least dangerous one).
fn weakest_seat_option(choice: &Choice, seen: &Observed<'_>) -> Option<usize> {
    let seats = choice.details.get("seats")?.as_object()?;
    choice
        .options
        .iter()
        .enumerate()
        .filter_map(|(i, o)| {
            let seat = PlayerId::new(seats.get(&o.id)?.as_str()?);
            Some((i, points(seen, &seat)))
        })
        .min_by_key(|&(i, p)| (p, i))
        .map(|(i, _)| i)
}

/// Politics' look at the agenda deck: an agenda with an outcome the seat wants goes on top, one
/// that harms it (an election of a rival for a card, a point for the leader) at the bottom.
fn place_agenda(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    let agenda = agenda_of(choice)?;
    let mut good = GOOD_FOR_ELECTED.contains(&agenda)
        || GOOD_PLANET.contains(&agenda)
        || matches!(agenda, "mutiny" | "unconventional" | "incentive");
    if tuning::agenda_value() {
        // The valuation of the card: on top when the best outcome is worth something to the seat.
        let outcomes = crate::agenda_vote::outcomes_for(seen, agenda);
        let refs: Vec<&str> = outcomes.iter().map(String::as_str).collect();
        if let Some(verdict) = crate::agenda_vote::evaluate(agenda, &refs, seen, me)
            && let Some((best, _)) = verdict.best()
        {
            good = verdict.score_of(best) >= tuning::agenda_min_margin();
        }
    }
    let id = if good { "top" } else { "bottom" };
    if tuning::agenda_budget() {
        let round = choice.context.as_ref().map_or(seen.round(), |c| c.round);
        crate::agenda_vote::note_placement(me, round, agenda, good);
    }
    choice.options.iter().position(|o| o.id == id)
}

// -- combat ---------------------------------------------------------------------------------------

fn value_of(seen: &Observed<'_>, unit: &str) -> f64 {
    ti4_engine::valuation::unit_value(seen.content(), seen.sources(), unit)
}

fn is_flagship_or_war_sun(unit: &str) -> bool {
    unit.contains("flagship") || unit.contains("warsun") || unit.contains("war_sun")
}

/// Sustain whenever a unit can, on the cheapest unit that can (a flagship or war sun last).
fn sustain(choice: &Choice, seen: &Observed<'_>) -> Option<usize> {
    choice
        .options
        .iter()
        .enumerate()
        .filter(|(_, o)| !o.is_decline())
        .filter_map(|(i, o)| {
            let unit = payload_str(o, "unit")?;
            let rank = value_of(seen, unit)
                + if is_flagship_or_war_sun(unit) {
                    100.0
                } else {
                    0.0
                };
            Some((i, rank))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
        .map(|(i, _)| i)
}

/// The least valuable casualty; a damaged unit counts as worth less than a fresh one.
fn casualty(choice: &Choice, seen: &Observed<'_>) -> Option<usize> {
    choice
        .options
        .iter()
        .enumerate()
        .filter_map(|(i, o)| {
            let unit = payload_str(o, "unit")?;
            let damaged = o
                .payload
                .get("damaged")
                .and_then(serde_json::Value::as_bool)
                == Some(true);
            let value = value_of(seen, unit) * if damaged { 0.7 } else { 1.0 };
            Some((i, value))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
        .map(|(i, _)| i)
}

// -- small windows --------------------------------------------------------------------------------

fn ready_best_planet(choice: &Choice, seen: &Observed<'_>) -> Option<usize> {
    choice
        .options
        .iter()
        .enumerate()
        .filter_map(|(i, o)| {
            let record = ti4_content::galaxy::planet(seen.content(), &o.id, seen.sources())?;
            let (r, f) = (record.resources(), record.influence());
            Some((i, 2 * r.max(f) + r.min(f)))
        })
        .max_by_key(|&(i, v)| (v, std::cmp::Reverse(i)))
        .map(|(i, _)| i)
}

/// Retreat where most of its own is: planets it controls, its ships, a dock.
fn retreat_to(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    choice
        .options
        .iter()
        .enumerate()
        .filter(|(_, o)| !o.is_decline())
        .map(|(i, o)| {
            let board = seen.system(&SystemId::new(&o.id));
            let planets = board.planet_control.values().filter(|p| *p == me).count();
            let ships = board
                .units
                .iter()
                .filter(|u| &u.owner == me && is_ship(seen, u))
                .count();
            (i, 3 * planets + ships)
        })
        .max_by_key(|&(i, v)| (v, std::cmp::Reverse(i)))
        .map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_content::ContentStore;
    use ti4_engine::fixtures::game;
    use ti4_model::POK;

    fn seen(state: &ti4_model::state::GameState) -> Observed<'_> {
        Observed::new(state, ContentStore::embedded(), POK, None)
    }

    fn with_subtype(mut choice: Choice, subtype: &str) -> Choice {
        choice.context = Some(ti4_engine::decision_context::DecisionContext::new(
            choice.player.clone(),
            ti4_engine::decision_context::DecisionSource::Rule("test".to_owned()),
            subtype,
            ti4_model::state::Phase::Action,
            3,
        ));
        choice
    }

    #[test]
    fn munitions_reserves_is_bought_for_two_or_more_misses_and_re_rolls_them_all() {
        let state = game(&["a", "b"]);
        let me = PlayerId::new("a");
        let offer = |misses: u64| {
            with_subtype(
                Choice::new(
                    me.clone(),
                    "spend 2 trade goods for Munitions Reserves",
                    vec![
                        ChoiceOption::labelled("munitions", "ability", "reroll all"),
                        ChoiceOption::labelled("munitions_choose", "ability", "choose"),
                        ChoiceOption::decline(),
                    ],
                )
                .detailed("dice", serde_json::json!([]))
                .detailed("misses", misses),
                "munitions_reserves_reroll",
            )
        };
        assert_eq!(pick(&offer(3), &seen(&state), &me), Some(0));
        assert_eq!(
            pick(&offer(1), &seen(&state), &me),
            Some(2),
            "one miss: not worth 2"
        );
        let die = with_subtype(
            Choice::new(
                me.clone(),
                "reroll die 1 of cruiser",
                vec![
                    ChoiceOption::labelled("reroll|0:0", "reroll_die", "reroll")
                        .with("face", 3)
                        .with("hits_on", 7),
                    ChoiceOption::decline(),
                ],
            ),
            "munitions_reroll_die",
        );
        assert_eq!(pick(&die, &seen(&state), &me), Some(0), "a miss is redrawn");
        let hit = with_subtype(
            Choice::new(
                me.clone(),
                "reroll die 2 of cruiser",
                vec![
                    ChoiceOption::labelled("reroll|0:1", "reroll_die", "reroll")
                        .with("face", 9)
                        .with("hits_on", 7),
                    ChoiceOption::decline(),
                ],
            ),
            "munitions_reroll_die",
        );
        assert_eq!(pick(&hit, &seen(&state), &me), Some(1), "a hit is kept");
    }

    #[test]
    fn casualties_and_sustain_take_the_cheapest_unit() {
        let state = game(&["a", "b"]);
        let me = PlayerId::new("a");
        let casualty = with_subtype(
            Choice::new(
                me.clone(),
                "assign a hit",
                vec![
                    ChoiceOption::labelled("destroy|0", "casualty", "destroy dreadnought")
                        .with("unit", "dreadnought")
                        .with("damaged", false),
                    ChoiceOption::labelled("destroy|1", "casualty", "destroy fighter")
                        .with("unit", "fighter")
                        .with("damaged", false),
                ],
            ),
            "assign_casualty",
        );
        let sustain = with_subtype(
            Choice::new(
                me.clone(),
                "cancel a hit",
                vec![
                    ChoiceOption::labelled("sustain|a", "sustain", "sustain damage on flagship")
                        .with("unit", "sol_flagship"),
                    ChoiceOption::labelled("sustain|b", "sustain", "sustain damage on dreadnought")
                        .with("unit", "dreadnought"),
                    ChoiceOption::decline(),
                ],
            ),
            "sustain_damage",
        );
        let on = crate::tuning::Tuning {
            casualty_rule: true,
            ..crate::tuning::Tuning::default()
        };
        crate::tuning::with(on, || {
            assert_eq!(pick(&casualty, &seen(&state), &me), Some(1), "the fighter");
            assert_eq!(
                pick(&sustain, &seen(&state), &me),
                Some(1),
                "the dreadnought sustains"
            );
        });
        let off = crate::tuning::for_version(1);
        crate::tuning::with(off, || {
            assert_eq!(pick(&casualty, &seen(&state), &me), None)
        });
    }

    #[test]
    fn votes_for_itself_and_makes_itself_speaker() {
        let state = game(&["a", "b"]);
        let me = PlayerId::new("a");
        let vote = with_subtype(
            Choice::new(
                me.clone(),
                "vote for which outcome",
                vec![
                    ChoiceOption::labelled("b", "vote", "b"),
                    ChoiceOption::labelled("a", "vote", "a"),
                    ChoiceOption::decline(),
                ],
            )
            .detailed("agenda", serde_json::json!({ "id": "minister_war" })),
            "cast_vote",
        );
        let speaker = with_subtype(
            Choice::new(
                me.clone(),
                "who becomes speaker",
                vec![
                    ChoiceOption::labelled("Sol", "speaker", "Sol becomes speaker"),
                    ChoiceOption::labelled("Hacan", "speaker", "Hacan becomes speaker"),
                ],
            )
            .detailed("seats", serde_json::json!({ "Sol": "b", "Hacan": "a" })),
            "politics_choose_speaker",
        );
        let on = crate::tuning::Tuning {
            vote_rule: true,
            ..crate::tuning::Tuning::default()
        };
        crate::tuning::with(on, || {
            assert_eq!(pick(&vote, &seen(&state), &me), Some(1));
            assert_eq!(pick(&speaker, &seen(&state), &me), Some(1));
        });
    }

    #[test]
    fn dictate_policy_votes_for_a_law() {
        let state = game(&["a", "b"]);
        let me = PlayerId::new("a");
        let vote = with_subtype(
            Choice::new(
                me.clone(),
                "vote for which outcome",
                vec![
                    ChoiceOption::labelled("against", "vote", "against"),
                    ChoiceOption::labelled("for", "vote", "for"),
                    ChoiceOption::decline(),
                ],
            )
            .detailed("agenda", serde_json::json!({ "id": "conventions" })),
            "cast_vote",
        );
        // Without the secret the seat abstains on an agenda it has no opinion on.
        assert_eq!(pick(&vote, &seen(&state), &me), Some(2));
        let bias = AgendaBias { dp: true };
        assert_eq!(pick_with(&vote, &seen(&state), &me, &bias), Some(1));
    }
}
