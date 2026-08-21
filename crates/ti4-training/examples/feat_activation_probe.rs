//! J1 instrumentation (M06-024 acceptance): three M06 scoring mechanisms have never been observed
//! firing in holdout play — the anti-barrage pause (Fight with Precision), Betray a Friend's note
//! issuer, and Become a Martyr's home-loss event. This probe plays 150 games with the r6 champions
//! and counts, per feat, how many times it was *recorded* against a seat versus how many times its
//! secret objective was actually scored. A non-zero record count with zero scores localises the
//! problem to eligibility or window placement; a zero record count closes the question as rarity at
//! this horizon.

use std::collections::BTreeMap;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;
use ti4_model::id::FactionId;
use ti4_model::state::{Feat, GameState};
use ti4_policy::learned::Profile;
use ti4_training::rollout::{Horizon, OpeningMap, audit_game};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TILE_SEED_OFFSET: u64 = 20_000_000;

/// The three feats J1 asks about, and the secret objective each one scores.
const PROBED: [(Feat, &str); 3] = [
    (Feat::BarrageTookTheLastFighters, "fwp"),
    (Feat::WonAgainstANoteHolder, "baf"),
    (Feat::LostAHomePlanet, "bam"),
];

/// Per-feat record/score counters for the probed secrets.
struct Tally {
    /// Total feat records across all seats and games.
    recorded: BTreeMap<Feat, usize>,
    /// Records by seats that still held the matching secret at game end — these three secrets only
    /// leave a hand by scoring (or Imperial's return-to-deck), so this is the alignment count.
    recorded_by_holders: BTreeMap<Feat, usize>,
    /// Secret objectives actually awarded.
    scored: BTreeMap<&'static str, usize>,
    /// Seats still holding each probed secret at game end (never scored it).
    held_at_end: BTreeMap<&'static str, usize>,
}

impl Tally {
    const fn new() -> Self {
        Self {
            recorded: BTreeMap::new(),
            recorded_by_holders: BTreeMap::new(),
            scored: BTreeMap::new(),
            held_at_end: BTreeMap::new(),
        }
    }

    /// Fold one finished game into the counters.
    fn observe(&mut self, state: &GameState) -> (i32, usize) {
        let mut vp = 0;
        for seat in &state.players {
            vp += seat.victory_points;
            for (_, alias) in PROBED {
                if seat
                    .secret_objectives
                    .iter()
                    .any(|held| held.as_str() == alias)
                {
                    *self.held_at_end.entry(alias).or_insert(0) += 1;
                }
            }
            for (feat, _occurrence) in &seat.event_feats {
                if let Some((probed, alias)) = PROBED.iter().find(|(p, _)| p == feat) {
                    *self.recorded.entry(*probed).or_insert(0) += 1;
                    if seat
                        .secret_objectives
                        .iter()
                        .any(|held| held.as_str() == *alias)
                    {
                        *self.recorded_by_holders.entry(*probed).or_insert(0) += 1;
                    }
                }
            }
        }
        for objectives in state.scored_objectives.values() {
            for objective in objectives {
                if let Some((_, alias)) = PROBED.iter().find(|(_, a)| *a == objective.as_str()) {
                    *self.scored.entry(alias).or_insert(0) += 1;
                }
            }
        }
        (vp, state.players.len())
    }

    fn report(&self, games: usize, seeds: u64, checkpoint: &str) {
        println!(
            "J1 feat activation probe: {games} games ({seeds} seeds x 6 rotations), \
             r6 champions at {checkpoint}"
        );
        for (feat, alias) in PROBED {
            let records = self.recorded.get(&feat).copied().unwrap_or(0);
            let with_card = self.recorded_by_holders.get(&feat).copied().unwrap_or(0);
            let held = self.held_at_end.get(alias).copied().unwrap_or(0);
            let scores = self.scored.get(alias).copied().unwrap_or(0);
            println!(
                "{feat:?}: recorded {records} time(s), of which {with_card} by seats still \
                 holding secret {alias}; {held} seat(s) ended the game holding {alias}; scored \
                 {scores} time(s)"
            );
        }
    }
}

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn main() {
    let content = ContentStore::embedded();
    let checkpoint =
        argument("--checkpoint").unwrap_or_else(|| "out/stage2_r6/final10000.json".to_owned());
    let seeds: u64 = argument("--seeds")
        .and_then(|v| v.parse().ok())
        .unwrap_or(25);
    let pool_path =
        argument("--map-pool").unwrap_or_else(|| "out/pools/full_np8_12_holdout.json".to_owned());

    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&checkpoint).expect("read")).expect("parse");
    let loaded: BTreeMap<String, Profile> =
        serde_json::from_value(document["profiles"].clone()).expect("profiles");
    let profiles: BTreeMap<FactionId, Profile> = loaded
        .into_iter()
        .map(|(faction, profile)| (FactionId::new(faction), profile))
        .collect();
    let pool = Arc::new(ti4_sim::MapPool::load(std::path::Path::new(&pool_path)).expect("pool"));
    let map = OpeningMap::PythonPool {
        pool: Arc::clone(&pool),
        tile_seed_offset: TILE_SEED_OFFSET,
    };

    let mut tally = Tally::new();
    let mut games = 0usize;
    let mut vp_total = 0i32;
    let mut seats_total = 0usize;
    for seed in 98_000_000..98_000_000 + seeds {
        for rotation in 0..FACTIONS.len() {
            // Pre-rotated slice: audit_game seats with rotation zero, so the rotation lives here.
            let rotated: Vec<FactionId> = (0..FACTIONS.len())
                .map(|index| FactionId::new(FACTIONS[(index + rotation) % FACTIONS.len()]))
                .collect();
            let (_events, state) = audit_game(
                content,
                &rotated,
                &profiles,
                FULL,
                seed,
                Horizon::rounds(4),
                &map,
            );
            games += 1;
            let (vp, seats) = tally.observe(&state);
            vp_total += vp;
            seats_total += seats;
        }
    }

    tally.report(games, seeds, &checkpoint);
    if seats_total > 0 {
        println!(
            "mean VP per seat: {:.3}",
            f64::from(vp_total) / f64::from(u32::try_from(seats_total).expect("fits"))
        );
    }
}
