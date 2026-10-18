//! What the text of a question names by an id, for clients (display only).
//!
//! A label or a prompt may name a seat, a promissory note or a card by the id of the engine
//! ("give cf:sol to the Empyrean", "hacan becomes speaker"). The label stays as it is: bots and
//! the models read it. Beside it the question carries `details.phrases` ([`PhraseHints`], written
//! with [`Choice::phrased_option`] and [`Choice::phrased_prompt`]): the same text as a template
//! whose `{key}` stands for a [`Subject`], so a client says each one as it names it (a seat by
//! its player, a card by its printed name and text) without reading ids out of the text.
//!
//! The same template carries what the engine words for display only: what a reaction card would
//! do ([`crate::decision_context::EffectPreview::phrase`]) and the lingering effects in force (the
//! `title` and `summary` of `ti4_host`'s `ActiveEffectView`). Those are written piece by piece with
//! [`Phrase::say`] and [`Phrase::name`].
//!
//! How a client says a subject:
//! - [`Subject::Seat`] is the player ("You" for the viewer, the player's name otherwise), and
//!   [`Subject::SeatPossessive`] the possessive of it ("your" for the viewer, "Bob's" otherwise).
//!   A phrase never appends "'s" to a seat itself.
//! - [`Subject::Units`] is a count with the unit's name, in the plural when the count is not one
//!   ("2 cruisers", "1 dreadnought"); [`Subject::Unit`] the name alone ("dreadnought"). A list of
//!   unit counts is one subject per type, joined by the words of the phrase (", ").
//! - [`Subject::System`], [`Subject::Planet`], [`Subject::Agenda`], [`Subject::StrategyCard`] and
//!   the cards are the names the client gives them ("System 27", a printed name).
//! - Numbers are words of the phrase ("2 hits"), not subjects.

use std::collections::BTreeMap;
use std::fmt::Display;

use ti4_content::ContentStore;
use ti4_model::id::{ActionCardId, FactionId, PlanetId, PlayerId, SecretObjectiveId, SystemId};

use crate::choice::{Choice, ChoiceOption};
use crate::decision_context::TriggerUnits;

/// The key of [`PhraseHints`] in `Choice::details`.
pub const PHRASES_KEY: &str = "phrases";

/// One thing that a [`Phrase`] names. The cards are named by the ids of their records in the
/// catalogue, as [`crate::transactions::TradeItem`] names them.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Subject {
    /// A seat of the table.
    Seat {
        player: PlayerId,
    },
    /// The possessive of a seat: "your" for the viewer, "<player>'s" otherwise.
    SeatPossessive {
        player: PlayerId,
    },
    System {
        system: SystemId,
    },
    Planet {
        planet: PlanetId,
    },
    /// A unit type by its id, its name alone ("dreadnought").
    Unit {
        unit_type: String,
    },
    /// A count of a unit type: "2 cruisers", "1 dreadnought".
    Units {
        unit_type: String,
        count: u32,
    },
    /// An agenda card by its id.
    Agenda {
        card: String,
    },
    /// A strategy card by its id.
    StrategyCard {
        card: String,
    },
    /// A promissory note: `card` is its record ([`crate::promissory::record_id`]), `owner` the
    /// faction whose copy it is (the one that "<color>" means in its text).
    Note {
        card: String,
        owner: FactionId,
    },
    ActionCard {
        card: ActionCardId,
    },
    SecretObjective {
        card: SecretObjectiveId,
    },
}

impl Subject {
    #[must_use]
    pub fn seat(player: &PlayerId) -> Self {
        Self::Seat {
            player: player.clone(),
        }
    }

    /// A note by its id of the engine (`cf:sol`, `support:hacan`).
    #[must_use]
    pub fn note(content: &ContentStore, note: &str) -> Self {
        Self::Note {
            card: crate::promissory::record_id(content, note),
            owner: FactionId::new(crate::promissory::owner_of(note).unwrap_or_default()),
        }
    }

    #[must_use]
    pub fn possessive(player: &PlayerId) -> Self {
        Self::SeatPossessive {
            player: player.clone(),
        }
    }

