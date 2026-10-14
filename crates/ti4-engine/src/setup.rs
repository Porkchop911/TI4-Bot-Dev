//! Setting a game up.
//!
//! Ported from the oracle's `engine/game.py`.

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, Source, SourceSet};
use ti4_model::id::{PlayerId, StrategyCardId};
use ti4_model::state::GameState;

use crate::deck::build_starting_decks;

/// The strategy-card set to use for a source scope, most recent expansion first.
///
/// Order matters: Thunder's Edge replaces the `PoK` set, which replaces the base set.
const STRATEGY_CARD_SETS: [(Source, &str); 3] = [
    (Source::ThundersEdge, "te"),
    (Source::Pok, "pok"),
    (Source::Base, "base_game"),
];

/// Something went wrong setting a game up.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SetupError {
    #[error("no strategy card set for the requested sources")]
    NoStrategyCardSet,
    #[error("a game needs at least one player")]
    NoPlayers,
}

/// LRR 83.2: two strategy cards each in a three or four player game, else one.
///
/// Eight cards exist regardless, so a three-player game drafts six and leaves two on the
/// mat collecting trade goods.
#[must_use]
pub const fn cards_per_player(player_count: usize) -> usize {
    if matches!(player_count, 3 | 4) { 2 } else { 1 }
}

/// The eight strategy cards in play, and their initiative numbers.
///
/// A **set**, not a union. There are always eight strategy cards; the expansions replace
/// individual ones rather than adding to them, so Construction exists three times in the
/// corpus as `base4`, `pok4construction`, and `te4construction`. Taking every card the
/// sources allow yields twelve cards and an initiative order with duplicates — a game that
/// is not TI4. The corpus records the real sets in `strategy_card_sets`, so the choice is
/// read rather than reconstructed.
///
/// # Errors
/// [`SetupError::NoStrategyCardSet`] if no set matches the sources.
pub fn strategy_card_setup(
    content: &ContentStore,
    sources: SourceSet,
) -> Result<(Vec<StrategyCardId>, BTreeMap<StrategyCardId, i32>), SetupError> {
    let chosen = STRATEGY_CARD_SETS
        .iter()
        .filter(|(source, _)| sources.contains(*source))
        .find_map(|(_, alias)| content.get(ContentType::StrategyCardSets, alias))
        .ok_or(SetupError::NoStrategyCardSet)?;
    card_setup_from_record(content, chosen)
}

/// The eight strategy cards of the named set (`te`, `pok`, `base_game`, `base_game_codex1`),
/// independent of the source scope. Server games choose their set explicitly at creation.
///
/// # Errors
/// [`SetupError::NoStrategyCardSet`] if `alias` names no set.
pub fn strategy_card_setup_for_set(
    content: &ContentStore,
    alias: &str,
) -> Result<(Vec<StrategyCardId>, BTreeMap<StrategyCardId, i32>), SetupError> {
    let chosen = content
        .get(ContentType::StrategyCardSets, alias)
        .ok_or(SetupError::NoStrategyCardSet)?;
    card_setup_from_record(content, chosen)
}

fn card_setup_from_record(
    content: &ContentStore,
    chosen: &ti4_content::Record,
) -> Result<(Vec<StrategyCardId>, BTreeMap<StrategyCardId, i32>), SetupError> {
    let mut ids = Vec::new();
    let mut initiative = BTreeMap::new();
    for card_id in chosen.strings("scIDs") {
        // Read initiative from the whole corpus, not the scoped view: a set may name a
        // card whose own source tag is narrower than the set's.
        let Some(card) = content.get(ContentType::StrategyCards, card_id) else {
            continue;
        };
        let id = StrategyCardId::new(card_id);
        initiative.insert(
            id.clone(),
            i32::try_from(card.int("initiative").unwrap_or(99)).unwrap_or(99),
        );
        ids.push(id);
    }
    Ok((ids, initiative))
}

