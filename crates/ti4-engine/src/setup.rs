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
    if player_ids.is_empty() {
        return Err(SetupError::NoPlayers);
    }
    let (cards, initiative) = strategy_card_setup(content, sources)?;
    let mut state = GameState::new(
        player_ids,
        &cards,
        initiative,
        speaker,
        cards_per_player(player_ids.len()),
    );
    let decks = build_starting_decks(content, sources, deck_seed);
    state.objective_deck = decks.objectives;
    state.exploration_decks = decks.exploration;
    state.relic_deck = decks.relics;
    state.agenda_deck = decks.agendas;
    state.action_card_deck = decks.action_cards;
    state.secret_deck = decks.secrets;

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
}
