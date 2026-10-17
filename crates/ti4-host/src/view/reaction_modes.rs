//! A seat's "do not offer me this" preference, applied where the question is asked.
//!
//! A seat sets a mode for a reaction: an action card by its printed name, or the card of an
//! ability by its key ([`crate::view::reaction::ReactionOffer::key`]). `Never` mutes it; `OwnTurn`
//! mutes it while another seat is the active player of the action phase. Outside the action
//! phase nobody has the turn, and `OwnTurn` mutes nothing.
//!
//! The preference is a *session* setting, not game state: it is not in [`GameState`], not in a
//! fingerprint and not in the decision log's inputs. It is applied by [`NeverOffer`], a decider
//! wrapper that sits between the table and the seat's real decider. A window whose every offer
//! the seat has muted is declined without asking; a window with something else left is asked
//! with the muted offers left out. Either way the table records an ordinary answer, so a replay
//! (which answers from the recorded log and never reaches this wrapper) reproduces the game
//! whatever the preference is at replay time.
//!
//! Cards are matched by printed name, so every copy of Sabotage is covered at once.
//!
//! [`GameState`]: ti4_model::state::GameState

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_model::state::ReactionMode;

/// The mode of each reaction that a seat set, by its key. A key that is not there is `Always`.
pub type Modes = BTreeMap<String, ReactionMode>;

/// The modes of a seat, shared with whoever changes them.
pub type NeverSet = Arc<Mutex<Modes>>;

/// Called with the declined choice and the names of what made it a skip.
pub type SkipObserver = Box<dyn FnMut(&Choice, &[String]) + Send>;

/// What the preference does to one question.
#[derive(Debug, Clone, PartialEq)]
pub enum Filtered {
    /// Nothing in it is muted.
    Unchanged,
    /// Some options were muted; the rest still need an answer.
    Narrowed(Box<Choice>),
    /// Every reaction option was muted and a decline exists: answer it, do not ask.
    Declined {
        /// The decline option of the original choice.
        option: ChoiceOption,
        /// The printed names of what was left out, for the note to the player.
        cards: Vec<String>,
    },
}

/// Whether `key` names a reaction that a mode can be set for: the printed name of an action
/// card, or the key of the card of an ability (one of the kinds of `crate::view::reaction`,
/// then the id of a record).
#[must_use]
pub fn is_reaction_key(key: &str) -> bool {
    is_action_card_name(key) || is_ability_key(key)
}

/// Whether `name` is the printed name of some action card in the content corpus.
#[must_use]
pub fn is_action_card_name(name: &str) -> bool {
    ContentStore::embedded()
        .records(ti4_model::content_types::ContentType::ActionCards)
        .iter()
        .any(|record| record.text("name") == Some(name))
}

fn is_ability_key(key: &str) -> bool {
    key.split_once(':').is_some_and(|(kind, id)| {
        matches!(
            kind,
            "leader" | "ability" | "technology" | "promissory" | "unit" | "breakthrough"
        ) && !id.is_empty()
            && id.len() <= 64
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    })
}

/// Set `key` to `mode` in `modes`. `Always` is the mode of a key that is not there.
pub fn set_mode(modes: &mut Modes, key: &str, mode: ReactionMode) {
    if mode == ReactionMode::Always {
        modes.remove(key);
    } else {
        modes.insert(key.to_owned(), mode);
    }
}

/// Whether the seat that `choice` asks is muted for a reaction of `mode` now.
fn muted(choice: &Choice, mode: Option<&ReactionMode>) -> bool {
    match mode {
        Some(ReactionMode::Never) => true,
        Some(ReactionMode::OwnTurn) => choice
            .details
            .get(ti4_engine::timing::ACTIVE_DETAIL)
            .and_then(serde_json::Value::as_str)
            .is_some_and(|active| active != choice.player.as_str()),
        Some(ReactionMode::Always) | None => false,
    }
}

/// Apply the modes of a seat to a question.
#[must_use]
pub fn filter(choice: &Choice, modes: &Modes) -> Filtered {
    // Only a window that the seat may decline: a mode never takes an ability that must resolve.
    if modes.is_empty() || !choice.options.iter().any(ChoiceOption::is_decline) {
        return Filtered::Unchanged;
    }
    let content = ContentStore::embedded();
    let mut dropped: Vec<String> = Vec::new();
    let mut kept: Vec<ChoiceOption> = Vec::new();
    for option in &choice.options {
        let offers = crate::view::reaction::offers_of(content, option);
        if !offers.is_empty()
            && offers
                .iter()
                .all(|offer| muted(choice, modes.get(&offer.key)))
        {
            for offer in offers {
                if !dropped.contains(&offer.name) {
                    dropped.push(offer.name);
                }
            }
        } else {
            kept.push(option.clone());
        }
    }
    if dropped.is_empty() {
        return Filtered::Unchanged;
    }
    if kept.iter().all(ChoiceOption::is_decline) {
        return match kept.into_iter().next() {
            Some(option) => Filtered::Declined {
                option,
                cards: dropped,
            },
            // Nothing left and nothing to decline with: ask the question whole.
            None => Filtered::Unchanged,
        };
    }
    let mut narrowed = choice.clone();
    narrowed.options = kept;
    Filtered::Narrowed(Box::new(narrowed))
}

