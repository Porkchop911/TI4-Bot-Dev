//! Agenda voting (LRR 8.2ii to 8.19).
//!
//! Ported from the oracle's `engine/agenda.py`: `outcomes`, `votable_planets`, `cast_votes`,
//! `tally`, and `winning_outcome`.
//!
//! Voting is the most choice-dense window in the game — an outcome per player, then a planet
//! per vote — so it is a resumable state machine rather than a loop, matching how the rest of
//! this driver resolves exactly one decision per step.

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_content::galaxy::all_planets;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{PlanetId, PlayerId};
use ti4_model::state::GameState;

use crate::choice::{Choice, ChoiceOption, IllegalChoice, validate};

/// The two outcomes of an agenda that elects nothing.
pub const FOR: &str = "for";
/// The other one.
pub const AGAINST: &str = "against";

/// Mecatol Rex, which "non-home other than Mecatol" elections exclude.
pub const MECATOL: &str = "18";

/// The choice kind for picking an outcome.
pub const VOTE_KIND: &str = "vote";
/// The choice kind for exhausting a planet to cast its influence.
pub const VOTE_PLANET_KIND: &str = "vote_planet";
/// The choice kind for the speaker's tie-break, which is not a vote (8.19a).
pub const TIEBREAK_KIND: &str = "tiebreak";

/// What may be voted for on one agenda (8.8 to 8.11).
///
/// Returns an empty list when the agenda elects something with no legal candidate — an
/// election over nothing is not a vote, and the caller must not offer it.
#[must_use]
pub fn outcomes(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    alias: &str,
) -> Vec<String> {
    let Some(record) = content.get(ContentType::Agendas, alias) else {
        // An agenda the corpus does not know still has the ordinary two outcomes rather
        // than none, which would silently skip the vote entirely.
        return vec![FOR.to_owned(), AGAINST.to_owned()];
    };
    // The corpus has no `electType` field — it is null on every card. What is elected is
    // read off the printed `target`, up to any parenthetical special rule, exactly as the
    // oracle's `Agenda.elects` does. Reading a field that does not exist would have made
    // every agenda a silent For/Against and no election would ever have been offered.
    let target = record.text("target").unwrap_or("For/Against");
    let head = target.split('(').next().unwrap_or(target).trim();
    if !head.starts_with("Elect") {
        return vec![FOR.to_owned(), AGAINST.to_owned()];
    }
    let elects = head;

    if elects.contains("Player") {
        return state.players.iter().map(|p| p.id.to_string()).collect();
    }
    if elects.contains("Planet") {
        // 8.11: only a planet somebody controls may be elected.
        let mut planets: Vec<String> = state
            .board
            .values()
            .flat_map(|system| system.planet_control.keys())
            .map(ToString::to_string)
            .collect();
        planets.sort_unstable();
        planets.dedup();
        if elects.contains("Non-Home") || elects.contains("Other Than Mecatol") {
            let catalogue = all_planets(content, sources);
            planets.retain(|planet| {
                planet != MECATOL
                    && catalogue
                        .get(planet.as_str())
                        .is_none_or(|record| record.homeworld_of().is_none())
            });
        }
        return planets;
    }
    if elects.contains("Law") {
        return state.laws_in_play().into_iter().cloned().collect();
    }
    // "Elect Scored Secret Objective" draws from the one place a secret is public (61.17),
    // and "Elect Strategy Card" from the cards in play. Neither is modelled here, so there
    // is no candidate and the caller must discard rather than hold an empty vote.
    Vec::new()
}