    #[must_use]
    pub fn system(system: &SystemId) -> Self {
        Self::System {
            system: system.clone(),
        }
    }

    #[must_use]
    pub fn planet(planet: &PlanetId) -> Self {
        Self::Planet {
            planet: planet.clone(),
        }
    }

    #[must_use]
    pub fn units(units: &TriggerUnits) -> Self {
        Self::Units {
            unit_type: units.unit_type.clone(),
            count: units.count,
        }
    }

    #[must_use]
    pub fn action_card(card: &ActionCardId) -> Self {
        Self::ActionCard { card: card.clone() }
    }

    #[must_use]
    pub fn secret_objective(card: &SecretObjectiveId) -> Self {
        Self::SecretObjective { card: card.clone() }
    }
}

/// A text of the engine as a template: `text` is the label or the prompt with `{key}` where it
/// names `refs[key]`.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Phrase {
    pub text: String,
    pub refs: BTreeMap<String, Subject>,
}

impl Phrase {
    /// A phrase from its template. [`Worded::with`] names each `{key}`; [`Worded::label`] is the
    /// text of the engine.
    #[must_use]
    pub fn of(template: &str) -> Worded {
        Worded {
            label: template.to_owned(),
            phrase: Self {
                text: template.to_owned(),
                refs: BTreeMap::new(),
            },
        }
    }
}

impl Phrase {
    /// `words` appended to the text.
    #[must_use]
    pub fn say(mut self, words: &str) -> Self {
        self.text.push_str(words);
        self
    }

    /// `subject` appended to the text, under the next free key (`{0}`, `{1}`, ...).
    #[must_use]
    pub fn name(mut self, subject: Subject) -> Self {
        let key = self.refs.len().to_string();
        self.text.push_str(&format!("{{{key}}}"));
        self.refs.insert(key, subject);
        self
    }

    /// `seats` as a list ("Bob", "Bob and Cleo", "Ann, Bob and Cleo"); `none` when there is none.
    #[must_use]
    pub fn seats(self, seats: &[PlayerId], none: &str) -> Self {
        if seats.is_empty() {
            return self.say(none);
        }
        seats
            .iter()
            .enumerate()
            .fold(self, |phrase, (index, seat)| {
                let phrase = match index {
                    0 => phrase,
                    _ if index + 1 == seats.len() => phrase.say(" and "),
                    _ => phrase.say(", "),
                };
                phrase.name(Subject::seat(seat))
            })
    }

    /// The possessive of `seat` ("your", "Bob's"); `none` without a seat.
    #[must_use]
    pub fn possessive(self, seat: Option<&PlayerId>, none: &str) -> Self {
        match seat {
            Some(seat) => self.name(Subject::possessive(seat)),
            None => self.say(none),
        }
    }

    /// Unit counts as a list: "2 cruisers, 1 dreadnought".
    #[must_use]
    pub fn units(self, units: &[TriggerUnits]) -> Self {
        units
            .iter()
            .enumerate()
            .fold(self, |phrase, (index, entry)| {
                let phrase = if index == 0 { phrase } else { phrase.say(", ") };
                phrase.name(Subject::units(entry))
            })
    }

    /// " in <system>" when there is a system.
    #[must_use]
    pub fn in_system(self, system: Option<&SystemId>) -> Self {
        match system {
            Some(system) => self.say(" in ").name(Subject::system(system)),
            None => self,
        }
    }
}

/// A [`Phrase`] and the text of the engine that it stands for, built together so that they
/// cannot differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worded {
    label: String,
    phrase: Phrase,
}

impl Worded {
    /// Name `{key}`: the text of the engine says `raw` there, the phrase `subject`.
    /// A key that the template does not have is left out.
    #[must_use]
    pub fn with(mut self, key: &str, raw: impl Display, subject: Subject) -> Self {
        let placeholder = format!("{{{key}}}");
        if !self.phrase.text.contains(&placeholder) {
            return self;
        }
        self.label = self.label.replace(&placeholder, &raw.to_string());
        self.phrase.refs.insert(key.to_owned(), subject);
        self
    }

