//! A scripted "page" for the hosted game: answers the questions of the person with the rule
//! bot, and runs commands at chosen points. Used by the history, save and batch tests.
#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use ti4_engine::choice::Choice;
use ti4_model::id::PlayerId;
use ti4_wasm::host_api;

/// The exports keep their game in statics: one game at a time.
static SERIAL: Mutex<()> = Mutex::new(());

/// Hold this for the length of a test (a test that failed does not stop the others).
pub fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub const GAME: &str = "local";

/// What the page does at the next question.
pub enum Act {
    /// Answer `n` questions with the rule bot.
    Answer(usize),
    /// Answer with the rule bot until the question satisfies the predicate (it stays asked).
    Until(Box<dyn Fn(&Choice) -> bool + Send>),
    /// Send a command; if it resumes the game, that is the answer to this question.
    Cmd(Value),
    /// Look at the page (everything received so far).
    Probe(Box<dyn FnMut(&mut Page) + Send>),
    /// Leave the game.
    Leave,
}

#[derive(Default)]
pub struct Page {
    /// Every `ServerMessage` the page received, in order (asks and commands).
    pub messages: Vec<Value>,
    /// The ids the page answered with by the rule bot or by a command.
    pub answers: Vec<String>,
    /// The `ask` envelopes, as received.
    pub asks: Vec<Value>,
    /// The `command` envelopes, as received.
    pub replies: Vec<Value>,
    pub progress: Vec<(u64, u64)>,
    pub script: VecDeque<Act>,
}

pub fn send(command: &Value) -> Value {
    let text = command.to_string();
    let len = host_api::put_input(&text);
    let status = host_api::command(len);
    let out = host_api::out_text();
    assert!(status >= 0, "command {text} failed: {out}");
    serde_json::from_str(&out).expect("an envelope is JSON")
}

/// The question the page is asked: its nonce, the version, the choice, the decisions so far.
pub fn pending(messages: &[Value]) -> Option<(String, u64, Choice, usize)> {
    messages.iter().rev().find_map(|message| {
        let kind = message["type"].as_str()?;
        if kind != "state_update" && kind != "initial_snapshot" {
            return None;
        }
        let pending = message.get("pending_choice")?;
        let choice: Choice = serde_json::from_value(pending["choice"].clone()).ok()?;
        Some((
            pending["nonce"].as_str()?.to_owned(),
            message["game_version"].as_u64()?,
            choice,
            usize::try_from(message["history"]["cursor"].as_u64().unwrap_or(0)).unwrap(),
        ))
    })
}

pub fn install(page: &Arc<Mutex<Page>>, seed: u64, viewer: &str) {
    let page = page.clone();
    let viewer = PlayerId::new(viewer);
    ti4_wasm::set_native_host(move |text: &str| {
        let envelope: Value = serde_json::from_str(text).expect("the output is JSON");
        let mut page = page.lock().unwrap();
        match envelope["kind"].as_str() {
            Some("progress") => {
                page.progress.push((
                    envelope["progress"]["done"].as_u64().unwrap(),
                    envelope["progress"]["total"].as_u64().unwrap(),
                ));
                return host_api::CONTINUE;
            }
            Some("ask") => {}
            other => panic!("the game asked with a {other:?} output"),
        }
        page.messages
            .extend(envelope["messages"].as_array().unwrap().clone());
        page.asks.push(envelope);
        loop {
            let (nonce, version, choice, cursor) =
                pending(&page.messages).expect("an ask has a pending choice");
            let answer_by_policy = |page: &mut Page| -> i32 {
                let (picked, _) = ti4_rule_bot::choose(&choice, None, seed, &viewer, cursor);
                let option = choice.options[picked].id.clone();
                let answered = send(&json!({
                    "type": "submit_choice", "protocol_version": 3, "game_id": GAME,
                    "nonce": nonce, "expected_version": version, "option_id": option,
                }));
                page.messages
                    .extend(answered["messages"].as_array().unwrap().clone());
                page.answers.push(option);
                let resume = answered["resume"].as_i64().unwrap_or_else(|| {
                    panic!("an answer resumes the game: {answered}");
                });
                page.replies.push(answered);
                i32::try_from(resume).unwrap()
            };
            match page.script.pop_front() {
                None => return answer_by_policy(&mut page),
                Some(Act::Answer(0)) => {}
                Some(Act::Answer(n)) => {
                    page.script.push_front(Act::Answer(n - 1));
                    return answer_by_policy(&mut page);
                }
                Some(Act::Until(predicate)) => {
                    if !predicate(&choice) {
                        page.script.push_front(Act::Until(predicate));
                        return answer_by_policy(&mut page);
                    }
                }
                Some(Act::Cmd(command)) => {
                    let reply = send(&command);
                    page.messages
                        .extend(reply["messages"].as_array().unwrap().clone());
                    let resume = reply["resume"].as_i64();
                    page.replies.push(reply);
                    if let Some(resume) = resume {
                        return i32::try_from(resume).unwrap();
                    }
                }
                Some(Act::Probe(mut probe)) => probe(&mut page),
                Some(Act::Leave) => return -1,
            }
        }
    });
}

/// Start a game; returns the `end` envelope.
pub fn start(setup: &Value, resume: Option<&Value>) -> Value {
    let request = json!({ "setup": setup, "resume": resume });
    let len = host_api::put_input(&request.to_string());
    let status = ti4_wasm::ti4_host_start(u32::try_from(len).unwrap());
    let out = host_api::out_text();
    assert!(status >= 0, "start failed: {out}");
    serde_json::from_str(&out).expect("the end is JSON")
}

pub fn setup(seed: u64, players: usize) -> Value {
    json!({ "game_id": GAME, "seed": seed, "players": players, "human_seat": 0 })
}

/// The last `state_update` or `initial_snapshot` that has a pending choice.
pub fn last_question(messages: &[Value]) -> &Value {
    messages
        .iter()
        .rev()
        .find(|m| {
            matches!(
                m["type"].as_str(),
                Some("state_update" | "initial_snapshot")
            ) && m["pending_choice"].is_object()
        })
        .expect("a question")
}

/// An event without its time and version.
pub fn plain_event(event: &Value) -> Value {
    let mut event = event.clone();
    let object = event.as_object_mut().unwrap();
    object.remove("timestamp");
    object.remove("version");
    event
}
