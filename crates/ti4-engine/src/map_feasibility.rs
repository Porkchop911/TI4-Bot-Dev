//! Objectives a particular map can never satisfy.
//!
//! **Project setup rule (not a printed rule).** Under the printed rules a revealed or dealt
//! objective that nobody can score simply stays in the game. This project decides otherwise: an
//! unwinnable draw spoils a game, and the ordinary board generators make that likely. The seeded
//! filler pool ([`crate::seating::neutral_systems`]) deliberately holds only plain planet systems,
//! so a generated map has no empty system, no wormhole, no anomaly, and no gamma source unless a
//! template places such a tile by hand. Explore Deep Space, Become the Gatekeeper and a handful of
//! others are then not merely hard but impossible.
//!
//! [`unsatisfiable_public`] and [`unsatisfiable_secret`] list the objectives that the *map alone*
//! rules out, judged generously: every bound below is an upper bound on what any one player could
//! ever reach, so an objective is listed only when even a perfect game cannot score it.
//! [`apply`] then removes those cards from the decks at setup. Nothing else about an objective
//! changes, a map that satisfies everything is left byte-for-byte untouched, and the result is a
//! function of the seed and the map only.
//!
//! Deliberately **not** judged here (they depend on play, not on the map): exploration-dependent
//! cards beyond the traits check, factions, technologies, combats, relics, laws, and reaching the
//! Wormhole Nexus through a gamma token (Gamma Relay, a gamma frontier card, Creuss), which is why
//! Defy Space and Time stays in. Become the Gatekeeper ignores that same long detour: it asks for
//! an alpha and a beta *on the map*.

use std::collections::BTreeSet;

use ti4_content::ContentStore;
use ti4_content::galaxy::{Galaxy, System, all_planets, all_systems};
use ti4_model::content_types::SourceSet;
use ti4_model::id::{ObjectiveId, SystemId};
use ti4_model::state::GameState;

use crate::deck::{Exclusions, build_starting_decks_excluding};

/// What a built map contains, as far as objectives care.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MapCensus {
    /// Players seated.
    pub players: usize,
    /// Systems that can hold units (on the grid, not hyperlane tiles, plus off-map tiles).
    pub holdable: usize,
    /// Of those, systems with no planet at all.
    pub planetless: usize,
    /// Systems with Mecatol Rex, a legendary planet, or an anomaly.
    pub notable: usize,
    /// Systems that hold units and have an anomaly next to them.
    pub beside_anomaly: usize,
    /// Systems with an anomaly (units may be there, so supernovae count: the bound is generous).
    pub anomalies: usize,
    /// Systems with a printed or token alpha wormhole / beta wormhole.
    pub alpha: usize,
    pub beta: usize,
    /// Planets on the map (stations excluded), and those outside every homeworld.
    pub planets: usize,
    pub non_home_planets: usize,
    /// The largest number of planets sharing one trait.
    pub most_common_trait: usize,
    pub cultural: usize,
    pub hazardous: usize,
    pub industrial: usize,
    pub legendary: usize,
    pub mecatol: bool,
    /// Total resource and influence value of all planets.
    pub resources: i64,
    pub influence: i64,
    /// Systems with two or more planets.
    pub multi_planet_systems: usize,
    /// The best any single seat can do on the rim (edge systems that hold units, minus its own
    /// home if that is on the edge).
    pub best_rim: usize,
    /// The best any single seat can do for structures on planets away from its own home.
    pub best_planets_away: usize,
    /// Systems that hold units and touch Mecatol Rex.
    pub beside_mecatol: usize,
    /// Systems that hold units and touch a home system.
    pub beside_a_home: usize,
}

