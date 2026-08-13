//! The faction plugin contract (M07-001).
//!
//! Ported from the oracle's `engine/faction_abilities/__init__.py`.
//!
//! A faction ability is a *query* the engine makes, not a reaction it waits for: how much to
//! shift a combat die, what the fleet limit is, whether this player may trade an action card.
//! None of the seven faction modules in the oracle imports the timing system, which is why this
//! layer needs no reactions to be useful.
//!
//! Registries are keyed by ability id and looked up through the player's faction, so an ability
//! belongs to whoever has the card rather than to a name checked at each call site. That matters
//! for coverage: [`unimplemented`] can then say which printed abilities nothing here answers,
//! and [`BLOCKED`] says which of those cannot be written yet and why — kept apart from the merely
//! unwritten so the numbers stay honest.

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;

/// Abilities that cannot be written until a subsystem exists, with the subsystem named.
///
/// Separate from merely unwritten abilities on purpose: one is work, the other is a dependency,
/// and a single number covering both hides which.
#[must_use]
pub fn blocked() -> BTreeMap<&'static str, &'static str> {
    [
        (
            "propagation",
            "Nekro cannot research, and technology theft is unmodelled",
        ),
        (
            "mitosis",
            "unit placement outside production is not a step yet",
        ),
        (
            "stall_tactics",
            "action-card discard as a free action has no window",
        ),
        (
            "telepathic",
            "the agenda deck is not inspectable before it is revealed",
        ),
        (
            "quash",
            "an agenda cannot be discarded and replaced mid-window",
        ),
        (
            "your_ships_have_no_shields",
            "no window exists between rolling and assigning hits",
        ),
    ]
    .into_iter()
    .collect()
}

/// Ability ids this player has, through their faction.
#[must_use]
pub fn of_player(state: &GameState, content: &ContentStore, player: &PlayerId) -> Vec<String> {
    let Some(seat) = state.player(player) else {
        return Vec::new();
    };
    ti4_content::factions::get(content, seat.faction.as_str())
        .map(|faction| {
            faction
                .abilities()
                .into_iter()
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Whether this player has a particular ability.
#[must_use]
pub fn has(state: &GameState, content: &ContentStore, player: &PlayerId, ability: &str) -> bool {
    of_player(state, content, player)
        .iter()
        .any(|id| id == ability)
}

// -- the hooks -----------------------------------------------------------------------------------
//
// Each is a query with a default, so a subsystem calls it unconditionally and a faction with
// nothing to say changes nothing. Adding a faction means adding an arm here, never a branch at
// the call site.

/// Shift applied to each of this player's combat dice, in `context` ("space" or "ground").
#[must_use]
pub fn combat_modifier(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    #[expect(
        unused_variables,
        reason = "space and ground differ for abilities not yet ported; the parameter is the                   contract, and dropping it would have every caller pass nothing and every                   future ability shift both"
    )]
    context: &str,
) -> i64 {
    of_player(state, content, player)
        .iter()
        .map(|ability| match ability.as_str() {
            // Jol-Nar's Fragile: -1 to every combat roll, in space and on the ground.
            "fragile" => -1,
            // Sardakk's Unrelenting: +1 to every combat roll.
            "unrelenting" => 1,
            // The Titans' Coalescence and Sol's Orbital Drop do not shift dice.
            _ => 0,
        })
        .sum()
}

/// The limit on non-fighter ships in one system, adjusted by anything this player has.
#[must_use]
pub fn fleet_supply(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    base: i32,
) -> i32 {
    of_player(state, content, player)
        .iter()
        .fold(base, |limit, ability| match ability.as_str() {
            // Letnev's Armada: two more non-fighter ships than the fleet pool allows.
            "armada" => limit + 2,
            _ => limit,
        })
}

/// Command tokens gained in the status phase, adjusted.
#[must_use]
pub fn status_tokens(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    base: i32,
) -> i32 {
    of_player(state, content, player)
        .iter()
        .fold(base, |count, ability| match ability.as_str() {
            // Sol's Versatile: one more token every status phase.
            "versatile" => count + 1,
            _ => count,
        })
}

/// Prerequisites this player may skip when researching `technology`.
#[must_use]
pub fn waived_prerequisites(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    _technology: &str,
) -> usize {
    of_player(state, content, player)
        .iter()
        .map(|ability| match ability.as_str() {
            // Jol-Nar's Brilliant: one prerequisite waived on any technology.
            "brilliant" => 1,
            _ => 0,
        })
        .sum()
}

/// Whether this player may include an action card in a transaction (94.3's exception).
#[must_use]
pub fn trades_action_cards(state: &GameState, content: &ContentStore, player: &PlayerId) -> bool {
    // Hacan's Arbiters.
    has(state, content, player, "arbiters")
}

/// Whether this player may transact with anybody, not only their neighbours.
#[must_use]
pub fn ignores_neighbours(state: &GameState, content: &ContentStore, player: &PlayerId) -> bool {
    // Hacan's Guild Ships.
    has(state, content, player, "guild_ships")
}

/// Whether a strategy card's secondary costs this player no token.
#[must_use]
pub fn secondary_is_free(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    card: &str,
) -> bool {
    of_player(state, content, player).iter().any(|ability| {
        match ability.as_str() {
            // Xxcha's Peace Accords are about Diplomacy; Hacan's Masters of Trade waive Trade.
            "master_of_trade" => card.eq_ignore_ascii_case("trade"),
            _ => false,
        }
    })
}

// -- coverage ------------------------------------------------------------------------------------

/// Every ability the corpus prints, by id.
#[must_use]
pub fn catalogue(content: &ContentStore, sources: SourceSet) -> Vec<String> {
    content
        .from_sources(ContentType::Abilities, sources)
        .filter_map(|record| record.text("id").or_else(|| record.text("alias")))
        .map(ToOwned::to_owned)
        .collect()
}

/// Ability ids this layer answers.
#[must_use]
pub fn registered() -> Vec<&'static str> {
    vec![
        "arbiters",
        "armada",
        "brilliant",
        "fragile",
        "guild_ships",
        "master_of_trade",
        "unrelenting",
        "versatile",
    ]
}

