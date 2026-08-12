//! Component limitations (LRR 31.4): you cannot field more plastic than you own.
//!
//! Ported from the oracle's `engine/supply.py`, which records what its absence looked like:
//! across eight measured games a single player held **18 carriers against the four in the box,
//! 14 PDS against six, and 10 dreadnoughts against five**. Every bot was obeying its scoring
//! perfectly while doing something impossible, which is why no amount of scoring analysis found
//! it — a player spotted it in one screenshot of a live table.
//!
//! **Fighters and infantry are not capped, and that is a rule rather than an omission.** The box
//! ships cardboard fighter and infantry tokens in 1× and 3× denominations exactly so those two
//! are never limited by plastic, and the rulebook says to substitute if you run out of tokens
//! too. The oracle records capping them as its own first mistake: "58 infantry against the twelve
//! in the box" was its headline, and 58 infantry is legal.
//!
//! **Counts are per player and keyed by base type.** `sol_carrier2` and `carrier` are the same
//! four pieces of plastic — an upgrade swaps the card, not the model — so the cap is read through
//! the unit's base type rather than its id, or a faction upgrade would silently double a fleet.
//!
//! 31.4's escape hatch, removing one of your own from the board to place it elsewhere, is
//! deliberately not modelled here. It is a real option and belongs in the production step as a
//! choice; [`remaining`] answers only "how many may still be placed".

use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlayerId, UnitTypeId};
use ti4_model::state::GameState;

/// Anything not listed here is uncapped, which is how fighters and infantry pass through.
const UNCAPPED: i64 = 99;

/// Plastic per player, fourth edition plus Prophecy of Kings, keyed by base type.
#[must_use]
pub fn plastic(base_type: &str) -> Option<i64> {
    let count = match base_type {
        "flagship" => 1,
        "warsun" => 2,
        "dreadnought" => 5,
        "cruiser" | "destroyer" => 8,
        "carrier" | "mech" => 4,
        "pds" => 6,
        "spacedock" => 3,
        _ => return None,
    };
    Some(count)
}

/// The plastic a unit id corresponds to.
///
/// Read from the corpus rather than by matching on the id: `sol_carrier2` and `letnev_flagship`
/// do not decompose reliably, and a wrong answer here quietly doubles a cap.
#[must_use]
pub fn base_type_of(content: &ContentStore, sources: SourceSet, unit: &UnitTypeId) -> String {
    ti4_content::units::catalogue(content, sources)
        .get(unit.as_str())
        .map_or_else(|| unit.to_string(), |kind| kind.base_type().to_owned())
}

/// How many of this plastic the player has on the board, everywhere.
///
/// The space area and every planet: a carrier in space and a carrier parked over a planet are the
/// same model out of the same box. Captured units count too — they are off the board but still
/// out of their owner's supply.
#[must_use]
pub fn held(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    base_type: &str,
) -> i64 {
    let mut total = 0;
    let mut count = |unit: &ti4_model::units::Unit| {
        if &unit.owner == player && base_type_of(content, sources, &unit.type_id) == base_type {
            total += 1;
        }
    };
    for board in state.board.values() {
        for unit in &board.units {
            count(unit);
        }
        for units in board.planet_units.values() {
            for unit in units {
                count(unit);
            }
        }
    }
    for captor in &state.players {
        for (owner, unit) in &captor.captured_units {
            if owner == player && base_type_of(content, sources, unit) == base_type {
                total += 1;
            }
        }
    }
    total
}

/// How many more of this unit may be placed before the box is empty.
#[must_use]
pub fn remaining(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    unit: &UnitTypeId,
) -> i64 {
    let base = base_type_of(content, sources, unit);
    let Some(limit) = plastic(&base) else {
        return UNCAPPED;
    };
    (limit - held(state, content, sources, player, &base)).max(0)
}

