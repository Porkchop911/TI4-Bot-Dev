//! The secondary preview of the hosted game answers as the server's session does: at each
//! question and each step of a game with stepping, the page asks for the secondaries of the other
//! seats, and every reply equals what `ti4_server::session::preview::answer` gives for the same
//! position (published state, decision log, open question, version and map).

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use ti4_host::preview::PreviewRequest;
use ti4_host::protocol::server::{PreviewResult, SecondaryPreviewMsg, ServerMessage};
use ti4_model::id::PlayerId;
use ti4_server::session::worker::{PendingDecision, PendingSubmissionState, SessionShared};
use ti4_wasm::host_api;

use crate::common::{pending, send, serial};

/// What the replies were: previews computed, refusals.
#[derive(Default)]
struct Tally {
    previews: usize,
    refusals: usize,
}

/// What the server's session answers to `request` of `seat` at the hosted game's position.
fn servers_answer(
    messages: &[Value],
    seat: &PlayerId,
    request: &PreviewRequest,
) -> SecondaryPreviewMsg {
    let (game_id, state, galaxy, log, version) = host_api::with(|session| {
        (
            session.game_id().to_owned(),
            session.state().clone(),
            session.galaxy().cloned(),
            session.decision_log().to_vec(),
            session.version(),
        )
    })
    .expect("a game");
    let mut shared = SessionShared::new(game_id.clone(), state.clone());
    shared.latest_state = state;
    shared.decision_log = log;
    shared.game_version = version;
    shared.replay_complete = true;
    shared.preview_galaxy = galaxy.map(Arc::new);
    shared.pending_decision =
        pending(messages).map(|(nonce, version, choice, _)| PendingDecision {
            seat: choice.player.clone(),
            nonce,
            game_version: version,
            choice,
            submission_state: PendingSubmissionState::AwaitingSubmission,
            submitted_option_id: None,
            reply_tx: None,
        });
    ti4_server::session::preview::answer(&Mutex::new(shared), &game_id, seat, request)
}

#[test]
fn the_hosted_preview_answers_as_the_server_does() {
    let _serial = serial();
    ti4_wasm::set_round_limit_native(Some(1));
    let tally = Arc::new(Mutex::new(Tally::default()));
    let counted = tally.clone();
    let viewer = PlayerId::new("a");
    let mut request_id = 0_u64;
    ti4_wasm::set_native_host(move |output| {
        let envelope: Value = serde_json::from_str(output).expect("an envelope");
        if envelope["kind"] == "progress" {
            return host_api::CONTINUE;
        }
        let messages = envelope["messages"].as_array().expect("messages").clone();
        let waiting = envelope["waiting"].is_string();
        // Every card another seat holds, asked as that seat's secondary, and one as the primary.
        let held: Vec<(String, String)> = host_api::with(|session| {
            session
                .state()
                .players
                .iter()
                .flat_map(|player| {
                    player
                        .strategy_cards
                        .iter()
                        .map(|card| (card.to_string(), player.id.to_string()))
                })
                .collect()
        })
        .expect("a game");
        for (card, primary) in held {
            request_id += 1;
            let request = PreviewRequest {
                request_id,
                card: card.clone(),
                primary: primary.clone(),
                answers: Vec::new(),
            };
            let reply = send(&json!({
                "type": "preview_secondary", "protocol_version": 4, "game_id": "local",
                "request_id": request_id, "card": card, "primary": primary,
            }));
            let replies: Vec<ServerMessage> = reply["messages"]
                .as_array()
                .expect("messages")
                .iter()
                .map(|message| serde_json::from_value(message.clone()).expect("a ServerMessage"))
                .collect();
            let [ServerMessage::SecondaryPreview(hosted)] = replies.as_slice() else {
                panic!("one secondary_preview: {reply}");
            };
            assert_eq!(hosted, &servers_answer(&messages, &viewer, &request));
            let mut tally = counted.lock().unwrap();
            match hosted.outcome {
                PreviewResult::Preview { .. } => tally.previews += 1,
                PreviewResult::Refused { .. } => tally.refusals += 1,
            }
        }
        let reply = if waiting {
            send(&json!({ "type": "step" }))
        } else {
            let (nonce, version, choice, _) = pending(&messages).expect("a question");
            send(&json!({
                "type": "submit_choice", "protocol_version": 4, "game_id": "local",
                "nonce": nonce, "expected_version": version, "option_id": choice.options[0].id,
            }))
        };
        i32::try_from(reply["resume"].as_i64().expect("the answer resumes")).unwrap()
    });
    let request = json!({ "local": { "seed": 5, "players": 4, "humans": 1 }, "stepping": 1 });
    let len = host_api::put_input(&request.to_string());
    let status = ti4_wasm::ti4_host_start(u32::try_from(len).unwrap());
    assert!(status >= 0, "{}", host_api::out_text());
    ti4_wasm::set_round_limit_native(None);
    let tally = tally.lock().unwrap();
    assert!(tally.previews > 0, "a secondary was previewed");
    assert!(tally.refusals > 0, "a request was refused");
}
