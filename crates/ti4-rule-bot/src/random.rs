//! The random pick under the rule bot: what a seat answers where the bot has no opinion.
//!
//! Every pick is seeded from (game seed, seat place, decision index), so the same position gets the
//! same answer in the server and in the wasm build. The seat place is the 0-based index of the seat
//! in the seat order of the game (`GameState::players`), not its id: the ids of a game differ
//! between the hosts (the server makes random ones), its seat order does not.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use ti4_engine::choice::{Choice, ChoiceOption};

fn mix(mut x: u64) -> u64 {
    // splitmix64 finaliser
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// The RNG for one decision: a pure function of (game seed, seat place, decision index). `seat`
/// is the place of the deciding seat in the seat order of the game, from 0.
#[must_use]
pub fn decision_rng(game_seed: u64, seat: usize, index: usize) -> ChaCha8Rng {
    let seed = mix(game_seed
        ^ mix(seat as u64 ^ 0x5EA7_0000_0000_0000)
        ^ mix(index as u64 ^ 0xA5A5_5A5A_0000_0000));
    ChaCha8Rng::seed_from_u64(seed)
}

/// Relative likelihood of one option. The harness `random` policy is uniform over the enabled
/// controls, so anything unmatched weighs 1. The exception mirrors its loop guard: opening a
/// transaction (its `trade-opt-` / `propose-trade-btn` controls weigh 0.1 in the steering table)
/// is rarely worth it and, picked as often as a move, drags a table into endless bargaining.
#[must_use]
pub fn weight(option: &ChoiceOption) -> f64 {
    if option.kind == ti4_engine::transactions::OPEN_KIND {
        0.1
    } else {
        1.0
    }
}

/// Pick one option index by weight with `rng`. The choice must have at least one option.
#[must_use]
pub fn pick_index(choice: &Choice, rng: &mut ChaCha8Rng) -> usize {
    let weights: Vec<f64> = choice.options.iter().map(weight).collect();
    let total: f64 = weights.iter().sum();
    let mut roll = rng.random_range(0.0..total);
    for (index, w) in weights.iter().enumerate() {
        if roll < *w {
            return index;
        }
        roll -= w;
    }
    weights.len() - 1
}