/// Count what `galaxy` holds. `homes` are the seated players' home systems, in seat order.
#[must_use]
pub fn census(
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &Galaxy,
    homes: &[SystemId],
) -> MapCensus {
    let systems = all_systems(content, sources);
    let planets = all_planets(content, sources);
    let mut holdable: Vec<&str> = galaxy.system_ids_holding_things();
    holdable.extend(galaxy.off_map_system_ids());
    let holdable_set: BTreeSet<&str> = holdable.iter().copied().collect();
    let mecatol = crate::seating::mecatol_in_galaxy(galaxy);

    let is_anomaly = |id: &str| systems.get(id).is_some_and(System::is_anomaly);
    let planets_of = |id: &str| -> Vec<&str> {
        systems.get(id).map_or_else(Vec::new, |s| {
            s.planets()
                .into_iter()
                .filter(|p| planets.get(p).is_some_and(|p| !p.is_space_station()))
                .collect()
        })
    };
    let is_legendary_system = |id: &str| {
        planets_of(id).iter().any(|p| {
            planets
                .get(p)
                .is_some_and(ti4_content::galaxy::Planet::is_legendary)
        })
    };

    let mut c = MapCensus {
        players: homes.len(),
        holdable: holdable.len(),
        mecatol: galaxy.coord_of(mecatol).is_some(),
        ..MapCensus::default()
    };
    let mut trait_counts = std::collections::BTreeMap::<String, usize>::new();
    for id in &holdable {
        let own = planets_of(id);
        if systems.get(id).is_some_and(|s| s.planets().is_empty()) {
            c.planetless += 1;
        }
        if own.len() >= 2 {
            c.multi_planet_systems += 1;
        }
        if is_anomaly(id) {
            c.anomalies += 1;
        }
        if crate::seating::is_mecatol(id) || is_anomaly(id) || is_legendary_system(id) {
            c.notable += 1;
        }
        if galaxy.adjacent(id).into_iter().any(|n| is_anomaly(n)) {
            c.beside_anomaly += 1;
        }
        let kinds = galaxy.wormhole_kinds(id);
        c.alpha += usize::from(kinds.contains("ALPHA"));
        c.beta += usize::from(kinds.contains("BETA"));
        for name in own {
            let Some(planet) = planets.get(name) else {
                continue;
            };
            c.planets += 1;
            c.non_home_planets += usize::from(planet.homeworld_of().is_none());
            c.resources += planet.resources();
            c.influence += planet.influence();
            c.legendary += usize::from(planet.is_legendary());
            for kind in planet.traits() {
                *trait_counts.entry(kind.to_ascii_uppercase()).or_default() += 1;
            }
        }
    }
    c.cultural = trait_counts.get("CULTURAL").copied().unwrap_or(0);
    c.hazardous = trait_counts.get("HAZARDOUS").copied().unwrap_or(0);
    c.industrial = trait_counts.get("INDUSTRIAL").copied().unwrap_or(0);
    c.most_common_trait = trait_counts.values().copied().max().unwrap_or(0);

    let edge: BTreeSet<&str> = galaxy
        .system_ids()
        .into_iter()
        .filter(|id| {
            galaxy.coord_of(id).is_some_and(|here| {
                here.neighbours()
                    .into_iter()
                    .any(|next| galaxy.system_at(next).is_none())
            })
        })
        .filter(|id| holdable_set.contains(id))
        .collect();
    let home_ids: BTreeSet<&str> = homes.iter().map(SystemId::as_str).collect();
    c.best_rim = homes
        .iter()
        .map(|home| edge.iter().filter(|id| **id != home.as_str()).count())
        .max()
        .unwrap_or(edge.len());
    c.best_planets_away = homes
        .iter()
        .map(|home| {
            holdable
                .iter()
                .filter(|id| **id != home.as_str())
                .map(|id| planets_of(id).len())
                .sum()
        })
        .max()
        .unwrap_or(c.planets);
    c.beside_mecatol = galaxy
        .adjacent(mecatol)
        .into_iter()
        .filter(|id| holdable_set.contains(id))
        .count();
    c.beside_a_home = holdable
        .iter()
        .filter(|id| {
            galaxy
                .adjacent(id)
                .into_iter()
                .any(|n| home_ids.contains(n))
        })
        .count();
    c
}