/// Readied planets this player controls that carry any influence at all.
#[must_use]
pub fn votable_planets(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<PlanetId> {
    let catalogue = all_planets(content, sources);
    state
        .controlled_planets(player)
        .into_iter()
        .map(|(_, planet)| planet.clone())
        .filter(|planet| !state.exhausted_planets.contains(planet))
        .filter(|planet| {
            catalogue
                .get(planet.as_str())
                .is_some_and(|record| record.influence() > 0)
        })
        .collect()
}

/// The influence a planet casts when exhausted.
///
/// 8.6a: exhausting a planet casts its *full* influence, never part of it.
#[must_use]
pub fn influence_of(content: &ContentStore, sources: SourceSet, planet: &PlanetId) -> i64 {
    all_planets(content, sources)
        .get(planet.as_str())
        .map_or(0, ti4_content::galaxy::Planet::influence)
}

/// Whether an agenda is a law, which decides if a passed outcome stays in play (8.20).
#[must_use]
pub fn is_law(content: &ContentStore, alias: &str) -> bool {
    content
        .get(ContentType::Agendas, alias)
        .and_then(|record| record.text("type"))
        .is_some_and(|kind| kind.eq_ignore_ascii_case("law"))
}

/// Who voted for what, and how much each outcome received (8.2ii).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ballot {
    pub votes: BTreeMap<PlayerId, String>,
    pub counts: BTreeMap<String, i64>,
}

impl Ballot {
    /// Everyone who voted for one outcome, in a stable order.
    #[must_use]
    pub fn voted_for(&self, outcome: &str) -> Vec<PlayerId> {
        self.votes
            .iter()
            .filter(|(_, chosen)| chosen.as_str() == outcome)
            .map(|(player, _)| player.clone())
            .collect()
    }
}

/// 8.19: most votes wins; the speaker breaks a tie, or decides if nobody voted.
///
/// Returns `None` when a tie or a silent table needs the speaker, which is a *choice* and so
/// cannot be resolved here. [`VoteWindow`] asks it.
#[must_use]
pub fn undisputed_winner(ballot: &Ballot, choices: &[String]) -> Option<String> {
    if choices.is_empty() {
        return None;
    }
    let best = ballot.counts.values().copied().max().unwrap_or(0);
    let tied: Vec<&String> = ballot
        .counts
        .iter()
        .filter(|(_, count)| **count == best && **count > 0)
        .map(|(outcome, _)| outcome)
        .collect();
    match tied.as_slice() {
        [only] => Some((*only).clone()),
        _ => None,
    }
}

/// The outcomes the speaker chooses between when the vote does not decide it.
#[must_use]
pub fn tiebreak_candidates(ballot: &Ballot, choices: &[String]) -> Vec<String> {
    let best = ballot.counts.values().copied().max().unwrap_or(0);
    let tied: Vec<String> = ballot
        .counts
        .iter()
        .filter(|(_, count)| **count == best && **count > 0)
        .map(|(outcome, _)| outcome.clone())
        .collect();
    // Silence means the speaker picks from everything on offer.
    if tied.is_empty() {
        choices.to_vec()
    } else {
        tied
    }
}

/// Where a vote has reached.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Stage {
    /// Asking `order[index]` which outcome to back.
    Outcome(usize),
    /// Asking `order[index]` which planet to exhaust for the outcome they picked.
    Planets {
        index: usize,
        outcome: String,
        votes: i64,
    },
    /// The speaker is deciding a tie or a silent table.
    Tiebreak,
    /// Finished, with the winning outcome if there was one.
    Done(Option<String>),
}

/// A failure while resolving a vote.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VoteError {
    #[error("the vote is complete")]
    Complete,
    #[error(transparent)]
    IllegalChoice(#[from] IllegalChoice),
}

/// One agenda's vote, resolvable one decision at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoteWindow {
    alias: String,
    choices: Vec<String>,
    order: Vec<PlayerId>,
    stage: Stage,
    ballot: Ballot,
}