    /// The text of the engine: the label or the prompt.
    #[must_use]
    pub fn label(&self) -> String {
        self.label.clone()
    }

    #[must_use]
    pub fn phrase(&self) -> Phrase {
        self.phrase.clone()
    }
}

/// `details.phrases` of a question: the prompt and the options (by id) whose text names
/// something by its id. Options that name nothing are absent.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PhraseHints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<Phrase>,
    #[serde(default)]
    pub options: BTreeMap<String, Phrase>,
}

impl Choice {
    /// The [`PhraseHints`] of the question; empty when it has none.
    #[must_use]
    pub fn phrases(&self) -> PhraseHints {
        self.hint(PHRASES_KEY).unwrap_or_default()
    }

    fn with_phrases(self, edit: impl FnOnce(&mut PhraseHints)) -> Self {
        let mut hints = self.phrases();
        edit(&mut hints);
        self.hinted(PHRASES_KEY, &hints)
    }

    /// Say what the label of option `id` names ([`PhraseHints::options`]).
    #[must_use]
    pub fn phrased_option(self, id: impl Into<String>, worded: &Worded) -> Self {
        let phrase = worded.phrase();
        self.with_phrases(|hints| {
            hints.options.insert(id.into(), phrase);
        })
    }

    /// Say what the prompt names ([`PhraseHints::prompt`]).
    #[must_use]
    pub fn phrased_prompt(self, worded: &Worded) -> Self {
        let phrase = worded.phrase();
        if phrase.refs.is_empty() {
            return self;
        }
        self.with_phrases(|hints| hints.prompt = Some(phrase))
    }

    /// [`Choice::phrased_option`] for each `(id, worded)`.
    #[must_use]
    pub fn phrased_options(self, worded: &[(String, Worded)]) -> Self {
        if worded.is_empty() {
            return self;
        }
        self.with_phrases(|hints| {
            hints.options.extend(
                worded
                    .iter()
                    .map(|(id, worded)| (id.clone(), worded.phrase())),
            );
        })
    }
}

/// One option of `kind` for each `(id, worded)`, labelled with the text of the engine. The
/// question says what the labels name with [`Choice::phrased_options`].
#[must_use]
pub fn options(kind: &str, worded: &[(String, Worded)]) -> Vec<ChoiceOption> {
    worded
        .iter()
        .map(|(id, worded)| ChoiceOption::labelled(id, kind, worded.label()))
        .collect()
}

/// Every `{key}` of `phrase` with the id that the text of the engine says there, for tests: the
/// label that a phrase stands for.
#[cfg(test)]
pub(crate) fn rendered(phrase: &Phrase, raw: &[(&str, &str)]) -> String {
    raw.iter().fold(phrase.text.clone(), |text, (key, value)| {
        text.replace(&format!("{{{key}}}"), value)
    })
}

/// `phrase` with each subject said as a test names it: a seat by its id ("you" for `viewer`), its
/// possessive with "'s" ("your"), a unit count as "2 cruiser", a place or a card by its id.
#[cfg(test)]
pub(crate) fn said(phrase: &Phrase, viewer: &str) -> String {
    let mut text = phrase.text.clone();
    for (key, subject) in &phrase.refs {
        let word = match subject {
            Subject::Seat { player } if player.as_str() == viewer => "You".to_owned(),
            Subject::Seat { player } => player.to_string(),
            Subject::SeatPossessive { player } if player.as_str() == viewer => "your".to_owned(),
            Subject::SeatPossessive { player } => format!("{player}'s"),
            Subject::System { system } => format!("System {system}"),
            Subject::Planet { planet } => planet.to_string(),
            Subject::Unit { unit_type } => unit_type.clone(),
            Subject::Units { unit_type, count } => format!("{count} {unit_type}"),
            Subject::Agenda { card } | Subject::StrategyCard { card } => card.clone(),
            Subject::Note { card, .. } => card.clone(),
            Subject::ActionCard { card } => card.to_string(),
            Subject::SecretObjective { card } => card.to_string(),
        };
        text = text.replace(&format!("{{{key}}}"), &word);
    }
    text
}

