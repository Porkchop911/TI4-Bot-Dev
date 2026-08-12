//! Relic effects (LRR 35.9's reward, M06-007).
//!
//! Ported from the oracle's `engine/relics.py`: `_dynamis_core`, `_book_of_latvinia`,
//! `_purge`, and the Circlet's standing gravity-rift immunity.
//!
//! A first tranche. A relic with no registered handler is held but does nothing, and
//! [`unimplemented`] reports which — the same design used for objectives, agendas and laws.

use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{PlayerId, RelicId};
use ti4_model::state::GameState;

use crate::objectives::VICTORY_TARGET;

/// The Circlet of the Void: its owner's units do not roll for gravity rifts.
pub const CIRCLET: &str = "circletofthevoid";

/// What using a relic did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Used {
    /// The relic resolved and was purged.
    Purged { relic: RelicId },
    /// The player does not hold it.
    NotHeld { relic: RelicId },
    /// Held, but this engine has no handler for it.
    Unresolved { relic: RelicId },
}

/// Relics this engine can resolve.
#[must_use]
pub fn registered_aliases() -> Vec<&'static str> {
    vec!["bookoflatvinia", "dynamiscore"]
}

/// Whether a player holds a relic.
#[must_use]
pub fn holds(state: &GameState, player: &PlayerId, relic: &RelicId) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.relics.contains(relic))
}

/// 41.2 immunity: the Circlet's owner never rolls for a gravity rift.
///
/// Read where the roll happens rather than at the card, so it cannot be honoured in one place
/// and forgotten in another — the mistake Nav Suite nearly made in `transit`.
#[must_use]
pub fn ignores_gravity_rifts(state: &GameState, player: &PlayerId) -> bool {
    holds(state, player, &RelicId::new(CIRCLET))
}

fn purge(state: &mut GameState, player: &PlayerId, relic: &RelicId) {
    if let Some(seat) = state.player_mut(player) {
        seat.relics.retain(|held| held != relic);
    }
}