impl VoteWindow {
    /// Open a vote on `alias`.
    ///
    /// 8.2ii: voting starts to the speaker's left and goes clockwise, so the speaker votes
    /// last — knowing every other vote, which is the whole point of the seat.
    #[must_use]
    pub fn new(state: &GameState, alias: &str, choices: Vec<String>) -> Self {
        let mut order = state.clockwise_from(&state.speaker);
        // Imperial Rider's cost: a player who predicted this agenda's outcome gives up their
        // vote on it. Dropped from the order rather than skipped later, so nothing downstream
        // has to remember they are barred.
        order.retain(|player| !state.agenda_predictions.contains_key(player));
        if !order.is_empty() {
            order.rotate_left(1); // drop the speaker from the front...
            order.pop();
            order.push(state.speaker.clone()); // ...and put them last
        }
        let opening = if choices.is_empty() {
            Stage::Done(None)
        } else {
            Stage::Outcome(0)
        };
        Self {
            alias: alias.to_owned(),
            choices,
            order,
            stage: opening,
            ballot: Ballot::default(),
        }
    }

    #[must_use]
    pub fn alias(&self) -> &str {
        &self.alias
    }

    #[must_use]
    pub const fn is_complete(&self) -> bool {
        matches!(self.stage, Stage::Done(_))
    }

    /// The winning outcome, once the vote is finished.
    #[must_use]
    pub fn winner(&self) -> Option<&str> {
        match &self.stage {
            Stage::Done(outcome) => outcome.as_deref(),
            _ => None,
        }
    }

    #[must_use]
    pub const fn ballot(&self) -> &Ballot {
        &self.ballot
    }

    /// The decision currently owed, or `None` once the vote is finished.
    #[must_use]
    pub fn pending_choice(
        &self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
    ) -> Option<Choice> {
        match &self.stage {
            Stage::Done(_) => None,
            Stage::Outcome(index) => {
                let player = self.order.get(*index)?;
                let mut options: Vec<ChoiceOption> = self
                    .choices
                    .iter()
                    .map(|outcome| ChoiceOption::labelled(outcome, VOTE_KIND, outcome))
                    .collect();
                options.push(ChoiceOption::decline());
                Some(Choice::new(
                    player.clone(),
                    "vote for which outcome",
                    options,
                ))
            }
            Stage::Planets {
                index,
                outcome,
                votes: _,
            } => {
                let player = self.order.get(*index)?;
                let remaining = votable_planets(state, content, sources, player);
                if remaining.is_empty() {
                    return None;
                }
                let mut options: Vec<ChoiceOption> = remaining
                    .iter()
                    .map(|planet| {
                        let influence = influence_of(content, sources, planet);
                        ChoiceOption::labelled(
                            planet.as_str(),
                            VOTE_PLANET_KIND,
                            format!("exhaust {planet} for {influence} votes"),
                        )
                    })
                    .collect();
                options.push(ChoiceOption::decline());
                Some(Choice::new(
                    player.clone(),
                    format!("exhaust a planet to vote {outcome}"),
                    options,
                ))
            }
            Stage::Tiebreak => {
                let candidates = tiebreak_candidates(&self.ballot, &self.choices);
                Some(Choice::new(
                    state.speaker.clone(),
                    "speaker breaks the tie",
                    candidates
                        .iter()
                        .map(|outcome| ChoiceOption::labelled(outcome, TIEBREAK_KIND, outcome))
                        .collect(),
                ))
            }
        }
    }

    /// Advance past any stage that has no decision left to make.
    fn settle(&mut self, state: &GameState, content: &ContentStore, sources: SourceSet) {
        loop {
            match &self.stage {
                Stage::Outcome(index) if *index >= self.order.len() => {
                    self.stage = self.close();
                }
                Stage::Planets {
                    index,
                    outcome,
                    votes,
                } => {
                    let player = &self.order[*index];
                    if votable_planets(state, content, sources, player).is_empty() {
                        let (index, outcome, votes) = (*index, outcome.clone(), *votes);
                        self.record(index, &outcome, votes);
                        self.stage = Stage::Outcome(index + 1);
                        continue;
                    }
                    return;
                }
                _ => return,
            }
        }
    }

    /// Bank one player's votes, ignoring an outcome nobody actually paid for (8.14).
    fn record(&mut self, index: usize, outcome: &str, votes: i64) {
        if votes <= 0 {
            return;
        }
        self.ballot
            .votes
            .insert(self.order[index].clone(), outcome.to_owned());
        *self.ballot.counts.entry(outcome.to_owned()).or_insert(0) += votes;
    }