/// How many of `wanted` may actually be placed, given what is left in the box.
///
/// Every ability that puts plastic on the board goes through this rather than doing its own
/// arithmetic, so a new ability gets the rule by using the helper rather than by remembering it.
/// Returns `wanted` unchanged for anything uncapped.
#[must_use]
pub fn allowed(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    unit: &UnitTypeId,
    wanted: usize,
) -> usize {
    let left = remaining(state, content, sources, player, unit);
    usize::try_from(left).unwrap_or(0).min(wanted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{a_placed_planet, game, put, put_on_planet};
    use ti4_model::content_types::POK;

    fn player() -> PlayerId {
        PlayerId::new("a")
    }

    #[test]
    fn fighters_and_infantry_are_not_plastic() {
        // The oracle's own recorded mistake: capping these was wrong. The box ships cardboard
        // tokens for both exactly so they are never limited, and 58 infantry is legal.
        assert_eq!(plastic("fighter"), None);
        assert_eq!(plastic("infantry"), None);
        assert_eq!(plastic("carrier"), Some(4));

        let state = game(&["a"]);
        assert_eq!(
            allowed(
                &state,
                ContentStore::embedded(),
                POK,
                &player(),
                &UnitTypeId::new("infantry"),
                50
            ),
            50,
            "an uncapped unit passes through untouched"
        );
    }

    #[test]
    fn the_cap_counts_what_is_already_on_the_board() {
        let mut state = game(&["a"]);
        let (system, _) = a_placed_planet();
        let carrier = UnitTypeId::new("carrier");

        assert_eq!(
            allowed(
                &state,
                ContentStore::embedded(),
                POK,
                &player(),
                &carrier,
                6
            ),
            4,
            "four in the box"
        );

        put(&mut state, &system, "carrier", &player(), 3);
        assert_eq!(
            allowed(
                &state,
                ContentStore::embedded(),
                POK,
                &player(),
                &carrier,
                6
            ),
            1,
            "three are already out"
        );

        put(&mut state, &system, "carrier", &player(), 1);
        assert_eq!(
            allowed(
                &state,
                ContentStore::embedded(),
                POK,
                &player(),
                &carrier,
                6
            ),
            0,
            "and now the box is empty"
        );
    }

    #[test]
    fn a_carrier_over_a_planet_is_the_same_model_as_one_in_space() {
        let mut state = game(&["a"]);
        let (system, planet) = a_placed_planet();
        put(&mut state, &system, "carrier", &player(), 2);
        put_on_planet(&mut state, &system, &planet, "carrier", &player(), 2);

        assert_eq!(
            held(&state, ContentStore::embedded(), POK, &player(), "carrier"),
            4,
            "counted wherever they sit"
        );
    }

    #[test]
    fn an_upgrade_is_the_same_plastic() {
        // `sol_carrier2` and `carrier` are the same four models. Reading the id rather than the
        // base type would let a faction upgrade double the fleet.
        let content = ContentStore::embedded();
        let upgraded = ti4_content::units::catalogue(content, POK)
            .iter()
            .find(|(id, kind)| kind.base_type() == "carrier" && **id != "carrier")
            .map(|(id, _)| UnitTypeId::new(*id));
        let Some(upgraded) = upgraded else {
            return; // this corpus has no carrier upgrade
        };

        let mut state = game(&["a"]);
        let (system, _) = a_placed_planet();
        put(&mut state, &system, upgraded.as_str(), &player(), 4);

        assert_eq!(
            allowed(
                &state,
                content,
                POK,
                &player(),
                &UnitTypeId::new("carrier"),
                2
            ),
            0,
            "four upgraded carriers are four carriers"
        );
    }

    #[test]
    fn another_players_fleet_is_not_yours() {
        let mut state = game(&["a", "b"]);
        let (system, _) = a_placed_planet();
        put(&mut state, &system, "carrier", &PlayerId::new("b"), 4);

        assert_eq!(
            allowed(
                &state,
                ContentStore::embedded(),
                POK,
                &player(),
                &UnitTypeId::new("carrier"),
                4
            ),
            4,
            "the cap is per player"
        );
    }

    #[test]
    fn a_captured_unit_is_still_out_of_its_owners_supply() {
        let mut state = game(&["a", "b"]);
        state
            .player_mut(&PlayerId::new("b"))
            .unwrap()
            .captured_units = vec![(player(), UnitTypeId::new("carrier")); 4];

        assert_eq!(
            allowed(
                &state,
                ContentStore::embedded(),
                POK,
                &player(),
                &UnitTypeId::new("carrier"),
                4
            ),
            0,
            "held by somebody else, but still not in the box"
        );
    }
}