/// Public objectives (by alias) that this map can never let anyone score.
#[must_use]
pub fn unsatisfiable_public(c: &MapCensus) -> Vec<&'static str> {
    let mut out = Vec::new();
    let mut when = |alias: &'static str, impossible: bool| {
        if impossible {
            out.push(alias);
        }
    };
    when("corner", c.most_common_trait < 4);
    when("unify_colonies", c.most_common_trait < 6);
    when("expand_borders", c.non_home_planets < 6);
    when("subdue", c.non_home_planets < 11);
    // Attachments come from exploring a planet, which needs a trait (frontier cards aside).
    when("lost_outposts", c.cultural + c.hazardous + c.industrial < 2);
    when(
        "ancient_monuments",
        c.cultural + c.hazardous + c.industrial < 3,
    );
    when("intimidate", c.beside_mecatol < 2);
    when("outer_rim", c.best_rim < 3);
    when("control_borderlands", c.best_rim < 5);
    when("infrastructure", c.best_planets_away < 3);
    when("protect_border", c.best_planets_away < 5);
    when("deep_space", c.planetless < 3);
    when("vast_territories", c.planetless < 5);
    when("make_history", c.notable < 2);
    when("become_legend", c.notable < 4);
    when("push_boundaries", c.players < 3);
    when("distant_lands", c.players < 3);
    when("conquer", c.players < 2);
    when("monument", c.resources < 8);
    when("golden_age", c.resources < 16);
    when("sway_council", c.influence < 8);
    when("manipulate_law", c.influence < 16);
    when("amass_wealth", c.resources < 3 || c.influence < 3);
    when("vast_reserves", c.resources < 6 || c.influence < 6);
    out
}

/// Secret objectives (by alias) that this map can never let anyone score.
#[must_use]
pub fn unsatisfiable_secret(c: &MapCensus) -> Vec<&'static str> {
    let mut out = Vec::new();
    let mut when = |alias: &'static str, impossible: bool| {
        if impossible {
            out.push(alias);
        }
    };
    when("btgk", c.alpha == 0 || c.beta == 0);
    when("lsc", c.beside_anomaly < 3);
    when("btv", c.anomalies == 0);
    // Mirage can only arrive from a frontier token, and those sit in empty systems.
    when("sai", c.legendary == 0 && c.planetless == 0);
    when("faa", c.cultural < 4);
    when("mrm", c.hazardous < 4);
    when("mp", c.industrial < 4);
    when("ose", !c.mecatol);
    when("syc", c.multi_planet_systems == 0);
    when("te", c.beside_a_home == 0);
    when("ctr", c.holdable < 6);
    when("eh", c.influence < 12);
    when("hrm", c.resources < 12);
    when("csl", c.players < 2);
    when("dts", c.players < 2);
    out
}

