//! Leaders: lock, unlock, ready, exhaust, purge (LRR 51).
//!
//! Ported from the oracle's `engine/leaders.py`: `for_faction`, `starting_states`, `status`,
//! `of_type`, `check_unlocks`, `ready_agents`, `exhaust` and `purge`.

use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{LeaderId, PlayerId};
use ti4_model::state::{GameState, LeaderStatus};

/// The three kinds of leader.
pub const AGENT: &str = "agent";
pub const COMMANDER: &str = "commander";
pub const HERO: &str = "hero";

/// Commanders that unlock on a condition this engine can check.
///
/// A commander behind a condition nobody registered can never leave the locked state — and an
/// ability behind an unreachable unlock is unreachable however well it is written. The oracle
/// records exactly that for Jol-Nar's, which had no check at all.
#[must_use]
pub fn commander_unlocks() -> Vec<&'static str> {
    vec![
        "hacancommander",
        "jolnarcommander",
        "l1z1xcommander",
        "letnevcommander",
        "naalucommander",
        "solcommander",
        "xxchacommander",
    ]
}

/// The combined resource or influence value of everything this player controls.
fn controlled_total(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    kind: crate::production::Spend,
) -> i64 {
    state
        .controlled_planets(player)
        .into_iter()
        .map(|(_, planet)| {
            crate::production::planet_value_now(state, content, sources, planet, kind)
        })
        .sum()
}

/// How many units of one base type this player has in space, across the board.
fn ships_of(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    base_type: &str,
) -> usize {
    let types = ti4_content::units::catalogue(content, sources);
    state
        .board
        .values()
        .flat_map(|board| board.units.iter())
        .filter(|unit| &unit.owner == player)
        .filter(|unit| {
            types
                .get(unit.type_id.as_str())
                .is_some_and(|kind| kind.base_type() == base_type)
        })
        .count()
}

/// Whether this commander's unlock condition is met.
///
/// `None` for a commander with no registered check, which keeps it locked rather than letting an
/// unknown condition read as satisfied.
#[must_use]
pub fn commander_unlocked(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    use crate::production::Spend;
    let met = match leader.as_str() {
        // Control planets with a combined total of at least 12 resources.
        "solcommander" => controlled_total(state, content, sources, player, Spend::Resources) >= 12,
        // The same, in influence.
        "xxchacommander" => {
            controlled_total(state, content, sources, player, Spend::Influence) >= 12
        }
        "hacancommander" => state
            .player(player)
            .is_some_and(|seat| seat.trade_goods >= 10),
        "jolnarcommander" => state
            .player(player)
            .is_some_and(|seat| seat.technologies.len() >= 8),
        "l1z1xcommander" => ships_of(state, content, sources, player, "dreadnought") >= 4,
        // Five non-fighter ships in *one* system, not five across the board.
        "letnevcommander" => {
            let types = ti4_content::units::catalogue(content, sources);
            state.board.values().any(|board| {
                board
                    .units_of(player)
                    .into_iter()
                    .filter(|unit| {
                        types
                            .get(unit.type_id.as_str())
                            .is_some_and(|kind| kind.is_ship() && !kind.is_fighter())
                    })
                    .count()
                    >= 5
            })
        }
        // Ground forces in or adjacent to Mecatol Rex.
        "naalucommander" => {
            let Some(galaxy) = galaxy else {
                return Some(false); // without a map there is no "adjacent"
            };
            let types = ti4_content::units::catalogue(content, sources);
            let mut nearby: std::collections::BTreeSet<String> = galaxy
                .adjacent(crate::seating::mecatol_in_galaxy(galaxy))
                .into_iter()
                .map(ToOwned::to_owned)
                .collect();
            nearby.insert(crate::seating::mecatol_in_galaxy(galaxy).to_owned());
            nearby.iter().any(|system| {
                let board = state.system_state(&ti4_model::id::SystemId::new(system));
                board
                    .units
                    .iter()
                    .chain(board.planet_units.values().flatten())
                    .filter(|unit| &unit.owner == player)
                    .any(|unit| {
                        types
                            .get(unit.type_id.as_str())
                            .is_some_and(ti4_content::units::UnitType::is_ground_force)
                    })
            })
        }
        other => {
            return crate::factions::commander_unlocked(
                state,
                content,
                sources,
                galaxy,
                player,
                &LeaderId::new(other),
            );
        }
    };
    Some(met)
}

/// 51.7: a hero unlocks once its owner has scored three objectives.
pub const HERO_OBJECTIVES: usize = 3;

/// What kind of leader this is.
#[must_use]
pub fn kind_of(content: &ContentStore, leader: &LeaderId) -> Option<String> {
    content
        .get(ContentType::Leaders, leader.as_str())
        .and_then(|record| record.text("type"))
        .map(str::to_ascii_lowercase)
}

/// The leaders a faction has, in corpus order.
///
/// A record that replaces another (`homebrewReplacesID`) excludes the replaced one from scope:
/// Thunder's Edge reprints Xxcha's hero as `xxchahero-te`, so FULL-scope games deploy only the
/// replacement rather than both cards for one slot (51.2a gives three leaders, not four).
#[must_use]
pub fn for_faction(content: &ContentStore, sources: SourceSet, faction: &str) -> Vec<LeaderId> {
    let records = content
        .from_sources(ContentType::Leaders, sources)
        .filter(|record| {
            record.text("faction").is_some_and(|owner| {
                crate::factions::keleres::tag_belongs_to(owner, faction, false)
            })
        })
        .collect::<Vec<_>>();
    let replaced: std::collections::BTreeSet<&str> = records
        .iter()
        .filter_map(|record| record.text("homebrewReplacesID"))
        .collect();
    // The faction sheet names its leaders. The corpus also carries alternates that sheet does not
    // list -- unofficial variants filed under an official source (Creuss's `redcreuss*`) and
    // superseded printings with no replacement link (Naalu's `naaluagent` beside the listed
    // `naaluagent-te`) -- and dealing those gave a seat six leaders, or two agents. A record is
    // kept if the sheet lists it, if it reprints one the sheet lists, or if it stands in for a
    // listed leader of its type that `sources` does not reach.
    let sheet: Vec<&str> = ti4_content::factions::get(content, faction)
        .map(|record| record.leaders())
        .unwrap_or_default();
    let available: std::collections::BTreeSet<&str> = records
        .iter()
        .filter_map(|record| record_id(record))
        .collect();
    let missing_types: std::collections::BTreeSet<String> = sheet
        .iter()
        .filter(|listed| !available.contains(*listed))
        .filter_map(|listed| kind_of(content, &LeaderId::new(*listed)))
        .collect();
    let on_sheet = |record: &&ti4_content::Record| {
        sheet.is_empty()
            || record_id(record).is_some_and(|id| sheet.contains(&id))
            || record
                .text("homebrewReplacesID")
                .is_some_and(|original| sheet.contains(&original))
            || record
                .text("type")
                .is_some_and(|kind| missing_types.contains(&kind.to_ascii_lowercase()))
    };
    records
        .into_iter()
        .filter(on_sheet)
        .filter_map(|record| record.text("id").or_else(|| record.text("alias")))
        .map(LeaderId::new)
        .filter(|leader| !replaced.contains(leader.as_str()))
        .collect()
}

/// A leader record's id.
fn record_id(record: &ti4_content::Record) -> Option<&str> {
    record.text("id").or_else(|| record.text("alias"))
}

/// 51.2a: a faction begins with its agents readied and everything else locked.
#[must_use]
pub fn starting_states(
    content: &ContentStore,
    sources: SourceSet,
    faction: &str,
) -> Vec<(LeaderId, LeaderStatus)> {
    for_faction(content, sources, faction)
        .into_iter()
        .map(|leader| {
            let status = if kind_of(content, &leader).as_deref() == Some(AGENT) {
                LeaderStatus::Readied
            } else {
                LeaderStatus::Locked
            };
            (leader, status)
        })
        .collect()
}

/// Give a player their faction's leaders.
pub fn deploy(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) {
    let Some(faction) = state.player(player).map(|seat| seat.faction.to_string()) else {
        return;
    };
    let starting = starting_states(content, sources, &faction);
    if let Some(seat) = state.player_mut(player) {
        for (leader, status) in starting {
            seat.leaders.insert(leader, status);
        }
    }
}

/// This leader's current state, if the player has it.
#[must_use]
pub fn status(state: &GameState, player: &PlayerId, leader: &LeaderId) -> Option<LeaderStatus> {
    state
        .player(player)
        .and_then(|seat| seat.leaders.get(leader).copied())
}

/// This player's leaders of one kind.
#[must_use]
pub fn of_kind(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    kind: &str,
) -> Vec<LeaderId> {
    state.player(player).map_or_else(Vec::new, |seat| {
        seat.leaders
            .keys()
            .filter(|leader| kind_of(content, leader).as_deref() == Some(kind))
            .cloned()
            .collect()
    })
}

/// 51.7: unlock any hero whose owner has scored three objectives.
///
/// Commanders have per-faction unlock conditions the oracle registers individually; none are
/// implemented, so a commander stays locked. That is the registry design used elsewhere — an
/// unimplemented condition leaves the leader unavailable rather than silently unlocked.
pub fn check_unlocks(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    player: &PlayerId,
) -> Vec<LeaderId> {
    // Commanders unlock on their own condition, not on scored objectives. A commander whose
    // check is unregistered stays locked rather than being treated as satisfied.
    let commanders: Vec<LeaderId> = state.player(player).map_or_else(Vec::new, |seat| {
        seat.leaders
            .iter()
            .filter(|(_, status)| **status == LeaderStatus::Locked)
            .map(|(leader, _)| leader.clone())
            .filter(|leader| {
                commander_unlocked(state, content, sources, galaxy, player, leader).unwrap_or(false)
            })
            .collect()
    });
    if let Some(seat) = state.player_mut(player) {
        for leader in &commanders {
            seat.leaders.insert(leader.clone(), LeaderStatus::Unlocked);
        }
    }

    let scored = state.scored_by(player).len();
    let heroes: Vec<LeaderId> = state.player(player).map_or_else(Vec::new, |seat| {
        seat.leaders
            .iter()
            .filter(|(_, status)| **status == LeaderStatus::Locked)
            .map(|(leader, _)| leader.clone())
            .filter(|leader| kind_of(content, leader).as_deref() == Some(HERO))
            .collect()
    });
    if scored < HERO_OBJECTIVES {
        return commanders;
    }
    if let Some(seat) = state.player_mut(player) {
        for leader in &heroes {
            seat.leaders.insert(leader.clone(), LeaderStatus::Unlocked);
        }
    }
    commanders.into_iter().chain(heroes).collect()
}

/// 81.6: exhausted cards ready in the status phase, agents among them.
///
/// Returns what was readied. The oracle notes this used to happen silently, so a driven table
/// could turn an agent face down when it was used and never turn it back — which after a round
/// or two reads as a player who has run out of agents.
pub fn ready_all(state: &mut GameState, player: &PlayerId) -> Vec<LeaderId> {
    let Some(seat) = state.player_mut(player) else {
        return Vec::new();
    };
    let exhausted: Vec<LeaderId> = seat
        .leaders
        .iter()
        .filter(|(_, status)| **status == LeaderStatus::Exhausted)
        .map(|(leader, _)| leader.clone())
        .collect();
    for leader in &exhausted {
        seat.leaders.insert(leader.clone(), LeaderStatus::Readied);
    }
    for leader in &exhausted {
        crate::factions::nomad_agents::agent_readied(state, player, leader);
    }
    exhausted
}

/// Ready one exhausted leader. `false` if it was not exhausted.
///
/// The status phase readies them all at once; this is for the effects that ready a single card
/// out of turn, like The Acropolis.
pub fn ready(state: &mut GameState, player: &PlayerId, leader: &LeaderId) -> bool {
    let Some(seat) = state.player_mut(player) else {
        return false;
    };
    if seat.leaders.get(leader) != Some(&LeaderStatus::Exhausted) {
        return false;
    }
    seat.leaders.insert(leader.clone(), LeaderStatus::Readied);
    crate::factions::nomad_agents::agent_readied(state, player, leader);
    true
}

/// Exhaust a leader to use it. `false` if it was not readied.
pub fn exhaust(state: &mut GameState, player: &PlayerId, leader: &LeaderId) -> bool {
    let Some(seat) = state.player_mut(player) else {
        return false;
    };
    if seat.leaders.get(leader) != Some(&LeaderStatus::Readied) {
        return false;
    }
    seat.leaders.insert(leader.clone(), LeaderStatus::Exhausted);
    // The Nomad's Temporal Command Suite hears every agent becoming exhausted.
    if crate::factions::nomad::watches_agent_exhaustion(state)
        && kind_of(ContentStore::embedded(), leader).as_deref() == Some(AGENT)
    {
        crate::supply::stage_event(
            state,
            crate::factions::nomad::AGENT_EXHAUSTED,
            &std::collections::BTreeMap::from([
                ("player".to_owned(), player.to_string().into()),
                ("leader".to_owned(), leader.as_str().into()),
            ]),
        );
    }
    true
}

/// Purge a leader, which is permanent — a hero is purged when its ability resolves (51.9).
pub fn purge(state: &mut GameState, player: &PlayerId, leader: &LeaderId) -> bool {
    let Some(seat) = state.player_mut(player) else {
        return false;
    };
    if !seat.leaders.contains_key(leader) {
        return false;
    }
    seat.leaders.insert(leader.clone(), LeaderStatus::Purged);
    true
}

/// Leaders this player could use now: readied agents, and unlocked heroes.
///
/// Commanders are deliberately absent. An unlocked commander is not a card you play as an
/// action — each one is a standing modifier or a triggered ability delivered by its own printed
/// window (voting, combat, sustain), so treating it as generically usable would let it fire at
/// times the rules never give it.
#[must_use]
pub fn usable(state: &GameState, content: &ContentStore, player: &PlayerId) -> Vec<LeaderId> {
    state.player(player).map_or_else(Vec::new, |seat| {
        seat.leaders
            .iter()
            .filter(|(leader, status)| {
                let kind = kind_of(content, leader);
                matches!(
                    (*status, kind.as_deref()),
                    (LeaderStatus::Readied, Some(AGENT)) | (LeaderStatus::Unlocked, Some(HERO))
                )
            })
            .map(|(leader, _)| leader.clone())
            .collect()
    })
}

/// Whether this leader's printed window is the action phase — usable as a component action on
/// its owner's turn.
///
/// The corpus spells two things as the action phase: `"ACTION:"`, and — for the Prophecy of Kings
/// agents — `"During the action phase:"`. Both carry the trailing colon the card frame prints, so
/// the comparison drops it first. Until this file trimmed it, `eq_ignore_ascii_case("during the
/// action phase")` matched nothing in the corpus, so a Prophecy of Kings agent printed for the
/// action phase was never offered; see `the_hacan_agent_is_offered_on_its_own_turn`.
fn is_action_window(content: &ContentStore, leader: &LeaderId) -> bool {
    content
        .get(ContentType::Leaders, leader.as_str())
        .is_some_and(|record| {
            record.text("abilityWindow").is_some_and(|window| {
                let printed = window.trim().trim_end_matches(':').trim_end();
                printed.starts_with("ACTION")
                    || printed.eq_ignore_ascii_case("during the action phase")
            })
        })
}

/// Whether using this leader is a component action that takes the turn (`"ACTION:"`), or an
/// ability usable during the action phase that does not (`"During the action phase:"`, the Prophecy
/// of Kings agents such as Carth of Golden Sands). Only the first ends the turn (OP-07).
#[must_use]
pub fn uses_the_action(content: &ContentStore, leader: &LeaderId) -> bool {
    let leader = split_borrowed(leader).map_or_else(|| leader.clone(), |(_, source)| source);
    let leader = &leader;
    content
        .get(ContentType::Leaders, leader.as_str())
        .and_then(|record| record.text("abilityWindow"))
        .is_some_and(|window| window.trim().starts_with("ACTION"))
}

/// Action-phase leaders whose effects this engine actually delivers.
///
/// Offering a leader with no delivery path would be an option that can never resolve, and legal
/// actions are generated rather than rejected late. The set grows as packages deliver more
/// windows; everything else stays out of the offer until it has one.
///
/// A list rather than a `matches!` so `every_delivered_action_leader_prints_an_action_window` can
/// hold the two halves of the offer — "we deliver it" and "the corpus says it is an action" —
/// against each other. Membership alone was never proof of delivery: Hacan's agent was in the
/// `matches!` from LEADER-FIX-001 and still never reached a table.
const DELIVERED_ACTION_LEADERS: [&str; 7] = [
    "xxchaagent",
    "hacanagent",
    "solhero",
    "letnevhero",
    "jolnarhero",
    "l1z1xhero",
    "xxchahero-te",
];

fn action_leader_delivered(leader: &LeaderId) -> bool {
    DELIVERED_ACTION_LEADERS.contains(&leader.as_str())
}