/// Printed abilities nothing here answers, excluding the ones [`blocked`] explains.
#[must_use]
pub fn unimplemented(content: &ContentStore, sources: SourceSet) -> Vec<String> {
    let known = registered();
    let blocked = blocked();
    catalogue(content, sources)
        .into_iter()
        .filter(|id| !known.contains(&id.as_str()))
        .filter(|id| !blocked.contains_key(id.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::game;
    use ti4_model::content_types::POK;
    use ti4_model::id::FactionId;

    fn seated(faction: &str) -> (GameState, PlayerId) {
        let player = PlayerId::new("a");
        let mut state = game(&["a", "b"]);
        state.player_mut(&player).unwrap().faction = FactionId::new(faction);
        (state, player)
    }

    /// The faction that prints an ability, if this corpus has one.
    fn faction_with(ability: &str) -> Option<String> {
        ti4_content::factions::catalogue(ContentStore::embedded(), POK)
            .iter()
            .find(|(_, faction)| faction.abilities().contains(&ability))
            .map(|(alias, _)| (*alias).to_owned())
    }

    #[test]
    fn a_faction_with_nothing_to_say_changes_nothing() {
        // Every hook is a query with a default, so a subsystem calls it unconditionally.
        let (state, player) = seated("sol");
        let content = ContentStore::embedded();

        assert_eq!(fleet_supply(&state, content, &player, 4), 4);
        assert_eq!(waived_prerequisites(&state, content, &player, "any"), 0);
        assert!(!trades_action_cards(&state, content, &player));
        assert!(!ignores_neighbours(&state, content, &player));
    }

    #[test]
    fn an_ability_belongs_to_whoever_has_the_card() {
        let Some(letnev) = faction_with("armada") else {
            return; // this corpus does not print Armada
        };
        let content = ContentStore::embedded();
        let (with, player) = seated(&letnev);
        let (without, _) = seated("sol");

        assert_eq!(
            fleet_supply(&with, content, &player, 4),
            6,
            "Armada allows two more"
        );
        assert_eq!(
            fleet_supply(&without, content, &player, 4),
            4,
            "and nobody else gets it"
        );
    }

    #[test]
    fn a_die_shift_is_signed() {
        // Fragile subtracts and Unrelenting adds. A hook that returned a magnitude would make
        // Jol-Nar the best shots in the game.
        let content = ContentStore::embedded();
        if let Some(jolnar) = faction_with("fragile") {
            let (state, player) = seated(&jolnar);
            assert_eq!(combat_modifier(&state, content, &player, "space"), -1);
        }
        if let Some(sardakk) = faction_with("unrelenting") {
            let (state, player) = seated(&sardakk);
            assert_eq!(combat_modifier(&state, content, &player, "space"), 1);
        }
    }

    #[test]
    fn armada_reaches_the_fleet_limit_that_is_actually_enforced() {
        // The hook existing is not the same as a subsystem asking it. `fleet::limit` is what
        // enforcement reads, so that is what this checks.
        let Some(letnev) = faction_with("armada") else {
            return;
        };
        let content = ContentStore::embedded();
        let (mut state, player) = seated(&letnev);
        state.player_mut(&player).unwrap().fleet_tokens = 3;

        assert_eq!(
            crate::fleet::limit(&state, content, &player),
            5,
            "three tokens and Armada's two"
        );

        let (mut plain, other) = seated("sol");
        plain.player_mut(&other).unwrap().fleet_tokens = 3;
        assert_eq!(crate::fleet::limit(&plain, content, &other), 3);
    }

    #[test]
    fn a_combat_shift_reaches_the_threshold_a_unit_actually_rolls_against() {
        let content = ContentStore::embedded();
        let Some(sardakk) = faction_with("unrelenting") else {
            return;
        };
        let (state, player) = seated(&sardakk);
        let (plain, other) = seated("sol");
        let unit =
            ti4_model::units::Unit::new(ti4_model::id::UnitTypeId::new("cruiser"), player.clone());

        let theirs = crate::combat::effective_hits_on(&state, content, POK, &player, &unit);
        let ordinary = crate::combat::effective_hits_on(&plain, content, POK, &other, &unit);

        assert!(
            theirs < ordinary,
            "Unrelenting hits on a lower number: {theirs:?} against {ordinary:?}"
        );
    }

    #[test]
    fn guild_ships_makes_the_whole_table_a_partner() {
        // 60.1 says neighbours; the card says anybody. A player with no neighbours at all still
        // has somebody to trade with.
        let Some(hacan) = faction_with("guild_ships") else {
            return;
        };
        let content = ContentStore::embedded();
        let hub = crate::fixtures::plain_hub();
        let (mut state, player) = seated(&hacan);
        // Nobody is anywhere near anybody.
        assert!(
            crate::transactions::neighbours(&state, &hub.galaxy, &player).is_empty(),
            "no fleets are placed, so nobody is a neighbour"
        );

        let reachable = crate::transactions::partners(&state, content, &hub.galaxy, &player);
        assert!(
            !reachable.is_empty(),
            "Guild Ships reaches the table anyway"
        );
        assert!(!reachable.contains(&player), "but not yourself");

        state.player_mut(&player).unwrap().faction = FactionId::new("sol");
        assert!(
            crate::transactions::partners(&state, content, &hub.galaxy, &player).is_empty(),
            "and nobody else gets it"
        );
    }

    #[test]
    fn the_blocked_abilities_are_named_with_a_reason() {
        // Kept apart from merely unwritten ones: one is work, the other is a dependency, and a
        // single number covering both hides which.
        let blocked = blocked();
        assert!(!blocked.is_empty());
        for (ability, reason) in &blocked {
            assert!(!reason.is_empty(), "{ability} is blocked on nothing stated");
            assert!(
                !registered().contains(ability),
                "{ability} is both blocked and registered"
            );
        }
    }

    #[test]
    fn every_registered_ability_is_one_the_corpus_prints() {
        // The trap this project has hit four times: an id somebody was sure of does not exist,
        // and the ability is unreachable for ever with nothing to say so.
        let printed = catalogue(ContentStore::embedded(), ti4_model::content_types::FULL);
        for ability in registered() {
            assert!(
                printed.contains(&ability.to_owned()),
                "{ability} is not an ability the corpus knows"
            );
        }
    }

    #[test]
    fn the_gap_is_reported_rather_than_implied() {
        let missing = unimplemented(ContentStore::embedded(), POK);
        assert!(!missing.is_empty(), "most abilities are still unanswered");
        for ability in registered() {
            assert!(!missing.contains(&ability.to_owned()));
        }
    }
}
