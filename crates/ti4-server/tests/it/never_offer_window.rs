//! The "never offer me this card" seat preference (`session::reaction_modes`) through a real
//! reaction window: a skipped window is journaled as an ordinary decline. Moved from the engine
//! (2026-10-07): seat preferences are a server concern; the engine asks every question.

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_model::content_types::POK;
use ti4_model::id::{ActionCardId, PlayerId};
use ti4_model::state::GameState;

fn payload(pairs: &[(&str, serde_json::Value)]) -> BTreeMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

/// What [`asked_with_never`] saw: the questions asked, the decisions recorded as (seat, answer),
/// and the cards of each skip note.
type Asked = (Vec<String>, Vec<(String, String)>, Vec<Vec<String>>);

/// Emit through an armed resolver with seat `reactor` answered by `NeverOffer` over a decider
/// that records every question it is really asked. Returns (questions asked, decisions
/// recorded, skip notes).
fn asked_with_never(
    mut state: GameState,
    reactor: &str,
    never: &[&str],
    event_type: &str,
    payload: BTreeMap<String, serde_json::Value>,
) -> Asked {
    use std::sync::{Arc, Mutex};
    struct Recorder(Arc<Mutex<Vec<String>>>);
    impl ti4_engine::choice::Decider for Recorder {
        fn choose(
            &mut self,
            choice: &ti4_engine::choice::Choice,
        ) -> Result<ti4_engine::choice::ChoiceOption, ti4_engine::choice::IllegalChoice> {
            self.0.lock().unwrap().push(choice.prompt.clone());
            ti4_engine::choice::AlwaysDecline.choose(choice)
        }
    }
    let asked = Arc::new(Mutex::new(Vec::new()));
    let skipped = Arc::new(Mutex::new(Vec::new()));
    let set: ti4_server::session::reaction_modes::NeverSet = Arc::new(Mutex::new(
        never
            .iter()
            .map(|name| ((*name).to_owned(), ti4_model::state::ReactionMode::Never))
            .collect(),
    ));
    let sink = skipped.clone();
    let wrapper = ti4_server::session::reaction_modes::NeverOffer::new(
        Box::new(Recorder(asked.clone())),
        set,
    )
    .on_skip(move |_, cards| sink.lock().unwrap().push(cards.to_vec()));
    let mut table = ti4_engine::choice::Table::with_default(Box::new(Recorder(asked.clone())));
    table.seat(PlayerId::new(reactor), Box::new(wrapper));
    let seats: Vec<PlayerId> = state.players.iter().map(|seat| seat.id.clone()).collect();
    let mut resolver = ti4_engine::timing::Resolver::new(
        seats.clone(),
        seats.first().cloned(),
        ti4_engine::choice::Table::default(),
    );
    ti4_engine::reactions::arm(&mut resolver, &state);
    let mut dice = ti4_engine::dice::Dice::new();
    let mut rng = ti4_engine::rng::GameRng::new(0);
    let mut event_sequence = ti4_engine::event::EventSequence::new();
    let mut context = ti4_engine::timing::TimingContext {
        state: &mut state,
        content: ContentStore::embedded(),
        sources: POK,
        table: &mut table,
        dice: &mut dice,
        rng: &mut rng,
        event_sequence: &mut event_sequence,
        galaxy: None,
    };
    let event = context.event_sequence.next(event_type, payload).unwrap();
    resolver
        .emit_with_context(&mut context, event, |_, _| {})
        .unwrap();
    let recorded = table
        .log
        .records
        .iter()
        .map(|record| (record.player.to_string(), record.chosen.clone()))
        .collect();
    let questions = asked.lock().unwrap().clone();
    let notes = skipped.lock().unwrap().clone();
    (questions, recorded, notes)
}

#[test]
fn a_never_card_window_is_declined_unasked_and_journaled_as_a_decline() {
    let build = || {
        let mut state = ti4_engine::fixtures::game(&["a", "b"]);
        state
            .player_mut(&PlayerId::new("a"))
            .unwrap()
            .action_cards
            .clear();
        state.player_mut(&PlayerId::new("b")).unwrap().action_cards =
            vec![ActionCardId::new("sabo1"), ActionCardId::new("sabo2")];
        state
    };
    let play = || payload(&[("player", "a".into()), ("card", "fs1".into())]);
    // Offered as before when the seat has no preference.
    let (asked, recorded, notes) =
        asked_with_never(build(), "b", &[], "ACTION_CARD_PLAYED", play());
    assert_eq!(asked.len(), 1, "the window is asked: {asked:?}");
    assert_eq!(recorded, vec![("b".to_owned(), "decline".to_owned())]);
    assert!(notes.is_empty());
    // Never: all copies by name, nothing asked, the same decline is journaled.
    let (asked, recorded, notes) =
        asked_with_never(build(), "b", &["Sabotage"], "ACTION_CARD_PLAYED", play());
    assert!(asked.is_empty(), "skipped without asking: {asked:?}");
    assert_eq!(recorded, vec![("b".to_owned(), "decline".to_owned())]);
    assert_eq!(notes, vec![vec!["Sabotage".to_owned()]]);
    // Another card name set to Never leaves Sabotage offered.
    let (asked, _, notes) =
        asked_with_never(build(), "b", &["Flank Speed"], "ACTION_CARD_PLAYED", play());
    assert_eq!(asked.len(), 1);
    assert!(notes.is_empty());
    // The preference belongs to one seat: the wrapper sits on "a", the holder is "b".
    let (asked, _, notes) =
        asked_with_never(build(), "a", &["Sabotage"], "ACTION_CARD_PLAYED", play());
    assert_eq!(asked.len(), 1);
    assert!(notes.is_empty());
}

#[test]
fn a_never_card_leaves_the_same_seats_other_cards_offered() {
    let mut state = ti4_engine::fixtures::game(&["a", "b"]);
    state
        .player_mut(&PlayerId::new("b"))
        .unwrap()
        .action_cards
        .clear();
    state.player_mut(&PlayerId::new("a")).unwrap().action_cards =
        vec![ActionCardId::new("silence_space"), ActionCardId::new("fs1")];
    let (asked, _, notes) = asked_with_never(
        state,
        "a",
        &["Sabotage"],
        "SYSTEM_ACTIVATED",
        payload(&[("player", "a".into()), ("system", "27".into())]),
    );
    assert!(
        !asked.is_empty(),
        "the seat is still asked about its other cards"
    );
    assert!(notes.is_empty());
}