/// Whether this player controls planets covering all four technology specialties.
fn controls_all_four_specialties(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> bool {
    let catalogue = ti4_content::galaxy::all_planets(content, sources);
    let mut found = std::collections::BTreeSet::new();
    for (_, planet) in state.controlled_planets(player) {
        if let Some(record) = catalogue.get(planet.as_str()) {
            for specialty in record.tech_specialties() {
                found.insert(specialty.to_ascii_uppercase());
            }
        }
    }
    found.len() >= 4
}

/// Use a relic's action, purging it.
pub fn use_relic(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    relic: &RelicId,
) -> Used {
    if !holds(state, player, relic) {
        return Used::NotHeld {
            relic: relic.clone(),
        };
    }
    match relic.as_str() {
        "dynamiscore" => {
            // "Gain trade goods equal to your commodity value, then purge this card." The
            // card's other half — commodity value increased by 2 — is a standing modifier, and
            // is applied here to the gain so the two halves cannot disagree about the number.
            let value = state
                .player(player)
                .map_or(0, |seat| seat.commodities.max(0) + 2);
            if let Some(seat) = state.player_mut(player) {
                seat.trade_goods += value;
            }
        }
        "bookoflatvinia" => {
            // All four specialties gains a victory point; otherwise the speaker token.
            if controls_all_four_specialties(state, content, sources, player) {
                if let Some(seat) = state.player_mut(player) {
                    seat.victory_points = (seat.victory_points + 1).min(VICTORY_TARGET);
                }
            } else {
                state.speaker = player.clone();
            }
        }
        _ => {
            return Used::Unresolved {
                relic: relic.clone(),
            };
        }
    }
    purge(state, player, relic);
    Used::Purged {
        relic: relic.clone(),
    }
}

/// Relics in the corpus that nothing here resolves.
#[must_use]
pub fn unimplemented(content: &ContentStore, sources: SourceSet) -> Vec<RelicId> {
    let known = registered_aliases();
    content
        .from_sources(ContentType::Relics, sources)
        .filter_map(|record| record.text("alias"))
        .filter(|alias| !known.contains(alias) && *alias != CIRCLET)
        .map(RelicId::new)
        .collect()
}

#[cfg(test)]
mod tests {
    use ti4_model::content_types::POK;

    use super::*;
    use crate::fixtures::game;

    fn player() -> PlayerId {
        PlayerId::new("a")
    }

    fn give(state: &mut GameState, alias: &str) -> RelicId {
        let relic = RelicId::new(alias);
        state
            .player_mut(&player())
            .unwrap()
            .relics
            .push(relic.clone());
        relic
    }

    #[test]
    fn a_relic_you_do_not_hold_does_nothing() {
        let mut state = game(&["a"]);
        let before = state.clone();
        let used = use_relic(
            &mut state,
            ContentStore::embedded(),
            POK,
            &player(),
            &RelicId::new("dynamiscore"),
        );
        assert!(matches!(used, Used::NotHeld { .. }));
        assert!(state.identical(&before));
    }

    #[test]
    fn an_unregistered_relic_is_held_but_reports_unresolved() {
        let mut state = game(&["a"]);
        let relic = give(&mut state, "nanoforge");

        let used = use_relic(&mut state, ContentStore::embedded(), POK, &player(), &relic);

        assert!(matches!(used, Used::Unresolved { .. }));
        assert!(holds(&state, &player(), &relic), "it was not purged");
    }

    #[test]
    fn dynamis_core_counts_its_own_bonus_into_the_gain() {
        // The card's standing half raises commodity value by 2, and its action gains that
        // value. Applying the halves separately is how the two end up disagreeing.
        let mut state = game(&["a"]);
        let relic = give(&mut state, "dynamiscore");
        state.player_mut(&player()).unwrap().commodities = 3;
        state.player_mut(&player()).unwrap().trade_goods = 0;

        use_relic(&mut state, ContentStore::embedded(), POK, &player(), &relic);

        assert_eq!(
            state.player(&player()).unwrap().trade_goods,
            5,
            "three commodities plus the card's own two"
        );
        assert!(!holds(&state, &player(), &relic), "and it purged itself");
    }

    #[test]
    fn the_book_gives_the_speaker_token_without_all_four_specialties() {
        let mut state = game(&["a", "b"]);
        state.speaker = PlayerId::new("b");
        let relic = give(&mut state, "bookoflatvinia");

        use_relic(&mut state, ContentStore::embedded(), POK, &player(), &relic);

        assert_eq!(state.speaker, player());
        assert_eq!(state.player(&player()).unwrap().victory_points, 0);
        assert!(!holds(&state, &player(), &relic));
    }

    #[test]
    fn the_book_gives_a_victory_point_with_all_four() {
        let mut state = game(&["a", "b"]);
        state.speaker = PlayerId::new("b");
        let relic = give(&mut state, "bookoflatvinia");

        // Control a planet of each specialty, if the corpus offers them.
        let mut covered = std::collections::BTreeSet::new();
        for (id, record) in &ti4_content::galaxy::all_planets(ContentStore::embedded(), POK) {
            let specialties = record.tech_specialties();
            if specialties.is_empty() || record.is_placed_during_play() {
                continue;
            }
            let system = ti4_model::id::SystemId::new(record.system_id().unwrap_or("18"));
            state
                .system_mut(&system)
                .set_control(ti4_model::id::PlanetId::new(*id), player());
            for specialty in specialties {
                covered.insert(specialty.to_ascii_uppercase());
            }
            if covered.len() >= 4 {
                break;
            }
        }
        if covered.len() < 4 {
            return;
        }

        use_relic(&mut state, ContentStore::embedded(), POK, &player(), &relic);

        assert_eq!(state.player(&player()).unwrap().victory_points, 1);
        assert_eq!(state.speaker, PlayerId::new("b"), "the token did not move");
    }

    #[test]
    fn the_circlet_makes_its_owner_immune_to_gravity_rifts() {
        let mut state = game(&["a", "b"]);
        assert!(!ignores_gravity_rifts(&state, &player()));

        give(&mut state, CIRCLET);

        assert!(ignores_gravity_rifts(&state, &player()));
        assert!(
            !ignores_gravity_rifts(&state, &PlayerId::new("b")),
            "it protects its owner, not the table"
        );
    }

    #[test]
    fn the_unresolved_relics_are_reported() {
        let missing = unimplemented(ContentStore::embedded(), POK);
        assert!(!missing.is_empty(), "most relics are still unresolved");
        for alias in registered_aliases() {
            assert!(!missing.contains(&RelicId::new(alias)));
        }
    }

    #[test]
    fn every_registered_alias_is_a_real_relic() {
        for alias in registered_aliases().into_iter().chain([CIRCLET]) {
            assert!(
                ContentStore::embedded()
                    .get(ContentType::Relics, alias)
                    .is_some(),
                "{alias} is not a relic the corpus knows"
            );
        }
    }
}
