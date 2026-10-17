//! Declaring which reactions a seat bluffs about (M18, step 1): who may declare, the bounds, the
//! cooldown, idempotent resends, and that nothing of it is visible to other seats or saved.

use std::fs;
use std::sync::Arc;

use ti4_host::protocol::server::ServerMessage;
use ti4_host::protocol::status::ViewerRole;
use ti4_model::id::PlayerId;
use ti4_server::dev::launch_scenario;
use ti4_server::session::{GameRegistry, GameSession, MockClient};
use ti4_server::storage::FileGameStore;

struct Seats {
    sol: PlayerId,
    hacan: PlayerId,
    letnev: PlayerId,
}

/// Sol holds five action cards, Letnev one (Sabotage), Hacan none.
fn launch(registry: &Arc<GameRegistry>) -> (Arc<GameSession>, Seats, String) {
    let launch =
        launch_scenario(registry, "ongoing_combat_four_views", Some(54_322)).expect("launch");
    let session = registry.get_game(&launch.game_id).expect("game");
    let state = session.current_state();
    let by = |faction: &str| {
        state
            .players
            .iter()
            .find(|player| player.faction.as_str() == faction)
            .map(|player| player.id.clone())
            .expect("seat")
    };
    let seats = Seats {
        sol: by("sol"),
        hacan: by("hacan"),
        letnev: by("letnev"),
    };
    (session, seats, launch.game_id)
}

fn declare(session: &GameSession, seat: &PlayerId, triggers: &[&str]) -> Result<(), String> {
    let triggers: Vec<String> = triggers.iter().map(|t| (*t).to_owned()).collect();
    session.set_reaction_intent(seat, &triggers)
}

#[test]
fn only_known_triggers_within_the_limit_and_the_first_declaration_is_free() {
    let registry = Arc::new(GameRegistry::new());
    let (session, seats, _) = launch(&registry);
    let letnev = &seats.letnev;
    assert!(
        declare(&session, letnev, &["nonsense"])
            .unwrap_err()
            .contains("unknown")
    );
    assert!(
        declare(
            &session,
            letnev,
            &["agenda", "movement", "production", "turn_end"]
        )
        .unwrap_err()
        .contains("at most 3")
    );
    assert!(session.reaction_intent_state(letnev).triggers.is_empty());
    declare(&session, letnev, &["movement", "agenda"]).expect("first declaration");
    assert_eq!(
        session.reaction_intent_state(letnev).triggers,
        ["agenda", "movement"]
    );
}

#[test]
fn a_change_waits_for_the_next_round_but_a_resend_and_a_clear_do_not() {
    let registry = Arc::new(GameRegistry::new());
    let (session, seats, _) = launch(&registry);
    let letnev = &seats.letnev;
    declare(&session, letnev, &["movement", "agenda"]).expect("first");
    // A reconnect resends the same set (any order, repeats): a no-op that counts for nothing.
    declare(&session, letnev, &["agenda", "movement", "agenda"]).expect("idempotent resend");
    let state = session.reaction_intent_state(letnev);
    assert_eq!(state.triggers, ["agenda", "movement"]);
    assert_eq!(state.locked_until_round, Some(2));

    let error = declare(&session, letnev, &["production"]).unwrap_err();
    assert!(error.contains("after round 1"), "{error}");
    assert_eq!(
        session.reaction_intent_state(letnev).triggers,
        ["agenda", "movement"]
    );

    // Clearing is always possible and does not re-open changes.
    declare(&session, letnev, &[]).expect("clear");
    assert!(session.reaction_intent_state(letnev).triggers.is_empty());
    assert!(declare(&session, letnev, &["production"]).is_err());
    declare(&session, letnev, &[]).expect("idempotent clear");
}

#[test]
fn a_seat_without_cards_or_with_a_never_setting_cannot_declare() {
    let registry = Arc::new(GameRegistry::new());
    let (session, seats, _) = launch(&registry);
    let error = declare(&session, &seats.hacan, &["agenda"]).unwrap_err();
    assert!(error.contains("no action cards"), "{error}");
    let state = session.reaction_intent_state(&seats.hacan);
    assert!(!state.eligible && state.triggers.is_empty());

    session
        .set_reaction_mode(
            &seats.sol,
            "Sabotage",
            ti4_model::state::ReactionMode::Never,
        )
        .expect("mode");
    let error = declare(&session, &seats.sol, &["movement"]).unwrap_err();
    assert!(error.contains("Never"), "{error}");
    assert!(!session.reaction_intent_state(&seats.sol).eligible);
}

#[test]
fn nobody_else_learns_the_declaration_and_nothing_is_saved() {
    let dir =
        std::env::temp_dir().join(format!("ti4_bluff_intent_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&dir).expect("store"));
    let registry = Arc::new(GameRegistry::new().with_store(store));
    let (session, seats, game_id) = launch(&registry);
    let before = files(&dir.join(&game_id));
    let letnev = MockClient::connect(session.clone(), ViewerRole::Player(seats.letnev.clone()));
    let sol = MockClient::connect(session.clone(), ViewerRole::Player(seats.sol.clone()));
    let spectator = MockClient::connect(session.clone(), ViewerRole::Spectator);

    declare(&session, &seats.letnev, &["space_combat"]).expect("declared");

    let mine = letnev.drain_messages();
    assert!(mine.iter().any(|m| matches!(m,
        ServerMessage::ReactionIntentState(s) if s.triggers == ["space_combat"])));
    for other in [&sol, &spectator] {
        let seen = other.drain_messages();
        assert!(seen.is_empty(), "other viewers get nothing: {seen:?}");
    }
    // The snapshot any viewer can fetch carries no trace either.
    for viewer in [ViewerRole::Player(seats.sol.clone()), ViewerRole::Spectator] {
        let wire = serde_json::to_string(&session.get_snapshot(&viewer)).expect("json");
        assert!(!wire.contains("space_combat\"") || !wire.contains("reaction_intent"));
        assert!(!wire.contains("reaction_intent"));
    }
    // Not in anything saved, not in the restart config.
    assert_eq!(
        files(&dir.join(&game_id)),
        before,
        "storage is byte-identical"
    );
    let _ = session.restart_config();
    let _ = fs::remove_dir_all(dir);
}

fn files(dir: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(path) = stack.pop() {
        for entry in fs::read_dir(&path).expect("dir") {
            let entry = entry.expect("entry").path();
            if entry.is_dir() {
                stack.push(entry);
            } else {
                out.push((
                    entry.to_string_lossy().into_owned(),
                    fs::read(&entry).expect("file"),
                ));
            }
        }
    }
    out.sort();
    out
}