/// Cheap preconditions for offering an action-phase leader: only what is checkable without a map.
fn can_resolve_action(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    leader: &LeaderId,
) -> bool {
    match leader.as_str() {
        // "Ready any planet" needs a planet to be readying.
        "xxchaagent" => !state.exhausted_planets.is_empty(),
        // "Gain 2 commodities or replenish another player's commodities" — with every hand at its
        // cap neither branch can change anything, and offering a use that burns the agent for
        // nothing is a dead button, not an option.
        "hacanagent" => state.players.iter().any(|seat| {
            ti4_content::factions::get(content, seat.faction.as_str())
                .is_none_or(|faction| seat.commodities < faction.commodities())
        }),
        // Already active this round: re-using it would change nothing, so it is not offered twice.
        "letnevhero" => state
            .player(player)
            .is_some_and(|seat| seat.fleet_supply_unlimited_until != Some(state.round)),
        // The hero purges when used, so offering it must mean at least one swap can actually be
        // made — otherwise the use would burn the card for nothing.
        "jolnarhero" => {
            let Some(seat) = state.player(player) else {
                return false;
            };
            seat.technologies.iter().any(|alias| {
                !crate::technology::is_unit_upgrade(content, alias)
                    && content
                        .get(ContentType::Technologies, alias.as_str())
                        .and_then(|r| r.strings("types").first().copied())
                        .is_some_and(|colour| {
                            content
                                .from_sources(ContentType::Technologies, SourceSet::all())
                                .any(|r| {
                                    r.strings("types")
                                        .first()
                                        .is_some_and(|kind| *kind == colour)
                                        && !r.strings("types").contains(&"UNITUPGRADE")
                                        && r.text("alias").is_some_and(|a| {
                                            !seat
                                                .technologies
                                                .contains(&ti4_model::id::TechnologyId::new(a))
                                        })
                                })
                        })
            })
        }
        // The Thunder's Edge Xxcha hero places PDS/mechs on controlled planets; nothing to place
        // on, or no unit in supply, means the effect cannot resolve. Supply is checked against
        // the full source set because this gate does not carry sources; the arm re-checks with
        // the real ones.
        "xxchahero-te" => {
            let Some(seat) = state.player(player) else {
                return false;
            };
            if state.controlled_planets(player).is_empty() {
                return false;
            }
            let sources = SourceSet::all();
            let catalogue = ti4_content::units::catalogue(content, sources);
            ["pds", "mech"].into_iter().any(|base_type| {
                let id = ti4_content::units::faction_unit(
                    content,
                    seat.faction.as_str(),
                    base_type,
                    sources,
                )
                .map(|unit| unit.id().to_owned())
                .or_else(|| catalogue.get(base_type).map(|unit| unit.id().to_owned()));
                id.is_some_and(|id| {
                    crate::supply::allowed(
                        state,
                        content,
                        sources,
                        player,
                        &ti4_model::id::UnitTypeId::new(&id),
                        1,
                    ) > 0
                })
            })
        }
        _ => crate::factions::leader_action(state, content, player, leader).unwrap_or(true),
    }
}

/// Component actions this player's leaders offer on their turn: readied agents and unlocked
/// heroes whose printed window is the action phase, offered only when they can resolve.
#[must_use]
pub fn component_actions(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
) -> Vec<crate::choice::ChoiceOption> {
    let Some(seat) = state.player(player) else {
        return Vec::new();
    };
    seat.leaders
        .iter()
        .filter(|(leader, status)| {
            let kind = kind_of(content, leader);
            matches!(
                (*status, kind.as_deref()),
                (LeaderStatus::Readied, Some(AGENT)) | (LeaderStatus::Unlocked, Some(HERO))
            )
        })
        .map(|(leader, _)| leader.clone())
        .filter(|leader| is_action_window(content, leader))
        .filter(|leader| {
            action_leader_delivered(leader)
                || crate::factions::leader_action(state, content, player, leader).is_some()
        })
        .filter(|leader| can_resolve_action(state, content, player, leader))
        .map(|leader| {
            let label = content
                .get(ContentType::Leaders, leader.as_str())
                .and_then(|record| record.text("name"))
                .unwrap_or(leader.as_str());
            crate::choice::ChoiceOption::labelled(
                format!("component|leader|{}", leader.as_str()),
                "component",
                label.to_owned(),
            )
        })
        .chain(borrowed_component_actions(state, content, player))
        .collect()
}

/// Ssruu, Clever Genome: each other seat's agent whose printed window is the action phase, offered
/// to the Ssruu holder as `component|leader|yssarilagent|<agent>` when its effect can resolve for
/// the holder. Empty unless `player` holds a readied Ssruu.
fn borrowed_component_actions(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
) -> Vec<crate::choice::ChoiceOption> {
    crate::factions::hooks_cards::borrowable_agents(state, content, player)
        .into_iter()
        .map(|(_, agent)| agent)
        .filter(|agent| is_action_window(content, agent))
        .filter(|agent| {
            action_leader_delivered(agent)
                || crate::factions::leader_action(state, content, player, agent).is_some()
        })
        .filter(|agent| can_resolve_action(state, content, player, agent))
        .map(|agent| {
            let name = content
                .get(ContentType::Leaders, agent.as_str())
                .and_then(|record| record.text("name"))
                .unwrap_or(agent.as_str());
            crate::choice::ChoiceOption::labelled(
                format!("component|leader|yssarilagent|{}", agent.as_str()),
                "component",
                format!("Ssruu: use {name}'s ability"),
            )
        })
        .collect()
}

/// Offer Harrugh Gefhara at the start of a use of PRODUCTION, and resolve it if taken.
///
/// "When 1 or more of your units use PRODUCTION: You may reduce the cost of each of your units to 0
/// during this use of PRODUCTION. If you do, purge this card." Call after the production's sequence
/// number has advanced: the free marker names that number, which is how the production window knows
/// this use, and no later one, is free.
///
/// # Errors
/// [`crate::choice::IllegalChoice`] if the decider answers with something not offered.
pub fn offer_production_hero(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    table: &mut crate::choice::Table,
    player: &PlayerId,
) -> Result<bool, crate::choice::IllegalChoice> {
    let hero = LeaderId::new("hacanhero");
    let unlocked = state
        .player(player)
        .is_some_and(|seat| seat.leaders.get(&hero) == Some(&LeaderStatus::Unlocked));
    if !unlocked {
        return Ok(false);
    }
    let choice = crate::choice::Choice::new(
        player.clone(),
        "Harrugh Gefhara: make this use of PRODUCTION free",
        vec![
            crate::choice::ChoiceOption::labelled(
                "use",
                "leader_hacanhero_free_production",
                "purge Harrugh Gefhara: every unit costs 0",
            ),
            crate::choice::ChoiceOption::decline(),
        ],
    )
    .contextualized(crate::decision_context::DecisionContext::new(
        player.clone(),
        crate::decision_context::DecisionSource::FactionAbility("hacanhero".to_owned()),
        "leader_hacanhero_free_production",
        state.phase,
        state.round,
    ));
    let answer = table.ask_seeing(
        &choice,
        &crate::choice::Observed::new(state, content, sources, galaxy),
    )?;
    if answer.is_decline() {
        return Ok(false);
    }
    let seq = state.production_seq;
    if let Some(seat) = state.player_mut(player) {
        seat.free_production_use = Some(seq);
        seat.leaders.insert(hero, LeaderStatus::Purged);
    }
    Ok(true)
}

/// End-of-round leader bookkeeping, run before the round counter advances.
///
/// Darktalon Treilla's effect lasts exactly the game round it was used in: "at the end of that
/// game round, purge this card" is where her purge lands, and with it the fleet-supply flag she
/// set. Returns what was purged, for events.
pub fn end_of_round(state: &mut GameState) -> Vec<(PlayerId, LeaderId)> {
    let hero = LeaderId::new("letnevhero");
    let mut purged = Vec::new();
    for seat in &mut state.players {
        if seat.fleet_supply_unlimited_until == Some(state.round) {
            seat.fleet_supply_unlimited_until = None;
            if seat.leaders.get(&hero) == Some(&LeaderStatus::Unlocked) {
                seat.leaders.insert(hero.clone(), LeaderStatus::Purged);
                purged.push((seat.id.clone(), hero.clone()));
            }
        }
    }
    purged
}

// -- abilities (M07-002 to M07-009) --------------------------------------------------------------

/// Leaders whose effect is a standing modifier rather than anything you use.
///
/// They live where they modify rather than in this registry, and are named here so coverage
/// counts them as implemented instead of reporting a gap that is not one — the same reason
/// `laws::enforced_aliases` exists.
#[must_use]
pub fn modifiers() -> std::collections::BTreeMap<&'static str, &'static str> {
    [
        (
            "xxchacommander",
            "leaders::elder_qanoj, read by vote::VoteWindow",
        ),
        ("hacancommander", "leaders::vote_bonus, read by vote::cast"),
        (
            "xxchahero",
            "leaders::combines_planet_values, read by production::payment_faces",
        ),
        (
            "jolnaragent",
            "strategy_cards::doctor_sucaban, read by strategy_cards::paid_research; a Ssruu copy is offered by the same function through strategy_cards::sucaban_sources",
        ),
        (
            "l1z1xcommander",
            "leaders::ignores_planetary_shield, read by invasion::can_bombard",
        ),
    ]
    .into_iter()
    .collect()
}

/// Extra votes this player casts, from an unlocked commander.
///
/// Read where votes are counted rather than applied at the card, so the bonus cannot be honoured
/// in one voting path and forgotten in another.
#[must_use]
pub fn vote_bonus(state: &GameState, player: &PlayerId) -> i64 {
    // Hacan's Gila the Silvertongue is not a flat bonus: the vote window asks how many trade
    // goods to spend, two votes each (`vote::Stage::TradeGoods`).
    if state.player(player).is_none() {
        return 0;
    }
    crate::factions::vote_bonus(state, player)
}

/// Elder Qanoj, Xxcha's commander: "Each planet you exhaust to cast votes provides 1 additional
/// vote. Game effects cannot prevent you from voting on an agenda."
#[must_use]
pub fn elder_qanoj(state: &GameState, player: &PlayerId) -> bool {
    crate::promissory::has_commander_ability(state, player, "xxchacommander")
}

/// Whether this player's units ignore a planetary shield when bombarding.
#[must_use]
pub fn ignores_planetary_shield(state: &GameState, player: &PlayerId) -> bool {
    crate::promissory::has_commander_ability(state, player, "l1z1xcommander")
}

/// Xxekir Grom: whether this player's exhausted planets pay their combined value.
///
/// > When you exhaust planets: combine the values of their resources and influence. Treat the
/// > combined value as if it were both resources and influence.
///
/// A passive while the hero is unlocked, not an ACTION -- the corpus text has no ACTION clause and
/// no purge, so the hero is never spent and the ability simply holds. Read where a planet's payable
/// value is computed rather than applied at the card, so it cannot be honoured on one spending path
/// and forgotten on another.
#[must_use]
pub fn combines_planet_values(state: &GameState, player: &PlayerId) -> bool {
    state.player(player).is_some_and(|seat| {
        seat.leaders.iter().any(|(leader, status)| {
            leader.as_str() == "xxchahero" && *status == LeaderStatus::Unlocked
        })
    })
}

/// Rear Admiral Farran: whether this player gains a trade good when one of their units sustains.
#[must_use]
pub fn pays_on_sustain(state: &GameState, content: &ContentStore, player: &PlayerId) -> bool {
    let _ = content;
    crate::promissory::has_commander_ability(state, player, "letnevcommander")
}

/// Leaders this engine can use, by id.
#[must_use]
pub fn registered_abilities() -> Vec<&'static str> {
    vec![
        "hacanagent",
        "hacanhero",
        "jolnaragent",
        "xxchahero",
        // The Thunder's Edge reprint `for_faction` deals at DEFAULT; delivered by `use_leader`.
        "xxchahero-te",
        "jolnarhero",
        "l1z1xagent",
        "l1z1xhero",
        "letnevagent",
        "letnevcommander",
        "letnevhero",
        "jolnarcommander",
        "solagent",
        "solcommander",
        "solhero",
        "xxchaagent",
    ]
    .into_iter()
    .chain(crate::factions::registered_leaders())
    .collect()
}

/// Leaders of these factions that still do nothing, by any of the three routes.
#[must_use]
pub fn unimplemented(content: &ContentStore, factions: &[&str]) -> Vec<LeaderId> {
    let known = registered_abilities();
    let standing = modifiers();
    factions
        .iter()
        .flat_map(|faction| for_faction(content, ti4_model::content_types::POK, faction))
        .filter(|leader| {
            !known.contains(&leader.as_str()) && !standing.contains_key(leader.as_str())
        })
        .collect()
}

/// Use an unlocked or readied leader's ability.
///
/// Returns `false` when the leader cannot be used — locked, purged, already exhausted, or with
/// no registered ability. A leader that reports success without doing anything is worse than one
/// that refuses, because nothing counts the gap.
///
/// # Panics
/// If a choice set read as non-empty loses its only element before it is asked — a content
/// inconsistency, not a reachable game state.
#[allow(
    clippy::too_many_lines,
    reason = "one arm per leader: the list is the point, and splitting it hides the set"
)]
pub fn use_leader(
    context: &mut crate::timing::TimingContext<'_>,
    player: &PlayerId,
    leader: &LeaderId,
) -> bool {
    // An offer made through Ssruu names `yssarilagent|<agent>` (see `component_actions`).
    if let Some((ssruu, source)) = split_borrowed(leader) {
        return ssruu.as_str() == "yssarilagent" && use_leader_text(context, player, &source);
    }
    // Agents are used readied; heroes and commanders unlocked. Commanders reach this point only
    // from their own delivery windows (combat, voting, sustain), never as a generic action —
    // which is why `usable` keeps them out of the offer while this gate admits them here.
    let ready = match kind_of(context.content, leader).as_deref() {
        Some(AGENT) => status(context.state, player, leader) == Some(LeaderStatus::Readied),
        Some(HERO | COMMANDER) => {
            status(context.state, player, leader) == Some(LeaderStatus::Unlocked)
        }
        _ => false,
    };
    if !ready {
        return false;
    }
    let done = dispatch_leader(context, player, leader);
    if done {
        // An agent exhausts; a hero is purged once used (51.9, 51.10) — except Darktalon
        // Treilla, whose card says she stays in play until the end of her game round.
        // `end_of_round` purges her there and clears the flag she set.
        if kind_of(context.content, leader).as_deref() == Some(HERO)
            && leader.as_str() != "letnevhero"
        {
            purge(context.state, player, leader);
        } else {
            exhaust(context.state, player, leader);
        }
    }
    done
}

