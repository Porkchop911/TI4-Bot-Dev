//! The read-only secondary preview on a live session: a follower can ask what the secondary in
//! progress would offer while the primary is still resolving; the answer is exactly what the
//! real question later offers, the preview changes nothing, and the cases that mean nothing are
//! refused with a reason.

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use ti4_content::ContentStore;
use ti4_engine::secondary_preview::SecondaryPreview;
use ti4_host::preview::PreviewRequest;
use ti4_host::protocol::server::{PreviewRefusal, PreviewResult, SecondaryPreviewMsg};
use ti4_host::protocol::status::ViewerRole;
use ti4_model::id::{PlayerId, StrategyCardId};
use ti4_server::session::{GameSession, MockClient, SeatController, SessionConfig};

struct Table {
    session: Arc<GameSession>,
    p1: PlayerId,
    p2: PlayerId,
    card: String,
    c1: MockClient,
    c2: MockClient,
}

/// p1 (human) holds Technology and is in its action; p2 (human) and p3 (bot) follow.
fn start(card_name: &str, card_id: &str) -> Table {
    let content = ContentStore::embedded();
    let p1 = PlayerId::new("p1");
    let p2 = PlayerId::new("p2");
    let p3 = PlayerId::new("p3");
    let ids = vec![p1.clone(), p2.clone(), p3.clone()];
    let (mut state, galaxy) =
        ti4_host::map::create_game_with_template(content, &ids, 4242, None).expect("game");
    let card = StrategyCardId::new(card_id);
    state.unclaimed_strategy_cards.retain(|c| c != &card);
    state.player_mut(&p1).unwrap().strategy_cards = vec![card.clone()];
    ti4_engine::phase::advance_phase(&mut state);
    ti4_engine::phase::begin_action_turn(&mut state, &p1);
    for seat in &mut state.players {
        seat.strategic_tokens = 2;
        seat.trade_goods = 6;
    }
    let tiles = ti4_host::map::build_board_tiles(content, &galaxy);
    let config = SessionConfig::new(format!("preview_{card_name}"), state)
        .with_player_ids(ids)
        .with_galaxy(galaxy, tiles)
        .with_seat(p1.clone(), SeatController::Human)
        .with_seat(p2.clone(), SeatController::Human)
        .with_seat(p3, SeatController::BotFirstOption);
    let session = Arc::new(GameSession::start(config));
    let c1 = MockClient::connect(session.clone(), ViewerRole::Player(p1.clone()));
    let c2 = MockClient::connect(session.clone(), ViewerRole::Player(p2.clone()));
    Table {
        session,
        p1,
        p2,
        card: card_id.to_owned(),
        c1,
        c2,
    }
}