/// Remove from the decks every objective the map rules out. Returns the aliases removed from
/// play as `(public, secret)`, both empty (and the state untouched) when the map rules out nothing.
///
/// Call once at setup, after the board exists and before any card is played. Public objectives are
/// redrawn from the same shuffled pools minus the excluded cards, so the two revealed cards and
/// the rest of the deck change only where a card was excluded. Secret hands keep every card they
/// hold that is still legal; an excluded one is replaced from the top of the deck.
pub fn apply(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &Galaxy,
) -> (Vec<&'static str>, Vec<&'static str>) {
    let homes: Vec<SystemId> = state
        .players
        .iter()
        .filter_map(|seat| seat.home_system.clone())
        .collect();
    let c = census(content, sources, galaxy, &homes);
    let public = unsatisfiable_public(&c);
    let secret = unsatisfiable_secret(&c);
    if public.is_empty() && secret.is_empty() {
        return (public, secret);
    }

    let revealed = state.revealed_objectives.len();
    let decks = build_starting_decks_excluding(
        content,
        sources,
        state.rng_seed,
        &Exclusions {
            public: public.iter().map(|a| (*a).to_owned()).collect(),
            secret: secret.iter().map(|a| (*a).to_owned()).collect(),
        },
    );
    let public_set: BTreeSet<&str> = public.iter().copied().collect();
    state.objective_deck = decks.objectives;
    state.revealed_objectives.clear();
    for _ in 0..revealed {
        let _ = state.reveal_objective();
    }
    debug_assert!(
        state
            .revealed_objectives
            .iter()
            .chain(&state.objective_deck)
            .all(|id: &ObjectiveId| !public_set.contains(id.as_str()))
    );

    let secret_set: BTreeSet<&str> = secret.iter().copied().collect();
    state
        .secret_deck
        .retain(|id| !secret_set.contains(id.as_str()));
    for index in 0..state.players.len() {
        for slot in 0..state.players[index].secret_objectives.len() {
            if secret_set.contains(state.players[index].secret_objectives[slot].as_str())
                && !state.secret_deck.is_empty()
            {
                state.players[index].secret_objectives[slot] = state.secret_deck.remove(0);
            }
        }
    }
    (public, secret)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::build_starting_decks;
    use crate::setup::start_game_seeded;
    use ti4_content::galaxy::Galaxy;
    use ti4_model::content_types::POK;
    use ti4_model::id::{FactionId, PlayerId};

    fn content() -> &'static ContentStore {
        ContentStore::embedded()
    }

    fn seated(n: usize, seed: u64) -> (GameState, Vec<SystemId>) {
        let ids: Vec<PlayerId> = (0..n).map(|i| PlayerId::new(format!("p{i}"))).collect();
        let mut state = start_game_seeded(content(), &ids, POK, None, seed).unwrap();
        let factions = seating::IN_SCOPE_FACTIONS;
        for (i, id) in ids.iter().enumerate() {
            seating::deploy(&mut state, content(), id, &FactionId::new(factions[i]), POK).unwrap();
        }
        let homes = state
            .players
            .iter()
            .filter_map(|p| p.home_system.clone())
            .collect();
        (state, homes)
    }

    use crate::seating;

    /// A plain generated board: filler only, as the seeded generator makes it.
    fn plain_board(state: &GameState, seed: u64) -> Galaxy {
        let by_seat = state
            .players
            .iter()
            .enumerate()
            .map(|(i, p)| (PlayerId::new(format!("seat_{i:02}")), p.faction.clone()))
            .collect();
        let filler = seating::map_filler(content(), 36, POK, seed);
        let refs: Vec<&str> = filler.iter().map(SystemId::as_str).collect();
        seating::build_board(content(), &by_seat, &refs, POK).unwrap()
    }

    /// A board that offers everything: empty systems, anomalies, both wormholes, planets.
    fn rich_board(state: &GameState) -> Galaxy {
        let own_homes: Vec<String> = state
            .players
            .iter()
            .filter_map(|p| p.home_system.as_ref().map(ToString::to_string))
            .collect();
        let systems = all_systems(content(), POK);
        let homes = ti4_content::galaxy::home_systems(content(), POK);
        let pick = |want: &dyn Fn(&System) -> bool, n: usize| -> Vec<&str> {
            systems
                .iter()
                .filter(|(id, s)| {
                    want(s)
                        && !homes.contains(*id)
                        && !s.is_hyperlane()
                        && !crate::seating::is_mecatol(id)
                        && !crate::fracture::is_fracture_system(
                            content(),
                            POK,
                            &SystemId::new(**id),
                        )
                        && s.id() != seating::LOCKED_NEXUS
                        && s.id() != seating::OPEN_NEXUS
                        && !["17", "51", "117", "118"].contains(id)
                })
                .map(|(id, _)| *id)
                .take(n)
                .collect()
        };
        let mut ids: Vec<&str> = vec!["18"];
        ids.extend(pick(
            &|s| s.planets().is_empty() && !s.is_anomaly() && s.wormholes().is_empty(),
            6,
        ));
        ids.extend(pick(&|s| s.is_anomaly() && !s.planets().is_empty(), 6));
        ids.extend(pick(&|s| s.wormholes().contains("ALPHA"), 1));
        ids.extend(pick(&|s| s.wormholes().contains("BETA"), 1));
        let mut seen = BTreeSet::new();
        ids.retain(|id| seen.insert(*id));
        let mut planets: Vec<&str> = pick(
            &|s| s.planets().len() >= 2 && !s.is_anomaly() && s.wormholes().is_empty(),
            2,
        );
        planets.extend(pick(
            &|s| s.planets().len() == 1 && !s.is_anomaly() && s.wormholes().is_empty(),
            30,
        ));
        let planets: Vec<&str> = planets
            .into_iter()
            .filter(|id| !seen.contains(id))
            .collect();
        ids.extend(planets);
        // Homes go in the first ring after Mecatol's, so each touches a ring-two filler tile.
        let mut placed = vec![ids[0]];
        placed.extend(own_homes.iter().map(String::as_str));
        placed.extend(ids[1..].iter().take(32));
        Galaxy::build(content(), &placed, POK, 3).unwrap()
    }

    fn ten(state: &GameState) -> Vec<String> {
        state
            .revealed_objectives
            .iter()
            .chain(&state.objective_deck)
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn a_generated_board_rules_out_the_objectives_it_has_no_tiles_for() {
        let (state, homes) = seated(4, 11);
        let galaxy = plain_board(&state, 11);
        let c = census(content(), POK, &galaxy, &homes);
        assert_eq!(c.planetless, 0, "the filler pool holds no empty systems");
        assert_eq!((c.alpha, c.beta, c.anomalies), (0, 0, 0));
        let public = unsatisfiable_public(&c);
        let secret = unsatisfiable_secret(&c);
        for alias in ["deep_space", "vast_territories"] {
            assert!(public.contains(&alias), "{alias}");
        }
        for alias in ["btgk", "lsc", "btv"] {
            assert!(secret.contains(&alias), "{alias}");
        }
        // Cards the board can serve are not touched.
        for alias in ["expand_borders", "outer_rim", "intimidate"] {
            assert!(!public.contains(&alias), "{alias}");
        }
        assert!(!secret.contains(&"ose") && !secret.contains(&"te"));
    }

    #[test]
    fn a_board_with_everything_changes_nothing() {
        let (mut state, homes) = seated(4, 11);
        let galaxy = rich_board(&state);
        let c = census(content(), POK, &galaxy, &homes);
        assert!(
            c.planetless >= 5 && c.alpha >= 1 && c.beta >= 1 && c.beside_anomaly >= 3,
            "{c:?}"
        );
        assert_eq!(unsatisfiable_public(&c), Vec::<&str>::new(), "{c:?}");
        assert_eq!(unsatisfiable_secret(&c), Vec::<&str>::new(), "{c:?}");
        let before = state.clone();
        let (p, s) = apply(&mut state, content(), POK, &galaxy);
        assert!(p.is_empty() && s.is_empty());
        assert_eq!(state, before);
    }

    #[test]
    fn setup_deals_the_ruled_out_cards_out_and_keeps_the_rest_in_seeded_order() {
        for n in 3..=6 {
            for seed in 0..12 {
                let (mut state, _) = seated(n, seed);
                let galaxy = plain_board(&state, seed);
                let original = state.clone();
                let (public, secret) = apply(&mut state, content(), POK, &galaxy);
                assert!(!public.is_empty() && !secret.is_empty());

                // Public: still five of each stage, two revealed, none ruled out.
                assert_eq!(ten(&state).len(), 10);
                assert_eq!(state.revealed_objectives.len(), 2);
                assert!(ten(&state).iter().all(|id| !public.contains(&id.as_str())));
                let stage_one = |id: &String| {
                    content()
                        .get(ti4_model::content_types::ContentType::PublicObjectives, id)
                        .and_then(|r| r.int("points"))
                };
                assert!(ten(&state)[..5].iter().all(|id| stage_one(id) == Some(1)));
                assert!(ten(&state)[5..].iter().all(|id| stage_one(id) == Some(2)));
                // Cards that were not ruled out keep their relative order.
                let kept: Vec<String> = ten(&original)
                    .into_iter()
                    .filter(|id| !public.contains(&id.as_str()))
                    .collect();
                let now = ten(&state);
                let survivors: Vec<&str> = now
                    .iter()
                    .filter(|id| kept.contains(id))
                    .map(String::as_str)
                    .collect();
                let expected: Vec<&str> = kept
                    .iter()
                    .filter(|id| now.contains(id))
                    .map(String::as_str)
                    .collect();
                assert_eq!(survivors, expected);

                // Secrets: nobody holds a ruled-out card, the deck has none, hands keep size,
                // and a legal card in hand stays.
                assert!(
                    state
                        .secret_deck
                        .iter()
                        .all(|id| !secret.contains(&id.as_str()))
                );
                for (before, after) in original.players.iter().zip(&state.players) {
                    assert_eq!(
                        before.secret_objectives.len(),
                        after.secret_objectives.len()
                    );
                    assert!(
                        after
                            .secret_objectives
                            .iter()
                            .all(|id| !secret.contains(&id.as_str()))
                    );
                    for card in &before.secret_objectives {
                        if !secret.contains(&card.as_str()) {
                            assert!(after.secret_objectives.contains(card));
                        }
                    }
                }
                let total = state.secret_deck.len()
                    + state
                        .players
                        .iter()
                        .map(|p| p.secret_objectives.len())
                        .sum::<usize>();
                assert_eq!(
                    total,
                    build_starting_decks(content(), POK, seed).secrets.len() - secret.len()
                );

                // Deterministic: the same seed and board give the same decks.
                let (mut again, _) = seated(n, seed);
                let board = plain_board(&again, seed);
                apply(&mut again, content(), POK, &board);
                assert_eq!(again, state);
            }
        }
    }
}