/// Start a hero's strategy primary without purging it before its continuation finishes.
pub(crate) fn use_leader_strategy_primary(
    context: &mut crate::timing::TimingContext<'_>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Result<
    Option<(
        ti4_model::id::StrategyCardId,
        crate::strategy_cards::Ability,
    )>,
    crate::timing::TimingError,
> {
    if kind_of(context.content, leader).as_deref() != Some(HERO)
        || status(context.state, player, leader) != Some(LeaderStatus::Unlocked)
    {
        return Ok(None);
    }
    let before = context.state.clone();
    let before_dice = context.dice.clone();
    let before_rng = context.rng.clone();
    let outcome = crate::factions::use_leader_strategy_primary(context, player, leader);
    if outcome.is_err()
        || matches!(
            &outcome,
            Ok(Some((_, crate::strategy_cards::Ability::Unresolved)))
        )
    {
        *context.state = before;
        *context.dice = before_dice;
        *context.rng = before_rng;
    }
    outcome
}

/// Use an action leader with access to the game's existing timing resolver.
/// Modules without a timed effect keep the ordinary dispatch and status transitions.
pub fn use_leader_timed(
    context: &mut crate::timing::TimingContext<'_>,
    resolver: &mut crate::timing::Resolver,
    player: &PlayerId,
    leader: &LeaderId,
) -> Result<bool, crate::timing::TimingError> {
    let kind = kind_of(context.content, leader);
    let ready = match kind.as_deref() {
        Some(AGENT) => status(context.state, player, leader) == Some(LeaderStatus::Readied),
        Some(HERO | COMMANDER) => {
            status(context.state, player, leader) == Some(LeaderStatus::Unlocked)
        }
        _ => false,
    };
    let response = if ready {
        let before = context.state.clone();
        let before_dice = context.dice.clone();
        let before_rng = context.rng.clone();
        let before_sequence = context.event_sequence.clone();
        let before_resolver = resolver.checkpoint();
        match crate::factions::use_leader_timed(context, resolver, player, leader) {
            Ok(response) => response,
            Err(error) => {
                *context.event_sequence = before_sequence;
                resolver.restore(before_resolver);
                *context.state = before;
                *context.dice = before_dice;
                *context.rng = before_rng;
                return Err(error);
            }
        }
    } else {
        None
    };
    if let Some(done) = response {
        if done {
            if kind.as_deref() == Some(HERO) && leader.as_str() != "letnevhero" {
                purge(context.state, player, leader);
            } else {
                exhaust(context.state, player, leader);
            }
        }
        return Ok(done);
    }
    Ok(use_leader(context, player, leader))
}

/// The per-leader effect dispatch shared by [`use_leader`] and [`use_leader_text`]: runs the leader's
/// printed effect for `player` and reports whether it resolved. No status gate, no exhaust or purge:
/// the callers own both.
#[allow(
    clippy::too_many_lines,
    reason = "one arm per leader: the list is the point, and splitting it hides the set"
)]
fn dispatch_leader(
    context: &mut crate::timing::TimingContext<'_>,
    player: &PlayerId,
    leader: &LeaderId,
) -> bool {
    match leader.as_str() {
        // Evelyn DeLouis and Viscount Unlenn: one unit in the active system rolls an extra die
        // this combat round. Held as the round number, so it expires with the round it was used
        // in rather than improving every later one.
        "solagent" | "letnevagent" => {
            let round = context.state.combat_round_seq;
            let wanted = if leader.as_str() == "solagent" {
                "infantry"
            } else {
                "cruiser"
            };
            let unit = ti4_content::units::faction_unit(
                context.content,
                &context
                    .state
                    .player(player)
                    .map(|seat| seat.faction.to_string())
                    .unwrap_or_default(),
                wanted,
                context.sources,
            )
            .map_or_else(
                || ti4_model::id::UnitTypeId::new(wanted),
                |kind| ti4_model::id::UnitTypeId::new(kind.id()),
            );
            if let Some(seat) = context.state.player_mut(player) {
                seat.extra_die_round = Some(round);
                seat.extra_die_unit = Some(unit);
            }
            true
        }
        // Claire Gibson: an infantry onto a planet you control, as a ground combat opens.
        "solcommander" => {
            let spot = context
                .state
                .controlled_planets(player)
                .first()
                .map(|(system, planet)| ((*system).clone(), (*planet).clone()));
            match spot {
                Some((system, planet)) => {
                    crate::action_cards::place_units(
                        context,
                        player,
                        &system,
                        Some(&planet),
                        "infantry",
                        1,
                    );
                    true
                }
                None => false,
            }
        }
        // Jace X: every command token off the board, back to reinforcements.
        "solhero" => {
            for board in context.state.board.values_mut() {
                board.command_tokens.remove(player);
            }
            true
        }
        // Ready any planet — which one is the player's choice, not the engine's. The optional
        // infantry removal is decided before anything mutates, so a refused follow-up leaves the
        // position exactly as it was.
        "xxchaagent" => {
            let exhausted: Vec<ti4_model::id::PlanetId> =
                context.state.exhausted_planets.iter().cloned().collect();
            if exhausted.is_empty() {
                return false; // nothing to ready, so it was never offered
            }
            let planet = if exhausted.len() > 1 {
                // more than one exhausted planet: the choice is the player's.
                {
                    let choice = crate::choice::Choice::new(
                        player.clone(),
                        "Leader: ready which planet",
                        exhausted
                            .iter()
                            .map(|p| {
                                crate::choice::ChoiceOption::labelled(
                                    p.to_string(),
                                    "planet",
                                    p.to_string(),
                                )
                            })
                            .collect(),
                    )
                    .contextualized(
                        crate::decision_context::DecisionContext::new(
                            player.clone(),
                            crate::decision_context::DecisionSource::FactionAbility(
                                "xxchaagent".to_owned(),
                            ),
                            "leader_xxchaagent_ready_planet",
                            context.state.phase,
                            context.state.round,
                        ),
                    );
                    let Ok(answer) = context.ask_seeing(&choice) else {
                        return false;
                    };
                    match exhausted.into_iter().find(|p| p.as_str() == answer.id) {
                        Some(planet) => planet,
                        None => return false,
                    }
                }
            } else {
                exhausted.into_iter().next().expect("one exhausted planet")
            };
            // "If that planet is in a system adjacent to one you control, you may remove 1
            // infantry from there" — adjacency runs between systems, so the readied planet's
            // system must touch a system holding a planet of yours.
            let mut removal = false;
            if let Some(galaxy) = context.galaxy {
                let home: Option<ti4_model::id::SystemId> = context
                    .state
                    .board
                    .iter()
                    .find(|(_, board)| board.planet_units.contains_key(&planet))
                    .map(|(system, _)| system.clone());
                if let Some(home) = home {
                    let mine: std::collections::BTreeSet<&str> = context
                        .state
                        .controlled_planets(player)
                        .iter()
                        .map(|(system, _)| system.as_str())
                        .collect();
                    let adjacent_to_mine = galaxy
                        .adjacent(home.as_str())
                        .into_iter()
                        .any(|system| mine.contains(system));
                    if adjacent_to_mine {
                        let types = ti4_content::units::catalogue(context.content, context.sources);
                        let has_infantry = context
                            .state
                            .board
                            .get(&home)
                            .and_then(|board| board.planet_units.get(&planet))
                            .is_some_and(|units| {
                                units.iter().any(|unit| {
                                    unit.owner == *player
                                        && types
                                            .get(unit.type_id.as_str())
                                            .is_some_and(|kind| kind.base_type() == "infantry")
                                })
                            });
                        if has_infantry {
                            let choice = crate::choice::Choice::new(
                                player.clone(),
                                "Leader: remove 1 infantry from that planet?",
                                vec![
                                    crate::choice::ChoiceOption::labelled(
                                        "remove",
                                        "leader_xxchaagent_infantry",
                                        "remove 1 infantry",
                                    ),
                                    crate::choice::ChoiceOption::labelled(
                                        "keep",
                                        "leader_xxchaagent_infantry",
                                        "keep it",
                                    ),
                                ],
                            )
                            .contextualized(
                                crate::decision_context::DecisionContext::new(
                                    player.clone(),
                                    crate::decision_context::DecisionSource::FactionAbility(
                                        "xxchaagent".to_owned(),
                                    ),
                                    "leader_xxchaagent_remove_infantry",
                                    context.state.phase,
                                    context.state.round,
                                ),
                            );
                            let Ok(answer) = context.ask_seeing(&choice) else {
                                return false;
                            };
                            removal = answer.id == "remove";
                        }
                    }
                }
            }
            // All choices are settled; now the position changes, atomically.
            context.state.exhausted_planets.remove(&planet);
            // Readying someone else's planet is a favour: a promise to use this agent for them is
            // kept here.
            let controller = context
                .state
                .players
                .iter()
                .map(|seat| seat.id.clone())
                .find(|seat| {
                    context
                        .state
                        .controlled_planets(seat)
                        .iter()
                        .any(|(_, owned)| **owned == planet)
                });
            if let Some(helped) = controller.filter(|helped| helped != player) {
                crate::diplomacy::evaluate_event(
                    context.state,
                    &crate::diplomacy::DiplomacyEventContext::LeaderUsedFor {
                        user: player.clone(),
                        leader: "xxchaagent".to_owned(),
                        beneficiary: helped,
                    },
                )
                .expect("validated diplomacy promises settle deterministically");
            }
            if removal {
                let types = ti4_content::units::catalogue(context.content, context.sources);
                for board in context.state.board.values_mut() {
                    if let Some(units) = board.planet_units.get_mut(&planet)
                        && let Some(index) = units.iter().position(|unit| {
                            unit.owner == *player
                                && types
                                    .get(unit.type_id.as_str())
                                    .is_some_and(|kind| kind.base_type() == "infantry")
                        })
                    {
                        units.remove(index);
                        break;
                    }
                }
            }
            true
        }
        // Thunder's Edge hero (replaces the PoK `xxchahero`): "Place any combination of up to 4
        // PDS or mechs onto planets you control; ready each planet that you place a unit on.
        // Then, purge this card." Every placement is decided before anything is placed, so a
        // failed nested choice leaves the position untouched. Declining early is legal ("up to
        // 4"); the purge happens either way — it is part of the effect, not a reward for using.
        "xxchahero-te" => {
            let controlled: Vec<(ti4_model::id::SystemId, ti4_model::id::PlanetId)> = context
                .state
                .controlled_planets(player)
                .into_iter()
                .map(|(system, planet)| (system.clone(), planet.clone()))
                .collect();
            if controlled.is_empty() {
                return false; // nothing to place on, so it was never offered
            }
            let faction = context
                .state
                .player(player)
                .map(|seat| seat.faction.to_string())
                .unwrap_or_default();
            let catalogue = ti4_content::units::catalogue(context.content, context.sources);
            let unit_id = |base_type: &str| -> Option<ti4_model::id::UnitTypeId> {
                let faction_unit = ti4_content::units::faction_unit(
                    context.content,
                    &faction,
                    base_type,
                    context.sources,
                )
                .map(|unit| unit.id().to_owned());
                let generic = catalogue.get(base_type).map(|unit| unit.id().to_owned());
                faction_unit.or(generic).map(ti4_model::id::UnitTypeId::new)
            };
            let pds = unit_id("pds");
            let mech = unit_id("mech");
            let available = |kind: &Option<ti4_model::id::UnitTypeId>| -> usize {
                kind.as_ref()
                    .map(|unit| {
                        crate::supply::allowed(
                            context.state,
                            context.content,
                            context.sources,
                            player,
                            unit,
                            1,
                        )
                    })
                    .unwrap_or_default()
            };
            let mut pds_left = available(&pds);
            let mut mech_left = available(&mech);
            let mut plan: Vec<(
                ti4_model::id::UnitTypeId,
                (ti4_model::id::SystemId, ti4_model::id::PlanetId),
            )> = Vec::new();
            for _ in 0..4 {
                if pds_left == 0 && mech_left == 0 {
                    break;
                }
                let mut options: Vec<crate::choice::ChoiceOption> = Vec::new();
                if pds_left > 0 {
                    options.push(crate::choice::ChoiceOption::labelled(
                        "place|pds",
                        "leader_xxchahero_te_place",
                        "place 1 PDS on a planet you control",
                    ));
                }
                if mech_left > 0 {
                    options.push(crate::choice::ChoiceOption::labelled(
                        "place|mech",
                        "leader_xxchahero_te_place",
                        "place 1 mech on a planet you control",
                    ));
                }
                // Decline last: a first-option decider places, which is the card's natural play.
                options.push(crate::choice::ChoiceOption::labelled(
                    "stop",
                    "leader_xxchahero_te_stop",
                    "stop placing",
                ));
                let choice = crate::choice::Choice::new(
                    player.clone(),
                    "Leader: place up to 4 PDS or mechs (how many more?)",
                    options,
                )
                .contextualized(crate::decision_context::DecisionContext::new(
                    player.clone(),
                    crate::decision_context::DecisionSource::FactionAbility(
                        "xxchahero-te".to_owned(),
                    ),
                    "leader_xxchahero_te_place",
                    context.state.phase,
                    context.state.round,
                ));
                let Ok(answer) = context.ask_seeing(&choice) else {
                    return false;
                };
                if answer.id == "stop" {
                    break;
                }
                let (kind, counter) = match answer.id.as_str() {
                    "place|pds" => (pds.clone(), &mut pds_left),
                    "place|mech" => (mech.clone(), &mut mech_left),
                    _ => return false,
                };
                let Some(kind) = kind else {
                    return false;
                };
                *counter -= 1;
                if controlled.len() == 1 {
                    plan.push((kind, controlled[0].clone()));
                } else {
                    let options: Vec<crate::choice::ChoiceOption> = controlled
                        .iter()
                        .map(|(system, planet)| {
                            crate::choice::ChoiceOption::labelled(
                                format!("planet|{}", planet.as_str()),
                                "leader_xxchahero_te_planet",
                                format!("{planet} in {system}"),
                            )
                        })
                        .collect();
                    let choice = crate::choice::Choice::new(
                        player.clone(),
                        "Leader: which planet gets the unit?",
                        options,
                    )
                    .contextualized(
                        crate::decision_context::DecisionContext::new(
                            player.clone(),
                            crate::decision_context::DecisionSource::FactionAbility(
                                "xxchahero-te".to_owned(),
                            ),
                            "leader_xxchahero_te_planet",
                            context.state.phase,
                            context.state.round,
                        ),
                    );
                    let Ok(answer) = context.ask_seeing(&choice) else {
                        return false;
                    };
                    let Some((system, planet)) = controlled
                        .iter()
                        .find(|(_, p)| answer.id == format!("planet|{}", p.as_str()))
                    else {
                        return false;
                    };
                    plan.push((kind, (system.clone(), planet.clone())));
                }
            }
            for (unit, (system, planet)) in &plan {
                context
                    .state
                    .system_mut(system)
                    .planet_units
                    .entry(planet.clone())
                    .or_default()
                    .push(ti4_model::units::Unit::new(unit.clone(), player.clone()));
                crate::supply::stage_naaz_mech_placed(context.state, player, system, unit);
                // "ready each planet that you place a unit on"
                context.state.exhausted_planets.remove(planet);
            }
            true
        }
        // "Gain 2 commodities or replenish another player's commodities" — the branch is the
        // player's, and so is the target when there is more than one way to spend it.
        "hacanagent" => {
            let others: Vec<PlayerId> = context
                .state
                .players
                .iter()
                .map(|seat| seat.id.clone())
                .filter(|id| id != player)
                .collect();
            if others.is_empty() {
                return false; // nobody to replenish and no self-gain branch to offer is a broken table
            }
            let choice = crate::choice::Choice::new(
                player.clone(),
                "Leader: gain 2 commodities or replenish another player",
                std::iter::once(crate::choice::ChoiceOption::labelled(
                    "self",
                    "leader_hacanagent_branch",
                    "gain 2 commodities",
                ))
                .chain(others.iter().map(|id| {
                    crate::choice::ChoiceOption::labelled(
                        id.to_string(),
                        "leader_hacanagent_branch",
                        format!("replenish {id}"),
                    )
                }))
                .collect(),
            )
            .contextualized(crate::decision_context::DecisionContext::new(
                player.clone(),
                crate::decision_context::DecisionSource::FactionAbility("hacanagent".to_owned()),
                "leader_hacanagent_branch",
                context.state.phase,
                context.state.round,
            ));
            let Ok(answer) = context.ask_seeing(&choice) else {
                return false;
            };
            if answer.id == "self" {
                let limit = context
                    .state
                    .player(player)
                    .and_then(|seat| {
                        ti4_content::factions::get(context.content, seat.faction.as_str())
                            .map(|faction| faction.commodities())
                    })
                    .unwrap_or(0);
                let mut gained = 0;
                if let Some(seat) = context.state.player_mut(player) {
                    let after = (seat.commodities + 2).min(limit);
                    gained = (after - seat.commodities).max(0);
                    seat.commodities = after;
                }
                crate::supply::note_commodities_gained(context.state, player, gained);
            } else {
                // "Replenish" fills the other player's commodities up to their own limit.
                match others.into_iter().find(|id| id.as_str() == answer.id) {
                    Some(target) => {
                        let limit = context
                            .state
                            .player(&target)
                            .and_then(|seat| {
                                ti4_content::factions::get(context.content, seat.faction.as_str())
                                    .map(|faction| faction.commodities())
                            })
                            .unwrap_or(0);
                        let mut gained = 0;
                        if let Some(seat) = context.state.player_mut(&target) {
                            gained = (limit - seat.commodities).max(0);
                            seat.commodities = limit;
                        }
                        crate::supply::note_commodities_gained(context.state, &target, gained);
                        // Trade Agreement: "When the <color> player replenishes commodities".
                        crate::promissory::trade_agreement_on_replenish(context.state, &target);
                        // A structured promise to use this agent for that seat is kept here.
                        crate::diplomacy::evaluate_event(
                            context.state,
                            &crate::diplomacy::DiplomacyEventContext::LeaderUsedFor {
                                user: player.clone(),
                                leader: "hacanagent".to_owned(),
                                beneficiary: target,
                            },
                        )
                        .expect("validated diplomacy promises settle deterministically");
                    }
                    None => return false,
                }
            }
            true
        }
        // Harrugh Gefhara: this use of PRODUCTION costs nothing. Marks the use rather than
        // acting now, so a later production in the same game is not free as well.
        "hacanhero" => {
            let seq = context.state.production_seq;
            if let Some(seat) = context.state.player_mut(player) {
                seat.free_production_use = Some(seq);
            }
            true
        }

        // The Helmsman: gather the flagship and any dreadnoughts into one system holding no
        // rival ships. "Choose 1 system" — which safe system is the player's call when more than
        // one qualifies. The card names no move value and is not a tactical action, so this
        // *places* rather than moves — range and anomalies do not apply by its own wording.
        "l1z1xhero" => {
            let Some(galaxy) = context.galaxy else {
                return false; // no map, no system to name
            };
            let types = ti4_content::units::catalogue(context.content, context.sources);
            // Nothing anywhere to gather means the use could do nothing, so it was never offered.
            let has_big_ships = context.state.board.values().any(|board| {
                board.units.iter().any(|unit| {
                    &unit.owner == player
                        && types.get(unit.type_id.as_str()).is_some_and(|kind| {
                            matches!(kind.base_type(), "flagship" | "dreadnought")
                        })
                })
            });
            if !has_big_ships {
                return false;
            }
            let safe: Vec<ti4_model::id::SystemId> = galaxy
                .system_ids()
                .into_iter()
                .map(ti4_model::id::SystemId::new)
                .filter(|system| {
                    !context.state.system_state(system).units.iter().any(|unit| {
                        &unit.owner != player
                            && types
                                .get(unit.type_id.as_str())
                                .is_some_and(ti4_content::units::UnitType::is_ship)
                    })
                })
                .collect();
            if safe.is_empty() {
                return false;
            }
            let destination = if safe.len() > 1 {
                // more than one safe system: the choice is the player's.
                {
                    let choice = crate::choice::Choice::new(
                        player.clone(),
                        "Leader: which system to gather your ships in",
                        safe.iter()
                            .map(|system| {
                                crate::choice::ChoiceOption::labelled(
                                    system.to_string(),
                                    "system",
                                    system.to_string(),
                                )
                            })
                            .collect(),
                    )
                    .contextualized(
                        crate::decision_context::DecisionContext::new(
                            player.clone(),
                            crate::decision_context::DecisionSource::FactionAbility(
                                "l1z1xhero".to_owned(),
                            ),
                            "leader_l1z1xhero_destination",
                            context.state.phase,
                            context.state.round,
                        ),
                    );
                    let Ok(answer) = context.ask_seeing(&choice) else {
                        return false;
                    };
                    match safe.into_iter().find(|system| system.as_str() == answer.id) {
                        Some(system) => system,
                        None => return false,
                    }
                }
            } else {
                safe.into_iter().next().expect("one safe system")
            };
            let mut gathered = Vec::new();
            for (system, board) in &mut context.state.board {
                if *system == destination {
                    continue;
                }
                board.units.retain(|unit| {
                    let named = &unit.owner == player
                        && types.get(unit.type_id.as_str()).is_some_and(|kind| {
                            matches!(kind.base_type(), "flagship" | "dreadnought")
                        });
                    if named {
                        gathered.push(unit.clone());
                    }
                    !named
                });
            }
            if gathered.is_empty() {
                return false;
            }
            context
                .state
                .system_mut(&destination)
                .units
                .extend(gathered);
            true
        }
        // I48S: one infantry in the active system becomes a mech. The card lets the *activating*
        // player benefit, which is usually somebody else — agents are traded favours.
        "l1z1xagent" => {
            let (Some(system), Some(target)) = (
                context.state.active_system.clone(),
                context.state.active.clone(),
            ) else {
                return false;
            };
            let faction = context
                .state
                .player(&target)
                .map(|seat| seat.faction.to_string())
                .unwrap_or_default();
            let Some(mech) = ti4_content::units::faction_unit(
                context.content,
                &faction,
                "mech",
                context.sources,
            )
            .map(|kind| ti4_model::id::UnitTypeId::new(kind.id())) else {
                return false; // a seat with no mech has nothing to upgrade into
            };
            if crate::supply::allowed(
                context.state,
                context.content,
                context.sources,
                &target,
                &mech,
                1,
            ) == 0
            {
                return false;
            }
            let types = ti4_content::units::catalogue(context.content, context.sources);
            let board = context.state.system_mut(&system);
            let mut swapped = false;
            for units in board.planet_units.values_mut() {
                if let Some(index) = units.iter().position(|unit| {
                    unit.owner == target
                        && types
                            .get(unit.type_id.as_str())
                            .is_some_and(|kind| kind.base_type() == "infantry")
                }) {
                    units[index] = ti4_model::units::Unit::new(mech.clone(), target.clone());
                    swapped = true;
                    break;
                }
            }
            if swapped {
                crate::supply::stage_naaz_mech_placed(context.state, &target, &system, &mech);
            }
            // Agents are traded favours: a structured promise to use this one for the active
            // seat is kept when the swap happens.
            if swapped && &target != player {
                crate::diplomacy::evaluate_event(
                    context.state,
                    &crate::diplomacy::DiplomacyEventContext::LeaderUsedFor {
                        user: player.clone(),
                        leader: "l1z1xagent".to_owned(),
                        beneficiary: target,
                    },
                )
                .expect("validated diplomacy promises settle deterministically");
            }
            swapped
        }
        // Rin, the Masters' Legacy: "for each non-unit upgrade technology you own, you may
        // replace that technology with any technology of the same colour from the deck. Then,
        // purge this card." One use settles every swap — each is its own "you may", so the
        // player decides per technology whether to trade it and, if so, for what — and the hero
        // purges in the tail that follows.
        "jolnarhero" => {
            let held: Vec<ti4_model::id::TechnologyId> = context
                .state
                .player(player)
                .map(|seat| seat.technologies.iter().cloned().collect())
                .unwrap_or_default();
            // "For each non-unit upgrade technology you own" (90.7b): an upgrade has no colour,
            // so there is nothing to replace it with in its own colour. The class is the
            // UNITUPGRADE type, not `baseUpgrade` — the generic upgrades (Carrier II, War Sun,
            // ...) carry that key not at all.
            let eligible: Vec<ti4_model::id::TechnologyId> = held
                .into_iter()
                .filter(|alias| {
                    !crate::technology::is_unit_upgrade(context.content, alias)
                        && context
                            .content
                            .get(ContentType::Technologies, alias.as_str())
                            .and_then(|r| r.strings("types").first().copied())
                            .is_some()
                })
                .collect();
            if eligible.is_empty() {
                return false; // nothing swappable, so the hero is not spent
            }
            let mut opportunity = false;
            for alias in &eligible {
                let colour = context
                    .content
                    .get(ContentType::Technologies, alias.as_str())
                    .and_then(|r| r.strings("types").first().copied());
                let Some(colour) = colour else {
                    continue; // unreachable: eligibility required a colour
                };
                // Recomputed per swap: what was just traded in is owned now and cannot be
                // traded for again.
                let replacements: Vec<ti4_model::id::TechnologyId> = context
                    .content
                    .from_sources(ContentType::Technologies, context.sources)
                    .filter(|r| {
                        r.strings("types")
                            .first()
                            .is_some_and(|kind| *kind == colour)
                    })
                    // The same UNITUPGRADE class test as above, at record level: an upgrade is
                    // never a valid replacement, whatever its `baseUpgrade` says.
                    .filter(|r| !r.strings("types").contains(&"UNITUPGRADE"))
                    .filter_map(|r| r.text("alias").map(ti4_model::id::TechnologyId::new))
                    // 90.11: a faction technology belongs to its faction alone. "From the deck" is
                    // the player's own technology deck -- the generic technologies and their own
                    // faction's -- so another faction's technology is never a replacement. The
                    // list used to filter only by colour and offered every faction's.
                    .filter(|candidate| {
                        crate::technology::faction_of(context.content, candidate).is_none_or(
                            |owner| {
                                context
                                    .state
                                    .player(player)
                                    .is_some_and(|seat| seat.faction.as_str() == owner)
                            },
                        )
                    })
                    .filter(|candidate| {
                        context
                            .state
                            .player(player)
                            .is_some_and(|seat| !seat.technologies.contains(candidate))
                    })
                    .collect();
                if replacements.is_empty() {
                    continue; // nothing to trade this one for
                }
                opportunity = true;
                // The decline comes last: a decider that simply takes the first option trades,
                // which is what "you may replace" resolves to when nobody says otherwise.
                let choice = crate::choice::Choice::new(
                    player.clone(),
                    "Leader: replace which technology with what",
                    replacements
                        .iter()
                        .map(|candidate| {
                            crate::choice::ChoiceOption::labelled(
                                candidate.to_string(),
                                "leader_jolnarhero_swap",
                                format!("trade for {candidate}"),
                            )
                        })
                        .chain(std::iter::once(crate::choice::ChoiceOption::labelled(
                            format!("keep|{alias}"),
                            "leader_jolnarhero_swap",
                            format!("keep {alias}"),
                        )))
                        .collect(),
                )
                .contextualized(crate::decision_context::DecisionContext::new(
                    player.clone(),
                    crate::decision_context::DecisionSource::FactionAbility(
                        "jolnarhero".to_owned(),
                    ),
                    "leader_jolnarhero_swap",
                    context.state.phase,
                    context.state.round,
                ));
                let Ok(answer) = context.ask_seeing(&choice) else {
                    return false;
                };
                if answer.id == format!("keep|{alias}") {
                    continue; // "you may": declining is a legal resolution
                }
                match replacements
                    .into_iter()
                    .find(|candidate| candidate.as_str() == answer.id)
                {
                    Some(replacement) => {
                        if let Some(seat) = context.state.player_mut(player) {
                            seat.technologies.remove(alias);
                            seat.technologies.insert(replacement);
                        }
                        crate::technology::apply_unit_upgrades(
                            context.state,
                            context.content,
                            context.sources,
                            player,
                        );
                    }
                    None => return false,
                }
            }
            // Declining every swap is a legal resolution, but a use with no swap on offer at all
            // must not burn the card — that case was never offered in the first place.
            opportunity
        }

        // Darktalon Treilla: fleet supply is limited by neither laws nor the pool this round.
        "letnevhero" => {
            let round = context.state.round;
            if context
                .state
                .player(player)
                .is_some_and(|seat| seat.fleet_supply_unlimited_until == Some(round))
            {
                return false; // already active this round, so it was not offered twice
            }
            if let Some(seat) = context.state.player_mut(player) {
                seat.fleet_supply_unlimited_until = Some(round);
            }
            true
        }
        _ => crate::factions::use_leader(context, player, leader).unwrap_or(false),
    }
}

