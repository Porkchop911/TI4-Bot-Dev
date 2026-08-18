//! Does the rotation balance who drafts first, or only who sits where?
//!
//! Seats are assigned `players[seat] -> factions[(seat + rotation) % n]`, which is a **cyclic
//! rotation**, not a scramble. That balances two things and silently fails to balance a third:
//!
//! * each faction occupies each seat exactly once  -- balanced
//! * each faction is speaker exactly once          -- balanced
//! * each faction drafts before each other faction -- NOT balanced
//!
//! Under a cyclic rotation the offset between any two factions is fixed, so their relative draft
//! order is decided entirely by where the cut falls. For factions at cyclic distance d, the first
//! drafts before the second in (n-d)/n of rotations -- 83% at d=1, 50% only at d=3, 17% at d=5.
//! A true scramble would give 50% for every pair.
//!
//! This matters wherever two factions want the same strategy card, because the loser takes a
//! fallback and the fallback may be worthless.
use std::collections::BTreeMap;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;
use ti4_model::id::FactionId;
use ti4_policy::learned::{Profile, blank_explicit_profile};
use ti4_training::rollout::{Horizon, play_rotated_save54_pool_batch};

fn main() {
    let store = ContentStore::embedded();
    let names = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
    let factions: Vec<FactionId> = names.iter().map(|n| FactionId::new(*n)).collect();
    let pool = Arc::new(
        ti4_sim::MapPool::load(std::path::Path::new("out/pools/save52_e400_holdout.json.gz"))
            .expect("pool"),
    );
    // Draft order is a property of seating, not of the policy, so blank weights are enough.
    let profiles: BTreeMap<FactionId, Profile> = factions
        .iter()
        .map(|f| (f.clone(), blank_explicit_profile(f.as_str())))
        .collect();
    let seeds: Vec<u64> = (98_000_000..98_000_030).collect();
    let games = play_rotated_save54_pool_batch(
        store,
        &factions,
        &profiles,
        FULL,
        &seeds,
        Horizon::opening(),
        ti4_engine::opening::DEFAULT_REQUIREMENT,
        Arc::clone(&pool),
        20_000_000,
    );

    let mut before: BTreeMap<(String, String), (usize, usize)> = BTreeMap::new();
    for game in &games {
        let mut position: BTreeMap<String, usize> = BTreeMap::new();
        for (index, seat) in game.seats.iter().enumerate() {
            // Seats arrive in seating order, which is draft order in the strategy phase.
            position.insert(seat.faction.to_string(), index);
        }
        for a in names {
            for b in names {
                if a == b {
                    continue;
                }
                let (Some(pa), Some(pb)) = (position.get(a), position.get(b)) else {
                    continue;
                };
                let row = before
                    .entry((a.to_owned(), b.to_owned()))
                    .or_insert((0, 0));
                row.0 += 1;
                row.1 += usize::from(pa < pb);
            }
        }
    }

    println!("{} games. Row drafts BEFORE column, as a percentage.", games.len());
    println!("A scramble would put every off-diagonal cell at 50.0%.\n");
    print!("{:<9}", "");
    for b in names {
        print!("{b:>9}");
    }
    println!();
    for a in names {
        print!("{a:<9}");
        for b in names {
            if a == b {
                print!("{:>9}", "-");
                continue;
            }
            let (n, wins) = before
                .get(&(a.to_owned(), b.to_owned()))
                .copied()
                .unwrap_or((0, 0));
            #[expect(clippy::cast_precision_loss, reason = "small counts")]
            let share = if n == 0 {
                0.0
            } else {
                100.0 * wins as f64 / n as f64
            };
            print!("{share:>8.1}%");
        }
        println!();
    }
    println!("\nsol vs jolnar is the contested pair: they both want Warfare.");
}