/// A game at the start of its first strategy phase.
///
/// # Errors
/// [`SetupError::NoPlayers`], or any error from [`strategy_card_setup`].
pub fn start_game(
    content: &ContentStore,
    player_ids: &[PlayerId],
    sources: SourceSet,
    speaker: Option<PlayerId>,
) -> Result<GameState, SetupError> {
    start_game_seeded(content, player_ids, sources, speaker, 0)
}

/// A new game with the complete setup decks derived from `deck_seed`.
///
/// The seed is explicit here rather than taken from ambient randomness: a setup replay is
/// reproducible from its inputs, while the native domain-separated RNG keeps future dice
/// and deck work independent.
///
/// # Errors
/// [`SetupError::NoPlayers`], or any error from [`strategy_card_setup`].
pub fn start_game_seeded(
    content: &ContentStore,
    player_ids: &[PlayerId],
    sources: SourceSet,
    speaker: Option<PlayerId>,
    deck_seed: u64,
) -> Result<GameState, SetupError> {
    start_game_seeded_with_card_set(content, player_ids, sources, speaker, deck_seed, None)
}

/// The opening speaker drawn from `seed`: a uniformly chosen seat, the same one for the same
/// seed and table. Drawn from its own RNG domain, so choosing it shifts no deck or die.
/// `None` for an empty table.
#[must_use]
pub fn seeded_speaker(player_ids: &[PlayerId], seed: u64) -> Option<PlayerId> {
    use rand::Rng;
    if player_ids.is_empty() {
        return None;
    }
    let mut rng = crate::rng::GameRng::new(seed);
    let index = rng
        .stream(crate::rng::domain::SPEAKER)
        .random_range(0..player_ids.len());
    Some(player_ids[index].clone())
}

