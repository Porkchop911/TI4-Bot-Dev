//! Does every revealed public objective emit a signal that links it to the decisions advancing it?
//!
//! Two separate questions per objective, and they fail differently:
//!
//! 1. **Is it represented at all?** `revealed_objective_progress` drops a card when neither
//!    `counting_progress` nor `remaining_position_progress` answers. A dropped card emits no
//!    `objective-need`, no `objective-progress`, no `objective-count` -- fully invisible.
//! 2. **Can any option be linked to it?** The option-side feature is a difference of
//!    `revealed_objective_progress_gaining`, whose contract imagines **planet control only**.
//!    A requirement that does not move when the seat is handed planets cancels on both sides,
//!    so no option ever carries a gain for it.
//!
//! The test for (2) is deliberately maximal: hand the seat **every planet it does not control**.
//! If a card's progress will not move under that, no realistic option can move it either.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, DEFAULT};
use ti4_model::id::{FactionId, ObjectiveId, PlanetId, PlayerId};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TILE_SEED_OFFSET: u64 = 20_000_000;

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn refuse(reason: &str) -> ! {
    eprintln!("\nREFUSED: {reason}");
    std::process::exit(2);
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_owned()
    } else {
        s.chars().take(n).collect()
    }
}

fn main() {
    let content = ContentStore::embedded();
    let seed: u64 = argument("--seed").map_or(920_500_000, |v| {
        v.parse().unwrap_or_else(|_| refuse("--seed expects a u64"))
    });
    let pool_path = argument("--map-pool").unwrap_or_else(|| refuse("--map-pool is required"));

    let pool_bytes = ti4_sim::artifacts::read_and_verify_pool_role(
        std::path::Path::new(&pool_path),
        &[
            ti4_sim::artifacts::ArtifactRole::Train,
            ti4_sim::artifacts::ArtifactRole::Validation,
        ],
    )
    .unwrap_or_else(|error| refuse(&format!("{pool_path}: {error}")));
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(&pool_bytes))
            .unwrap_or_else(|error| refuse(&format!("parsing the pool: {error}"))),
    );

    let players: Vec<PlayerId> = (0..6).map(|i| PlayerId::new(format!("seat{i}"))).collect();
    let assignments: BTreeMap<PlayerId, FactionId> = players
        .iter()
        .enumerate()
        .map(|(i, p)| (p.clone(), FactionId::new(FACTIONS[i])))
        .collect();

    let mut state = ti4_engine::setup::start_game_seeded(content, &players, DEFAULT, None, seed)
        .unwrap_or_else(|e| refuse(&format!("setup: {e}")));
    for (player, faction) in &assignments {
        if let Some(seat) = state.player_mut(player) {
            seat.faction = faction.clone();
        }
    }

    let homes: Vec<String> = players
        .iter()
        .map(|p| {
            let faction = &assignments[p];
            ti4_content::factions::get(content, faction.as_str())
                .and_then(|r| r.home_system())
                .map(str::to_owned)
                .unwrap_or_else(|| refuse(&format!("{faction} has no home system")))
        })
        .collect();
    let borrowed: Vec<&str> = homes.iter().map(String::as_str).collect();
    let galaxy = pool
        .galaxy(
            content,
            DEFAULT,
            seed.wrapping_add(TILE_SEED_OFFSET),
            &borrowed,
        )
        .unwrap_or_else(|e| refuse(&format!("galaxy: {e}")));
    for (player, faction) in &assignments {
        ti4_engine::seating::deploy(&mut state, content, player, faction, DEFAULT)
            .unwrap_or_else(|e| refuse(&format!("deploy: {e}")));
    }

    // Advance the board before auditing. At deployment a seat has no neighbours and the map
    // carries no attachments, so requirements that read the *real* board to find comparison
    // targets (weaker_neighbours) or planet attachments would report a flat zero on both sides
    // and be misread as invisible. Playing a few rounds removes that artifact.
    let rounds: u32 = argument("--rounds").map_or(3, |v| {
        v.parse()
            .unwrap_or_else(|_| refuse("--rounds expects a u32"))
    });
    if rounds > 0 {
        let game_galaxy = pool
            .galaxy(
                content,
                DEFAULT,
                seed.wrapping_add(TILE_SEED_OFFSET),
                &borrowed,
            )
            .unwrap_or_else(|e| refuse(&format!("galaxy: {e}")));
        let mut table =
            ti4_engine::choice::Table::with_default(Box::new(ti4_engine::choice::SeededRandom::new(
                seed,
            )));
        for player in &players {
            table.seat(
                player.clone(),
                Box::new(ti4_engine::choice::FirstOption) as Box<dyn ti4_engine::choice::Decider>,
            );
        }
        let mut game = ti4_engine::Game::with_table(state, content, table)
            .with_sources(DEFAULT)
            .with_galaxy(game_galaxy);
        let target = game.state.round.saturating_add(rounds);
        let mut steps = 0usize;
        while game.state.round < target && !game.state.finished && steps < 200_000 {
            if game.step().error.is_some() {
                break;
            }
            steps += 1;
        }
        eprintln!(
            "  advanced to round {} in {steps} steps",
            game.state.round
        );
        state = game.state.clone();
    }

    // Every public objective in the corpus, both stages.
    let all_public: Vec<(String, String, i64)> = content
        .records(ContentType::PublicObjectives)
        .iter()
        .filter(|r| r.in_sources(DEFAULT))
        .map(|r| {
            (
                r.text("alias").unwrap_or("?").to_owned(),
                r.text("name").unwrap_or("?").to_owned(),
                r.int("points").unwrap_or(0),
            )
        })
        .collect();

    // Reveal all of them at once, so one pass covers the whole corpus.
    state.revealed_objectives = all_public
        .iter()
        .map(|(alias, _, _)| ObjectiveId::new(alias.clone()))
        .collect();

    let player = PlayerId::new("seat0");
    let seen = ti4_engine::choice::Observed::new(&state, content, DEFAULT, Some(&galaxy));

    let controlled: BTreeSet<String> = state
        .controlled_planets(&player)
        .into_iter()
        .map(|(_, p)| p.to_string())
        .collect();
    let uncontrolled: Vec<PlanetId> = ti4_content::galaxy::all_planets(content, DEFAULT)
        .keys()
        .filter(|name| !controlled.contains(**name))
        .map(|name| PlanetId::new((*name).to_owned()))
        .collect();

    // Maximal counterfactual on every axis an option can change: if a card will not move under
    // this, nothing an option can do will move it either.
    let all_systems: Vec<String> = ti4_content::galaxy::all_systems(content, DEFAULT)
        .keys()
        .map(|s| (*s).to_owned())
        .collect();
    let all_technologies: Vec<String> = content
        .records(ContentType::Technologies)
        .iter()
        .filter(|r| r.in_sources(DEFAULT))
        .filter_map(|r| r.text("alias").map(ToOwned::to_owned))
        .collect();
    let all_structures: Vec<(String, String)> = ti4_content::galaxy::all_systems(content, DEFAULT)
        .iter()
        .flat_map(|(sid, sys)| {
            sys.planets()
                .into_iter()
                .map(move |p| ((*sid).to_owned(), p.to_owned()))
        })
        .collect();

    let imagined = ti4_engine::objectives::Imagined {
        planets: &uncontrolled,
        systems: &all_systems,
        technologies: &all_technologies,
        structures: &all_structures,
    };

    let before = seen.revealed_objective_progress(&player);
    let after = seen.revealed_objective_progress_imagining(&player, &imagined);

    let index = |v: &[ti4_engine::objectives::CardProgress]| {
        v.iter()
            .map(|c| {
                (
                    c.alias.clone(),
                    (c.family_token.clone(), c.have, c.threshold),
                )
            })
            .collect::<BTreeMap<_, _>>()
    };
    let b = index(&before);
    let a = index(&after);

    println!("objective signal audit");
    println!(
        "  seed {seed} | {} public objectives | {} planets imagined | seat0 controls {}\n",
        all_public.len(),
        uncontrolled.len(),
        controlled.len()
    );
    println!(
        "{:<22} {:<32} {:>2}  {:<30} {:>6} {:>7}  {}",
        "alias", "name", "pt", "family:threshold", "have", "gained", "verdict"
    );

    let mut linked: Vec<&str> = Vec::new();
    let mut invisible: Vec<(&str, String)> = Vec::new();
    let mut unrepresented: Vec<&str> = Vec::new();
    let mut saturated_rows: Vec<&str> = Vec::new();

    for (alias, name, points) in &all_public {
        match (b.get(alias), a.get(alias)) {
            (None, _) => {
                unrepresented.push(alias);
                println!(
                    "{alias:<22} {:<32} {points:>2}  {:<30} {:>6} {:>7}  NO PROGRESS RECORD",
                    truncate(name, 32),
                    "-",
                    "-",
                    "-"
                );
            }
            (Some((family, have, threshold)), Some((_, gained, _))) => {
                let moves = (gained - have).abs() > 1e-9;
                // Already at the bar is not the same as unreachable: there is simply no headroom
                // left for a counterfactual to show. Calling that "invisible" reported Erect a
                // Monument as broken while Found a Golden Age, the same family with a higher bar,
                // moved correctly.
                let saturated = !moves && *have >= *threshold - 1e-9;
                let ft = format!("{family}:{threshold:.0}");
                let verdict = if moves {
                    linked.push(alias);
                    "linked"
                } else if saturated {
                    saturated_rows.push(alias);
                    "already at the bar"
                } else {
                    invisible.push((alias, ft.clone()));
                    "INVISIBLE"
                };
                println!(
                    "{alias:<22} {:<32} {points:>2}  {ft:<30} {have:>6.1} {gained:>7.1}  {verdict}",
                    truncate(name, 32),
                );
            }
            (Some(_), None) => {
                println!(
                    "{alias:<22} {:<32} {points:>2}  vanished under the counterfactual",
                    truncate(name, 32)
                );
            }
        }
    }

    println!("\n=== summary ===");
    println!("linked (an option can carry a gain): {:>3}", linked.len());
    println!("INVISIBLE to every option:           {:>3}", invisible.len());
    println!("already at the bar (no headroom):    {:>3}", saturated_rows.len());
    println!("no progress record at all:           {:>3}", unrepresented.len());

    if !invisible.is_empty() {
        println!("\nINVISIBLE -- requirement is known, but no option is ever linked to it:");
        let mut by_family: BTreeMap<String, Vec<&str>> = BTreeMap::new();
        for (alias, ft) in &invisible {
            let family = ft.split(':').next().unwrap_or(ft).to_owned();
            by_family.entry(family).or_default().push(alias);
        }
        for (family, aliases) in by_family {
            println!("  {family:<28} {}", aliases.join(", "));
        }
    }
    if !unrepresented.is_empty() {
        println!("\nNO RECORD -- emits no objective-need/progress/count at all:");
        for alias in &unrepresented {
            println!("  {alias}");
        }
    }
}