fn wait_until(what: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if ready() {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for {what}");
}

fn pending(table: &Table) -> Option<(PlayerId, String, u64)> {
    table.session.current_pending_decision()
}

fn pending_prompt(table: &Table, seat: &PlayerId) -> Option<String> {
    let client = if seat == &table.p1 {
        &table.c1
    } else {
        &table.c2
    };
    client
        .snapshot()
        .pending_choice
        .map(|envelope| envelope.choice.prompt)
}

fn ask(table: &Table, seat: &PlayerId, answers: &[&str]) -> SecondaryPreviewMsg {
    table.session.preview_secondary(
        seat,
        &PreviewRequest {
            request_id: 1,
            card: table.card.clone(),
            primary: table.p1.to_string(),
            answers: answers.iter().map(ToString::to_string).collect(),
        },
    )
}

/// p1 takes the strategic action and is then asked to research (the primary), so the game is
/// mid-step with p2 not yet asked.
fn into_the_primary(table: &Table) {
    wait_until("p1's action", || {
        pending(table).is_some_and(|(seat, _, _)| seat == table.p1)
            && pending_prompt(table, &table.p1).as_deref() == Some("action phase")
    });
    let (_, nonce, version) = pending(table).unwrap();
    table
        .c1
        .submit(&nonce, version, "strategic")
        .expect("strategic accepted");
    wait_until("the primary's research question", || {
        pending_prompt(table, &table.p1).as_deref() == Some("research a technology")
    });
}

/// From rules version `RULES_TECH_PAYMENT_CHOICE` a paid research asks the researcher which
/// planets pay: the first offered payment is taken.
fn pay_if_asked(table: &Table) {
    let Some(choice) = table.c1.snapshot().pending_choice.map(|e| e.choice) else {
        return;
    };
    let is_payment = choice
        .context
        .as_ref()
        .is_some_and(|context| context.subtype == "pay_resources");
    if !is_payment {
        return;
    }
    let Some((seat, nonce, version)) = pending(table) else {
        return;
    };
    if seat != table.p1 {
        return;
    }
    let option = choice
        .options
        .first()
        .expect("a payment is offered")
        .id
        .clone();
    table.c1.submit(&nonce, version, &option).expect("p1 pays");
    thread::sleep(Duration::from_millis(150));
}

/// The primary researches the first technology that needs no faction waiver, which ends its part.
fn finish_primary(table: &Table) {
    let options: Vec<String> = table
        .c1
        .snapshot()
        .pending_choice
        .expect("the primary is asked")
        .choice
        .options
        .iter()
        .map(|option| option.id.clone())
        .collect();
    for option in options {
        let Some((seat, nonce, version)) = pending(table) else {
            return;
        };
        if seat != table.p1 {
            return;
        }
        table
            .c1
            .submit(&nonce, version, &option)
            .expect("p1 researches");
        thread::sleep(Duration::from_millis(150));
        pay_if_asked(table);
        if pending_prompt(table, &table.p1).as_deref() != Some("research a technology") {
            return;
        }
    }
    panic!("no technology could be researched by the primary");
}

fn preview_of(message: &SecondaryPreviewMsg) -> &SecondaryPreview {
    match &message.outcome {
        PreviewResult::Preview { preview } => preview,
        PreviewResult::Refused { reason, detail } => panic!("refused {reason:?}: {detail}"),
    }
}

#[test]
fn a_follower_previews_exactly_what_the_real_question_then_offers() {
    let table = start("technology", "pok7technology");
    into_the_primary(&table);

    // The primary is still resolving: p2 asks what the secondary would offer, and what follows a yes.
    let window = ask(&table, &table.p2, &[]);
    let SecondaryPreview::Question {
        choice: window_question,
        step: 0,
        ..
    } = preview_of(&window)
    else {
        panic!("expected the window question, got {window:?}");
    };
    let research = ask(&table, &table.p2, &["yes"]);
    let SecondaryPreview::Question {
        choice: research_question,
        payment,
        step: 1,
        ..
    } = preview_of(&research)
    else {
        panic!("expected the research question, got {research:?}");
    };
    let payment = payment.as_ref().expect("the payment plan is previewed");
    assert_eq!(payment.cost, 4);
    assert_eq!(window.card, "pok7technology");
    assert!(window.as_of_decisions > 0);

    // Now p1 finishes the primary; p2 is really asked the same two questions.
    finish_primary(&table);
    wait_until("p2's secondary question", || {
        pending(&table).is_some_and(|(seat, _, _)| seat == table.p2)
    });
    let real = table.c2.snapshot().pending_choice.unwrap().choice;
    assert_eq!(
        &real, window_question,
        "the window question is exactly the preview"
    );
    let (_, nonce, version) = pending(&table).unwrap();
    table.c2.submit(&nonce, version, "yes").expect("p2 follows");
    wait_until("p2's research question", || {
        pending_prompt(&table, &table.p2).as_deref() == Some("research a technology")
    });
    let real = table.c2.snapshot().pending_choice.unwrap().choice;
    assert_eq!(
        &real, research_question,
        "the follow-up is exactly the preview"
    );

    // Once asked, a follower is refused.
    let late = ask(&table, &table.p2, &[]);
    assert!(matches!(
        late.outcome,
        PreviewResult::Refused {
            reason: PreviewRefusal::AlreadyAsked,
            ..
        }
    ));
    table.session.stop();
}

#[test]
fn the_primary_and_a_game_without_an_action_in_progress_are_refused() {
    let table = start("tech_refusals", "pok7technology");
    wait_until("p1's action", || {
        pending_prompt(&table, &table.p1).as_deref() == Some("action phase")
    });
    // Nothing is in progress yet.
    let early = ask(&table, &table.p2, &[]);
    assert!(matches!(
        early.outcome,
        PreviewResult::Refused {
            reason: PreviewRefusal::NoStrategicAction,
            ..
        }
    ));
    into_the_primary(&table);
    let own = ask(&table, &table.p1, &[]);
    assert!(matches!(
        own.outcome,
        PreviewResult::Refused {
            reason: PreviewRefusal::IsPrimary,
            ..
        }
    ));
    // A different card than the one in progress.
    let mut wrong = PreviewRequest {
        request_id: 2,
        card: "pok1leadership".to_owned(),
        primary: table.p1.to_string(),
        answers: vec![],
    };
    let refused = table.session.preview_secondary(&table.p2, &wrong);
    assert!(matches!(
        refused.outcome,
        PreviewResult::Refused {
            reason: PreviewRefusal::NoStrategicAction,
            ..
        }
    ));
    // A primary that did not play it.
    wrong.card = table.card.clone();
    wrong.primary = "p3".to_owned();
    let refused = table.session.preview_secondary(&table.p2, &wrong);
    assert!(matches!(
        refused.outcome,
        PreviewResult::Refused {
            reason: PreviewRefusal::NoStrategicAction,
            ..
        }
    ));
    table.session.stop();
}

#[test]
fn a_preview_changes_nothing_in_the_live_session() {
    let table = start("tech_readonly", "pok7technology");
    into_the_primary(&table);
    let state = serde_json::to_vec(&table.session.current_state()).unwrap();
    let log = table.session.decision_log();
    let version = table.session.game_version();
    let pending_before = pending(&table);
    let history = serde_json::to_vec(&table.session.replay_export().0).unwrap();
    for answers in [&[][..], &["yes"][..], &["no"][..], &["yes", "zzz"][..]] {
        let _ = ask(&table, &table.p2, answers);
    }
    assert_eq!(
        state,
        serde_json::to_vec(&table.session.current_state()).unwrap()
    );
    assert_eq!(log, table.session.decision_log());
    assert_eq!(version, table.session.game_version());
    assert_eq!(pending_before, pending(&table));
    assert_eq!(
        history,
        serde_json::to_vec(&table.session.replay_export().0).unwrap()
    );
    // The game still plays on: the preview did not disturb the worker.
    finish_primary(&table);
    wait_until("p2's real question", || {
        pending(&table).is_some_and(|(seat, _, _)| seat == table.p2)
    });
    table.session.stop();
}

#[test]
fn the_answer_does_not_depend_on_other_seats_hidden_information() {
    // Two games that differ only in what the OTHER seats hold privately give p2 the same preview.
    let preview_with = |tweak: &dyn Fn(&mut ti4_model::state::GameState)| {
        let content = ContentStore::embedded();
        let ids = vec![
            PlayerId::new("p1"),
            PlayerId::new("p2"),
            PlayerId::new("p3"),
        ];
        let (mut state, _) =
            ti4_host::map::create_game_with_template(content, &ids, 4242, None).expect("game");
        let card = StrategyCardId::new("pok7technology");
        state.unclaimed_strategy_cards.retain(|c| c != &card);
        state.player_mut(&ids[0]).unwrap().strategy_cards = vec![card.clone()];
        for seat in &mut state.players {
            seat.strategic_tokens = 2;
            seat.trade_goods = 6;
        }
        tweak(&mut state);
        ti4_engine::secondary_preview::preview_secondary(
            &state,
            content,
            ti4_model::content_types::POK,
            None,
            &card,
            &ids[0],
            &ids[1],
            &["yes".to_owned()],
        )
    };
    let plain = preview_with(&|_| {});
    let tweaked = preview_with(&|state| {
        for seat in &mut state.players {
            if seat.id.as_str() != "p2" {
                seat.action_cards.clear();
                seat.trade_goods += 3;
                seat.commodities = 0;
            }
        }
    });
    assert_eq!(plain, tweaked);
}

fn files(dir: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(path) = stack.pop() {
        for entry in std::fs::read_dir(&path).expect("dir") {
            let entry = entry.expect("entry").path();
            if entry.is_dir() {
                stack.push(entry);
            } else {
                out.push((
                    entry.to_string_lossy().into_owned(),
                    std::fs::read(&entry).expect("file"),
                ));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn a_preview_writes_nothing_to_storage() {
    use ti4_server::dev::launch_scenario;
    use ti4_server::session::GameRegistry;
    use ti4_server::storage::FileGameStore;
    let dir =
        std::env::temp_dir().join(format!("ti4_preview_store_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&dir).expect("store"));
    let registry = Arc::new(GameRegistry::new().with_store(store));
    // Sol (human) has played Technology and is being asked its research; a bot seat follows.
    let launch = launch_scenario(&registry, "research_tech_skips", Some(17)).expect("launch");
    let session = registry.get_game(&launch.game_id).expect("game");
    let state = session.current_state();
    let Some(ViewerRole::Player(primary)) = session.viewer_for_seat_token(&launch.player_session)
    else {
        panic!("the launch token names a seat");
    };
    let follower = state
        .players
        .iter()
        .map(|seat| seat.id.clone())
        .find(|id| id != &primary)
        .expect("another seat");
    let before = files(&dir.join(&launch.game_id));
    let history = serde_json::to_vec(&session.replay_export().0).unwrap();
    let request = PreviewRequest {
        request_id: 1,
        card: "pok7technology".to_owned(),
        primary: primary.to_string(),
        answers: vec![],
    };
    let mut answered = 0;
    for answers in [&[][..], &["yes"][..], &["no"][..], &["yes", "zzz"][..]] {
        let message = session.preview_secondary(
            &follower,
            &PreviewRequest {
                answers: answers.iter().map(ToString::to_string).collect(),
                ..request.clone()
            },
        );
        if matches!(message.outcome, PreviewResult::Preview { .. }) {
            answered += 1;
        }
    }
    assert!(
        answered >= 2,
        "the follower is previewed while the primary resolves"
    );
    assert_eq!(
        files(&dir.join(&launch.game_id)),
        before,
        "storage is byte-identical"
    );
    assert_eq!(
        history,
        serde_json::to_vec(&session.replay_export().0).unwrap()
    );
    session.stop();
    let _ = std::fs::remove_dir_all(dir);
}

/// Answers the primary's Leadership questions (three pools, then no purchase) so p2's window opens.
fn finish_leadership_primary(table: &Table) {
    for _ in 0..8 {
        let Some((seat, nonce, version)) = pending(table) else {
            return;
        };
        if seat != table.p1 {
            return;
        }
        let prompt = pending_prompt(table, &table.p1).unwrap_or_default();
        let answer = if prompt.contains("which pool") {
            "tactic_tokens"
        } else {
            "no"
        };
        table
            .c1
            .submit(&nonce, version, answer)
            .expect("p1 answers");
        thread::sleep(Duration::from_millis(150));
    }
}

/// Leadership on a live session: the whole payment of a purchase is previewed by scripted
/// answers (payment, pool, again), each question is exactly the one the follower is then really
/// asked, and none of it changes the game.
#[test]
fn a_leadership_purchase_is_previewed_and_then_asked_exactly_that_way() {
    let table = start("leadership", "pok1leadership");
    wait_until("p1's action", || {
        pending(&table).is_some_and(|(seat, _, _)| seat == table.p1)
            && pending_prompt(&table, &table.p1).as_deref() == Some("action phase")
    });
    let (_, nonce, version) = pending(&table).unwrap();
    table
        .c1
        .submit(&nonce, version, "strategic")
        .expect("strategic accepted");
    wait_until("the primary's first token question", || {
        pending_prompt(&table, &table.p1).is_some_and(|prompt| prompt.contains("which pool"))
    });
    let state = serde_json::to_vec(&table.session.current_state()).unwrap();
    let log = table.session.decision_log();
    let version_before = table.session.game_version();
    let history = serde_json::to_vec(&table.session.replay_export().0).unwrap();

    // Script a two-token purchase with the engine's own questions: the first payment option,
    // the tactic pool, "yes" for the second token, "no" at the end.
    let mut answers: Vec<String> = vec!["yes".to_owned()];
    let mut questions = Vec::new();
    let window_message = ask(&table, &table.p2, &[]);
    let SecondaryPreview::Question { choice: window, .. } = preview_of(&window_message) else {
        panic!("the window question");
    };
    questions.push(window.clone());
    let mut bought = 0;
    for _ in 0..40 {
        let refs: Vec<&str> = answers.iter().map(String::as_str).collect();
        let message = ask(&table, &table.p2, &refs);
        match preview_of(&message) {
            SecondaryPreview::Question { choice, .. } => {
                questions.push(choice.clone());
                let subtype = choice.context.as_ref().unwrap().subtype.as_str();
                let pick = match subtype {
                    "pay_influence" => choice.options[0].id.clone(),
                    "gain_command_token" => {
                        bought += 1;
                        "tactic_tokens".to_owned()
                    }
                    "buy_token_with_influence" => if bought >= 2 { "no" } else { "yes" }.to_owned(),
                    other => panic!("unexpected question {other}"),
                };
                answers.push(pick);
            }
            SecondaryPreview::Complete {
                unused_answers: 0, ..
            } => break,
            other => panic!("unexpected preview {other:?}"),
        }
    }
    assert!(bought >= 1, "at least one token could be bought");
    assert!(
        questions.len() >= 3,
        "payment, pool and again were previewed"
    );
    // The whole script replays in one request; a payment the engine does not offer is rejected.
    let refs: Vec<&str> = answers.iter().map(String::as_str).collect();
    assert!(matches!(
        preview_of(&ask(&table, &table.p2, &refs)),
        SecondaryPreview::Complete {
            unused_answers: 0,
            ..
        }
    ));
    assert!(matches!(
        preview_of(&ask(&table, &table.p2, &["yes", "exhaust|nowhere"])),
        SecondaryPreview::Rejected { at: 1, .. }
    ));

    // Read only.
    assert_eq!(
        state,
        serde_json::to_vec(&table.session.current_state()).unwrap()
    );
    assert_eq!(log, table.session.decision_log());
    assert_eq!(version_before, table.session.game_version());
    assert_eq!(
        history,
        serde_json::to_vec(&table.session.replay_export().0).unwrap()
    );

    // The primary finishes; p2 is then really asked, and every question equals its preview.
    finish_leadership_primary(&table);
    wait_until("p2's window", || {
        pending(&table).is_some_and(|(seat, _, _)| seat == table.p2)
    });
    for (index, expected) in questions.iter().enumerate() {
        let real = table
            .c2
            .snapshot()
            .pending_choice
            .expect("p2 is asked")
            .choice;
        assert_eq!(&real, expected, "question {index} is exactly the preview");
        let Some(answer) = answers.get(index) else {
            break;
        };
        let (_, nonce, version) = pending(&table).unwrap();
        table
            .c2
            .submit(&nonce, version, answer)
            .expect("p2 answers");
        if let Some(next) = questions.get(index + 1) {
            let want = next.prompt.clone();
            wait_until("the next question", || {
                pending_prompt(&table, &table.p2).as_deref() == Some(want.as_str())
            });
        }
    }
    table.session.stop();
}
