//! How much of the measured opening clearance rests on space stations.
//!
//! Thunder's Edge adds four `SPACESTATION` unit holders -- Oluz Station, Revelation, The
//! Watchtower, Tsion Station -- and the corpus lists each in its system's `planets` array. The
//! engine already declines to treat `SPACESTATION` as a planet *trait*, because "control four
//! planets that each have the same trait" plainly does not mean four space stations. Everywhere
//! else they are planets: `landable_planets` offers them to an invasion, control transfers, and
//! `opening::planets_of` counts them toward both the three-planet and the three-system bar.
//!
//! Each is a one-planet system, so taking one is worth a planet *and* a system -- two thirds of
//! the systems requirement from a single landing.
//!
//! If space stations are not planets, every clearance figure that counted one is overstated. This
//! recomputes each seat's opening with space-station control removed and reports how many cleared
//! seats would drop below the bar.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_engine::choice::Decider;
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlanetId, PlayerId, SystemId};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TILE_SEED_OFFSET: u64 = 20_000_000;

fn argument(name: &str) -> Option<String> {
    let mut args = std::env::args();
    while let Some(argument) = args.next() {
        if argument == name {
            return args.next();
        }
    }
    None
}

fn refuse(reason: &str) -> ! {
    eprintln!("\nREFUSED: {reason}");
    std::process::exit(2);
}

#[derive(Default)]
struct Tally {
    seats: usize,
    cleared: usize,
    /// Cleared seats holding at least one space station.
    cleared_holding: usize,
    /// Cleared seats that would fall below the bar without their space stations.
    cleared_only_with: usize,
    /// Seat-games where a space station was on the board at all.
    station_on_board: usize,
}