/// Separator in a borrowed-agent id: `yssarilagent|<agent>` (see [`use_leader_text`]).
pub const BORROWED_SEPARATOR: char = '|';

/// Split a borrowed-agent leader id (`yssarilagent|solagent`) into Ssruu's id and the source agent.
#[must_use]
pub fn split_borrowed(leader: &LeaderId) -> Option<(LeaderId, LeaderId)> {
    let (ssruu, source) = leader.as_str().split_once(BORROWED_SEPARATOR)?;
    Some((LeaderId::new(ssruu), LeaderId::new(source)))
}

/// Use `source_agent`'s text ability as `as_player`, through Ssruu, Clever Genome (`yssarilagent`):
/// "This card has the text ability of each other player's agent, even if that agent is exhausted."
///
/// Runs the same per-leader dispatch as [`use_leader`] (including `factions::use_leader`) but
/// without the readied gate on `source_agent` and without exhausting it. Returns `false`, changing
/// nothing, unless `source_agent` is in `hooks_cards::borrowable_agents` for `as_player` (the seat
/// holds a readied Ssruu and the agent is another seat's, readied or exhausted).
///
/// On success Ssruu itself exhausts: the borrowed text says "exhaust this card", and the card
/// carrying that text is Ssruu. Then every module's `borrowed_agent_used` hook runs.
///
/// Limit: the effect runs as `as_player`. A module agent whose `use_leader`/`leader_action` hook
/// requires its own seat to hold the leader declines here (returns `false`, nothing happens), so
/// it is simply not borrowable until that module accepts a borrower.
pub fn use_leader_text(
    context: &mut crate::timing::TimingContext<'_>,
    as_player: &PlayerId,
    source_agent: &LeaderId,
) -> bool {
    let borrowable =
        crate::factions::hooks_cards::borrowable_agents(context.state, context.content, as_player);
    if !borrowable.iter().any(|(_, agent)| agent == source_agent) {
        return false;
    }
    if !dispatch_leader(context, as_player, source_agent) {
        return false;
    }
    exhaust(context.state, as_player, &LeaderId::new("yssarilagent"));
    crate::factions::hooks_cards::borrowed_agent_used(context, as_player, source_agent);
    true
}

/// Standing AFTER SYSTEM_ACTIVATED slots for each seated L1Z1X source and Ssruu borrower.
///
/// The roster is fixed when the resolver is armed. Each slot rechecks the live, exact source
/// pair through `hooks_cards::borrowable_agents`, so a source that is removed or locked after
/// arming is not offered, while an Exhausted source remains copyable as printed.
#[must_use]
pub fn ssruu_l1z1x_activation_abilities(state: &GameState) -> Vec<crate::timing::Ability> {
    use crate::timing::{Ability, Relation};
    use std::sync::Arc;

    let borrowers: Vec<_> = state
        .players
        .iter()
        .filter(|seat| seat.faction.as_str() == "yssaril")
        .map(|seat| seat.id.clone())
        .collect();
    let sources: Vec<_> = state
        .players
        .iter()
        .filter(|seat| seat.faction.as_str() == "l1z1x")
        .map(|seat| seat.id.clone())
        .collect();

    borrowers
        .into_iter()
        .flat_map(|borrower| {
            let borrower_for_filter = borrower.clone();
            sources
                .iter()
                .filter(move |source| *source != &borrower_for_filter)
                .map(move |source_owner| {
                    let borrower = borrower.clone();
                    let source_owner = source_owner.clone();
                    let condition_borrower = borrower.clone();
                    let condition_source = source_owner.clone();
                    let effect_borrower = borrower.clone();
                    Ability::stateful(
                        format!(
                            "leader:ssruu-copy:{}:l1z1xagent:{}:SYSTEM_ACTIVATED:after",
                            borrower.as_str(),
                            source_owner.as_str()
                        ),
                        borrower,
                        "SYSTEM_ACTIVATED",
                        Relation::After,
                        Arc::new(move |event, _, context| {
                            let Some((beneficiary, system, mech, planets)) =
                                l1z1x_copy_activation_plan(
                                    context.state,
                                    context.content,
                                    context.sources,
                                    event,
                                    &source_owner,
                                    &effect_borrower,
                                )
                            else {
                                return Ok(());
                            };
                            let planet = if planets.len() == 1 {
                                planets.into_iter().next().expect("one legal planet")
                            } else {
                                let choice = crate::choice::Choice::new(
                                    beneficiary.clone(),
                                    "I48S (Ssruu): replace which infantry in the active system?",
                                    planets
                                        .iter()
                                        .map(|planet| {
                                            crate::choice::ChoiceOption::labelled(
                                                planet.to_string(),
                                                "leader_l1z1xagent_copy_planet",
                                                format!(
                                                    "replace infantry on {} with a mech",
                                                    ti4_content::galaxy::planet(
                                                        context.content,
                                                        planet.as_str(),
                                                        context.sources
                                                    )
                                                    .and_then(|record| record.name())
                                                    .unwrap_or(planet.as_str())
                                                ),
                                            )
                                        })
                                        .collect(),
                                )
                                .contextualized(
                                    crate::decision_context::DecisionContext::new(
                                        beneficiary.clone(),
                                        crate::decision_context::DecisionSource::Content(
                                            "l1z1xagent".to_owned(),
                                        ),
                                        "leader_l1z1xagent_copy_planet",
                                        context.state.phase,
                                        context.state.round,
                                    ),
                                );
                                let answer = context
                                    .ask_seeing(&choice)
                                    .map_err(crate::timing::TimingError::IllegalChoice)?;
                                let Some(planet) = planets
                                    .into_iter()
                                    .find(|planet| planet.as_str() == answer.id)
                                else {
                                    return Ok(());
                                };
                                planet
                            };
                            let types =
                                ti4_content::units::catalogue(context.content, context.sources);
                            let Some(units) = context
                                .state
                                .system_mut(&system)
                                .planet_units
                                .get_mut(&planet)
                            else {
                                return Ok(());
                            };
                            let Some(index) = units.iter().position(|unit| {
                                unit.owner == beneficiary
                                    && types
                                        .get(unit.type_id.as_str())
                                        .is_some_and(|kind| kind.base_type() == "infantry")
                            }) else {
                                return Ok(());
                            };
                            units[index] =
                                ti4_model::units::Unit::new(mech.clone(), beneficiary.clone());
                            crate::supply::stage_naaz_mech_placed(
                                context.state,
                                &beneficiary,
                                &system,
                                &mech,
                            );
                            if effect_borrower != beneficiary {
                                crate::diplomacy::evaluate_event(
                                    context.state,
                                    &crate::diplomacy::DiplomacyEventContext::LeaderUsedFor {
                                        user: effect_borrower.clone(),
                                        leader: "l1z1xagent".to_owned(),
                                        beneficiary,
                                    },
                                )
                                .expect("validated diplomacy promises settle deterministically");
                            }
                            let source_agent = LeaderId::new("l1z1xagent");
                            // The selected active player's infantry changed; only the borrower
                            // owns the Ssruu card that carried this copied text.
                            exhaust(
                                context.state,
                                &effect_borrower,
                                &LeaderId::new("yssarilagent"),
                            );
                            crate::factions::hooks_cards::borrowed_agent_used(
                                context,
                                &effect_borrower,
                                &source_agent,
                            );
                            Ok(())
                        }),
                    )
                    .with_optional(true)
                    .with_stateful_condition(Arc::new(
                        move |event, _, context| {
                            l1z1x_copy_activation_plan(
                                context.state,
                                context.content,
                                context.sources,
                                event,
                                &condition_source,
                                &condition_borrower,
                            )
                            .is_some()
                        },
                    ))
                })
        })
        .collect()
}

/// Recheck live Ssruu/source rights, exact activation actor/location and legal I48S supply.
fn l1z1x_copy_activation_plan(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    event: &crate::event::Event,
    source_owner: &PlayerId,
    borrower: &PlayerId,
) -> Option<(
    PlayerId,
    ti4_model::id::SystemId,
    ti4_model::id::UnitTypeId,
    Vec<ti4_model::id::PlanetId>,
)> {
    let Some(beneficiary) = event.text("player").map(PlayerId::new) else {
        return None;
    };
    let Some(system) = event.text("system").map(ti4_model::id::SystemId::new) else {
        return None;
    };
    if state.active.as_ref() != Some(&beneficiary)
        || state.active_system.as_ref() != Some(&system)
        || !crate::factions::hooks_cards::borrowable_agents(state, content, borrower)
            .iter()
            .any(|(owner, agent)| owner == source_owner && agent.as_str() == "l1z1xagent")
    {
        return None;
    }
    let Some(mech) = state
        .player(&beneficiary)
        .and_then(|seat| {
            ti4_content::units::faction_unit(content, seat.faction.as_str(), "mech", sources)
        })
        .map(|kind| ti4_model::id::UnitTypeId::new(kind.id()))
    else {
        return None;
    };
    let types = ti4_content::units::catalogue(content, sources);
    let planets: Vec<_> = state
        .board
        .get(&system)?
        .planet_units
        .iter()
        .filter(|(_, units)| {
            units.iter().any(|unit| {
                unit.owner == beneficiary
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(|kind| kind.base_type() == "infantry")
            })
        })
        .map(|(planet, _)| planet.clone())
        .collect();
    (!planets.is_empty()
        && crate::supply::allowed(state, content, sources, &beneficiary, &mech, 1) > 0)
        .then_some((beneficiary, system, mech, planets))
}

