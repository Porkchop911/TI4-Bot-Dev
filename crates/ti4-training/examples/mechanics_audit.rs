//! Does every important TI4 mechanic actually happen in a simulated game?
//!
//! The existing wiring checks assert that `game.rs` still contains a call to each subsystem. That
//! is a check on the *source*, and it passed for the whole time the agenda phase was unreachable:
//! the driver called `agenda_effects::resolve`, and no game ever ran an agenda, because nothing
//! lifted the custodians token. Wiring is not reachability.
//!
//! This drives real games and asks, of each mechanic, whether it was exercised — from the events
//! the engine emitted and from the state the game ended in. A mechanic that is implemented,
//! wired, tested, and never reached in play is reported as NEVER, because that is what it is worth
//! to a learner.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;
use ti4_model::id::FactionId;
use ti4_model::state::GameState;
use ti4_policy::learned::Profile;
use ti4_training::rollout::{Horizon, OpeningMap, audit_game};

/// What counts as evidence that a mechanic happened.
enum Evidence {
    /// One of these events was emitted.
    Event(&'static [&'static str]),
    /// A predicate over the state the game ended in.
    State(fn(&GameState) -> bool),
}

/// The mechanics a game of TI4 is made of, and how to tell each one ran.
///
/// Grouped as the rules are, so a gap reads as "the agenda phase is missing" rather than as a
/// list of absent event names.
fn mechanics() -> Vec<(&'static str, &'static str, Evidence)> {
    vec![
        // -- round structure ------------------------------------------------------------------
        (
            "structure",
            "round begins",
            Evidence::Event(&["ROUND_BEGAN"]),
        ),
        (
            "structure",
            "strategy phase: cards chosen",
            Evidence::Event(&["STRATEGY_CARD_CHOSEN"]),
        ),
        (
            "structure",
            "action phase",
            Evidence::Event(&["ACTION_PHASE_BEGAN"]),
        ),
        (
            "structure",
            "status phase",
            Evidence::Event(&["STATUS_PHASE_BEGAN"]),
        ),
        (
            "structure",
            "status: scoring window",
            Evidence::Event(&["STATUS_SCORING_BEGAN"]),
        ),
        (
            "structure",
            "status: bookkeeping",
            Evidence::Event(&["STATUS_BOOKKEEPING_RESOLVED"]),
        ),
        (
            "structure",
            "agenda phase",
            Evidence::Event(&["AGENDA_PHASE_BEGAN"]),
        ),
        (
            "structure",
            "passing",
            Evidence::Event(&["PLAYER_PASSED", "TURN_PASSED"]),
        ),
        ("structure", "game end", Evidence::Event(&["GAME_FINISHED"])),
        // -- the tactical action --------------------------------------------------------------
        (
            "tactical",
            "tactical action",
            Evidence::Event(&["TACTICAL_ACTION_BEGAN"]),
        ),
        (
            "tactical",
            "system activation",
            Evidence::Event(&["SYSTEM_ACTIVATED"]),
        ),
        ("tactical", "movement", Evidence::Event(&["SHIP_MOVED"])),
        (
            "tactical",
            "space combat",
            Evidence::Event(&["SPACE_COMBAT_STARTED"]),
        ),
        (
            "tactical",
            "combat rounds fought",
            Evidence::Event(&["COMBAT_ROUND_STARTED"]),
        ),
        (
            "tactical",
            "space combat resolved",
            Evidence::Event(&["SPACE_COMBAT_RESOLVED"]),
        ),
        ("tactical", "invasion", Evidence::Event(&["INVASION_BEGAN"])),
        (
            "tactical",
            "planet control taken",
            Evidence::Event(&["PLANET_CONTROL_GAINED"]),
        ),
        (
            "tactical",
            "production",
            Evidence::Event(&["PRODUCTION_RESOLVED", "PRODUCTION_USED"]),
        ),
        (
            "tactical",
            "gravity rift losses",
            Evidence::Event(&["SHIP_LOST_TO_GRAVITY_RIFT"]),
        ),
        // -- strategy cards -------------------------------------------------------------------
        (
            "strategy",
            "strategic action",
            Evidence::Event(&["STRATEGIC_ACTION_BEGAN"]),
        ),
        (
            "strategy",
            "secondary followed",
            Evidence::Event(&["STRATEGY_SECONDARY_FOLLOWED"]),
        ),
        (
            "strategy",
            "secondary declined",
            Evidence::Event(&["STRATEGY_SECONDARY_DECLINED"]),
        ),
        (
            "strategy",
            "command tokens gained",
            Evidence::Event(&["COMMAND_TOKENS_GAINED", "COMMAND_TOKEN_GAINED"]),
        ),
        // -- economy --------------------------------------------------------------------------
        (
            "economy",
            "transaction opened",
            Evidence::Event(&["TRANSACTION_OPENED", "TRANSACTION_OFFERED"]),
        ),
        (
            "economy",
            "transaction agreed",
            Evidence::Event(&["TRANSACTION"]),
        ),
        (
            "economy",
            "trade goods held",
            Evidence::State(|s| s.players.iter().any(|p| p.trade_goods > 0)),
        ),
        (
            "economy",
            "commodities held",
            Evidence::State(|s| s.players.iter().any(|p| p.commodities > 0)),
        ),
        // -- cards ----------------------------------------------------------------------------
        (
            "cards",
            "action card played",
            Evidence::Event(&["ACTION_CARD_PLAYED", "CARD_PLAYED"]),
        ),
        (
            "cards",
            "component action",
            Evidence::Event(&["COMPONENT_ACTION_RESOLVED"]),
        ),
        (
            "cards",
            "promissory: ceasefire used",
            Evidence::Event(&["CEASEFIRE_USED"]),
        ),
        (
            "cards",
            "promissory: support lent",
            Evidence::State(|s| !s.support_holders.is_empty()),
        ),
        // -- agenda ---------------------------------------------------------------------------
        (
            "agenda",
            "agenda revealed",
            Evidence::Event(&["AGENDA_REVEALED"]),
        ),
        ("agenda", "votes cast", Evidence::Event(&["VOTES_CAST"])),
        (
            "agenda",
            "agenda resolved",
            Evidence::Event(&["AGENDA_RESOLVED"]),
        ),
        (
            "agenda",
            "law in play",
            Evidence::State(|s| !s.laws.is_empty()),
        ),
        // -- progression ----------------------------------------------------------------------
        (
            "progression",
            "custodians lifted",
            Evidence::State(|s| s.custodians_removed),
        ),
        (
            "progression",
            "technology researched",
            Evidence::State(|s| s.players.iter().any(|p| p.technologies.len() > 3)),
        ),
        (
            "progression",
            "unit upgrade researched",
            Evidence::State(|s| {
                s.players.iter().any(|p| {
                    p.technologies.iter().any(|t| {
                        let id = t.to_string();
                        id.contains('2') || id.ends_with("ii")
                    })
                })
            }),
        ),
        (
            "progression",
            "objective scored",
            Evidence::Event(&["OBJECTIVE_SCORED"]),
        ),
        (
            "progression",
            "relic acquired",
            Evidence::State(|s| s.players.iter().any(|p| !p.relics.is_empty())),
        ),
        (
            "progression",
            "relic fragments found",
            Evidence::State(|s| {
                s.players
                    .iter()
                    .any(|p| p.relic_fragments.values().any(|n| *n > 0))
            }),
        ),
        (
            "progression",
            "leaders unlocked",
            Evidence::State(|s| s.players.iter().any(|p| !p.leaders.is_empty())),
        ),
        (
            "progression",
            "exploration attachments",
            Evidence::State(|s| !s.planet_attachments.is_empty()),
        ),
        (
            "progression",
            "secret objectives scored",
            Evidence::State(|s| s.scored_objectives.values().any(|set| !set.is_empty())),
        ),
        (
            "progression",
            "victory points scored",
            Evidence::State(|s| s.players.iter().any(|p| p.victory_points > 0)),
        ),
    ]
}

