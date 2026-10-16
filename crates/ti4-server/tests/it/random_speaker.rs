//! New lobby games open with a speaker drawn from the seed; `seat` keeps the first seat.

use std::sync::Arc;

use ti4_model::id::PlayerId;
use ti4_server::map::SpeakerOrder;
use ti4_server::session::GameRegistry;
use ti4_server::storage::FileGameStore;

/// Creates a full, all-human lobby of `count`, starts it, and returns the seat order.
fn start(
    registry: &GameRegistry,
    game_id: &str,
    count: usize,
    seed: u64,
    order: SpeakerOrder,
) -> Vec<PlayerId> {
    let (_, _, host_token) = registry
        .create_player_lobby_with_speaker(
            game_id.to_owned(),
            count,
            seed,
            "Host",
            None,
            None,
            ti4_server::card_set::DEFAULT,
            order,
        )
        .unwrap();
    let mut tokens = vec![host_token];
    for i in 1..count {
        let (_, _, token) = registry
            .join_player_lobby(game_id, None, Some(&format!("Guest{i}")))
            .unwrap();
        tokens.push(token.unwrap());
    }
    let view = registry
        .start_player_lobby(game_id, tokens[0].as_str())
        .unwrap();
    assert_eq!(view.speaker_order, order.as_str());
    view.slots
        .iter()
        .map(|slot| slot.occupant.clone().unwrap())
        .collect()
}

#[test]
fn a_lobby_game_opens_with_the_seeded_speaker_and_recovery_keeps_it() {
    let dir = std::env::temp_dir().join(format!(
        "ti4_random_speaker_{:032x}",
        rand::random::<u128>()
    ));
    let store = Arc::new(FileGameStore::new(&dir).unwrap());
    let registry = GameRegistry::new().with_store(store.clone());
    let mut expected = Vec::new();
    let mut not_first_seat = 0;
    for seed in 0..6_u64 {
        let game_id = format!("speaker_{seed}");
        let seats = start(&registry, &game_id, 3, seed, SpeakerOrder::Random);
        let state = registry.get_game(&game_id).unwrap().current_state();
        let drawn = ti4_engine::seeded_speaker(&seats, seed).unwrap();
        assert_eq!(state.speaker, drawn, "seed {seed}");
        // Seats keep their identities and order; only who opens changes.
        assert_eq!(state.seating_order, seats);
        // The speaker picks the first strategy card.
        assert_eq!(
            ti4_engine::phase::next_strategy_picker(&state),
            Some(drawn.clone())
        );
        if drawn != seats[0] {
            not_first_seat += 1;
        }
        expected.push((game_id, state));
    }
    assert!(not_first_seat > 0, "some seed must open with another seat");

    // A restarted server rebuilds every game from its stored opening state and log.
    let restarted = GameRegistry::new().with_store(store);
    restarted.recover_all_games().unwrap();
    for (game_id, state) in &expected {
        let recovered = restarted.get_game(game_id).unwrap().current_state();
        assert_eq!(recovered.speaker, state.speaker, "{game_id}");
        assert_eq!(&recovered, state, "{game_id}");
        restarted.remove_game(game_id);
        registry.remove_game(game_id);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn seat_order_keeps_the_first_seat_as_speaker() {
    let registry = GameRegistry::new();
    for seed in 0..6_u64 {
        let game_id = format!("seat_{seed}");
        let seats = start(&registry, &game_id, 4, seed, SpeakerOrder::Seat);
        let state = registry.get_game(&game_id).unwrap().current_state();
        assert_eq!(state.speaker, seats[0], "seed {seed}");
        registry.remove_game(&game_id);
    }
}