#[cfg(test)]
mod tests {

    /// Give this player a leader in a usable state.
    fn holding(state: &mut GameState, leader: &str, status: LeaderStatus) -> LeaderId {
        let id = LeaderId::new(leader);
        state
            .player_mut(&player())
            .unwrap()
            .leaders
            .insert(id.clone(), status);
        id
    }

    fn use_it(state: &mut GameState, leader: &LeaderId) -> bool {
        let mut table = crate::choice::Table::new();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let mut context = crate::timing::TimingContext {
            state,
            content: ContentStore::embedded(),
            sources: POK,
            table: &mut table,
            dice: &mut dice,
            rng: &mut rng,
            event_sequence: &mut sequence,
            galaxy: None,
        };
        use_leader(&mut context, &player(), leader)
    }

    fn two_planet_system() -> (
        ti4_model::id::SystemId,
        ti4_model::id::PlanetId,
        ti4_model::id::PlanetId,
    ) {
        let content = ContentStore::embedded();
        let (_, system) = ti4_content::galaxy::all_systems(content, POK)
            .into_iter()
            .find(|(_, system)| system.planets().len() >= 2)
            .expect("content has a system with two printed planets");
        let planets = system.planets();
        (
            ti4_model::id::SystemId::new(system.id()),
            ti4_model::id::PlanetId::new(planets[0]),
            ti4_model::id::PlanetId::new(planets[1]),
        )
    }