fn main() {
    let store = ContentStore::embedded();
    let factions: Vec<FactionId> = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"]
        .iter()
        .map(|name| FactionId::new(*name))
        .collect();
    ti4_training::rollout::set_seat_scramble(true);
    let rounds: u32 = std::env::args()
        .find_map(|a| a.strip_prefix("--rounds=").and_then(|v| v.parse().ok()))
        .unwrap_or(8);
    let games: u64 = std::env::args()
        .find_map(|a| a.strip_prefix("--games=").and_then(|v| v.parse().ok()))
        .unwrap_or(30);
    let path = std::env::args()
        .find(|a| a.ends_with(".json"))
        .unwrap_or_else(|| "out/stage2_ppo/s0.json".to_owned());
    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("parse");
    let loaded: BTreeMap<String, Profile> =
        serde_json::from_value(document["profiles"].clone()).expect("profiles");
    let profiles: BTreeMap<FactionId, Profile> = factions
        .iter()
        .filter_map(|f| loaded.get(f.as_str()).map(|p| (f.clone(), p.clone())))
        .collect();
    let pool = Arc::new(
        ti4_sim::MapPool::load(std::path::Path::new(
            "D:/Projects/ti4-engine/data/map_pools/save52_e400_n8192.json.gz",
        ))
        .expect("pool"),
    );
    let map = OpeningMap::PythonPool {
        pool,
        tile_seed_offset: 20_000_000,
    };

    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut hits: BTreeMap<&'static str, usize> = BTreeMap::new();
    let list = mechanics();
    for seed in 98_000_000..98_000_000 + games {
        let (events, state) = audit_game(
            store,
            &factions,
            &profiles,
            FULL,
            seed,
            Horizon::rounds(rounds),
            &map,
        );
        // Per game. An earlier version extended one shared set and asked whether the event had
        // ever been seen, which reported "the fraction of games at or after the first occurrence"
        // -- so the agenda phase read 97% while the custodians token it is gated on read 67%.
        // The inconsistency between an event count and a state count is what gave it away.
        let this_game: BTreeSet<String> = events.iter().cloned().collect();
        seen.extend(events);
        for (_, name, evidence) in &list {
            let fired = match evidence {
                Evidence::Event(names) => names.iter().any(|n| {
                    this_game
                        .iter()
                        .any(|event| event == n || event.starts_with(&format!("{n}:")))
                }),
                Evidence::State(check) => check(&state),
            };
            if fired {
                *hits.entry(name).or_default() += 1;
            }
        }
    }

    println!("{games} games at {rounds} rounds, policy {path}\n");
    let mut group = "";
    let (mut ok, mut missing) = (0, 0);
    for (section, name, _) in &list {
        if *section != group {
            group = section;
            println!("-- {group}");
        }
        let count = hits.get(name).copied().unwrap_or(0);
        if count == 0 {
            missing += 1;
            println!("   {name:<34} NEVER");
        } else {
            ok += 1;
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let share = 100.0 * count as f64 / games as f64;
            println!("   {name:<34} {share:>5.0}% of games");
        }
    }
    println!("\n{ok} mechanics exercised, {missing} never reached");

    // Why the one that never fires, does not. A purge needs three fragments of one trait, with
    // frontier fragments making up the shortfall, so the question is whether anyone ever holds
    // that many rather than whether the option is wired.
    println!(
        "
relic fragments held at the end, per seat:"
    );
    let mut hist: BTreeMap<i32, usize> = BTreeMap::new();
    let mut best = 0;
    let mut ever_purgeable = 0usize;
    for seed in 98_000_000..98_000_000 + games {
        let (_, state) = audit_game(
            store,
            &factions,
            &profiles,
            FULL,
            seed,
            Horizon::rounds(rounds),
            &map,
        );
        for seat in &state.players {
            let frontier = seat.relic_fragments.get("FRONTIER").copied().unwrap_or(0);
            let total: i32 = seat.relic_fragments.values().sum();
            *hist.entry(total).or_default() += 1;
            best = best.max(total);
            let purgeable = seat
                .relic_fragments
                .iter()
                .filter(|(name, _)| name.as_str() != "FRONTIER")
                .any(|(_, held)| *held + frontier >= 3);
            ever_purgeable += usize::from(purgeable);
        }
    }
    for (count, seats) in &hist {
        println!("   {count} fragments  {seats} seats");
    }
    println!("   most held by any seat: {best}, purge available to {ever_purgeable} seats");
    let mut traits: BTreeMap<String, i32> = BTreeMap::new();
    for seed in 98_000_000..98_000_000 + games {
        let (_, state) = audit_game(
            store,
            &factions,
            &profiles,
            FULL,
            seed,
            Horizon::rounds(rounds),
            &map,
        );
        for seat in &state.players {
            for (name, held) in &seat.relic_fragments {
                *traits.entry(name.clone()).or_default() += *held;
            }
        }
    }
    println!("   fragments by trait: {traits:?}");

    println!("\nevents the engine emitted at least once:");
    println!("  {}", seen.iter().cloned().collect::<Vec<_>>().join(" "));
}