    /// Everyone has voted: decide, or hand it to the speaker.
    fn close(&self) -> Stage {
        undisputed_winner(&self.ballot, &self.choices).map_or_else(
            || {
                let candidates = tiebreak_candidates(&self.ballot, &self.choices);
                match candidates.as_slice() {
                    [] => Stage::Done(None),
                    [only] => Stage::Done(Some(only.clone())),
                    _ => Stage::Tiebreak,
                }
            },
            |winner| Stage::Done(Some(winner)),
        )
    }

    /// Apply one decision.
    ///
    /// # Errors
    /// [`VoteError::Complete`] when nothing is owed, and [`VoteError::IllegalChoice`] when the
    /// answer was not one of the options generated for it.
    pub fn resolve(
        &mut self,
        state: &mut GameState,
        content: &ContentStore,
        sources: SourceSet,
        answer: ChoiceOption,
    ) -> Result<(), VoteError> {
        let choice = self
            .pending_choice(state, content, sources)
            .ok_or(VoteError::Complete)?;
        let option = validate(&choice, answer)?;

        match self.stage.clone() {
            Stage::Done(_) => return Err(VoteError::Complete),
            Stage::Outcome(index) => {
                if option.is_decline() {
                    // 8.14: an abstention casts nothing and is not recorded as a vote.
                    self.stage = Stage::Outcome(index + 1);
                } else {
                    self.stage = Stage::Planets {
                        index,
                        outcome: option.id,
                        votes: 0,
                    };
                }
            }
            Stage::Planets {
                index,
                outcome,
                votes,
            } => {
                if option.is_decline() {
                    self.record(index, &outcome, votes);
                    self.stage = Stage::Outcome(index + 1);
                } else {
                    let planet = PlanetId::new(option.id);
                    let influence = influence_of(content, sources, &planet);
                    state.exhaust_planet(planet);
                    self.stage = Stage::Planets {
                        index,
                        outcome,
                        votes: votes + influence,
                    };
                }
            }
            Stage::Tiebreak => {
                self.stage = Stage::Done(Some(option.id));
            }
        }
        self.settle(state, content, sources);
        Ok(())
    }

