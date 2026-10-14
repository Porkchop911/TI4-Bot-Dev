//! The catalogue of map choices is in `ti4-host`; the server adds the per-process secret that
//! the numbers of the card pictures are mixed with.

pub use ti4_host::maps::catalog::*;

use std::sync::OnceLock;

/// A repeatable seed for the picture of variant `variant` of a card. Mixed with a per-process
/// secret so the numbers cannot be walked to learn anything about a real game's seed.
#[must_use]
pub fn variant_seed(choice: &MapChoice, player_count: usize, variant: u32) -> u64 {
    static SALT: OnceLock<u64> = OnceLock::new();
    let salt = *SALT.get_or_init(rand::random);
    variant_seed_salted(salt, choice, player_count, variant)
}
