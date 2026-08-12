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
    vec!["bookoflatvinia", "dynamiscore", "shard", "thesilverflame"]
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

/// A faction's printed commodity value (21.1).
fn commodity_value(state: &GameState, content: &ContentStore, player: &PlayerId) -> i32 {
    state.player(player).map_or(0, |seat| {
        ti4_content::factions::get(content, seat.faction.as_str())
            .map_or(0, |faction| faction.commodities())
    })
}

/// The Shard of the Throne, which is worth a victory point simply for being held.
pub const SHARD: &str = "shard";

/// Draw the top relic (73.2).
///
/// Every path that hands a player a relic goes through here, because a relic can be worth a
/// point the moment it arrives: the Shard was worth nothing when exploration drew it straight
/// off the deck, and would have been worth nothing again for the next path written.
pub fn gain(state: &mut GameState, player: &PlayerId) -> Option<RelicId> {
    let top = state.relic_deck.first().cloned()?; // 73.2a: an empty deck yields nothing
    state.relic_deck.remove(0);
    if let Some(seat) = state.player_mut(player) {
        seat.relics.push(top.clone());
    }
    if top.as_str() == SHARD
        && let Some(seat) = state.player_mut(player)
    {
        seat.victory_points = (seat.victory_points + 1).min(VICTORY_TARGET);
    }
    Some(top)
}