    /// Advance past players and stages with nothing to decide, before the first question.
    pub fn open(&mut self, state: &GameState, content: &ContentStore, sources: SourceSet) {
        self.settle(state, content, sources);
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_player_who_predicted_the_outcome_does_not_vote_on_it() {
        // Imperial Rider's cost. Without this the card is a free victory point.
        let (mut state, _) = game(&["a", "b", "c"]);
        state.speaker = PlayerId::new("a");
        let choices = for_against();

        let open = VoteWindow::new(&state, "some_agenda", choices.clone());
        let before = open
            .pending_choice(&state, ContentStore::embedded(), POK)
            .map(|choice| choice.player);
        assert!(before.is_some(), "somebody votes first");

        state
            .agenda_predictions
            .insert(PlayerId::new("b"), "for".to_owned());
        let barred = VoteWindow::new(&state, "some_agenda", choices);

        let mut asked = Vec::new();
        let mut window = barred;
        while let Some(choice) = window.pending_choice(&state, ContentStore::embedded(), POK) {
            asked.push(choice.player.clone());
            let answer = choice.options.first().cloned().expect("an option");
            if window
                .resolve(&mut state, ContentStore::embedded(), POK, answer)
                .is_err()
            {
                break;
            }
            if asked.len() > 6 {
                break;
            }
        }

        assert!(
            !asked.contains(&PlayerId::new("b")),
            "b predicted, so b does not vote; asked {asked:?}"
        );
        assert!(asked.contains(&PlayerId::new("a")), "a still votes");
    }

    use ti4_model::content_types::POK;

    use super::*;
    use crate::setup::start_game;

    fn game(names: &[&str]) -> (GameState, Vec<PlayerId>) {
        let players: Vec<PlayerId> = names.iter().map(|n| PlayerId::new(*n)).collect();
        let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        (state, players)
    }

    fn for_against() -> Vec<String> {
        vec![FOR.to_owned(), AGAINST.to_owned()]
    }

    /// Give `player` a planet with influence, so they have something to vote with.
    fn give_voting_planet(state: &mut GameState, player: &PlayerId) -> PlanetId {
        let catalogue = all_planets(ContentStore::embedded(), POK);
        let (id, record) = catalogue
            .iter()
            .find(|(_, planet)| planet.influence() > 0 && !planet.is_placed_during_play())
            .expect("the corpus has an influential planet");
        let planet = PlanetId::new(*id);
        let system = record.system_id().unwrap_or("18");
        state
            .system_mut(&ti4_model::id::SystemId::new(system))
            .set_control(planet.clone(), player.clone());
        planet
    }

    fn pick(window: &VoteWindow, state: &GameState, id: &str) -> ChoiceOption {
        window
            .pending_choice(state, ContentStore::embedded(), POK)
            .expect("a decision is owed")
            .option(id)
            .expect("option was offered")
            .clone()
    }

    #[test]
    fn an_agenda_that_elects_nothing_is_voted_for_or_against() {
        let (state, _) = game(&["a", "b"]);
        assert_eq!(
            outcomes(&state, ContentStore::embedded(), POK, "not_a_real_agenda"),
            for_against()
        );
    }

    #[test]
    fn the_speaker_votes_last() {
        // 8.2ii: voting starts to the speaker's left, which means the speaker votes knowing
        // every other vote. Ordering them first would invert the seat's entire value.
        let (state, _) = game(&["a", "b", "c"]);
        let window = VoteWindow::new(&state, "x", for_against());

        assert_eq!(window.order.last(), Some(&state.speaker));
        assert_eq!(window.order.len(), 3);
    }

    #[test]
    fn a_player_with_no_influence_is_never_asked_to_exhaust_anything() {
        let (mut state, _) = game(&["a", "b"]);
        let mut window = VoteWindow::new(&state, "x", for_against());
        window.open(&state, ContentStore::embedded(), POK);
        let first = window
            .pending_choice(&state, ContentStore::embedded(), POK)
            .unwrap()
            .player;

        let option = pick(&window, &state, FOR);
        window
            .resolve(&mut state, ContentStore::embedded(), POK, option)
            .unwrap();

        // Nobody controls a planet, so the vote fell straight through to the next player
        // rather than offering an exhaust choice with nothing to exhaust.
        let next = window
            .pending_choice(&state, ContentStore::embedded(), POK)
            .unwrap();
        assert_ne!(next.player, first);
        assert_eq!(next.prompt, "vote for which outcome");
    }

    #[test]
    fn exhausting_a_planet_casts_its_full_influence() {
        // 8.6a: full influence, never part of it.
        let (mut state, players) = game(&["a"]);
        let planet = give_voting_planet(&mut state, &players[0]);
        let expected = influence_of(ContentStore::embedded(), POK, &planet);
        assert!(expected > 0);

        let mut window = VoteWindow::new(&state, "x", for_against());
        window.open(&state, ContentStore::embedded(), POK);
        let option = pick(&window, &state, FOR);
        window
            .resolve(&mut state, ContentStore::embedded(), POK, option)
            .unwrap();
        let option = pick(&window, &state, planet.as_str());
        window
            .resolve(&mut state, ContentStore::embedded(), POK, option)
            .unwrap();

        assert!(state.exhausted_planets.contains(&planet));
        assert!(window.is_complete());
        assert_eq!(window.ballot().counts.get(FOR), Some(&expected));
        assert_eq!(window.winner(), Some(FOR));
    }

    #[test]
    fn an_abstention_casts_nothing_and_is_not_recorded() {
        // 8.14.
        let (mut state, players) = game(&["a"]);
        give_voting_planet(&mut state, &players[0]);
        let mut window = VoteWindow::new(&state, "x", for_against());
        window.open(&state, ContentStore::embedded(), POK);

        let option = pick(&window, &state, "decline");
        window
            .resolve(&mut state, ContentStore::embedded(), POK, option)
            .unwrap();

        assert!(window.ballot().votes.is_empty());
        assert!(state.exhausted_planets.is_empty(), "nothing was exhausted");
        // The table was silent, so 8.19 hands the decision to the speaker rather than
        // finishing with no outcome.
        assert!(!window.is_complete());
        assert_eq!(
            window
                .pending_choice(&state, ContentStore::embedded(), POK)
                .unwrap()
                .prompt,
            "speaker breaks the tie"
        );
    }

    #[test]
    fn choosing_an_outcome_then_casting_no_votes_records_nothing() {
        // Picking a side and then exhausting nothing is not a vote for that side.
        let (mut state, players) = game(&["a"]);
        give_voting_planet(&mut state, &players[0]);
        let mut window = VoteWindow::new(&state, "x", for_against());
        window.open(&state, ContentStore::embedded(), POK);

        let option = pick(&window, &state, FOR);
        window
            .resolve(&mut state, ContentStore::embedded(), POK, option)
            .unwrap();
        let option = pick(&window, &state, "decline");
        window
            .resolve(&mut state, ContentStore::embedded(), POK, option)
            .unwrap();

        assert!(window.ballot().counts.is_empty());
        assert!(window.ballot().votes.is_empty());
    }

    #[test]
    fn a_silent_table_hands_the_decision_to_the_speaker() {
        // 8.19: nobody voted, so the speaker decides between every outcome on offer.
        let (mut state, _) = game(&["a", "b"]);
        let mut window = VoteWindow::new(&state, "x", for_against());
        window.open(&state, ContentStore::embedded(), POK);

        while !window.is_complete() {
            let choice = window
                .pending_choice(&state, ContentStore::embedded(), POK)
                .unwrap();
            if choice.prompt == "speaker breaks the tie" {
                assert_eq!(choice.player, state.speaker);
                let option = choice.option(AGAINST).unwrap().clone();
                window
                    .resolve(&mut state, ContentStore::embedded(), POK, option)
                    .unwrap();
                break;
            }
            let option = choice.option("decline").unwrap().clone();
            window
                .resolve(&mut state, ContentStore::embedded(), POK, option)
                .unwrap();
        }

        assert_eq!(window.winner(), Some(AGAINST));
        assert!(
            window.ballot().votes.is_empty(),
            "the speaker's decision is not a vote (8.19a)"
        );
    }

    #[test]
    fn an_election_with_no_candidate_is_not_put_to_a_vote() {
        let (state, _) = game(&["a"]);
        let window = VoteWindow::new(&state, "x", Vec::new());
        assert!(window.is_complete());
        assert_eq!(window.winner(), None);
        assert!(
            window
                .pending_choice(&state, ContentStore::embedded(), POK)
                .is_none()
        );
    }

    #[test]
    fn only_controlled_readied_influential_planets_can_vote() {
        let (mut state, players) = game(&["a"]);
        let planet = give_voting_planet(&mut state, &players[0]);
        assert_eq!(
            votable_planets(&state, ContentStore::embedded(), POK, &players[0]),
            vec![planet.clone()]
        );

        state.exhaust_planet(planet);
        assert!(
            votable_planets(&state, ContentStore::embedded(), POK, &players[0]).is_empty(),
            "an exhausted planet cannot vote again"
        );
    }

    #[test]
    fn an_answer_that_was_not_offered_changes_nothing() {
        let (mut state, players) = game(&["a"]);
        give_voting_planet(&mut state, &players[0]);
        let mut window = VoteWindow::new(&state, "x", for_against());
        window.open(&state, ContentStore::embedded(), POK);
        let before = state.clone();
        let settled = window.clone();

        let error = window
            .resolve(
                &mut state,
                ContentStore::embedded(),
                POK,
                ChoiceOption::new("sideways", VOTE_KIND),
            )
            .unwrap_err();

        assert!(matches!(error, VoteError::IllegalChoice(_)));
        assert!(state.identical(&before));
        assert_eq!(window, settled);
    }

    #[test]
    fn a_planet_election_offers_only_controlled_planets() {
        let (mut state, players) = game(&["a", "b"]);
        let planet = give_voting_planet(&mut state, &players[0]);
        let mut controlled: Vec<String> = state
            .board
            .values()
            .flat_map(|system| system.planet_control.keys())
            .map(ToString::to_string)
            .collect();
        controlled.sort_unstable();
        controlled.dedup();

        assert!(controlled.contains(&planet.to_string()));
        assert!(
            !controlled.is_empty(),
            "8.11 elects only planets somebody controls"
        );
    }

    #[test]
    fn an_election_is_read_off_the_printed_target_not_a_missing_field() {
        // The corpus carries no `electType`; it is null on every card. Reading it would make
        // every agenda a silent For/Against and no election would ever be offered.
        let (mut state, players) = game(&["a", "b"]);
        let elect_player = ContentStore::embedded()
            .records(ContentType::Agendas)
            .iter()
            .find(|record| {
                record
                    .text("target")
                    .is_some_and(|t| t.starts_with("Elect Player"))
            })
            .and_then(|record| record.text("alias"))
            .expect("the corpus has an Elect Player agenda")
            .to_owned();

        let elected = outcomes(&state, ContentStore::embedded(), POK, &elect_player);
        assert_eq!(
            elected,
            players.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "an Elect Player agenda elects between the seated players"
        );

        // And a planet election offers only planets somebody controls (8.11).
        let planet = give_voting_planet(&mut state, &players[0]);
        let elect_planet = ContentStore::embedded()
            .records(ContentType::Agendas)
            .iter()
            .find(|record| record.text("target") == Some("Elect Planet"))
            .and_then(|record| record.text("alias"))
            .expect("the corpus has an Elect Planet agenda")
            .to_owned();
        assert_eq!(
            outcomes(&state, ContentStore::embedded(), POK, &elect_planet),
            vec![planet.to_string()]
        );
    }

    #[test]
    fn laws_are_distinguished_from_directives() {
        let laws = ContentStore::embedded()
            .records(ContentType::Agendas)
            .iter()
            .filter(|record| record.text("type") == Some("Law"))
            .count();
        let directives = ContentStore::embedded()
            .records(ContentType::Agendas)
            .iter()
            .filter(|record| record.text("type") == Some("Directive"))
            .count();
        assert!(laws > 0 && directives > 0, "the corpus has both kinds");

        let a_law = ContentStore::embedded()
            .records(ContentType::Agendas)
            .iter()
            .find(|record| record.text("type") == Some("Law"))
            .and_then(|record| record.text("alias"))
            .unwrap()
            .to_owned();
        assert!(is_law(ContentStore::embedded(), &a_law));
        assert!(!is_law(ContentStore::embedded(), "not_an_agenda"));
    }

    #[test]
    fn the_most_voted_outcome_wins_without_troubling_the_speaker() {
        let ballot = Ballot {
            votes: BTreeMap::new(),
            counts: BTreeMap::from([(FOR.to_owned(), 5), (AGAINST.to_owned(), 3)]),
        };
        assert_eq!(undisputed_winner(&ballot, &for_against()), Some(FOR.into()));
    }

    #[test]
    fn a_tie_has_no_undisputed_winner() {
        let ballot = Ballot {
            votes: BTreeMap::new(),
            counts: BTreeMap::from([(FOR.to_owned(), 4), (AGAINST.to_owned(), 4)]),
        };
        assert_eq!(undisputed_winner(&ballot, &for_against()), None);
        // Sorted, matching the oracle's `sorted(tied)`.
        assert_eq!(
            tiebreak_candidates(&ballot, &for_against()),
            vec![AGAINST.to_owned(), FOR.to_owned()]
        );
    }
}