#[expect(
    clippy::too_many_lines,
    reason = "one pass over the sampled games; the recomputation and its table belong together"
)]
fn main() {
    let bundle_path =
        argument("--bundle").unwrap_or_else(|| refuse("--bundle is required: this measures a policy"));
    let seeds: u64 = argument("--seeds").map_or(200, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--seeds expects a positive integer"))
    });
    let seed_base: u64 = argument("--seed-base").map_or(700_000_000, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--seed-base expects an unsigned integer"))
    });
    let temperature: f64 = argument("--temperature").map_or(0.25, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--temperature expects a number"))
    });

    ti4_tensor::configure_deterministic(20_260_828)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));

    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let vocabulary = loaded.vocabulary;
    let actor = std::rc::Rc::new(
        loaded
            .actor
            .inference_copy()
            .to_device(ti4_tensor::Device::Cpu),
    );
    let pool_path =
        argument("--map-pool").unwrap_or_else(|| "out/pools/full_np8_12_train.json".to_owned());
    let pool_bytes = ti4_sim::artifacts::read_and_verify_pool_role(
        std::path::Path::new(&pool_path),
        &[ti4_sim::artifacts::ArtifactRole::Train],
    )
    .unwrap_or_else(|error| refuse(&format!("{pool_path}: {error}")));
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(&pool_bytes))
            .unwrap_or_else(|error| refuse(&format!("parsing the pool: {error}"))),
    );
    let content = ContentStore::embedded();
    let factions: Vec<FactionId> = FACTIONS.iter().map(|name| FactionId::new(*name)).collect();

    // Every planet the corpus marks SPACESTATION, in the active scope.
    let stations: BTreeSet<String> = ti4_content::galaxy::all_planets(content, DEFAULT)
        .into_iter()
        .filter(|(_, planet)| planet.has_trait("SPACESTATION"))
        .map(|(name, _)| name.to_owned())
        .collect();
    println!("space-station reliance for {bundle_path}");
    println!("temperature {temperature}, seeds {seed_base}..+{seeds}");
    println!("stations in scope: {}", {
        let names: Vec<&str> = stations.iter().map(String::as_str).collect();
        names.join(", ")
    });

    let mut tallies: BTreeMap<String, Tally> = BTreeMap::new();

    for seed in seed_base..seed_base + seeds {
        for rotation in 0..FACTIONS.len() {
            let (_events, _picks, assignments, openings, final_state) =
                ti4_training::rollout::audit_game_with_deciders(
                    content,
                    &factions,
                    DEFAULT,
                    seed,
                    rotation,
                    ti4_training::rollout::Horizon {
                        rounds: 1,
                        steps: 200_000,
                    },
                    &ti4_training::rollout::OpeningMap::PythonPool {
                        pool: Arc::clone(&pool),
                        tile_seed_offset: TILE_SEED_OFFSET,
                    },
                    |seated, baselines| {
                        let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
                        for (index, (player, faction)) in seated.iter().enumerate() {
                            let row = ti4_mlp::FactionRow::of(faction.as_str())
                                .map_err(|error| format!("{player}: {error}"))?;
                            let baseline = baselines
                                .get(player)
                                .copied()
                                .ok_or_else(|| format!("{player} has no setup baseline"))?;
                            let stream = seed
                                .wrapping_mul(1_000_003)
                                .wrapping_add(u64::try_from(index).unwrap_or(0));
                            let (decider, _status) = ti4_mlp::bot::MlpBot::sharing(
                                &actor,
                                vocabulary.clone(),
                                row,
                                stream,
                            )
                            .from_setup(baseline)
                            .at_temperature(temperature)
                            .seat();
                            deciders.insert(player.clone(), decider);
                        }
                        Ok(deciders)
                    },
                )
                .unwrap_or_else(|error| refuse(&error));

            // Was a station even on this map?
            let station_present = final_state.board.keys().any(|system| {
                ti4_content::galaxy::system(content, system.as_str(), DEFAULT).is_some_and(|tile| {
                    tile.planets()
                        .into_iter()
                        .any(|planet| stations.contains(planet))
                })
            });

            for (player, opening) in &openings {
                let Some(faction) = assignments.get(player) else {
                    continue;
                };
                let entry = tallies.entry(faction.to_string()).or_default();
                entry.seats += 1;
                if station_present {
                    entry.station_on_board += 1;
                }
                if !opening.cleared() {
                    continue;
                }
                entry.cleared += 1;

                // What this seat controls, split on whether it is a station.
                let held: Vec<(&SystemId, &PlanetId)> = final_state.controlled_planets(player);
                let station_count = held
                    .iter()
                    .filter(|(_, planet)| stations.contains(planet.as_str()))
                    .count();
                if station_count == 0 {
                    continue;
                }
                entry.cleared_holding += 1;

                // Recompute the bar with stations struck out. `planets_gained` is a delta against
                // setup, and no home system carries a station, so subtracting the stations held
                // subtracts exactly what they contributed.
                let systems_without: BTreeSet<&SystemId> = held
                    .iter()
                    .filter(|(_, planet)| !stations.contains(planet.as_str()))
                    .map(|(system, _)| *system)
                    .collect();
                // Since the fix, `opening::planets_of` already excludes stations, so the bar and
                // the station-free recomputation must agree exactly. A non-zero count here is a
                // regression: it means a station is being counted somewhere in the bar again.
                let still_clears = opening.planets_gained >= opening.requirement.planets_gained
                    && systems_without.len() >= opening.requirement.systems
                    && opening.units_ok();
                if !still_clears {
                    entry.cleared_only_with += 1;
                }
            }
        }
    }

    let share = |part: usize, whole: usize| -> String {
        if whole == 0 {
            return "    --".to_owned();
        }
        #[expect(clippy::cast_precision_loss, reason = "counts are far below 2^53")]
        let value = 100.0 * part as f64 / whole as f64;
        format!("{value:5.1}%")
    };

    println!();
    println!(
        "  faction      seats  station on map   cleared  held a station  would fail without it"
    );
    let mut totals = Tally::default();
    for (faction, tally) in &tallies {
        totals.seats += tally.seats;
        totals.cleared += tally.cleared;
        totals.cleared_holding += tally.cleared_holding;
        totals.cleared_only_with += tally.cleared_only_with;
        totals.station_on_board += tally.station_on_board;
        println!(
            "  {:<10} {:>6}          {}    {}          {}                 {}",
            faction,
            tally.seats,
            share(tally.station_on_board, tally.seats),
            share(tally.cleared, tally.seats),
            share(tally.cleared_holding, tally.cleared),
            share(tally.cleared_only_with, tally.cleared),
        );
    }
    println!("  {:-<92}", "");
    println!(
        "  {:<10} {:>6}          {}    {}          {}                 {}",
        "table",
        totals.seats,
        share(totals.station_on_board, totals.seats),
        share(totals.cleared, totals.seats),
        share(totals.cleared_holding, totals.cleared),
        share(totals.cleared_only_with, totals.cleared),
    );
    println!();
    println!("  clearance as measured   {}", share(totals.cleared, totals.seats));
    println!(
        "  cleared seats where the bar counted a station   {} (must be 0.0%)",
        share(totals.cleared_only_with, totals.cleared)
    );
}