    fn ssruu_l1z1x_activation_game(
        source_status: LeaderStatus,
        with_infantry: bool,
    ) -> (GameState, ti4_model::id::SystemId, ti4_model::id::PlanetId) {
        let mut state = game(&["a", "b", "c"]);
        let borrower = PlayerId::new("a");
        let source = PlayerId::new("b");
        let beneficiary = PlayerId::new("c");
        state.player_mut(&borrower).unwrap().faction = ti4_model::id::FactionId::new("yssaril");
        state.player_mut(&source).unwrap().faction = ti4_model::id::FactionId::new("l1z1x");
        state.player_mut(&beneficiary).unwrap().faction = ti4_model::id::FactionId::new("sol");
        state
            .player_mut(&borrower)
            .unwrap()
            .leaders
            .insert(LeaderId::new("yssarilagent"), LeaderStatus::Readied);
        state
            .player_mut(&source)
            .unwrap()
            .leaders
            .insert(LeaderId::new("l1z1xagent"), source_status);
        let (system, planet, _) = two_planet_system();
        if with_infantry {
            crate::fixtures::put_on_planet(
                &mut state,
                &system,
                &planet,
                "infantry",
                &beneficiary,
                1,
            );
        }
        // These units must remain untouched: neither the borrower nor the source is the printed
        // activation beneficiary.
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &borrower, 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &source, 1);
        state.active = Some(beneficiary);
        state.active_system = Some(system.clone());
        (state, system, planet)
    }

    fn emit_ssruu_l1z1x_activation(
        state: &mut GameState,
        abilities: Vec<crate::timing::Ability>,
        answers: &[String],
    ) -> Result<crate::event::Event, crate::timing::TimingError> {
        use std::collections::BTreeMap;

        let borrower = PlayerId::new("a");
        let source = PlayerId::new("b");
        let beneficiary = PlayerId::new("c");
        let system = state.active_system.as_ref().expect("test active system");
        let mut payload = BTreeMap::new();
        payload.insert("player".to_owned(), beneficiary.to_string().into());
        payload.insert("system".to_owned(), system.to_string().into());
        let event = crate::event::Event::new(1, "SYSTEM_ACTIVATED", payload);

        let mut resolver = crate::timing::Resolver::new(
            vec![borrower, source, beneficiary.clone()],
            Some(beneficiary),
            crate::choice::Table::new(),
        );
        resolver.register(abilities);
        let mut table = crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new(
            answers.to_vec(),
        )));
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let mut context = crate::timing::TimingContext {
            state,
            content: ContentStore::embedded(),
            sources: POK,
            table: &mut table,
            dice: &mut dice,
            rng: &mut rng,
            event_sequence: &mut sequence,
            galaxy: None,
        };
        resolver.emit_with_context(&mut context, event, |_, _| {})
    }

    #[test]
    fn ssruu_copies_l1z1x_after_activation_for_the_event_actor_only() {
        let content = ContentStore::embedded();
        for source_status in [LeaderStatus::Readied, LeaderStatus::Exhausted] {
            let (mut state, system, planet) = ssruu_l1z1x_activation_game(source_status, true);
            let (_, _, other_planet) = two_planet_system();
            crate::fixtures::put_on_planet(
                &mut state,
                &system,
                &other_planet,
                "infantry",
                &PlayerId::new("c"),
                1,
            );
            let abilities = ssruu_l1z1x_activation_abilities(&state);
            assert_eq!(abilities.len(), 1, "one static borrower/source slot");
            let choice_id = abilities[0].id.clone();
            emit_ssruu_l1z1x_activation(
                &mut state,
                abilities,
                &[choice_id, other_planet.to_string()],
            )
            .expect("resolver accepts the copied activation");

            let board = state.system_state(&system);
            let types = ti4_content::units::catalogue(content, POK);
            let base = |location: &ti4_model::id::PlanetId, owner: &str, kind: &str| {
                board.planet_units[location]
                    .iter()
                    .filter(|unit| {
                        unit.owner.as_str() == owner
                            && types
                                .get(unit.type_id.as_str())
                                .is_some_and(|unit_type| unit_type.base_type() == kind)
                    })
                    .count()
            };
            assert_eq!(base(&planet, "c", "infantry"), 1, "unchosen target stays");
            assert_eq!(base(&planet, "c", "mech"), 0, "the choice is honored");
            assert_eq!(
                base(&other_planet, "c", "infantry"),
                0,
                "chosen infantry is replaced"
            );
            assert_eq!(
                base(&other_planet, "c", "mech"),
                1,
                "the actor receives its mech"
            );
            assert_eq!(base(&planet, "a", "infantry"), 1, "borrower is untouched");
            assert_eq!(base(&planet, "b", "infantry"), 1, "source is untouched");
            assert_eq!(
                status(&state, &PlayerId::new("a"), &LeaderId::new("yssarilagent")),
                Some(LeaderStatus::Exhausted),
                "only Ssruu exhausts"
            );
            assert_eq!(
                status(&state, &PlayerId::new("b"), &LeaderId::new("l1z1xagent")),
                Some(source_status),
                "the source agent remains unchanged"
            );
        }
    }

    #[test]
    fn game_tactical_activation_dispatches_the_registered_ssruu_l1z1x_copy() {
        let content = ContentStore::embedded();
        let mut state = game(&["a", "b", "c"]);
        let (borrower, source, beneficiary) =
            (PlayerId::new("a"), PlayerId::new("b"), PlayerId::new("c"));
        state.player_mut(&borrower).unwrap().faction = ti4_model::id::FactionId::new("yssaril");
        state.player_mut(&source).unwrap().faction = ti4_model::id::FactionId::new("l1z1x");
        state.player_mut(&beneficiary).unwrap().faction = ti4_model::id::FactionId::new("sol");
        state
            .player_mut(&borrower)
            .unwrap()
            .leaders
            .insert(LeaderId::new("yssarilagent"), LeaderStatus::Readied);
        state
            .player_mut(&source)
            .unwrap()
            .leaders
            .insert(LeaderId::new("l1z1xagent"), LeaderStatus::Exhausted);
        state.phase = ti4_model::state::Phase::Action;
        state.active = Some(beneficiary.clone());
        let (system, planet, other_planet) = two_planet_system();
        crate::fixtures::put(&mut state, &system, "destroyer", &beneficiary, 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &beneficiary, 1);
        crate::fixtures::put_on_planet(
            &mut state,
            &system,
            &other_planet,
            "infantry",
            &beneficiary,
            1,
        );
        let copy_id = ssruu_l1z1x_activation_abilities(&state)
            .into_iter()
            .next()
            .expect("a seated borrower/source pair has a static slot")
            .id;
        let galaxy = ti4_content::galaxy::Galaxy::placed(
            content,
            &[(system.as_str(), ti4_model::hex::Hex::new(0, 0))],
            POK,
        )
        .expect("the target system is on the map");
        let table = crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new([
            crate::game::TACTICAL_ACTION_ID.to_owned(),
            system.to_string(),
            copy_id,
            other_planet.to_string(),
        ])));
        let mut game = crate::game::Game::with_table(state, content, table).with_galaxy(galaxy);

        let mut activated = false;
        for _ in 0..8 {
            let step = game.step();
            assert_eq!(
                step.error, None,
                "the integrated tactical path remains legal"
            );
            if game.events.iter().any(|event| event == "SYSTEM_ACTIVATED") {
                activated = true;
                break;
            }
        }
        assert!(activated, "Game reached the typed activation window");
        let units = &game.state.system_state(&system).planet_units;
        let types = ti4_content::units::catalogue(content, POK);
        assert!(
            units[&planet].iter().any(|unit| {
                unit.owner == beneficiary
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(|kind| kind.base_type() == "infantry")
            }),
            "the unchosen planet remains untouched"
        );
        assert!(
            units[&other_planet].iter().any(|unit| {
                unit.owner == beneficiary
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(|kind| kind.base_type() == "mech")
            }),
            "the activating player chose the planet that received the mech"
        );
        assert_eq!(
            status(&game.state, &borrower, &LeaderId::new("yssarilagent")),
            Some(LeaderStatus::Exhausted)
        );
        assert_eq!(
            status(&game.state, &source, &LeaderId::new("l1z1xagent")),
            Some(LeaderStatus::Exhausted),
            "an Exhausted source is read as text and remains unchanged"
        );
    }

    #[test]
    fn ssruu_activation_copy_rechecks_source_and_supply_at_resolution() {
        let (mut state, _, _) = ssruu_l1z1x_activation_game(LeaderStatus::Readied, true);
        let abilities = ssruu_l1z1x_activation_abilities(&state);
        state
            .player_mut(&PlayerId::new("b"))
            .unwrap()
            .leaders
            .insert(LeaderId::new("l1z1xagent"), LeaderStatus::Locked);
        let before = state.clone();
        emit_ssruu_l1z1x_activation(&mut state, abilities, vec!["decline".to_owned()].as_slice())
            .expect("missing live source makes the listener ineligible");
        assert_eq!(state, before, "source rights are recomputed, not cached");

        let (mut state, _, _) = ssruu_l1z1x_activation_game(LeaderStatus::Exhausted, false);
        let abilities = ssruu_l1z1x_activation_abilities(&state);
        let before = state.clone();
        emit_ssruu_l1z1x_activation(&mut state, abilities, &["decline".to_owned()])
            .expect("no infantry makes the listener ineligible");
        assert_eq!(
            state, before,
            "missing target leaves all pieces and cards unchanged"
        );

        let (mut state, system, _) = ssruu_l1z1x_activation_game(LeaderStatus::Exhausted, true);
        let beneficiary = PlayerId::new("c");
        state.player_mut(&beneficiary).unwrap().faction = ti4_model::id::FactionId::new("naaz");
        crate::fixtures::put(&mut state, &system, "naaz_voltron", &beneficiary, 1);
        let abilities = ssruu_l1z1x_activation_abilities(&state);
        let before = state.clone();
        emit_ssruu_l1z1x_activation(&mut state, abilities, &["decline".to_owned()])
            .expect("Maximum blocks game-effect mech placement before an offer");
        assert_eq!(
            state, before,
            "a standing Maximum protects its mech placement ban"
        );
    }

    #[test]
    fn ssruu_activation_copy_decline_and_illegal_resolver_choice_do_not_spend_cards() {
        for answer in ["decline", "not-an-offered-copy"] {
            let (mut state, _, _) = ssruu_l1z1x_activation_game(LeaderStatus::Readied, true);
            let abilities = ssruu_l1z1x_activation_abilities(&state);
            let before = state.clone();
            let result = emit_ssruu_l1z1x_activation(&mut state, abilities, &[answer.to_owned()]);
            if answer == "decline" {
                result.expect("decline is a valid resolver answer");
            } else {
                assert!(result.is_err(), "an invented id must be rejected");
            }
            assert_eq!(state, before, "no mutation occurs before valid acceptance");
        }

        let (mut state, system, _) = ssruu_l1z1x_activation_game(LeaderStatus::Readied, true);
        let (_, _, other_planet) = two_planet_system();
        crate::fixtures::put_on_planet(
            &mut state,
            &system,
            &other_planet,
            "infantry",
            &PlayerId::new("c"),
            1,
        );
        let abilities = ssruu_l1z1x_activation_abilities(&state);
        let answers = vec![abilities[0].id.clone(), "not-an-offered-planet".to_owned()];
        let before = state.clone();
        assert!(
            emit_ssruu_l1z1x_activation(&mut state, abilities, &answers).is_err(),
            "an invalid beneficiary target propagates through the resolver"
        );
        assert_eq!(
            state, before,
            "invalid target selection spends no units or leader"
        );
    }

    #[test]
    fn the_helmsman_gathers_the_named_hulls_and_leaves_the_rest() {
        // The card names a flagship and dreadnoughts. It places rather than moves — it gives no
        // move value and is not a tactical action — so range and anomalies do not apply.
        let hub = crate::fixtures::plain_hub();
        let mut state = game(&["a"]);
        let hero = holding(&mut state, "l1z1xhero", LeaderStatus::Unlocked);
        let far = ti4_model::id::SystemId::new(hub.across(&hub.outer[0]));
        crate::fixtures::put(&mut state, &far, "dreadnought", &player(), 2);
        crate::fixtures::put(&mut state, &far, "cruiser", &player(), 1);

        let mut table = crate::choice::Table::new();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let done = {
            let mut context = crate::timing::TimingContext {
                state: &mut state,
                content: ContentStore::embedded(),
                sources: POK,
                table: &mut table,
                dice: &mut dice,
                rng: &mut rng,
                event_sequence: &mut sequence,
                galaxy: Some(&hub.galaxy),
            };
            use_leader(&mut context, &player(), &hero)
        };

        assert!(done);
        let left: Vec<String> = state
            .system_state(&far)
            .units
            .iter()
            .map(|unit| unit.type_id.to_string())
            .collect();
        assert_eq!(
            left,
            vec!["cruiser".to_owned()],
            "the cruiser stayed behind"
        );
    }

    #[test]
    fn i48s_upgrades_the_active_players_infantry_not_your_own() {
        // Agents are traded favours: the card benefits whoever activated, which is usually
        // somebody else.
        let mut state = game(&["a", "b"]);
        let agent = holding(&mut state, "l1z1xagent", LeaderStatus::Readied);
        let active = PlayerId::new("b");
        state.player_mut(&active).unwrap().faction = ti4_model::id::FactionId::new("sol");
        let (system, planet) = crate::fixtures::a_placed_planet();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &active, 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &player(), 1);
        state.active_system = Some(system.clone());
        state.active = Some(active.clone());

        assert!(use_it(&mut state, &agent));

        let units = state
            .system_state(&system)
            .planet_units
            .get(&planet)
            .cloned()
            .unwrap_or_default();
        assert!(
            units
                .iter()
                .any(|unit| unit.owner == active && unit.type_id.as_str().contains("mech")),
            "b got the mech: {units:?}"
        );
        assert!(
            units
                .iter()
                .any(|unit| unit.owner == player() && unit.type_id.as_str().contains("infantry")),
            "and a's infantry was untouched"
        );
    }

    #[test]
    fn rin_swaps_a_technology_for_another_of_the_same_colour() {
        let content = ContentStore::embedded();
        let mut state = game(&["a"]);
        let hero = holding(&mut state, "jolnarhero", LeaderStatus::Unlocked);
        let ordinary = content
            .from_sources(ContentType::Technologies, POK)
            .find(|r| {
                !r.text("alias").is_some_and(|alias| {
                    crate::technology::is_unit_upgrade(
                        content,
                        &ti4_model::id::TechnologyId::new(alias),
                    )
                }) && r.text("faction").is_none()
                    && !r.strings("types").is_empty()
            })
            .and_then(|r| r.text("alias").map(ti4_model::id::TechnologyId::new))
            .expect("an ordinary technology");
        state
            .player_mut(&player())
            .unwrap()
            .technologies
            .insert(ordinary.clone());
        let before = state.player(&player()).unwrap().technologies.len();

        assert!(use_it(&mut state, &hero));

        let after = state.player(&player()).unwrap();
        assert_eq!(after.technologies.len(), before, "a swap, not a gain");
        assert!(
            !after.technologies.contains(&ordinary),
            "the old one went back to the deck"
        );
    }

    #[test]
    fn rin_never_offers_another_factions_technology() {
        // 90.11: a faction technology belongs to its faction alone, and Rin replaces "from the
        // deck" -- the player's own deck of generic technologies plus their own faction's. The
        // replacement list filtered only by colour, so every faction's technology was offered.
        let content = ContentStore::embedded();
        let mut state = game(&["a"]);
        state.player_mut(&player()).unwrap().faction = ti4_model::id::FactionId::new("jolnar");
        let hero = holding(&mut state, "jolnarhero", LeaderStatus::Unlocked);
        let (colour, foreign) = content
            .from_sources(ContentType::Technologies, POK)
            .find_map(|r| {
                let alias = r.text("alias")?;
                let owner = crate::technology::faction_of(
                    content,
                    &ti4_model::id::TechnologyId::new(alias),
                )?;
                if owner == "jolnar" || r.strings("types").contains(&"UNITUPGRADE") {
                    return None;
                }
                Some((
                    r.strings("types").first().copied()?.to_owned(),
                    alias.to_owned(),
                ))
            })
            .expect("another faction's technology with a colour");
        let ordinary = content
            .from_sources(ContentType::Technologies, POK)
            .find(|r| {
                r.strings("types")
                    .first()
                    .is_some_and(|kind| *kind == colour)
                    && r.text("faction").is_none()
                    && !r.strings("types").contains(&"UNITUPGRADE")
            })
            .and_then(|r| r.text("alias").map(ti4_model::id::TechnologyId::new))
            .expect("an ordinary technology of that colour");
        state
            .player_mut(&player())
            .unwrap()
            .technologies
            .insert(ordinary.clone());

        let (decider, seen) = crate::choice::Capturing::new(Box::new(
            crate::choice::Scripted::new([format!("keep|{ordinary}")]),
        ));
        let mut table = crate::choice::Table::with_default(Box::new(decider));
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        {
            let mut context = crate::timing::TimingContext {
                state: &mut state,
                content,
                sources: POK,
                table: &mut table,
                dice: &mut dice,
                rng: &mut rng,
                event_sequence: &mut sequence,
                galaxy: None,
            };
            use_leader(&mut context, &player(), &hero);
        }

        let offered: Vec<String> = seen
            .borrow()
            .iter()
            .filter(|choice| choice.prompt == "Leader: replace which technology with what")
            .flat_map(|choice| {
                choice
                    .ids()
                    .into_iter()
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .collect();
        assert!(
            offered.len() > 1,
            "Rin offered at least one replacement to check: {offered:?}"
        );
        assert!(
            !offered.contains(&foreign),
            "{foreign} belongs to another faction and was offered"
        );
        for id in &offered {
            if let Some(owner) =
                crate::technology::faction_of(content, &ti4_model::id::TechnologyId::new(id))
            {
                assert_eq!(owner, "jolnar", "{id} is {owner}'s faction technology");
            }
        }
    }

    #[test]
    fn harrugh_gefhara_is_offered_at_production_and_makes_that_use_free() {
        // Its window is "when 1 or more of your units use PRODUCTION", not an ACTION, so the
        // component-action offer rightly never lists it -- and nothing else offered it either.
        let mut state = game(&["a"]);
        let hero = holding(&mut state, "hacanhero", LeaderStatus::Unlocked);
        state.production_seq = 7;
        let mut table =
            crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new([
                "use".to_owned()
            ])));
        let used = offer_production_hero(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            &player(),
        )
        .expect("the offer is answered");
        assert!(used);
        let seat = state.player(&player()).unwrap();
        assert_eq!(seat.free_production_use, Some(7), "this production is free");
        assert_eq!(seat.leaders.get(&hero), Some(&LeaderStatus::Purged));

        let mut state = game(&["a"]);
        let hero = holding(&mut state, "hacanhero", LeaderStatus::Unlocked);
        let mut table =
            crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new([
                crate::choice::DECLINE_ID.to_owned(),
            ])));
        let used = offer_production_hero(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            &player(),
        )
        .expect("the offer is answered");
        assert!(!used, "declining keeps the hero");
        let seat = state.player(&player()).unwrap();
        assert_eq!(seat.free_production_use, None);
        assert_eq!(seat.leaders.get(&hero), Some(&LeaderStatus::Unlocked));
    }

    #[test]
    fn rin_leaves_held_unit_upgrades_untouched() {
        // "For each non-unit upgrade technology you own" — the generic upgrades (Carrier II,
        // War Sun, ...) have no `baseUpgrade` to name them, and their only type is the
        // colourless UNITUPGRADE, which is not a colour. Treating them as swappable let Rin
        // trade one generic upgrade for another; the ability must ignore the whole class.
        let content = ContentStore::embedded();
        let mut state = game(&["a"]);
        let hero = holding(&mut state, "jolnarhero", LeaderStatus::Unlocked);
        let cv2 = ti4_model::id::TechnologyId::new("cv2");
        assert!(
            crate::technology::is_unit_upgrade(content, &cv2),
            "Carrier II is a unit upgrade in this corpus"
        );
        state
            .player_mut(&player())
            .unwrap()
            .technologies
            .insert(cv2.clone());

        assert!(
            !use_it(&mut state, &hero),
            "nothing is swappable, so the hero is not spent"
        );

        let after = state.player(&player()).unwrap();
        assert!(
            after.technologies.contains(&cv2),
            "a held unit upgrade is not a swappable technology"
        );
        assert_eq!(
            after.leaders.get(&hero),
            Some(&LeaderStatus::Unlocked),
            "an unused hero is not purged"
        );
    }

    #[test]
    fn rear_admiral_farran_pays_only_once_unlocked() {
        let mut state = game(&["a"]);
        assert!(!pays_on_sustain(
            &state,
            ContentStore::embedded(),
            &player()
        ));

        holding(&mut state, "letnevcommander", LeaderStatus::Locked);
        assert!(
            !pays_on_sustain(&state, ContentStore::embedded(), &player()),
            "locked is not unlocked"
        );

        holding(&mut state, "letnevcommander", LeaderStatus::Unlocked);
        assert!(pays_on_sustain(&state, ContentStore::embedded(), &player()));
    }

    #[test]
    fn a_locked_leader_cannot_be_used() {
        let mut state = game(&["a"]);
        let leader = holding(&mut state, "solhero", LeaderStatus::Locked);
        assert!(!use_it(&mut state, &leader));
    }

    #[test]
    fn an_agent_exhausts_and_a_hero_is_purged() {
        // 51.9, 51.10. An agent that never exhausts is usable every turn for ever; a hero that
        // is not purged is a second hero.
        let mut state = game(&["a"]);
        let agent = holding(&mut state, "xxchaagent", LeaderStatus::Readied);
        state
            .exhausted_planets
            .insert(ti4_model::id::PlanetId::new("somewhere"));
        assert!(use_it(&mut state, &agent));
        assert_eq!(
            state.player(&player()).unwrap().leaders.get(&agent),
            Some(&LeaderStatus::Exhausted)
        );
        assert!(!use_it(&mut state, &agent), "and not again this round");

        let hero = holding(&mut state, "solhero", LeaderStatus::Unlocked);
        assert!(use_it(&mut state, &hero));
        assert_eq!(
            state.player(&player()).unwrap().leaders.get(&hero),
            Some(&LeaderStatus::Purged)
        );
    }

    #[test]
    fn jace_takes_every_token_off_the_board() {
        let mut state = game(&["a", "b"]);
        let hero = holding(&mut state, "solhero", LeaderStatus::Unlocked);
        let systems = crate::fixtures::plain_systems(3);
        for id in &systems {
            let system = ti4_model::id::SystemId::new(id.clone());
            state.system_mut(&system).command_tokens.insert(player());
            state
                .system_mut(&system)
                .command_tokens
                .insert(PlayerId::new("b"));
        }

        assert!(use_it(&mut state, &hero));

        for id in &systems {
            let board = state.system_state(&ti4_model::id::SystemId::new(id.clone()));
            assert!(!board.command_tokens.contains(&player()), "yours came back");
            assert!(
                board.command_tokens.contains(&PlayerId::new("b")),
                "and nobody else's moved"
            );
        }
    }

    #[test]
    fn an_extra_die_expires_with_the_round_it_was_used_in() {
        let mut state = game(&["a"]);
        let agent = holding(&mut state, "letnevagent", LeaderStatus::Readied);
        state.combat_round_seq = 5;

        assert!(use_it(&mut state, &agent));

        let seat = state.player(&player()).unwrap();
        assert_eq!(seat.extra_die_round, Some(5));
        assert!(
            seat.extra_die_unit.is_some(),
            "and names which unit rolls it"
        );
    }

    #[test]
    fn a_commander_adds_votes_only_once_unlocked() {
        let mut state = game(&["a"]);
        assert_eq!(vote_bonus(&state, &player()), 0);

        holding(&mut state, "hacancommander", LeaderStatus::Locked);
        assert_eq!(vote_bonus(&state, &player()), 0, "locked is not unlocked");

        holding(&mut state, "hacancommander", LeaderStatus::Unlocked);
        assert_eq!(
            vote_bonus(&state, &player()),
            0,
            "Gila spends trade goods in the vote window, not as a flat bonus"
        );

        holding(&mut state, "xxchacommander", LeaderStatus::Unlocked);
        assert!(elder_qanoj(&state, &player()));
        assert_eq!(
            vote_bonus(&state, &player()),
            0,
            "Elder Qanoj counts per planet, in the vote, not as a flat bonus"
        );
    }

    #[test]
    fn the_l1z1x_commander_ignores_a_planetary_shield() {
        let mut state = game(&["a"]);
        assert!(!ignores_planetary_shield(&state, &player()));

        holding(&mut state, "l1z1xcommander", LeaderStatus::Unlocked);
        assert!(ignores_planetary_shield(&state, &player()));
    }

    #[test]
    fn a_standing_modifier_counts_as_implemented() {
        // Naming them keeps coverage honest: a leader whose effect lives where it modifies is
        // not a gap, and reporting it as one trains the reader to ignore the list.
        let missing = unimplemented(ContentStore::embedded(), &["xxcha"]);
        assert!(
            !missing.contains(&LeaderId::new("xxchacommander")),
            "its effect lives in elder_qanoj"
        );
        assert!(
            !missing.contains(&LeaderId::new("xxchahero")),
            "and the hero's effect lives in combines_planet_values"
        );
        // The half that keeps this test honest now that every leader of the six trained factions
        // is implemented: a faction outside that scope still reports its leaders, so the list is
        // not simply always empty. Any out-of-scope faction does; Naalu is one.
        assert!(
            !unimplemented(ContentStore::embedded(), &["naalu"]).is_empty(),
            "a leader this module does not handle is still reported"
        );
    }

    #[test]
    fn a_commander_with_no_registered_check_stays_locked() {
        // An unknown condition must not read as satisfied: a commander that unlocks itself is
        // worse than one that never unlocks, because nothing says it happened.
        let state = game(&["a"]);
        assert_eq!(
            commander_unlocked(
                &state,
                ContentStore::embedded(),
                POK,
                None,
                &player(),
                &LeaderId::new("nobodyscommander")
            ),
            None
        );
    }

    #[test]
    fn each_commander_unlocks_on_its_own_condition() {
        let content = ContentStore::embedded();
        let hacan = LeaderId::new("hacancommander");
        let jolnar = LeaderId::new("jolnarcommander");

        let mut state = game(&["a"]);
        assert_eq!(
            commander_unlocked(&state, content, POK, None, &player(), &hacan),
            Some(false)
        );

        state.player_mut(&player()).unwrap().trade_goods = 10;
        assert_eq!(
            commander_unlocked(&state, content, POK, None, &player(), &hacan),
            Some(true),
            "ten trade goods"
        );
        assert_eq!(
            commander_unlocked(&state, content, POK, None, &player(), &jolnar),
            Some(false),
            "and it is not somebody else's condition"
        );
    }

    #[test]
    fn letnev_counts_five_ships_in_one_system_not_five_anywhere() {
        let content = ContentStore::embedded();
        let letnev = LeaderId::new("letnevcommander");
        let mut state = game(&["a"]);
        let systems = crate::fixtures::plain_systems(2);
        let (first, second) = (
            ti4_model::id::SystemId::new(systems[0].clone()),
            ti4_model::id::SystemId::new(systems[1].clone()),
        );
        crate::fixtures::put(&mut state, &first, "cruiser", &player(), 3);
        crate::fixtures::put(&mut state, &second, "cruiser", &player(), 3);

        assert_eq!(
            commander_unlocked(&state, content, POK, None, &player(), &letnev),
            Some(false),
            "six ships, but three and three"
        );

        crate::fixtures::put(&mut state, &first, "cruiser", &player(), 2);
        assert_eq!(
            commander_unlocked(&state, content, POK, None, &player(), &letnev),
            Some(true),
            "five in one system"
        );
    }

    #[test]
    fn letnev_does_not_count_fighters() {
        let content = ContentStore::embedded();
        let letnev = LeaderId::new("letnevcommander");
        let mut state = game(&["a"]);
        let (system, _) = crate::fixtures::a_placed_planet();
        crate::fixtures::put(&mut state, &system, "fighter", &player(), 8);

        assert_eq!(
            commander_unlocked(&state, content, POK, None, &player(), &letnev),
            Some(false),
            "the card says non-fighter ships"
        );
    }

    #[test]
    fn a_commander_unlocks_without_waiting_for_three_objectives() {
        // Heroes need three scored objectives; commanders do not, and gating both on the hero
        // condition would leave every commander locked for most of a game.
        let content = ContentStore::embedded();
        let mut state = game(&["a"]);
        state
            .player_mut(&player())
            .unwrap()
            .leaders
            .insert(LeaderId::new("hacancommander"), LeaderStatus::Locked);
        state.player_mut(&player()).unwrap().trade_goods = 10;

        let unlocked = check_unlocks(&mut state, content, POK, None, &player());

        assert!(
            unlocked.contains(&LeaderId::new("hacancommander")),
            "nothing has been scored, and it still unlocked"
        );
        assert_eq!(
            state
                .player(&player())
                .unwrap()
                .leaders
                .get(&LeaderId::new("hacancommander")),
            Some(&LeaderStatus::Unlocked)
        );
    }

    use ti4_model::content_types::POK;

    use super::*;
    use crate::fixtures::game;

    fn player() -> PlayerId {
        PlayerId::new("a")
    }

    /// A faction the corpus gives all three kinds of leader.
    fn a_faction_with_leaders() -> String {
        ti4_content::factions::catalogue(ContentStore::embedded(), POK)
            .iter()
            .find(|(alias, _)| {
                let leaders = for_faction(ContentStore::embedded(), POK, alias);
                leaders
                    .iter()
                    .any(|l| kind_of(ContentStore::embedded(), l).as_deref() == Some(HERO))
                    && leaders
                        .iter()
                        .any(|l| kind_of(ContentStore::embedded(), l).as_deref() == Some(AGENT))
            })
            .map(|(alias, _)| (*alias).to_owned())
            .expect("some faction has an agent and a hero")
    }

    fn seated() -> (GameState, String) {
        let faction = a_faction_with_leaders();
        let mut state = game(&["a"]);
        state.player_mut(&player()).unwrap().faction =
            ti4_model::id::FactionId::new(faction.clone());
        deploy(&mut state, ContentStore::embedded(), POK, &player());
        (state, faction)
    }

    #[test]
    fn a_faction_starts_with_agents_readied_and_the_rest_locked() {
        // 51.2a.
        let (state, _) = seated();
        let seat = state.player(&player()).unwrap();
        assert!(!seat.leaders.is_empty());

        for (leader, status) in &seat.leaders {
            let expected = if kind_of(ContentStore::embedded(), leader).as_deref() == Some(AGENT) {
                LeaderStatus::Readied
            } else {
                LeaderStatus::Locked
            };
            assert_eq!(*status, expected, "{leader}");
        }
    }

    #[test]
    fn an_agent_exhausts_when_used_and_readies_in_the_status_phase() {
        // 81.6. Readying is reported, because a table that turned a card face down and never
        // back reads after a round or two as a player who has run out of agents.
        let (mut state, _) = seated();
        let agent = of_kind(&state, ContentStore::embedded(), &player(), AGENT)
            .first()
            .cloned()
            .expect("this faction has an agent");

        assert!(exhaust(&mut state, &player(), &agent));
        assert_eq!(
            status(&state, &player(), &agent),
            Some(LeaderStatus::Exhausted)
        );
        assert!(
            !exhaust(&mut state, &player(), &agent),
            "an exhausted agent cannot be used again"
        );

        let readied = ready_all(&mut state, &player());
        assert_eq!(readied, vec![agent.clone()]);
        assert_eq!(
            status(&state, &player(), &agent),
            Some(LeaderStatus::Readied)
        );
    }

    #[test]
    fn a_locked_leader_cannot_be_exhausted() {
        let (mut state, _) = seated();
        let hero = of_kind(&state, ContentStore::embedded(), &player(), HERO)
            .first()
            .cloned()
            .expect("this faction has a hero");

        assert!(!exhaust(&mut state, &player(), &hero));
    }

    #[test]
    fn a_hero_unlocks_on_the_third_objective() {
        // 51.7, and not before: two is not three.
        let (mut state, _) = seated();
        let hero = of_kind(&state, ContentStore::embedded(), &player(), HERO)
            .first()
            .cloned()
            .expect("this faction has a hero");

        for alias in ["o1", "o2"] {
            state.record_score(&player(), ti4_model::id::ObjectiveId::new(alias));
        }
        assert!(
            check_unlocks(&mut state, ContentStore::embedded(), POK, None, &player()).is_empty()
        );
        assert_eq!(status(&state, &player(), &hero), Some(LeaderStatus::Locked));

        state.record_score(&player(), ti4_model::id::ObjectiveId::new("o3"));
        let unlocked = check_unlocks(&mut state, ContentStore::embedded(), POK, None, &player());

        assert!(unlocked.contains(&hero));
        assert_eq!(
            status(&state, &player(), &hero),
            Some(LeaderStatus::Unlocked)
        );
    }

    #[test]
    fn a_commander_stays_locked_because_its_condition_is_unimplemented() {
        // The registry design used elsewhere: an unimplemented condition leaves the leader
        // unavailable rather than silently unlocked.
        let (mut state, _) = seated();
        for alias in ["o1", "o2", "o3", "o4"] {
            state.record_score(&player(), ti4_model::id::ObjectiveId::new(alias));
        }
        check_unlocks(&mut state, ContentStore::embedded(), POK, None, &player());

        for commander in of_kind(&state, ContentStore::embedded(), &player(), COMMANDER) {
            assert_eq!(
                status(&state, &player(), &commander),
                Some(LeaderStatus::Locked),
                "{commander}"
            );
        }
    }

    #[test]
    fn purging_is_permanent() {
        // 51.9: a hero is purged when its ability resolves, and does not come back.
        let (mut state, _) = seated();
        let hero = of_kind(&state, ContentStore::embedded(), &player(), HERO)
            .first()
            .cloned()
            .unwrap();
        state.record_score(&player(), ti4_model::id::ObjectiveId::new("o1"));
        state.record_score(&player(), ti4_model::id::ObjectiveId::new("o2"));
        state.record_score(&player(), ti4_model::id::ObjectiveId::new("o3"));
        check_unlocks(&mut state, ContentStore::embedded(), POK, None, &player());

        assert!(purge(&mut state, &player(), &hero));
        assert_eq!(status(&state, &player(), &hero), Some(LeaderStatus::Purged));

        ready_all(&mut state, &player());
        check_unlocks(&mut state, ContentStore::embedded(), POK, None, &player());
        assert_eq!(
            status(&state, &player(), &hero),
            Some(LeaderStatus::Purged),
            "neither readying nor unlocking brings it back"
        );
    }

    #[test]
    fn only_readied_agents_and_unlocked_heroes_are_usable() {
        let (mut state, _) = seated();
        let agent = of_kind(&state, ContentStore::embedded(), &player(), AGENT)
            .first()
            .cloned()
            .unwrap();

        assert!(usable(&state, ContentStore::embedded(), &player()).contains(&agent));
        exhaust(&mut state, &player(), &agent);
        assert!(!usable(&state, ContentStore::embedded(), &player()).contains(&agent));
    }

    /// Drive one leader use through a table whose answers are scripted in order.
    fn use_scripted(
        state: &mut GameState,
        leader: &str,
        galaxy: Option<&ti4_content::galaxy::Galaxy>,
        answers: Vec<String>,
    ) -> bool {
        let (decider, _seen) =
            crate::choice::Capturing::new(Box::new(crate::choice::Scripted::new(answers)));
        let mut table = crate::choice::Table::with_default(Box::new(decider));
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let id = LeaderId::new(leader);
        {
            let mut context = crate::timing::TimingContext {
                state,
                content: ContentStore::embedded(),
                sources: POK,
                table: &mut table,
                dice: &mut dice,
                rng: &mut rng,
                event_sequence: &mut sequence,
                galaxy,
            };
            use_leader(&mut context, &player(), &id)
        }
    }

    #[test]
    fn letnev_hero_stays_until_end_of_its_round() {
        // "Place this card near the game board ... At the end of that game round, purge this
        // card." The effect and the card both outlive the use itself.
        let mut state = game(&["a"]);
        let hero = holding(&mut state, "letnevhero", LeaderStatus::Unlocked);

        assert!(use_it(&mut state, &hero));

        let seat = state.player(&player()).unwrap();
        assert_eq!(
            seat.fleet_supply_unlimited_until,
            Some(state.round),
            "active for this round"
        );
        assert_eq!(
            seat.leaders.get(&hero),
            Some(&LeaderStatus::Unlocked),
            "not purged on use — she stays in play until the end of her game round"
        );

        crate::phase::begin_next_round(&mut state, Vec::new());

        let seat = state.player(&player()).unwrap();
        assert_eq!(
            seat.fleet_supply_unlimited_until, None,
            "the effect expired with its round"
        );
        assert_eq!(
            seat.leaders.get(&hero),
            Some(&LeaderStatus::Purged),
            "purged at the end of that game round"
        );
    }

    #[test]
    fn the_unlimited_fleet_bill_stays_within_printed_integers() {
        // The observation surface (decision-context constraints and preview headroom) encodes
        // these values as printed-scale integers; an unbounded sentinel would panic the policy
        // features that read them.
        let mut state = game(&["a"]);
        let hero = holding(&mut state, "letnevhero", LeaderStatus::Unlocked);
        assert!(use_it(&mut state, &hero));

        let system = ti4_model::id::SystemId::new(crate::fixtures::plain_systems(1)[0].as_str());
        let standing = crate::fleet::standing(
            &state,
            ContentStore::embedded(),
            ti4_model::content_types::POK,
            &player(),
            &system,
            None,
        );
        assert!(i32::try_from(standing.fleet_limit).is_ok());
        assert!(i32::try_from(standing.fleet_headroom()).is_ok());
    }

    #[test]
    fn action_leaders_are_not_offered_when_they_cannot_resolve() {
        let mut state = game(&["a"]);
        holding(&mut state, "xxchaagent", LeaderStatus::Readied);
        // No exhausted planets: there is nothing to ready.
        assert!(component_actions(&state, ContentStore::embedded(), &player()).is_empty());

        state
            .exhausted_planets
            .insert(ti4_model::id::PlanetId::new("somewhere"));
        let ids: Vec<String> = component_actions(&state, ContentStore::embedded(), &player())
            .into_iter()
            .map(|option| option.id)
            .collect();
        assert!(ids.contains(&"component|leader|xxchaagent".to_owned()));

        // A hero already active this round is not offered twice.
        let hero = holding(&mut state, "letnevhero", LeaderStatus::Unlocked);
        state
            .player_mut(&player())
            .unwrap()
            .fleet_supply_unlimited_until = Some(state.round);
        let ids: Vec<String> = component_actions(&state, ContentStore::embedded(), &player())
            .into_iter()
            .map(|option| option.id)
            .collect();
        assert!(!ids.contains(&format!("component|leader|{hero}")));
    }

    #[test]
    fn unimplemented_action_window_leaders_are_never_offered() {
        // Muaat's agent prints an action window, but its effect has no delivery path in this
        // engine — offering it would be an option that can never resolve.
        let mut state = game(&["a"]);
        holding(&mut state, "muaatagent", LeaderStatus::Readied);

        assert!(component_actions(&state, ContentStore::embedded(), &player()).is_empty());
    }

    #[test]
    fn xxcha_agent_chooses_which_planet_to_ready() {
        let mut state = game(&["a"]);
        holding(&mut state, "xxchaagent", LeaderStatus::Readied);
        let first = ti4_model::id::PlanetId::new("first");
        let second = ti4_model::id::PlanetId::new("second");
        state.exhausted_planets.insert(first.clone());
        state.exhausted_planets.insert(second.clone());

        assert!(use_scripted(
            &mut state,
            "xxchaagent",
            None,
            vec![second.to_string()],
        ));

        assert!(
            !state.exhausted_planets.contains(&second),
            "the chosen planet was readied"
        );
        assert!(
            state.exhausted_planets.contains(&first),
            "the other one stayed exhausted"
        );
    }

    #[test]
    fn xxcha_agent_may_remove_infantry_from_an_adjacent_planet() {
        // The optional second half: the readied planet's system touches a system holding a
        // planet of yours, and your infantry sits on that planet.
        let hub = crate::fixtures::plain_hub();
        let mut state = game(&["a"]);
        holding(&mut state, "xxchaagent", LeaderStatus::Readied);
        let centre = ti4_model::id::SystemId::new(hub.centre.clone());
        let outer0 = ti4_model::id::SystemId::new(hub.outer[0].clone());
        let controlled = ti4_model::id::PlanetId::new("controlled-planet");
        let target = ti4_model::id::PlanetId::new("target-planet");
        state
            .system_mut(&outer0)
            .planet_control
            .insert(controlled.clone(), player().clone());
        crate::fixtures::put_on_planet(&mut state, &centre, &target, "infantry", &player(), 1);
        state.exhausted_planets.insert(target.clone());

        // One exhausted planet means no planet choice is asked — only the removal question.
        assert!(use_scripted(
            &mut state,
            "xxchaagent",
            Some(&hub.galaxy),
            vec!["remove".to_owned()],
        ));

        let units = state
            .system_state(&centre)
            .planet_units
            .get(&target)
            .cloned()
            .unwrap_or_default();
        assert!(
            !units.iter().any(|unit| unit.owner == player()),
            "the infantry went back to reinforcements: {units:?}"
        );
    }

    #[test]
    fn hacan_agent_can_replenish_another_player() {
        let content = ContentStore::embedded();
        let mut state = game(&["a", "b"]);
        holding(&mut state, "hacanagent", LeaderStatus::Readied);
        let other = PlayerId::new("b");
        state.player_mut(&other).unwrap().faction = ti4_model::id::FactionId::new("hacan");

        assert!(use_scripted(
            &mut state,
            "hacanagent",
            None,
            vec!["b".to_owned()],
        ));

        let limit = ti4_content::factions::get(content, "hacan")
            .expect("Hacan exists")
            .commodities();
        assert_eq!(
            state.player(&other).unwrap().commodities,
            limit,
            "replenished to their own cap"
        );
    }

    /// The offer path, not the use path: `component_actions` is what a seat sees on its turn, and
    /// it filters on the printed window before it looks at anything else.
    #[test]
    fn the_hacan_agent_is_offered_on_its_own_turn() {
        // "During the action phase:" is what the card prints, colon and all. `is_action_window`
        // compared that with `eq_ignore_ascii_case` to "during the action phase", so this — the
        // only action-phase agent among the in-scope factions, every other one of which prints
        // "ACTION:" — never reached an offer list. LEADER-FIX-001 (`16f389a`) put `hacanagent` in
        // the delivered set and proved `use_leader` resolves it; nothing proved the offer, and the
        // offer is the half the window predicate governs.
        let mut state = game(&["a", "b"]);
        state.player_mut(&player()).unwrap().faction = ti4_model::id::FactionId::new("hacan");
        holding(&mut state, "hacanagent", LeaderStatus::Readied);

        let offered: Vec<String> = component_actions(&state, ContentStore::embedded(), &player())
            .into_iter()
            .map(|option| option.id)
            .collect();
        assert!(
            offered.iter().any(|id| id == "component|leader|hacanagent"),
            "Carth of Golden Sands was not on the offer list: {offered:?}"
        );
    }

    /// A game where `a` holds a readied Ssruu and `b` (Hacan) holds Carth, exhausted.
    fn ssruu_game() -> GameState {
        let mut state = game(&["a", "b"]);
        holding(&mut state, "yssarilagent", LeaderStatus::Readied);
        let other = PlayerId::new("b");
        let seat = state.player_mut(&other).unwrap();
        seat.faction = ti4_model::id::FactionId::new("hacan");
        seat.commodities = 0;
        seat.leaders
            .insert(LeaderId::new("hacanagent"), LeaderStatus::Exhausted);
        state
    }

    #[test]
    fn ssruu_uses_an_exhausted_agent_without_exhausting_or_readying_it() {
        let content = ContentStore::embedded();
        let mut state = ssruu_game();
        assert!(use_scripted(
            &mut state,
            "yssarilagent|hacanagent",
            None,
            vec!["b".to_owned()],
        ));
        let limit = ti4_content::factions::get(content, "hacan")
            .expect("Hacan exists")
            .commodities();
        assert_eq!(
            state.player(&PlayerId::new("b")).unwrap().commodities,
            limit
        );
        assert_eq!(
            status(&state, &PlayerId::new("b"), &LeaderId::new("hacanagent")),
            Some(LeaderStatus::Exhausted),
            "the source agent is untouched"
        );
        assert_eq!(
            status(&state, &player(), &LeaderId::new("yssarilagent")),
            Some(LeaderStatus::Exhausted),
            "Ssruu carries the text, so Ssruu exhausts"
        );
        // Exhausted Ssruu has nothing left to borrow.
        let before = state.clone();
        assert!(!use_scripted(
            &mut state,
            "yssarilagent|hacanagent",
            None,
            vec!["b".to_owned()],
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn ssruu_copies_arborec_text_as_user_and_routes_replacement_to_ship_owner() {
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let c = PlayerId::new("c");
        let mut state = game(&["a", "b", "c"]);
        state.player_mut(&a).unwrap().faction = ti4_model::id::FactionId::new("yssaril");
        state.player_mut(&b).unwrap().faction = ti4_model::id::FactionId::new("arborec");
        state.player_mut(&c).unwrap().faction = ti4_model::id::FactionId::new("hacan");
        holding(&mut state, "yssarilagent", LeaderStatus::Readied);
        state
            .player_mut(&b)
            .unwrap()
            .leaders
            .insert(LeaderId::new("arborecagent"), LeaderStatus::Exhausted);

        for board in state.board.values_mut() {
            board.units.clear();
            board.planet_units.clear();
        }
        let system = crate::fixtures::a_placed_planet().0;
        crate::fixtures::put(&mut state, &system, "cruiser", &c, 1);

        let borrowed = LeaderId::new("yssarilagent|arborecagent");
        assert!(
            component_actions(&state, ContentStore::embedded(), &a)
                .iter()
                .any(|option| option.id == format!("component|leader|{borrowed}"))
        );

        let before = state.clone();
        assert!(
            !use_scripted(
                &mut state,
                borrowed.as_str(),
                None,
                vec!["not-a-replacement".to_owned()],
            ),
            "an invalid ship-owner answer refuses the copy"
        );
        assert_eq!(state, before, "failed borrowed resolution is atomic");

        let (decider, seen) =
            crate::choice::Capturing::new(Box::new(crate::choice::Scripted::new([
                "replace|carrier",
            ])));
        let mut table = crate::choice::Table::with_default(Box::new(decider));
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let used = {
            let mut context = crate::timing::TimingContext {
                state: &mut state,
                content: ContentStore::embedded(),
                sources: POK,
                table: &mut table,
                dice: &mut dice,
                rng: &mut rng,
                event_sequence: &mut sequence,
                galaxy: None,
            };
            use_leader(&mut context, &a, &borrowed)
        };
        assert!(used);
        let asks = seen.borrow();
        assert_eq!(asks.len(), 1);
        assert_eq!(asks[0].player, c, "the ship owner chooses the replacement");
        drop(asks);
        let ships = &state.board[&system].units;
        assert!(
            ships
                .iter()
                .any(|ship| ship.owner == c && ship.type_id.as_str() == "carrier")
        );
        assert_eq!(
            status(&state, &a, &LeaderId::new("yssarilagent")),
            Some(LeaderStatus::Exhausted)
        );
        assert_eq!(
            status(&state, &b, &LeaderId::new("arborecagent")),
            Some(LeaderStatus::Exhausted),
            "copying leaves the source agent untouched"
        );
    }

    #[test]
    fn ssruu_copies_xxcha_to_ready_the_selected_exhausted_controlled_planet() {
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let mut state = game(&["a", "b"]);
        state.player_mut(&a).unwrap().faction = ti4_model::id::FactionId::new("yssaril");
        state.player_mut(&b).unwrap().faction = ti4_model::id::FactionId::new("xxcha");
        holding(&mut state, "yssarilagent", LeaderStatus::Readied);
        state
            .player_mut(&b)
            .unwrap()
            .leaders
            .insert(LeaderId::new("xxchaagent"), LeaderStatus::Exhausted);

        let system = crate::fixtures::a_placed_planet().0;
        let mine = ti4_model::id::PlanetId::new("borrower-planet");
        let theirs = ti4_model::id::PlanetId::new("source-owner-planet");
        state
            .system_mut(&system)
            .planet_control
            .insert(mine.clone(), a.clone());
        state
            .system_mut(&system)
            .planet_control
            .insert(theirs.clone(), b.clone());
        state.exhausted_planets.insert(mine.clone());
        state.exhausted_planets.insert(theirs.clone());

        let borrowed = "yssarilagent|xxchaagent";
        let before = state.clone();
        assert!(
            !use_scripted(
                &mut state,
                borrowed,
                None,
                vec!["unknown-planet".to_owned()]
            ),
            "an invalid target leaves the copy unused"
        );
        assert_eq!(state, before, "failed readying is atomic");

        assert!(use_scripted(
            &mut state,
            borrowed,
            None,
            vec![theirs.to_string()],
        ));
        assert!(state.exhausted_planets.contains(&mine));
        assert!(
            !state.exhausted_planets.contains(&theirs),
            "the borrower may ready the other player's controlled planet"
        );
        assert_eq!(
            status(&state, &a, &LeaderId::new("yssarilagent")),
            Some(LeaderStatus::Exhausted)
        );
        assert_eq!(
            status(&state, &b, &LeaderId::new("xxchaagent")),
            Some(LeaderStatus::Exhausted),
            "copying leaves the source agent untouched"
        );
    }

    #[test]
    fn borrowing_another_ssruu_does_not_reenter_the_copy_dispatch() {
        // Duplicate Yssaril seats are excluded by normal faction seating. This defensive case
        // makes the boundary explicit: a copied Ssruu is handled as one source agent, never by
        // recursively invoking `use_leader_text` for the source's own copy ability.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let mut state = game(&["a", "b"]);
        state.player_mut(&a).unwrap().faction = ti4_model::id::FactionId::new("yssaril");
        state.player_mut(&b).unwrap().faction = ti4_model::id::FactionId::new("yssaril");
        holding(&mut state, "yssarilagent", LeaderStatus::Readied);
        state
            .player_mut(&b)
            .unwrap()
            .leaders
            .insert(LeaderId::new("yssarilagent"), LeaderStatus::Readied);

        assert!(
            crate::factions::hooks_cards::borrowable_agents(&state, ContentStore::embedded(), &a,)
                .iter()
                .any(|(owner, agent)| owner == &b && agent.as_str() == "yssarilagent")
        );
        let before = state.clone();
        assert!(
            !component_actions(&state, ContentStore::embedded(), &a)
                .iter()
                .any(|option| option.id == "component|leader|yssarilagent|yssarilagent"),
            "Ssruu's non-ACTION window is not recursively offered"
        );
        assert!(!use_scripted(
            &mut state,
            "yssarilagent|yssarilagent",
            None,
            Vec::new(),
        ));
        assert_eq!(
            state, before,
            "the nested copy has no dispatch and no mutation"
        );
    }

    #[test]
    fn use_leader_text_refuses_without_ssruu_or_for_a_non_agent_and_changes_nothing() {
        let mut state = ssruu_game();
        // b does not hold Ssruu, so b borrows nothing (not even a's agent).
        let (decider, _seen) = crate::choice::Capturing::new(Box::new(
            crate::choice::Scripted::new(Vec::<String>::new()),
        ));
        let mut table = crate::choice::Table::with_default(Box::new(decider));
        let before = state.clone();
        let refused = crate::fixtures::with_context(&mut state, POK, None, &mut table, |ctx| {
            (
                use_leader_text(ctx, &PlayerId::new("b"), &LeaderId::new("yssarilagent")),
                use_leader_text(ctx, &PlayerId::new("b"), &LeaderId::new("hacanagent")),
                // a holds Ssruu, but a hero is not an agent.
                use_leader_text(ctx, &player(), &LeaderId::new("hacanhero")),
                // and a's own Ssruu is not another seat's agent.
                use_leader_text(ctx, &player(), &LeaderId::new("yssarilagent")),
            )
        });
        assert_eq!(refused, (false, false, false, false));
        assert_eq!(state, before);
    }

    #[test]
    fn ssruu_offers_other_seats_action_agents_with_distinct_ids() {
        let content = ContentStore::embedded();
        let state = ssruu_game();
        let ids = |who: &str| -> Vec<String> {
            component_actions(&state, content, &PlayerId::new(who))
                .into_iter()
                .map(|option| option.id)
                .collect()
        };
        let mine = ids("a");
        assert!(
            mine.iter()
                .any(|id| id == "component|leader|yssarilagent|hacanagent"),
            "{mine:?}"
        );
        assert!(
            !mine.iter().any(|id| id == "component|leader|yssarilagent"),
            "Ssruu's own window is not an action"
        );
        let mut distinct = mine.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(distinct.len(), mine.len(), "option ids are distinct");
        assert!(
            ids("b").iter().all(|id| !id.contains("yssarilagent")),
            "only the Ssruu holder is offered borrowed agents"
        );
        // A borrowed ACTION agent takes the turn; a "During the action phase" one does not.
        assert!(!uses_the_action(
            content,
            &LeaderId::new("yssarilagent|hacanagent")
        ));
        assert!(uses_the_action(
            content,
            &LeaderId::new("yssarilagent|xxchaagent")
        ));
        // Not offered when the borrowed effect could not resolve for the holder.
        let mut full = state.clone();
        for seat in &mut full.players {
            seat.faction = ti4_model::id::FactionId::new("hacan");
            seat.commodities = 99;
        }
        assert!(
            ids_of(&full, "a")
                .iter()
                .all(|id| !id.contains("hacanagent")),
            "a full table leaves Carth with nothing to do"
        );
    }

    fn ids_of(state: &GameState, who: &str) -> Vec<String> {
        component_actions(state, ContentStore::embedded(), &PlayerId::new(who))
            .into_iter()
            .map(|option| option.id)
            .collect()
    }

    #[test]
    fn the_borrowed_agent_hook_runs_after_a_successful_borrow_only() {
        let hook = crate::factions::hooks_cards::CardHooks {
            borrowed_agent_used: Some(|ctx, who, source| {
                ctx.state
                    .faction_marks
                    .insert(format!("test:borrowed:{who}"), source.to_string());
            }),
            ..crate::factions::hooks_cards::CardHooks::NONE
        };
        crate::factions::hooks_cards::with_test_hooks(hook, || {
            let mut state = ssruu_game();
            assert!(use_scripted(
                &mut state,
                "yssarilagent|hacanagent",
                None,
                vec!["b".to_owned()],
            ));
            assert_eq!(
                state
                    .faction_marks
                    .get("test:borrowed:a")
                    .map(String::as_str),
                Some("hacanagent")
            );
            let mut refused = game(&["a", "b"]);
            assert!(!use_scripted(
                &mut refused,
                "yssarilagent|hacanagent",
                None,
                vec![]
            ));
            assert!(refused.faction_marks.is_empty(), "a refusal fires nothing");
        });
    }

    #[test]
    fn every_delivered_action_leader_prints_an_action_window() {
        // The delivered set and the corpus have to agree. Membership says the engine can resolve
        // the use; the printed window says a seat can ever be asked. Either half on its own is how
        // "leaders are delivered" and "the Hacan agent is never offered" were both true at once.
        let content = ContentStore::embedded();
        for leader in DELIVERED_ACTION_LEADERS {
            let record = content
                .get(ContentType::Leaders, leader)
                .unwrap_or_else(|| panic!("{leader} is not in the corpus"));
            assert!(
                is_action_window(content, &LeaderId::new(leader)),
                "{leader} is in the delivered action-leader set, but its printed window {:?} is not an action window — it can never be offered",
                record.text("abilityWindow")
            );
        }
    }

    #[test]
    fn an_agent_with_no_commodities_to_gain_is_not_offered() {
        // Both branches of Carth are commodity-shaped. With every seat at its cap, the use would
        // exhaust the card and change nothing.
        let content = ContentStore::embedded();
        let mut state = game(&["a", "b"]);
        for name in ["a", "b"] {
            let seat = state.player_mut(&PlayerId::new(name)).unwrap();
            seat.faction = ti4_model::id::FactionId::new("hacan");
            seat.commodities = ti4_content::factions::get(content, "hacan")
                .expect("Hacan exists")
                .commodities();
        }
        holding(&mut state, "hacanagent", LeaderStatus::Readied);

        let offered: Vec<String> = component_actions(&state, content, &player())
            .into_iter()
            .map(|option| option.id)
            .collect();
        assert!(
            !offered.iter().any(|id| id.ends_with("hacanagent")),
            "a use that can do nothing was offered: {offered:?}"
        );

        // One seat below its cap and it is a live option again.
        state.player_mut(&PlayerId::new("b")).unwrap().commodities -= 1;
        assert!(
            component_actions(&state, content, &player())
                .iter()
                .any(|option| option.id.ends_with("hacanagent")),
            "somebody can still be replenished"
        );
    }

    #[test]
    fn the_helmsman_chooses_the_destination_system() {
        let hub = crate::fixtures::plain_hub();
        let mut state = game(&["a"]);
        holding(&mut state, "l1z1xhero", LeaderStatus::Unlocked);
        let far = ti4_model::id::SystemId::new(hub.across(&hub.outer[0]));
        let destination = ti4_model::id::SystemId::new(hub.across(&hub.outer[1]));
        crate::fixtures::put(&mut state, &far, "flagship", &player(), 1);
        crate::fixtures::put(&mut state, &far, "dreadnought", &player(), 2);

        assert!(use_scripted(
            &mut state,
            "l1z1xhero",
            Some(&hub.galaxy),
            vec![destination.to_string()],
        ));

        let moved: Vec<String> = state
            .system_state(&destination)
            .units
            .iter()
            .map(|unit| unit.type_id.to_string())
            .collect();
        assert_eq!(
            moved.len(),
            3,
            "flagship and both dreadnoughts gathered: {moved:?}"
        );
        let left: Vec<String> = state
            .system_state(&far)
            .units
            .iter()
            .map(|u| u.type_id.to_string())
            .collect();
        assert!(
            left.is_empty(),
            "the far system is empty of big ships: {left:?}"
        );
    }

    #[test]
    fn the_te_hero_places_units_and_readies_their_planets() {
        let mut state = game(&["a"]);
        // Xxcha seat with the Thunder's Edge hero unlocked (FULL scope deploys exactly this one).
        state.player_mut(&player()).unwrap().faction = ti4_model::id::FactionId::new("xxcha");
        let hero = holding(&mut state, "xxchahero-te", LeaderStatus::Unlocked);

        // Two controlled planets in different systems; one of them starts exhausted.
        let all = ti4_content::galaxy::all_planets(ti4_content::ContentStore::embedded(), POK);
        let mut seen_systems = std::collections::BTreeSet::new();
        let picks: Vec<(String, String)> = all
            .iter()
            .filter(|(_, planet)| planet.system_id().is_some() && !planet.is_placed_during_play())
            .filter_map(|(id, planet)| {
                let system = planet.system_id()?.to_owned();
                seen_systems
                    .insert(system.clone())
                    .then(|| (system, id.to_string()))
            })
            .take(2)
            .collect();
        assert_eq!(picks.len(), 2);
        for (system, planet) in &picks {
            state
                .system_mut(&ti4_model::id::SystemId::new(system))
                .set_control(ti4_model::id::PlanetId::new(planet), player().clone());
        }
        let exhausted_planet = ti4_model::id::PlanetId::new(&picks[0].1);
        state.exhausted_planets.insert(exhausted_planet.clone());

        // Place a PDS on the first planet, a mech on the second, then stop.
        assert!(
            use_scripted(
                &mut state,
                "xxchahero-te",
                None,
                vec![
                    "place|pds".to_owned(),
                    format!("planet|{}", picks[0].1),
                    "place|mech".to_owned(),
                    format!("planet|{}", picks[1].1),
                    "stop".to_owned(),
                ],
            ),
            "the effect resolves"
        );

        let (system_a, planet_a) = &picks[0];
        let (system_b, planet_b) = &picks[1];
        let placed_a: Vec<String> = state
            .system_state(&ti4_model::id::SystemId::new(system_a))
            .planet_units
            .get(&ti4_model::id::PlanetId::new(planet_a))
            .map(|units| {
                units
                    .iter()
                    .map(|unit| unit.type_id.as_str().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        let catalogue =
            ti4_content::units::catalogue(ti4_content::ContentStore::embedded(), SourceSet::all());
        assert_eq!(placed_a.len(), 1, "exactly one unit on the first planet");
        assert_eq!(
            catalogue
                .get(placed_a[0].as_str())
                .map(|unit| unit.base_type().to_owned()),
            Some("pds".to_string()),
            "the placed unit is a PDS"
        );
        let placed_b: Vec<String> = state
            .system_state(&ti4_model::id::SystemId::new(system_b))
            .planet_units
            .get(&ti4_model::id::PlanetId::new(planet_b))
            .map(|units| {
                units
                    .iter()
                    .map(|unit| unit.type_id.as_str().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(placed_b.len(), 1, "one unit on the second planet");
        assert_eq!(
            catalogue
                .get(placed_b[0].as_str())
                .map(|unit| unit.base_type().to_owned()),
            Some("mech".to_string()),
            "the placed unit is a mech"
        );
        assert!(
            !state.exhausted_planets.contains(&exhausted_planet),
            "its planet was readied"
        );
        assert_eq!(
            state.player(&player()).unwrap().leaders.get(&hero),
            Some(&LeaderStatus::Purged),
            "purged after the effect"
        );
    }

    #[test]
    fn rin_settles_every_swap_in_one_use_then_purges() {
        // "For each ... you may replace that technology with any technology of the same colour.
        // Then, purge this card." One use settles every swap — each declined or taken on its own
        // terms — and the hero is gone afterwards.
        let content = ContentStore::embedded();
        let mut by_colour: std::collections::BTreeMap<String, Vec<ti4_model::id::TechnologyId>> =
            std::collections::BTreeMap::new();
        for record in content.from_sources(ContentType::Technologies, POK) {
            if let (Some(alias), Some(colour)) =
                (record.text("alias"), record.strings("types").first())
                && !record.strings("types").contains(&"UNITUPGRADE")
                && record.text("faction").is_none()
                && !crate::technology::is_unit_upgrade(
                    content,
                    &ti4_model::id::TechnologyId::new(alias),
                )
            {
                by_colour
                    .entry((*colour).to_string())
                    .or_default()
                    .push(ti4_model::id::TechnologyId::new(alias));
            }
        }
        let (_, techs) = by_colour
            .iter()
            .find(|(_, candidates)| candidates.len() >= 3)
            .expect("a colour with three ordinary technologies");
        let mut techs: Vec<ti4_model::id::TechnologyId> = techs.clone();
        techs.sort();
        let (first, second, third) = (&techs[0], &techs[1], &techs[2]);

        let mut state = game(&["a"]);
        let hero = holding(&mut state, "jolnarhero", LeaderStatus::Unlocked);
        state
            .player_mut(&player())
            .unwrap()
            .technologies
            .insert(first.clone());
        state
            .player_mut(&player())
            .unwrap()
            .technologies
            .insert(second.clone());

        // The eligible list is the seat's BTreeSet order: first, then second. Trade the first
        // for the third; keep the second.
        assert!(use_scripted(
            &mut state,
            "jolnarhero",
            None,
            vec![third.to_string(), format!("keep|{second}")],
        ));

        let after = state.player(&player()).unwrap();
        assert!(after.technologies.contains(third), "the trade landed");
        assert!(
            !after.technologies.contains(first),
            "the traded one is gone"
        );
        assert!(after.technologies.contains(second), "the kept one stayed");
        assert_eq!(
            after.leaders.get(&hero),
            Some(&LeaderStatus::Purged),
            "then, purge this card"
        );
    }

    #[test]
    fn full_scope_replaces_the_pok_xxcha_hero_with_the_te_hero() {
        let heroes: Vec<LeaderId> = for_faction(
            ContentStore::embedded(),
            ti4_model::content_types::FULL,
            "xxcha",
        )
        .into_iter()
        .filter(|leader| kind_of(ContentStore::embedded(), leader).as_deref() == Some(HERO))
        .collect();

        assert_eq!(heroes, vec![LeaderId::new("xxchahero-te")]);
    }

    #[test]
    fn an_unlocked_commander_is_not_a_usable_card() {
        let mut state = game(&["a"]);
        let commander = holding(&mut state, "hacancommander", LeaderStatus::Unlocked);

        assert!(
            !usable(&state, ContentStore::embedded(), &player()).contains(&commander),
            "commanders are standing or triggered abilities, never generic usable cards"
        );
    }

    #[test]
    fn action_options_contain_only_action_window_leaders() {
        let mut state = game(&["a"]);
        holding(&mut state, "xxchaagent", LeaderStatus::Readied);
        holding(&mut state, "solhero", LeaderStatus::Unlocked);
        holding(&mut state, "hacanhero", LeaderStatus::Unlocked);
        state
            .exhausted_planets
            .insert(ti4_model::id::PlanetId::new("somewhere"));

        let ids: Vec<String> = component_actions(&state, ContentStore::embedded(), &player())
            .into_iter()
            .map(|option| option.id)
            .collect();

        assert!(ids.contains(&"component|leader|xxchaagent".to_owned()));
        assert!(ids.contains(&"component|leader|solhero".to_owned()));
        assert!(
            !ids.contains(&"component|leader|hacanhero".to_owned()),
            "Hacan's hero belongs to the production timing window"
        );
    }

    #[test]
    fn exhausting_an_agent_stages_agent_exhausted_only_for_a_watching_nomad() {
        let a = PlayerId::new("a");
        let staged = |state: &GameState| crate::supply::staged_event_types(state);
        let mut state = crate::fixtures::seated_game(
            &[("a", "nomad"), ("b", "sol")],
            ti4_model::content_types::DEFAULT,
        );
        // Without Temporal Command Suite nothing is staged.
        assert!(exhaust(&mut state, &a, &LeaderId::new("nomadagentartuno")));
        assert!(staged(&state).is_empty());
        state
            .player_mut(&a)
            .unwrap()
            .technologies
            .insert(ti4_model::id::TechnologyId::new("tcs"));
        assert!(exhaust(&mut state, &a, &LeaderId::new("nomadagentmercer")));
        assert_eq!(staged(&state), ["AGENT_EXHAUSTED"]);
        // A hero is not an agent.
        let before = staged(&state).len();
        state
            .player_mut(&a)
            .unwrap()
            .leaders
            .insert(LeaderId::new("nomadhero"), LeaderStatus::Readied);
        assert!(exhaust(&mut state, &a, &LeaderId::new("nomadhero")));
        assert_eq!(staged(&state).len(), before);
    }
}