/// As [`start_game_seeded`], but the strategy-card set is named (`Some("te")`, `Some("pok")`, ...)
/// instead of derived from the source scope. `None` keeps the scope-derived set, so callers that
/// do not pass a set (simulation, training, legacy replays) behave exactly as before.
///
/// # Errors
/// [`SetupError::NoPlayers`], [`SetupError::NoStrategyCardSet`] for an unknown set name.
pub fn start_game_seeded_with_card_set(
    content: &ContentStore,
    player_ids: &[PlayerId],
    sources: SourceSet,
    speaker: Option<PlayerId>,
    deck_seed: u64,
    card_set: Option<&str>,
) -> Result<GameState, SetupError> {
    if player_ids.is_empty() {
        return Err(SetupError::NoPlayers);
    }
    let (cards, initiative) = match card_set {
        Some(alias) => strategy_card_setup_for_set(content, alias)?,
        None => strategy_card_setup(content, sources)?,
    };
    let mut state = GameState::new(
        player_ids,
        &cards,
        initiative,
        speaker,
        cards_per_player(player_ids.len()),
    );
    let decks = build_starting_decks(content, sources, deck_seed);
    // The same seed drives the dice. `Game::with_table` reads it back, so every game built from
    // this state rolls from its own stream rather than all of them sharing seed 0's.
    state.rng_seed = deck_seed;
    state.objective_deck = decks.objectives;
    state.exploration_decks = decks.exploration;
    state.relic_deck = decks.relics;
    state.agenda_deck = decks.agendas;
    state.action_card_deck = decks.action_cards;
    state.secret_deck = decks.secrets;
    // 69.1: every player starts holding their own notes. Without this nobody owns one, and a
    // transaction naming a note moves a card that does not exist.
    crate::promissory::deal(&mut state, content, sources);

    for _ in 0..2 {
        let _ = state.reveal_objective();
    }
    for player_id in player_ids {
        let Some(secret) = state.secret_deck.first().cloned() else {
            break;
        };
        state.secret_deck.remove(0);
        if let Some(player) = state.player_mut(player_id) {
            player.secret_objectives.push(secret);
        }
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::content_types::{BASE, FULL, POK};

    fn content() -> &'static ContentStore {
        ContentStore::embedded()
    }

    fn players(n: usize) -> Vec<PlayerId> {
        (0..n).map(|i| PlayerId::new(format!("p{i}"))).collect()
    }

    #[test]
    fn three_and_four_player_games_deal_two_cards_each() {
        assert_eq!(cards_per_player(3), 2);
        assert_eq!(cards_per_player(4), 2);
        assert_eq!(cards_per_player(2), 1);
        assert_eq!(cards_per_player(5), 1);
        assert_eq!(cards_per_player(6), 1);
    }

    #[test]
    fn there_are_always_exactly_eight_strategy_cards() {
        // The trap this guards: taking the union across sources yields twelve cards,
        // because Construction exists as base4, pok4construction and te4construction.
        for sources in [BASE, POK, FULL] {
            let (ids, initiative) = strategy_card_setup(content(), sources).unwrap();
            assert_eq!(ids.len(), 8, "wrong card count for {sources:?}");
            assert_eq!(initiative.len(), 8);
        }
    }

    #[test]
    fn initiative_numbers_are_one_through_eight_without_duplicates() {
        for sources in [BASE, POK, FULL] {
            let (_, initiative) = strategy_card_setup(content(), sources).unwrap();
            let mut numbers: Vec<i32> = initiative.values().copied().collect();
            numbers.sort_unstable();
            assert_eq!(numbers, vec![1, 2, 3, 4, 5, 6, 7, 8], "for {sources:?}");
        }
    }

    #[test]
    fn a_later_expansion_replaces_the_set_rather_than_adding_to_it() {
        let (base, _) = strategy_card_setup(content(), BASE).unwrap();
        let (pok, _) = strategy_card_setup(content(), POK).unwrap();
        let (full, _) = strategy_card_setup(content(), FULL).unwrap();
        assert_ne!(base, pok, "PoK replaces the base set");
        assert_ne!(pok, full, "Thunder's Edge replaces the PoK set");
        assert_eq!(base.len(), pok.len());
        assert_eq!(pok.len(), full.len());
    }

    #[test]
    fn a_named_card_set_ignores_the_source_scope() {
        let ids = |alias: &str| -> Vec<String> {
            strategy_card_setup_for_set(content(), alias)
                .unwrap()
                .0
                .iter()
                .map(ToString::to_string)
                .collect()
        };
        let te = ids("te");
        assert!(
            te.contains(&"te4construction".to_owned()) && te.contains(&"te6warfare".to_owned())
        );
        let pok = ids("pok");
        assert!(
            pok.contains(&"pok4construction".to_owned()) && pok.contains(&"pok6warfare".to_owned())
        );
        let codex = ids("base_game_codex1");
        assert!(codex.contains(&"base4".to_owned()) && codex.contains(&"pok2diplomacy".to_owned()));
        // The scope-derived `pok` set is the named `pok` set; FULL derives `te`.
        let (scoped, _) = strategy_card_setup(content(), POK).unwrap();
        assert_eq!(
            scoped.iter().map(ToString::to_string).collect::<Vec<_>>(),
            pok
        );
        assert_eq!(
            strategy_card_setup(content(), FULL)
                .unwrap()
                .0
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            te
        );
        assert_eq!(
            strategy_card_setup_for_set(content(), "nope").unwrap_err(),
            SetupError::NoStrategyCardSet
        );
    }

    #[test]
    fn a_game_started_with_a_card_set_holds_that_sets_cards_under_a_pok_scope() {
        let g = start_game_seeded_with_card_set(content(), &players(4), POK, None, 3, Some("te"))
            .unwrap();
        assert_eq!(g.unclaimed_strategy_cards.len(), 8);
        assert!(
            g.unclaimed_strategy_cards
                .iter()
                .any(|c| c.as_str() == "te6warfare")
        );
        let legacy = start_game_seeded(content(), &players(4), POK, None, 3).unwrap();
        let none =
            start_game_seeded_with_card_set(content(), &players(4), POK, None, 3, None).unwrap();
        assert_eq!(
            legacy.unclaimed_strategy_cards,
            none.unclaimed_strategy_cards
        );
        assert!(
            legacy
                .unclaimed_strategy_cards
                .iter()
                .any(|c| c.as_str() == "pok6warfare")
        );
    }

    #[test]
    fn a_new_game_holds_all_eight_cards_unclaimed() {
        let g = start_game(content(), &players(6), POK, None).unwrap();
        assert_eq!(g.unclaimed_strategy_cards.len(), 8);
        assert_eq!(g.strategy_cards_per_player, 1);
        assert!(g.players.iter().all(|p| p.strategy_cards.is_empty()));
    }

    #[test]
    fn setup_builds_decks_reveals_objectives_and_deals_secrets() {
        // `start_game` owns the setup-only parts of 61.13: build every deck, reveal two
        // stage-I objectives, then deal one secret objective to each seat.  Leaving the
        // decks empty makes a game look initialized while silently disabling scoring.
        let players = players(3);
        let game = start_game(content(), &players, POK, None).unwrap();

        assert_eq!(game.revealed_objectives.len(), 2);
        assert_eq!(game.objective_deck.len(), 8);
        assert!(!game.action_card_deck.is_empty());
        assert!(!game.agenda_deck.is_empty());
        assert!(!game.relic_deck.is_empty());
        assert_eq!(game.exploration_decks.len(), 4);
        assert!(
            game.players
                .iter()
                .all(|player| player.secret_objectives.len() == 1)
        );
        assert_eq!(game.secret_deck.len() + game.players.len(), 40);
    }

    #[test]
    fn a_three_player_game_drafts_six_and_leaves_two_on_the_mat() {
        let g = start_game(content(), &players(3), POK, None).unwrap();
        assert_eq!(g.strategy_cards_per_player, 2);
        assert_eq!(
            g.unclaimed_strategy_cards.len() - g.players.len() * g.strategy_cards_per_player,
            2
        );
    }

    #[test]
    fn the_first_seat_speaks_unless_told_otherwise() {
        let g = start_game(content(), &players(4), POK, None).unwrap();
        assert_eq!(g.speaker, PlayerId::new("p0"));

        let g = start_game(content(), &players(4), POK, Some(PlayerId::new("p2"))).unwrap();
        assert_eq!(g.speaker, PlayerId::new("p2"));
    }

    #[test]
    fn a_game_with_no_players_is_refused() {
        assert_eq!(
            start_game(content(), &[], POK, None).unwrap_err(),
            SetupError::NoPlayers
        );
    }

    #[test]
    fn every_dealt_card_has_an_initiative_number() {
        let g = start_game(content(), &players(6), FULL, None).unwrap();
        for card in &g.unclaimed_strategy_cards {
            assert!(
                g.card_initiative.contains_key(card),
                "{card} has no initiative"
            );
        }
    }

    #[test]
    fn the_seeded_speaker_is_reproducible_and_varies_across_seeds() {
        assert_eq!(seeded_speaker(&[], 1), None);
        for n in 1..=8 {
            let table = players(n);
            let mut seen = std::collections::BTreeSet::new();
            for seed in 0..200_u64 {
                let speaker = seeded_speaker(&table, seed).unwrap();
                assert!(table.contains(&speaker));
                assert_eq!(
                    seeded_speaker(&table, seed),
                    Some(speaker.clone()),
                    "{n}p seed {seed}"
                );
                seen.insert(speaker);
            }
            assert_eq!(
                seen.len(),
                n,
                "every seat of a {n}p table opens as speaker for some seed"
            );
        }
    }

    #[test]
    fn the_seeded_speaker_does_not_shift_the_decks() {
        let table = players(6);
        let a = start_game_seeded(content(), &table, POK, None, 11).unwrap();
        let speaker = seeded_speaker(&table, 11);
        let b = start_game_seeded(content(), &table, POK, speaker.clone(), 11).unwrap();
        assert_eq!(b.speaker, speaker.unwrap());
        assert_eq!(a.objective_deck, b.objective_deck);
        assert_eq!(a.agenda_deck, b.agenda_deck);
        assert_eq!(a.action_card_deck, b.action_card_deck);
        assert_eq!(a.seating_order, b.seating_order);
    }
}