/// A decider wrapper that honours the modes of a seat before delegating.
pub struct NeverOffer {
    inner: Box<dyn Decider>,
    never: NeverSet,
    on_skip: Option<SkipObserver>,
}

impl NeverOffer {
    #[must_use]
    pub fn new(inner: Box<dyn Decider>, never: NeverSet) -> Self {
        Self {
            inner,
            never,
            on_skip: None,
        }
    }

    /// Be told whenever a window is declined without asking.
    #[must_use]
    pub fn on_skip(mut self, observer: impl FnMut(&Choice, &[String]) + Send + 'static) -> Self {
        self.on_skip = Some(Box::new(observer));
        self
    }

    fn current(&self) -> Modes {
        self.never.lock().expect("never set lock").clone()
    }
}

impl Decider for NeverOffer {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        match filter(choice, &self.current()) {
            Filtered::Unchanged => self.inner.choose(choice),
            Filtered::Narrowed(narrowed) => self.inner.choose(&narrowed),
            Filtered::Declined { option, cards } => {
                if let Some(observer) = &mut self.on_skip {
                    observer(choice, &cards);
                }
                Ok(option)
            }
        }
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        match filter(choice, &self.current()) {
            Filtered::Unchanged => self.inner.choose_seeing(choice, seen),
            Filtered::Narrowed(narrowed) => self.inner.choose_seeing(&narrowed, seen),
            Filtered::Declined { option, cards } => {
                if let Some(observer) = &mut self.on_skip {
                    observer(choice, &cards);
                }
                Ok(option)
            }
        }
    }

    fn stage_scores(&mut self, scores: Vec<(Option<f64>, Option<f64>)>) {
        self.inner.stage_scores(scores);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::id::PlayerId;

    const MERCER: &str = "leader:nomad:nomadagentmercer:TACTICAL_ACTION_ENDED:after";
    const MERCER_KEY: &str = "leader:nomadagentmercer";

    fn card(id: &str, name: &str) -> ChoiceOption {
        ChoiceOption::labelled(
            id,
            ti4_engine::reactions::ACTION_CARD_KIND,
            format!("play {name}"),
        )
        .with("card", id.to_owned())
        .with("card_name", name.to_owned())
    }

    fn slot(names: &[&str]) -> ChoiceOption {
        let mut option =
            ChoiceOption::labelled("reaction:x:EV:when", "ability", "reaction:x:EV:when");
        option
            .payload
            .insert("card_names".to_owned(), serde_json::json!(names));
        option
    }

    fn ability(id: &str) -> ChoiceOption {
        ChoiceOption::labelled(id, "ability", id)
    }

    fn never(names: &[&str]) -> Modes {
        names
            .iter()
            .map(|n| ((*n).to_owned(), ReactionMode::Never))
            .collect()
    }

    fn own_turn(names: &[&str]) -> Modes {
        names
            .iter()
            .map(|n| ((*n).to_owned(), ReactionMode::OwnTurn))
            .collect()
    }

    fn window(options: Vec<ChoiceOption>) -> Choice {
        Choice::new(PlayerId::new("b"), "when EV", options)
    }

    /// A window of the action phase, in the turn of `active`.
    fn in_turn_of(active: &str, options: Vec<ChoiceOption>) -> Choice {
        window(options).detailed(ti4_engine::timing::ACTIVE_DETAIL, active)
    }

    #[test]
    fn a_window_of_only_never_cards_is_declined_without_asking() {
        let choice = window(vec![slot(&["Sabotage"]), ChoiceOption::decline()]);
        match filter(&choice, &never(&["Sabotage"])) {
            Filtered::Declined { option, cards } => {
                assert!(option.is_decline());
                assert_eq!(cards, vec!["Sabotage".to_owned()]);
            }
            other => panic!("expected a skip, got {other:?}"),
        }
    }

    #[test]
    fn another_card_in_the_window_keeps_it_open_without_the_never_card() {
        let choice = window(vec![
            card("sab1", "Sabotage"),
            card("fs1", "Flank Speed"),
            ChoiceOption::decline(),
        ]);
        let Filtered::Narrowed(narrowed) = filter(&choice, &never(&["Sabotage"])) else {
            panic!("expected a narrowed question");
        };
        let ids: Vec<&str> = narrowed.options.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, vec!["fs1", "decline"]);
    }

    #[test]
    fn a_slot_holding_one_never_and_one_other_name_stays_offered() {
        let choice = window(vec![
            slot(&["Sabotage", "Flank Speed"]),
            ChoiceOption::decline(),
        ]);
        assert_eq!(filter(&choice, &never(&["Sabotage"])), Filtered::Unchanged);
    }

    #[test]
    fn non_card_options_and_empty_sets_are_untouched() {
        let other = ChoiceOption::labelled("instinct_training", "ability", "Instinct Training");
        let choice = window(vec![other, ChoiceOption::decline()]);
        assert_eq!(filter(&choice, &never(&["Sabotage"])), Filtered::Unchanged);
        let choice = window(vec![slot(&["Sabotage"]), ChoiceOption::decline()]);
        assert_eq!(filter(&choice, &never(&[])), Filtered::Unchanged);
    }

    #[test]
    fn an_ability_is_muted_by_its_key_and_the_note_has_its_printed_name() {
        let choice = window(vec![ability(MERCER), ChoiceOption::decline()]);
        match filter(&choice, &never(&[MERCER_KEY])) {
            Filtered::Declined { option, cards } => {
                assert!(option.is_decline());
                assert_eq!(cards, vec!["Field Marshal Mercer".to_owned()]);
            }
            other => panic!("expected a skip, got {other:?}"),
        }
        // Another ability of the window is asked without it.
        let two = window(vec![
            ability(MERCER),
            ability("ability:nomad:future_sight:AGENDA_RESOLVED:after"),
            ChoiceOption::decline(),
        ]);
        let Filtered::Narrowed(narrowed) = filter(&two, &never(&[MERCER_KEY])) else {
            panic!("expected a narrowed question");
        };
        assert_eq!(narrowed.options.len(), 2);
        assert!(narrowed.options.iter().all(|option| option.id != MERCER));
    }

    #[test]
    fn a_window_with_no_decline_is_asked_whole() {
        let choice = window(vec![
            ability(MERCER),
            ability("ability:nomad:future_sight:AGENDA_RESOLVED:after"),
        ]);
        assert_eq!(filter(&choice, &never(&[MERCER_KEY])), Filtered::Unchanged);
    }

    #[test]
    fn own_turn_mutes_in_the_turn_of_another_seat_only() {
        let options = || vec![ability(MERCER), ChoiceOption::decline()];
        let modes = own_turn(&[MERCER_KEY]);
        assert!(matches!(
            filter(&in_turn_of("a", options()), &modes),
            Filtered::Declined { .. }
        ));
        // The turn of the seat that is asked.
        assert_eq!(
            filter(&in_turn_of("b", options()), &modes),
            Filtered::Unchanged
        );
        // Outside the action phase nobody has the turn: the reaction is offered.
        assert_eq!(filter(&window(options()), &modes), Filtered::Unchanged);
        // The same for a card.
        let cards = || vec![slot(&["Sabotage"]), ChoiceOption::decline()];
        let modes = own_turn(&["Sabotage"]);
        assert!(matches!(
            filter(&in_turn_of("a", cards()), &modes),
            Filtered::Declined { .. }
        ));
        assert_eq!(
            filter(&in_turn_of("b", cards()), &modes),
            Filtered::Unchanged
        );
    }

    #[test]
    fn set_mode_keeps_no_entry_for_always() {
        let mut modes = Modes::new();
        set_mode(&mut modes, "Sabotage", ReactionMode::OwnTurn);
        assert_eq!(modes.get("Sabotage"), Some(&ReactionMode::OwnTurn));
        set_mode(&mut modes, "Sabotage", ReactionMode::Always);
        assert!(modes.is_empty());
    }

    #[test]
    fn the_wrapper_declines_and_reports_but_forwards_everything_else() {
        let skipped = Arc::new(Mutex::new(Vec::<Vec<String>>::new()));
        let sink = skipped.clone();
        let set: NeverSet = Arc::new(Mutex::new(never(&["Sabotage"])));
        let mut decider = NeverOffer::new(Box::new(ti4_engine::choice::FirstOption), set.clone())
            .on_skip(move |_, cards| sink.lock().unwrap().push(cards.to_vec()));
        let sabotage = window(vec![slot(&["Sabotage"]), ChoiceOption::decline()]);
        assert!(decider.choose(&sabotage).unwrap().is_decline());
        assert_eq!(skipped.lock().unwrap().len(), 1);
        // A different card is asked as usual: FirstOption takes the first option.
        let other = window(vec![slot(&["Flank Speed"]), ChoiceOption::decline()]);
        assert_eq!(decider.choose(&other).unwrap().id, "reaction:x:EV:when");
        assert_eq!(skipped.lock().unwrap().len(), 1);
        // Changing the set takes effect on the next question.
        set.lock().unwrap().clear();
        assert_eq!(decider.choose(&sabotage).unwrap().id, "reaction:x:EV:when");
    }
}