/// A table that answers from `answers` (then the first option) and keeps every question that
/// it was asked, for tests of what a question carries.
#[cfg(test)]
pub(crate) fn capturing(
    answers: &[&str],
) -> (
    crate::choice::Table,
    std::rc::Rc<std::cell::RefCell<Vec<Choice>>>,
) {
    let (decider, seen) = crate::choice::Capturing::new(Box::new(crate::choice::Scripted::new(
        answers.iter().map(|answer| (*answer).to_owned()),
    )));
    (crate::choice::Table::with_default(Box::new(decider)), seen)
}

/// The phrase of each option of `choice` but the decline, checked against its label: every
/// `{key}` of its text has a subject, every subject a `{key}`, and the words around them are
/// those of the label, in its order.
///
/// # Panics
/// When an option has no phrase, or a phrase does not match its label.
#[cfg(test)]
pub(crate) fn phrased(choice: &Choice) -> PhraseHints {
    let hints = choice.phrases();
    for option in choice.options.iter().filter(|option| !option.is_decline()) {
        let phrase = hints
            .options
            .get(&option.id)
            .unwrap_or_else(|| panic!("{} has a phrase", option.id));
        assert_matches(phrase, &option.label);
    }
    if let Some(prompt) = &hints.prompt {
        assert_matches(prompt, &choice.prompt);
    }
    hints
}

#[cfg(test)]
fn assert_matches(phrase: &Phrase, label: &str) {
    let mut words = Vec::new();
    let mut rest = phrase.text.as_str();
    while let Some(open) = rest.find('{') {
        let close = rest[open..].find('}').expect("a closed {key}") + open;
        let key = &rest[open + 1..close];
        assert!(
            phrase.refs.contains_key(key),
            "{key} of {:?} names a subject",
            phrase.text
        );
        words.push(&rest[..open]);
        rest = &rest[close + 1..];
    }
    words.push(rest);
    assert_eq!(
        words.len() - 1,
        phrase.refs.len(),
        "a {{key}} for each subject of {:?}",
        phrase.text
    );
    let mut at = 0;
    for (index, word) in words.iter().enumerate() {
        let found = label[at..]
            .find(word)
            .unwrap_or_else(|| panic!("{label:?} says {word:?} of {:?}", phrase.text));
        if index == 0 {
            assert_eq!(found, 0, "{label:?} starts as {:?}", phrase.text);
        }
        at += found + word.len();
    }
    assert!(
        words.last().is_none_or(|last| label.ends_with(last)),
        "{label:?} ends as {:?}",
        phrase.text
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_phrase_and_its_label_are_one_text() {
        let content = ContentStore::embedded();
        let worded = Phrase::of("give {note} to {seat}")
            .with("note", "cf:sol", Subject::note(content, "cf:sol"))
            .with("seat", "b", Subject::seat(&PlayerId::new("b")));
        assert_eq!(worded.label(), "give cf:sol to b");
        let phrase = worded.phrase();
        assert_eq!(phrase.text, "give {note} to {seat}");
        assert_eq!(
            phrase.refs["note"],
            Subject::Note {
                card: "<color>_cf".to_owned(),
                owner: FactionId::new("sol"),
            }
        );
        assert_eq!(
            rendered(&phrase, &[("note", "cf:sol"), ("seat", "b")]),
            worded.label()
        );
        assert_matches(&phrase, &worded.label());

        let choice = Choice::new(
            PlayerId::new("a"),
            "which",
            vec![ChoiceOption::labelled("x", "k", worded.label())],
        )
        .phrased_option("x", &worded)
        .phrased_prompt(&Phrase::of("which"));
        let hints = choice.phrases();
        assert_eq!(hints.options["x"], phrase);
        assert_eq!(
            hints.prompt, None,
            "a prompt that names nothing has no phrase"
        );
        // The JSON that the clients read.
        let wire = &choice.details[PHRASES_KEY];
        assert_eq!(wire["options"]["x"]["refs"]["note"]["kind"], "note");
        assert_eq!(wire["options"]["x"]["refs"]["seat"]["kind"], "seat");
        assert_eq!(wire["options"]["x"]["refs"]["seat"]["player"], "b");
    }
}