/// Use a relic's action, purging it.
pub fn use_relic(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    dice: &mut crate::dice::Dice,
    rng: &mut crate::rng::GameRng,
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
            // Commodity *value* is the faction's printed number, not how many commodities the
            // player happens to be holding. Reading the holding pays a full seat nothing and an
            // empty one two, which is the card backwards.
            let value = commodity_value(state, content, player) + 2;
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
        "thesilverflame" => {
            // A ten scores; anything else consumes the home system and bars this player from
            // public objectives for the rest of the game. The roll happens either way, so the
            // card is purged before the branch rather than in one arm of it.
            let roll = dice
                .roll(rng, 1, "silver_flame", None)
                .faces
                .first()
                .copied()
                .unwrap_or(0);
            purge(state, player, relic);
            if roll == 10 {
                if let Some(seat) = state.player_mut(player) {
                    seat.victory_points = (seat.victory_points + 1).min(VICTORY_TARGET);
                }
                return Used::Purged {
                    relic: relic.clone(),
                };
            }
            let home = state.player(player).and_then(|seat| {
                seat.home_system.clone().or_else(|| {
                    ti4_content::factions::get(content, seat.faction.as_str())
                        .and_then(|faction| faction.home_system())
                        .map(ti4_model::id::SystemId::new)
                })
            });
            if let Some(seat) = state.player_mut(player) {
                seat.public_objectives_forbidden = true;
            }
            if let Some(home) = home {
                state.board.remove(&home);
                state.purged_systems.insert(home);
            }
            return Used::Purged {
                relic: relic.clone(),
            };
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

// -- the component action (22) -----------------------------------------------------------------

/// The kind of a relic component action.
pub const ACTION_KIND: &str = "component";

/// The prefix of an option that purges fragments, and of one that uses a held relic.
const PURGE_PREFIX: &str = "purge|";
const USE_PREFIX: &str = "relic|";

/// Component actions this player could take with relics and fragments right now.
///
/// Two kinds, and only the first ever existed here: purging three fragments for a new relic,
/// and using a relic already in the play area. Without the second a relic could be drawn,
/// held and counted while being unusable for the whole game.
#[must_use]
pub fn available_actions(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<crate::choice::ChoiceOption> {
    let mut options: Vec<crate::choice::ChoiceOption> =
        crate::exploration::purgeable(state, player)
            .into_iter()
            .map(|trait_name| {
                crate::choice::ChoiceOption::labelled(
                    format!("{PURGE_PREFIX}{trait_name}"),
                    ACTION_KIND,
                    format!(
                        "purge 3 {} relic fragments for a relic",
                        trait_name.to_lowercase()
                    ),
                )
            })
            .collect();

    let held = state
        .player(player)
        .map(|seat| seat.relics.clone())
        .unwrap_or_default();
    let known = registered_aliases();
    options.extend(
        held.into_iter()
            // 22.3: an action that cannot fully resolve is never offered, and a relic with no
            // handler cannot resolve at all.
            .filter(|relic| known.contains(&relic.as_str()))
            .filter(|relic| relic.as_str() != SHARD) // held for its point; it has no action
            .map(|relic| {
                crate::choice::ChoiceOption::labelled(
                    format!("{USE_PREFIX}{relic}"),
                    ACTION_KIND,
                    format!("use {relic}"),
                )
            }),
    );
    let _ = (content, sources);
    options
}

/// Perform a relic component action. Returns `false` for an option that is not one.
pub fn perform(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    dice: &mut crate::dice::Dice,
    rng: &mut crate::rng::GameRng,
    player: &PlayerId,
    option: &crate::choice::ChoiceOption,
) -> bool {
    if let Some(trait_name) = option.id.strip_prefix(PURGE_PREFIX) {
        // The fragments are spent by `purge_for_relic`, which draws straight from the deck, so
        // the Shard's point is applied here rather than being lost on this one path.
        let before = state.player(player).map(|seat| seat.relics.len());
        let gained = crate::exploration::purge_for_relic(state, player, trait_name);
        if let (Some(relic), Some(_)) = (gained.as_ref(), before)
            && relic.as_str() == SHARD
            && let Some(seat) = state.player_mut(player)
        {
            seat.victory_points = (seat.victory_points + 1).min(VICTORY_TARGET);
        }
        return gained.is_some();
    }
    if let Some(alias) = option.id.strip_prefix(USE_PREFIX) {
        let relic = RelicId::new(alias);
        return matches!(
            use_relic(state, content, sources, dice, rng, player, &relic),
            Used::Purged { .. }
        );
    }
    false
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
    fn the_shard_is_worth_a_point_the_moment_it_arrives() {
        let mut state = game(&["a"]);
        state.relic_deck = vec![RelicId::new(SHARD)];
        let before = state.player(&player()).unwrap().victory_points;

        let gained = gain(&mut state, &player());

        assert_eq!(gained, Some(RelicId::new(SHARD)));
        assert_eq!(
            state.player(&player()).unwrap().victory_points,
            before + 1,
            "held, not used: the point comes with the card"
        );
    }

    #[test]
    fn an_ordinary_relic_is_worth_no_points() {
        let mut state = game(&["a"]);
        state.relic_deck = vec![RelicId::new("dynamiscore")];
        let before = state.player(&player()).unwrap().victory_points;

        gain(&mut state, &player());

        assert_eq!(state.player(&player()).unwrap().victory_points, before);
    }

    #[test]
    fn an_empty_relic_deck_gives_nothing() {
        let mut state = game(&["a"]);
        state.relic_deck.clear();
        assert_eq!(gain(&mut state, &player()), None);
    }

    #[test]
    fn the_silver_flame_scores_on_a_ten_and_burns_you_otherwise() {
        // Both halves are reachable, so the roll is forced rather than hoped for: a test that
        // takes whatever the stream gives would exercise one branch and call it the card.
        for (face, scored) in [(10, true), (1, false)] {
            let mut state = game(&["a"]);
            let relic = give(&mut state, "thesilverflame");
            state.player_mut(&player()).unwrap().faction = ti4_model::id::FactionId::new("sol");
            let home = ti4_content::factions::get(ContentStore::embedded(), "sol")
                .and_then(|faction| faction.home_system())
                .map(ti4_model::id::SystemId::new)
                .expect("sol has a home system");
            state.system_mut(&home);
            let before = state.player(&player()).unwrap().victory_points;

            let mut dice = crate::dice::Dice::from_faces([face]);
            use_relic(
                &mut state,
                ContentStore::embedded(),
                POK,
                &mut dice,
                &mut crate::rng::GameRng::new(0),
                &player(),
                &relic,
            );

            let seat = state.player(&player()).unwrap();
            assert!(!holds(&state, &player(), &relic), "purged either way");
            if scored {
                assert_eq!(seat.victory_points, before + 1);
                assert!(!seat.public_objectives_forbidden);
                assert!(state.purged_systems.is_empty());
            } else {
                assert_eq!(seat.victory_points, before);
                assert!(
                    seat.public_objectives_forbidden,
                    "the price is every public objective for the rest of the game"
                );
                assert!(state.purged_systems.contains(&home), "and the home system");
            }
        }
    }

    #[test]
    fn a_relic_with_no_handler_is_never_offered_as_an_action() {
        // 22.3: an action that cannot fully resolve is not offered. A relic the engine cannot
        // resolve would otherwise be a turn spent on nothing.
        let mut state = game(&["a"]);
        let unknown = unimplemented(ContentStore::embedded(), POK)
            .into_iter()
            .next()
            .expect("some relic is still unimplemented");
        state.player_mut(&player()).unwrap().relics = vec![unknown.clone()];

        let offered = available_actions(&state, ContentStore::embedded(), POK, &player());

        assert!(
            offered.is_empty(),
            "{unknown} has no handler and must not be offered: {offered:?}"
        );
    }

    #[test]
    fn a_held_relic_is_offered_and_using_it_purges_it() {
        let mut state = game(&["a"]);
        let relic = give(&mut state, "dynamiscore");

        let offered = available_actions(&state, ContentStore::embedded(), POK, &player());
        let option = offered
            .iter()
            .find(|option| option.id.contains(relic.as_str()))
            .cloned()
            .expect("the relic is offered");

        assert!(perform(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut crate::dice::Dice::new(),
            &mut crate::rng::GameRng::new(0),
            &player(),
            &option,
        ));
        assert!(!holds(&state, &player(), &relic));
    }

    #[test]
    fn fragments_are_offered_as_an_action_and_buy_a_relic() {
        let mut state = game(&["a"]);
        state.relic_deck = vec![RelicId::new("dynamiscore")];
        state.player_mut(&player()).unwrap().relic_fragments =
            [("CULTURAL".to_owned(), 3)].into_iter().collect();

        let offered = available_actions(&state, ContentStore::embedded(), POK, &player());
        let option = offered
            .iter()
            .find(|option| option.id.starts_with("purge|"))
            .cloned()
            .expect("three fragments buy a relic");

        assert!(perform(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut crate::dice::Dice::new(),
            &mut crate::rng::GameRng::new(0),
            &player(),
            &option,
        ));
        assert_eq!(state.player(&player()).unwrap().relics.len(), 1);
    }

    #[test]
    fn a_relic_you_do_not_hold_does_nothing() {
        let mut state = game(&["a"]);
        let before = state.clone();
        let used = use_relic(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut crate::dice::Dice::new(),
            &mut crate::rng::GameRng::new(0),
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

        let used = use_relic(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut crate::dice::Dice::new(),
            &mut crate::rng::GameRng::new(0),
            &player(),
            &relic,
        );

        assert!(matches!(used, Used::Unresolved { .. }));
        assert!(holds(&state, &player(), &relic), "it was not purged");
    }

    #[test]
    fn dynamis_core_counts_its_own_bonus_into_the_gain() {
        // The card's standing half raises commodity *value* by 2, and its action gains that
        // value. Value is the faction's printed number: reading the commodities in hand pays a
        // full seat nothing extra and an empty one two, which is the card backwards.
        let mut state = game(&["a"]);
        let relic = give(&mut state, "dynamiscore");
        let seat = state.player_mut(&player()).unwrap();
        seat.faction = ti4_model::id::FactionId::new("sol");
        seat.commodities = 0;
        seat.trade_goods = 0;
        let printed = ti4_content::factions::get(ContentStore::embedded(), "sol")
            .expect("sol is a faction")
            .commodities();

        use_relic(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut crate::dice::Dice::new(),
            &mut crate::rng::GameRng::new(0),
            &player(),
            &relic,
        );

        assert_eq!(
            state.player(&player()).unwrap().trade_goods,
            printed + 2,
            "the printed value plus the card's own two, with none in hand"
        );
        assert!(!holds(&state, &player(), &relic), "and it purged itself");
    }

    #[test]
    fn the_book_gives_the_speaker_token_without_all_four_specialties() {
        let mut state = game(&["a", "b"]);
        state.speaker = PlayerId::new("b");
        let relic = give(&mut state, "bookoflatvinia");

        use_relic(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut crate::dice::Dice::new(),
            &mut crate::rng::GameRng::new(0),
            &player(),
            &relic,
        );

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

        use_relic(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut crate::dice::Dice::new(),
            &mut crate::rng::GameRng::new(0),
            &player(),
            &relic,
        );

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
