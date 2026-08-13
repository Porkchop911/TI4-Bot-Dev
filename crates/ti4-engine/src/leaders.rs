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
        .map(|(_, planet)| crate::production::planet_value(content, sources, planet, kind))
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
                .adjacent(crate::seating::MECATOL)
                .into_iter()
                .map(ToOwned::to_owned)
                .collect();
            nearby.insert(crate::seating::MECATOL.to_owned());
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
        _ => return None,
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
#[must_use]
pub fn for_faction(content: &ContentStore, sources: SourceSet, faction: &str) -> Vec<LeaderId> {
    content
        .from_sources(ContentType::Leaders, sources)
        .filter(|record| {
            record
                .text("faction")
                .is_some_and(|owner| owner.eq_ignore_ascii_case(faction))
        })
        .filter_map(|record| record.text("id").or_else(|| record.text("alias")))
        .map(LeaderId::new)
        .collect()
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
    exhausted
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
#[must_use]
pub fn usable(state: &GameState, content: &ContentStore, player: &PlayerId) -> Vec<LeaderId> {
    state.player(player).map_or_else(Vec::new, |seat| {
        seat.leaders
            .iter()
            .filter(|(leader, status)| match status {
                LeaderStatus::Readied => kind_of(content, leader).as_deref() == Some(AGENT),
                LeaderStatus::Unlocked => true,
                _ => false,
            })
            .map(|(leader, _)| leader.clone())
            .collect()
    })
}

#[cfg(test)]
mod tests {

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
}
