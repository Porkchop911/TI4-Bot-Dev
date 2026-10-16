//! Agenda voting (LRR 8.2ii to 8.19).
//!
//! Ported from the oracle's `engine/agenda.py`: `outcomes`, `votable_planets`, `cast_votes`,
//! `tally`, and `winning_outcome`.
//!
//! Voting is the most choice-dense window in the game — an outcome per player, then a planet
//! per vote — so it is a resumable state machine rather than a loop, matching how the rest of
//! this driver resolves exactly one decision per step.

use std::collections::BTreeMap;

/// Key of the display-only `details` entry that names the agenda being voted on.
const AGENDA_DETAIL: &str = "agenda";

use ti4_content::ContentStore;
use ti4_content::galaxy::all_planets;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{PlanetId, PlayerId};
use ti4_model::state::GameState;

use crate::choice::{Choice, ChoiceOption, IllegalChoice, validate};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::preview::{Delta, Preview, Quantity};

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
/// Gila the Silvertongue: trade goods spent for two votes each, as `"spend|<n>"`.
pub const VOTE_TRADE_GOODS_KIND: &str = "vote_trade_goods";
/// Genetic Recombination (Mahact): the holder names the outcome a voter must back, as
/// `"recombine|<outcome>"`.
pub const RECOMBINE_KIND: &str = "recombine";
/// Genetic Recombination: the voter's answer, `"tribute|vote"` or `"tribute|token"`.
pub const TRIBUTE_KIND: &str = "tribute";

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
                !crate::seating::is_mecatol_planet(planet)
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

/// Whether this agenda elects a planet, read off the printed `target` exactly as [`outcomes`]
/// reads it. Only used to tell a map UI that an outcome is a planet (`payload.planet`).
fn elects_planet(content: &ContentStore, alias: &str) -> bool {
    let Some(record) = content.get(ContentType::Agendas, alias) else {
        return false;
    };
    let target = record.text("target").unwrap_or("For/Against");
    let head = target.split('(').next().unwrap_or(target).trim();
    head.starts_with("Elect") && !head.contains("Player") && head.contains("Planet")
}

/// An outcome option, carrying `planet`/`system` when the agenda elects a planet.
fn locate_outcome(
    option: ChoiceOption,
    planet_outcomes: bool,
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
) -> ChoiceOption {
    if !planet_outcomes || option.id == FOR || option.id == AGAINST {
        return option;
    }
    let planet = option.id.clone();
    option.with_planet_located(state, content, sources, &planet)
}

/// Votes a card has added to this seat's total for the agenda being voted on.
///
/// Distinguished Councilor casts five more; Bribery casts one per trade good spent. Both say "that
/// outcome", meaning this agenda, so the bonus is stored against [`GameState::agenda_seq`] and is
/// silently worth nothing once the next agenda is revealed. Read where votes are counted rather
/// than applied at the card, so it cannot be honoured in one voting path and forgotten in another.
#[must_use]
pub fn extra_votes(state: &GameState, player: &PlayerId) -> i64 {
    state
        .player(player)
        .and_then(|seat| seat.extra_votes_agenda)
        .filter(|(agenda, _)| *agenda == state.agenda_seq)
        .map_or(0, |(_, votes)| votes)
}

/// Add votes for the agenda currently being voted on, accumulating across cards.
pub fn add_votes(state: &mut GameState, player: &PlayerId, votes: i64) {
    let agenda = state.agenda_seq;
    if let Some(seat) = state.player_mut(player) {
        let running = match seat.extra_votes_agenda {
            Some((held, had)) if held == agenda => had,
            _ => 0,
        };
        seat.extra_votes_agenda = Some((agenda, running + votes));
    }
}

/// Representative Government: each player casts exactly one vote, and exhausts nothing.
///
/// Returned as an amount rather than a flag so a caller cannot honour "one vote" and forget "no
/// exhausting" — under this law there is nothing to exhaust, because influence is not what is being
/// cast.
#[must_use]
pub fn flat_vote_amount(state: &GameState) -> Option<i64> {
    crate::laws::flat_votes(state).then_some(1)
}

/// Readied planets this player controls that carry any influence at all.
#[must_use]
pub fn votable_planets(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<PlanetId> {
    state
        .controlled_planets(player)
        .into_iter()
        .map(|(_, planet)| planet.clone())
        // The Deepwrought's ocean cards count as influence for voting, like any spendable planet.
        .chain(crate::factions::deepwrought::oceans(state, player))
        .filter(|planet| !state.exhausted_planets.contains(planet))
        .filter(|planet| influence_of(state, content, sources, planet) > 0)
        .collect()
}

/// The influence a planet casts when exhausted: its value as it now stands, attachments and
/// attachment laws included.
///
/// 8.6a: exhausting a planet casts its *full* influence, never part of it.
#[must_use]
pub fn influence_of(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    planet: &PlanetId,
) -> i64 {
    crate::production::planet_value_now(
        state,
        content,
        sources,
        planet,
        crate::production::Spend::Influence,
    )
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
    /// Mahact's Genetic Recombination: asking its holder whether to exhaust it before
    /// `order[index]` casts votes, and for which outcome.
    Recombine(usize),
    /// Genetic Recombination was used on `order[index]`, who can either cast a vote for the named
    /// outcome or remove a token from their fleet pool: asking which.
    Tribute(usize),
    /// Asking `order[index]` which planet to exhaust for the outcome they picked.
    Planets {
        index: usize,
        outcome: String,
        votes: i64,
    },
    /// Asking `order[index]`, who holds Hacan's commander ability, how many trade goods to spend
    /// for two more votes each ("When you cast votes: You may spend any number of trade goods").
    TradeGoods {
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
    /// Executive Order (Keleres): the seat that "can spend trade goods and resources on this agenda
    /// as if they were votes". Besides influence, its planets may be exhausted for resources, and
    /// its trade goods are a vote each.
    spender: Option<PlayerId>,
    /// Seats whose turn to vote has already been offered to Genetic Recombination's holder.
    recombination_offered: std::collections::BTreeSet<usize>,
    /// Genetic Recombination in force: the voter's index and the outcome they must back. Set when
    /// the voter must cast at least 1 vote for it (they chose to, or have no token to remove).
    obligation: Option<(usize, String)>,
    /// The outcome the holder chose for the voter now answering the [`Stage::Tribute`] question.
    demanded: Option<(usize, String)>,
}

/// One way for a voter to exhaust a planet: the option id, the planet and the votes it casts.
type PlanetOffer = (String, PlanetId, i64);

/// The suffix of the option id that exhausts a planet for its resources instead of its influence.
const RESOURCES_SUFFIX: &str = "|resources";

impl VoteWindow {
    /// Open a vote on `alias`.
    ///
    /// 8.2ii: voting starts to the speaker's left and goes clockwise, so the speaker votes
    /// last — knowing every other vote, which is the whole point of the seat. A seat that
    /// played Hack Election into this agenda's reveal window (`Player::hack_votes_last_agenda`
    /// == the current `agenda_seq`) moves to the very end of the order, after the speaker;
    /// several such seats take the last seats in turn, clockwise.
    #[must_use]
    pub fn new(state: &GameState, alias: &str, choices: Vec<String>) -> Self {
        let mut order = state.clockwise_from(&state.speaker);
        // Imperial Rider's cost: a player who predicted this agenda's outcome gives up their
        // vote on it. Dropped from the order rather than skipped later, so nothing downstream
        // has to remember they are barred. A speaker who is barred is simply gone from the
        // order — the rotation below only re-seats players who still vote.
        // Elder Qanoj: game effects cannot prevent Xxcha voting, so neither a rider's cost nor a
        // Political Secret bars them.
        // Galactic Threat (Nekro): "You cannot vote on agendas."
        order.retain(|player| {
            (!state.agenda_predictions.contains_key(player)
                && !crate::factions::nekro::is_nekro(state, player))
                || crate::leaders::elder_qanoj(state, player)
        });
        let votes_last = |player: &PlayerId| {
            state
                .player(player)
                .and_then(|seat| seat.hack_votes_last_agenda)
                .is_some_and(|agenda| agenda == state.agenda_seq)
        };
        let (hackers, rest): (Vec<PlayerId>, Vec<PlayerId>) =
            order.into_iter().partition(|player| votes_last(player));
        // A module that seats a player first (Argent's Zeal) takes the opening seats, clockwise
        // from the speaker among themselves; nobody else's relative order changes.
        let (firsts, rest): (Vec<PlayerId>, Vec<PlayerId>) = rest
            .into_iter()
            .partition(|player| crate::factions::hooks_cards::votes_first(state, player));
        let mut final_order: Vec<PlayerId> = firsts;
        final_order.extend(
            rest.iter()
                .filter(|player| *player != &state.speaker)
                .cloned(),
        );
        if rest.contains(&state.speaker) {
            final_order.push(state.speaker.clone());
        }
        final_order.extend(hackers);
        let opening = if choices.is_empty() {
            Stage::Done(None)
        } else {
            Stage::Outcome(0)
        };
        Self {
            alias: alias.to_owned(),
            choices,
            order: final_order,
            stage: opening,
            ballot: Ballot::default(),
            spender: None,
            recombination_offered: std::collections::BTreeSet::new(),
            obligation: None,
            demanded: None,
        }
    }

    /// Let `player` spend trade goods and resources on this agenda as if they were votes
    /// (Executive Order). Set before [`Self::open`].
    #[must_use]
    pub fn with_spender(mut self, player: PlayerId) -> Self {
        self.spender = Some(player);
        self
    }

    /// The planets `player` may exhaust now, with the option id and votes each casts. Everyone
    /// exhausts a planet for its influence (option id: the planet). The spender may also exhaust it
    /// for its resources (`"<planet>|resources"`). Elder Qanoj adds a vote per planet.
    fn planet_offers(
        &self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
        player: &PlayerId,
    ) -> Vec<PlanetOffer> {
        let qanoj = i64::from(crate::leaders::elder_qanoj(state, player));
        let spends = self.spender.as_ref() == Some(player);
        let mut offers = Vec::new();
        let oceans = crate::factions::deepwrought::oceans(state, player);
        let cards = state
            .controlled_planets(player)
            .into_iter()
            .map(|(_, planet)| planet)
            .chain(oceans.iter());
        for planet in cards {
            if state.exhausted_planets.contains(planet) {
                continue;
            }
            let influence = influence_of(state, content, sources, planet);
            if influence > 0 {
                offers.push((planet.to_string(), planet.clone(), influence + qanoj));
            }
            if spends {
                let resources = crate::production::planet_value_now(
                    state,
                    content,
                    sources,
                    planet,
                    crate::production::Spend::Resources,
                );
                if resources > 0 {
                    offers.push((
                        format!("{planet}{RESOURCES_SUFFIX}"),
                        planet.clone(),
                        resources + qanoj,
                    ));
                }
            }
        }
        offers
    }

    /// Votes one trade good casts for `player`, `0` if none may be spent. Gila the Silvertongue's
    /// holder gets two for each once a vote is cast; the Executive Order spender one for each.
    fn trade_goods_rate(&self, state: &GameState, player: &PlayerId, votes: i64) -> i64 {
        if votes > 0 && crate::promissory::has_commander_ability(state, player, "hacancommander") {
            2
        } else {
            i64::from(self.spender.as_ref() == Some(player))
        }
    }

    #[must_use]
    pub fn alias(&self) -> &str {
        &self.alias
    }

    /// The outcomes this agenda offered.
    #[must_use]
    pub fn choices(&self) -> &[String] {
        &self.choices
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

    /// The order the votes are asked in, starting with the first voter. Public knowledge in
    /// the printed game as well: everyone sees who is asked next.
    #[must_use]
    pub fn order(&self) -> &[PlayerId] {
        &self.order
    }

    /// The seat now deciding how many trade goods to spend on votes, if that is the stage. The game
    /// driver offers the Keleres agent to this seat before asking
    /// ([`crate::supply::open_goods_window`]).
    #[must_use]
    pub fn trade_goods_payer(&self) -> Option<&PlayerId> {
        match &self.stage {
            Stage::TradeGoods { index, .. } => self.order.get(*index),
            _ => None,
        }
    }

    /// The decision currently owed, or `None` once the vote is finished. Every vote decision
    /// carries the agenda being voted on as display-only `details.agenda` (name, type, text) so a
    /// client can keep it on screen for the whole vote; option ids and kinds are untouched.
    #[must_use]
    pub fn pending_choice(
        &self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
    ) -> Option<Choice> {
        self.pending_choice_bare(state, content, sources)
            .map(|choice| {
                let agenda = crate::strategy_cards::agenda_details(content, &self.alias);
                choice.detailed(AGENDA_DETAIL, agenda)
            })
    }

    fn pending_choice_bare(
        &self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
    ) -> Option<Choice> {
        match &self.stage {
            Stage::Done(_) => None,
            Stage::Outcome(index) => {
                let player = self.order.get(*index)?;
                // OBS-009: 8.2ii seats the speaker last specifically so they vote knowing every
                // other vote already cast -- the public vote ledger, not merely a legal-set
                // effect. `Ballot` lives on this window, not on `GameState`, so nothing an
                // `Observed` builds from state alone could recover it; each option's own running
                // tally closes that gap through the existing generic payload-number pipeline.
                let planet_outcomes = elects_planet(content, &self.alias);
                let mut options: Vec<ChoiceOption> = self
                    .choices
                    .iter()
                    .map(|outcome| {
                        let tally = self.ballot.counts.get(outcome).copied().unwrap_or(0);
                        let option = ChoiceOption::labelled(outcome, VOTE_KIND, outcome)
                            .with("current_votes", tally);
                        locate_outcome(option, planet_outcomes, state, content, sources)
                    })
                    .collect();
                // Genetic Recombination: a voter who must cast a vote for an outcome may back
                // nothing else and may not abstain.
                let bound = self
                    .obligation
                    .as_ref()
                    .filter(|(at, _)| at == index)
                    .map(|(_, outcome)| outcome);
                if let Some(outcome) = bound {
                    options.retain(|option| option.id == *outcome);
                } else {
                    options.push(ChoiceOption::decline());
                }
                Some(
                    Choice::new(player.clone(), "vote for which outcome", options).contextualized(
                        DecisionContext::new(
                            player.clone(),
                            DecisionSource::Rule("8.10".to_owned()),
                            "cast_vote",
                            state.phase,
                            state.round,
                        ),
                    ),
                )
            }
            Stage::Planets {
                index,
                outcome,
                votes,
            } => {
                let player = self.order.get(*index)?;
                let remaining = self.planet_offers(state, content, sources, player);
                if remaining.is_empty() {
                    return None;
                }
                let votes_so_far = *votes;
                let mut options: Vec<ChoiceOption> = remaining
                    .iter()
                    .map(|(id, planet, cast)| {
                        let face = if id.ends_with(RESOURCES_SUFFIX) {
                            "resources"
                        } else {
                            "influence"
                        };
                        let label = if self.spender.is_some() {
                            format!("exhaust {planet} for {cast} votes ({face})")
                        } else {
                            format!("exhaust {planet} for {cast} votes")
                        };
                        ChoiceOption::labelled(id, VOTE_PLANET_KIND, label)
                            .with_planet_located(state, content, sources, planet.as_str())
                            .previewed(Preview::certain(vec![Delta::new(
                                Quantity::Votes,
                                votes_so_far,
                                votes_so_far + cast,
                            )]))
                    })
                    .collect();
                // Genetic Recombination: the first planet is not optional.
                let must_cast = *votes == 0
                    && self
                        .obligation
                        .as_ref()
                        .is_some_and(|(at, backed)| at == index && backed == outcome);
                if !must_cast {
                    options.push(ChoiceOption::decline());
                }
                Some(
                    Choice::new(
                        player.clone(),
                        format!("exhaust a planet to vote {outcome}"),
                        options,
                    )
                    .contextualized(DecisionContext::new(
                        player.clone(),
                        DecisionSource::Rule("8.11".to_owned()),
                        "vote_exhaust_planet",
                        state.phase,
                        state.round,
                    )),
                )
            }
            Stage::TradeGoods {
                index,
                outcome,
                votes,
            } => {
                let player = self.order.get(*index)?;
                // Trade goods, plus the commodities an open Keleres agent window lets this seat
                // spend as trade goods (the game driver offers the agent before asking).
                let goods = crate::supply::spendable_goods(state, player);
                let rate = self.trade_goods_rate(state, player, *votes);
                let mut options: Vec<ChoiceOption> = (1..=goods)
                    .map(|spent| {
                        ChoiceOption::labelled(
                            format!("spend|{spent}"),
                            VOTE_TRADE_GOODS_KIND,
                            format!("spend {spent} trade goods for {} votes", rate * spent),
                        )
                        .with("trade_goods", spent)
                        .previewed(Preview::certain(vec![Delta::new(
                            Quantity::Votes,
                            *votes,
                            *votes + rate * spent,
                        )]))
                    })
                    .collect();
                options.push(ChoiceOption::decline());
                Some(
                    Choice::new(
                        player.clone(),
                        format!("spend trade goods for votes on {outcome}"),
                        options,
                    )
                    .detailed("kind", "vote_trade_goods")
                    .detailed(
                        "card",
                        crate::strategy_cards::commander_card(content, "hacancommander"),
                    )
                    .detailed("outcome", outcome.as_str())
                    .detailed("votes", *votes)
                    .detailed("goods", i64::from(goods))
                    .detailed("votes_per_good", 2)
                    .contextualized(DecisionContext::new(
                        player.clone(),
                        DecisionSource::Content(
                            if rate == 2 {
                                "hacancommander"
                            } else {
                                "executiveorder"
                            }
                            .to_owned(),
                        ),
                        "vote_spend_trade_goods",
                        state.phase,
                        state.round,
                    )),
                )
            }
            Stage::Recombine(index) => {
                let voter = self.order.get(*index)?;
                let holder = crate::factions::mahact::recombination_holder(state, voter)?;
                let mut options: Vec<ChoiceOption> = self
                    .choices
                    .iter()
                    .map(|outcome| {
                        ChoiceOption::labelled(
                            format!("recombine|{outcome}"),
                            RECOMBINE_KIND,
                            format!("{voter} must vote {outcome} or return a fleet token"),
                        )
                    })
                    .collect();
                options.push(ChoiceOption::decline());
                Some(
                    Choice::new(
                        holder.clone(),
                        format!("Genetic Recombination before {voter} votes"),
                        options,
                    )
                    .contextualized(DecisionContext::new(
                        holder,
                        DecisionSource::Content(crate::factions::mahact::RECOMBINATION.to_owned()),
                        "recombination_outcome",
                        state.phase,
                        state.round,
                    )),
                )
            }
            Stage::Tribute(index) => {
                let voter = self.order.get(*index)?;
                let (_, outcome) = self.demanded.as_ref().filter(|(at, _)| at == index)?;
                let mut options = Vec::new();
                if !self
                    .planet_offers(state, content, sources, voter)
                    .is_empty()
                {
                    options.push(ChoiceOption::labelled(
                        "tribute|vote",
                        TRIBUTE_KIND,
                        format!("cast at least 1 vote for {outcome}"),
                    ));
                }
                if crate::factions::mahact::can_pay_tribute(state, voter) {
                    options.push(ChoiceOption::labelled(
                        "tribute|token",
                        TRIBUTE_KIND,
                        "remove 1 token from your fleet pool and return it to reinforcements",
                    ));
                }
                if options.is_empty() {
                    return None;
                }
                Some(
                    Choice::new(
                        voter.clone(),
                        format!("Genetic Recombination: vote {outcome} or return a fleet token"),
                        options,
                    )
                    .contextualized(DecisionContext::new(
                        voter.clone(),
                        DecisionSource::Content(crate::factions::mahact::RECOMBINATION.to_owned()),
                        "recombination_tribute",
                        state.phase,
                        state.round,
                    )),
                )
            }
            Stage::Tiebreak => {
                let candidates = tiebreak_candidates(&self.ballot, &self.choices);
                let planet_outcomes = elects_planet(content, &self.alias);
                Some(
                    Choice::new(
                        state.speaker.clone(),
                        "speaker breaks the tie",
                        candidates
                            .iter()
                            .map(|outcome| {
                                locate_outcome(
                                    ChoiceOption::labelled(outcome, TIEBREAK_KIND, outcome),
                                    planet_outcomes,
                                    state,
                                    content,
                                    sources,
                                )
                            })
                            .collect(),
                    )
                    .contextualized(DecisionContext::new(
                        state.speaker.clone(),
                        DecisionSource::Rule("8.16".to_owned()),
                        "vote_tiebreak",
                        state.phase,
                        state.round,
                    )),
                )
            }
        }
    }

    /// Advance past any stage that has no decision left to make.
    fn settle(&mut self, state: &GameState, content: &ContentStore, sources: SourceSet) {
        loop {
            // Genetic Recombination: "before a player casts votes", once per voter, the holder
            // is asked.
            if let Stage::Outcome(index) = self.stage
                && index < self.order.len()
                && self.recombination_offered.insert(index)
                && crate::factions::mahact::recombination_holder(state, &self.order[index])
                    .is_some()
            {
                self.stage = Stage::Recombine(index);
                return;
            }
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
                    if self
                        .planet_offers(state, content, sources, player)
                        .is_empty()
                    {
                        let (index, outcome, votes) = (*index, outcome.clone(), *votes);
                        self.finish_planets(state, content, index, outcome, votes);
                        continue;
                    }
                    return;
                }
                _ => return,
            }
        }
    }

    /// A player is done exhausting planets: offer trade goods for votes when they may spend them
    /// (Gila the Silvertongue's holder once casting votes, or the Executive Order spender) and have
    /// some; else bank the votes.
    fn finish_planets(
        &mut self,
        state: &GameState,
        content: &ContentStore,
        index: usize,
        outcome: String,
        votes: i64,
    ) {
        let player = &self.order[index];
        let may_spend = self.trade_goods_rate(state, player, votes) > 0
            && crate::supply::potential_goods(state, player) > 0;
        if may_spend {
            self.stage = Stage::TradeGoods {
                index,
                outcome,
                votes,
            };
        } else {
            self.record(state, content, index, &outcome, votes);
            self.stage = Stage::Outcome(index + 1);
        }
    }

    /// Bank one player's votes, ignoring an outcome nobody actually paid for (8.14).
    ///
    /// A commander's bonus is added here rather than at the card, so it cannot be honoured on
    /// one voting path and forgotten on another. It rides on votes actually cast: a player who
    /// exhausts nothing casts nothing, and a bonus alone is not a vote.
    fn record(
        &mut self,
        state: &GameState,
        content: &ContentStore,
        index: usize,
        outcome: &str,
        votes: i64,
    ) {
        if votes <= 0 {
            return;
        }
        let votes = votes
            + crate::leaders::vote_bonus(state, &self.order[index])
            + crate::factions::hooks_cards::vote_bonus_with_content(
                state,
                content,
                &self.order[index],
            )
            + extra_votes(state, &self.order[index])
            + crate::factions::empyrean_units::blood_pact_votes(
                state,
                &self.ballot.votes,
                &self.order[index],
                outcome,
            );
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
            Stage::Recombine(index) => {
                if option.is_decline() {
                    self.stage = Stage::Outcome(index);
                } else {
                    let holder =
                        crate::factions::mahact::recombination_holder(state, &self.order[index])
                            .ok_or(VoteError::Complete)?;
                    let outcome = option
                        .id
                        .strip_prefix("recombine|")
                        .unwrap_or_default()
                        .to_owned();
                    crate::factions::mahact::exhaust_recombination(state, &holder);
                    let voter = self.order[index].clone();
                    let can_vote = !self
                        .planet_offers(state, content, sources, &voter)
                        .is_empty();
                    let can_pay = crate::factions::mahact::can_pay_tribute(state, &voter);
                    self.stage = Stage::Outcome(index);
                    match (can_vote, can_pay) {
                        (true, true) => {
                            self.demanded = Some((index, outcome));
                            self.stage = Stage::Tribute(index);
                        }
                        (true, false) => self.obligation = Some((index, outcome)),
                        (false, true) => {
                            crate::factions::mahact::pay_tribute(state, &voter);
                        }
                        (false, false) => {}
                    }
                }
            }
            Stage::Tribute(index) => {
                let demanded = self.demanded.take();
                if option.id == "tribute|vote" {
                    self.obligation = demanded;
                } else {
                    crate::factions::mahact::pay_tribute(state, &self.order[index].clone());
                }
                self.stage = Stage::Outcome(index);
            }
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
                    self.finish_planets(state, content, index, outcome, votes);
                } else {
                    // Elder Qanoj: each planet exhausted to vote gives one vote more (counted in
                    // the offer).
                    let offer = self
                        .planet_offers(state, content, sources, &self.order[index])
                        .into_iter()
                        .find(|(id, _, _)| *id == option.id);
                    let Some((_, planet, cast)) = offer else {
                        return Err(VoteError::Complete);
                    };
                    state.exhaust_planet(planet);
                    self.stage = Stage::Planets {
                        index,
                        outcome,
                        votes: votes + cast,
                    };
                }
            }
            Stage::TradeGoods {
                index,
                outcome,
                votes,
            } => {
                let spent = option
                    .id
                    .strip_prefix("spend|")
                    .and_then(|n| n.parse::<i32>().ok())
                    .unwrap_or(0);
                let rate = self.trade_goods_rate(state, &self.order[index], votes);
                if spent > 0 && !crate::supply::spend_goods(state, &self.order[index], spent) {
                    return Err(VoteError::Complete);
                }
                self.record(
                    state,
                    content,
                    index,
                    &outcome,
                    votes + rate * i64::from(spent),
                );
                self.stage = Stage::Outcome(index + 1);
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
    /// Extra votes accumulate across cards and expire with the agenda.
    #[test]
    fn extra_votes_accumulate_and_expire_with_the_agenda() {
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.agenda_seq = 7;

        assert_eq!(extra_votes(&state, &player), 0);
        add_votes(&mut state, &player, 5); // Distinguished Councilor
        add_votes(&mut state, &player, 2); // Bribery, two trade goods
        assert_eq!(
            extra_votes(&state, &player),
            7,
            "two cards on one agenda add up"
        );

        state.agenda_seq = 8;
        assert_eq!(
            extra_votes(&state, &player),
            0,
            "both cards said *that* outcome, meaning that agenda"
        );
    }

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

    /// A speaker who paid the Imperial Rider's cost gives up their vote too. The old
    /// rotation unseated the speaker and then re-seated them, so a barred speaker ended up
    /// voting last anyway while the player on their left was dropped.
    #[test]
    fn a_speaker_who_predicted_the_outcome_gives_up_their_vote() {
        let (mut state, _) = game(&["a", "b", "c"]);
        state.speaker = PlayerId::new("a");
        state
            .agenda_predictions
            .insert(PlayerId::new("a"), "for".to_owned());

        let vote = VoteWindow::new(&state, "some_agenda", for_against());
        assert_eq!(
            vote.order(),
            [PlayerId::new("b"), PlayerId::new("c")],
            "the barred speaker is simply gone; the others keep their clockwise order"
        );
    }

    #[test]
    fn hack_votes_last_moves_the_holder_to_the_end_of_the_order() {
        // "During this agenda, you vote last": the holder takes the very last seat, after
        // the speaker.
        let (mut state, _) = game(&["a", "b", "c"]);
        state.speaker = PlayerId::new("a");
        state.agenda_seq = 7;
        state
            .player_mut(&PlayerId::new("b"))
            .expect("b sits at the table")
            .hack_votes_last_agenda = Some(7);

        let vote = VoteWindow::new(&state, "some_agenda", for_against());
        assert_eq!(
            vote.order(),
            [PlayerId::new("c"), PlayerId::new("a"), PlayerId::new("b")],
            "the speaker keeps their seat ahead of the hacker, b votes dead last"
        );
    }

    #[test]
    fn a_seat_a_module_puts_first_votes_first_and_others_keep_their_order() {
        let (mut state, _) = game(&["a", "b", "c"]);
        state.speaker = PlayerId::new("a");
        let baseline = VoteWindow::new(&state, "some_agenda", for_against());
        assert_eq!(
            baseline.order(),
            [PlayerId::new("b"), PlayerId::new("c"), PlayerId::new("a")],
            "no module: unchanged, the speaker last"
        );
        let hook = crate::factions::hooks_cards::CardHooks {
            votes_first: Some(|_, player| player.as_str() == "c"),
            ..crate::factions::hooks_cards::CardHooks::NONE
        };
        crate::factions::hooks_cards::with_test_hooks(hook, || {
            let vote = VoteWindow::new(&state, "some_agenda", for_against());
            assert_eq!(
                vote.order(),
                [PlayerId::new("c"), PlayerId::new("b"), PlayerId::new("a")]
            );
            // The speaker as first voter really votes first; a hacker still goes last.
            state.speaker = PlayerId::new("c");
            let vote = VoteWindow::new(&state, "some_agenda", for_against());
            assert_eq!(
                vote.order(),
                [PlayerId::new("c"), PlayerId::new("a"), PlayerId::new("b")]
            );
        });
    }

    #[test]
    fn several_hacks_take_the_last_seats_in_clockwise_turn() {
        let (mut state, _) = game(&["a", "b", "c"]);
        state.speaker = PlayerId::new("a");
        state.agenda_seq = 7;
        for who in ["a", "c"] {
            state
                .player_mut(&PlayerId::new(who))
                .expect("a seat")
                .hack_votes_last_agenda = Some(7);
        }

        let vote = VoteWindow::new(&state, "some_agenda", for_against());
        assert_eq!(
            vote.order(),
            [PlayerId::new("b"), PlayerId::new("a"), PlayerId::new("c")],
            "the hackers keep their relative clockwise order at the end"
        );
    }

    #[test]
    fn hack_votes_last_expires_with_the_agenda_it_was_played_into() {
        let (mut state, _) = game(&["a", "b", "c"]);
        state.speaker = PlayerId::new("a");
        state.agenda_seq = 7;
        state
            .player_mut(&PlayerId::new("b"))
            .expect("b sits at the table")
            .hack_votes_last_agenda = Some(7);

        let this_agenda = VoteWindow::new(&state, "some_agenda", for_against());
        assert_eq!(
            this_agenda.order().last().expect("somebody votes last"),
            &PlayerId::new("b")
        );

        state.agenda_seq = 8; // the next reveal
        let next = VoteWindow::new(&state, "some_agenda", for_against());
        assert_eq!(
            next.order(),
            [PlayerId::new("b"), PlayerId::new("c"), PlayerId::new("a")],
            "the card said *this* agenda; the next one votes in the ordinary order"
        );
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

    /// OBS-003e: casting a vote and the speaker's tiebreak are typed distinctly, though both
    /// arise from the same window over the same agenda.
    #[test]
    fn obs003e_vote_and_tiebreak_are_typed_distinctly() {
        let (mut state, _) = game(&["a", "b"]);
        state.speaker = PlayerId::new("a");
        let window = VoteWindow::new(&state, "some_agenda", for_against());
        let cast = window
            .pending_choice(&state, ContentStore::embedded(), POK)
            .expect("somebody votes first");
        let cast_context = cast.context.as_ref().expect("typed context");
        assert_eq!(cast_context.subtype, "cast_vote");

        let mut window = window;
        window.stage = Stage::Tiebreak;
        let tiebreak = window
            .pending_choice(&state, ContentStore::embedded(), POK)
            .expect("the speaker breaks the tie");
        let tiebreak_context = tiebreak.context.as_ref().expect("typed context");
        assert_eq!(tiebreak_context.subtype, "vote_tiebreak");
        assert_eq!(tiebreak.player, state.speaker);
        assert_ne!(cast_context.subtype, tiebreak_context.subtype);
    }

    /// An Elect Planet agenda's outcomes (cast and tiebreak) and every planet exhausted to vote
    /// carry `planet` + `system`; For/Against outcomes and declines do not.
    #[test]
    fn planet_votes_carry_planet_and_system_payloads() {
        use crate::choice::planet_payload::{assert_locates, assert_not_a_planet, offered};
        let content = ContentStore::embedded();
        let (mut state, players) = game(&["a", "b"]);
        state
            .system_mut(&ti4_model::id::SystemId::new("26"))
            .set_control(PlanetId::new("lodor"), players[0].clone());
        state
            .system_mut(&ti4_model::id::SystemId::new("28"))
            .set_control(PlanetId::new("torkan"), players[1].clone());
        let subtype = |choice: &Choice| choice.context.as_ref().unwrap().subtype.clone();

        let ordinary = VoteWindow::new(&state, "some_agenda", for_against());
        let cast = ordinary.pending_choice(&state, content, POK).unwrap();
        for option in &cast.options {
            assert_not_a_planet(option);
        }

        let elect_planet = content
            .records(ContentType::Agendas)
            .iter()
            .find(|record| record.text("target") == Some("Elect Planet"))
            .and_then(|record| record.text("alias"))
            .expect("the corpus has an Elect Planet agenda")
            .to_owned();
        let choices = outcomes(&state, content, POK, &elect_planet);
        let mut window = VoteWindow::new(&state, &elect_planet, choices);
        let cast = window.pending_choice(&state, content, POK).unwrap();
        assert_eq!(subtype(&cast), "cast_vote");
        assert_locates(offered(&cast, "lodor"), "lodor", "26");
        assert_locates(offered(&cast, "torkan"), "torkan", "28");
        assert_not_a_planet(offered(&cast, crate::choice::DECLINE_ID));

        let voter = cast.player.clone();
        let (own, own_system) = if voter == players[0] {
            ("lodor", "26")
        } else {
            ("torkan", "28")
        };
        window
            .resolve(&mut state, content, POK, offered(&cast, "torkan").clone())
            .unwrap();
        let exhaust = window.pending_choice(&state, content, POK).unwrap();
        assert_eq!(subtype(&exhaust), "vote_exhaust_planet");
        assert_locates(offered(&exhaust, own), own, own_system);
        assert_not_a_planet(offered(&exhaust, crate::choice::DECLINE_ID));

        window.stage = Stage::Tiebreak;
        let tiebreak = window.pending_choice(&state, content, POK).unwrap();
        assert_eq!(subtype(&tiebreak), "vote_tiebreak");
        assert_locates(offered(&tiebreak, "lodor"), "lodor", "26");
        assert_locates(offered(&tiebreak, "torkan"), "torkan", "28");
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

    /// Give `player` two distinct planets with influence, so a two-exhaust vote is possible.
    fn give_two_voting_planets(
        state: &mut GameState,
        player: &PlayerId,
    ) -> (PlanetId, i64, PlanetId, i64) {
        let catalogue = all_planets(ContentStore::embedded(), POK);
        let mut found: Vec<(PlanetId, &str, i64)> = catalogue
            .iter()
            .filter(|(_, planet)| planet.influence() > 0 && !planet.is_placed_during_play())
            .take(2)
            .map(|(id, record)| {
                (
                    PlanetId::new(*id),
                    record.system_id().unwrap_or("18"),
                    record.influence(),
                )
            })
            .collect();
        assert_eq!(found.len(), 2, "the corpus has two influential planets");
        let (second_planet, second_system, second_influence) = found.pop().unwrap();
        let (first_planet, first_system, first_influence) = found.pop().unwrap();
        state
            .system_mut(&ti4_model::id::SystemId::new(first_system))
            .set_control(first_planet.clone(), player.clone());
        state
            .system_mut(&ti4_model::id::SystemId::new(second_system))
            .set_control(second_planet.clone(), player.clone());
        (
            first_planet,
            first_influence,
            second_planet,
            second_influence,
        )
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
    fn obs009_a_later_voter_sees_the_running_tally_of_every_outcome() {
        // 8.2ii: the whole point of voting last is knowing every earlier vote. Ballot lives on
        // this window, not on GameState, so a decision built from state alone could not recover
        // it without this payload.
        let (mut state, _) = game(&["a", "b"]);
        let mut window = VoteWindow::new(&state, "x", for_against());
        let first_voter = window.order[0].clone();
        let voting_planet = give_voting_planet(&mut state, &first_voter);
        let expected = influence_of(&state, ContentStore::embedded(), POK, &voting_planet);
        window.open(&state, ContentStore::embedded(), POK);

        let first = window
            .pending_choice(&state, ContentStore::embedded(), POK)
            .unwrap();
        assert_eq!(
            first.option(FOR).unwrap().payload.get("current_votes"),
            Some(&0.into()),
            "nobody has voted yet"
        );
        let outcome = pick(&window, &state, FOR);
        window
            .resolve(&mut state, ContentStore::embedded(), POK, outcome)
            .unwrap();
        let exhaust = window
            .pending_choice(&state, ContentStore::embedded(), POK)
            .unwrap()
            .options[0]
            .clone();
        window
            .resolve(&mut state, ContentStore::embedded(), POK, exhaust)
            .unwrap();

        let second = window
            .pending_choice(&state, ContentStore::embedded(), POK)
            .unwrap();
        assert_eq!(
            second.option(FOR).unwrap().payload.get("current_votes"),
            Some(&expected.into()),
            "the first vote is already on the ledger"
        );
        assert_eq!(
            second.option(AGAINST).unwrap().payload.get("current_votes"),
            Some(&0.into())
        );
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
        let expected = influence_of(&state, ContentStore::embedded(), POK, &planet);
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

    /// Elder Qanoj (reported 2026-09-23 as not working): one vote more for each planet exhausted
    /// to vote, and no game effect bars Xxcha from voting. It was a flat three votes, and a
    /// Political Secret still silenced its owner.
    #[test]
    fn elder_qanoj_adds_a_vote_per_planet_and_cannot_be_silenced() {
        let (mut state, players) = game(&["a"]);
        let (first, first_influence, second, second_influence) =
            give_two_voting_planets(&mut state, &players[0]);
        state.player_mut(&players[0]).unwrap().leaders.insert(
            ti4_model::id::LeaderId::new("xxchacommander"),
            ti4_model::state::LeaderStatus::Unlocked,
        );
        // A Political Secret played on them.
        state
            .agenda_predictions
            .insert(players[0].clone(), "none|political_secret".to_owned());

        let mut window = VoteWindow::new(&state, "x", for_against());
        window.open(&state, ContentStore::embedded(), POK);
        let option = pick(&window, &state, FOR);
        window
            .resolve(&mut state, ContentStore::embedded(), POK, option)
            .unwrap();
        for planet in [first.as_str(), second.as_str()] {
            let option = pick(&window, &state, planet);
            window
                .resolve(&mut state, ContentStore::embedded(), POK, option)
                .unwrap();
        }
        // With no planet left the window records the vote itself; otherwise decline to finish.
        if let Some(done) = window
            .pending_choice(&state, ContentStore::embedded(), POK)
            .and_then(|choice| choice.options.into_iter().find(ChoiceOption::is_decline))
        {
            window
                .resolve(&mut state, ContentStore::embedded(), POK, done)
                .unwrap();
        }
        assert_eq!(
            window.ballot.counts.get(FOR).copied(),
            Some(first_influence + second_influence + 2)
        );
    }

    #[test]
    fn gila_spends_trade_goods_for_two_votes_each_after_the_planets() {
        let (mut state, players) = game(&["a"]);
        let (first, first_influence, _second, _) = give_two_voting_planets(&mut state, &players[0]);
        state.player_mut(&players[0]).unwrap().leaders.insert(
            ti4_model::id::LeaderId::new("hacancommander"),
            ti4_model::state::LeaderStatus::Unlocked,
        );
        state.player_mut(&players[0]).unwrap().trade_goods = 3;
        let content = ContentStore::embedded();
        let mut window = VoteWindow::new(&state, "x", for_against());
        window.open(&state, content, POK);
        let option = pick(&window, &state, FOR);
        window.resolve(&mut state, content, POK, option).unwrap();
        let option = pick(&window, &state, first.as_str());
        window.resolve(&mut state, content, POK, option).unwrap();
        let decline = window
            .pending_choice(&state, content, POK)
            .and_then(|choice| choice.options.into_iter().find(ChoiceOption::is_decline))
            .expect("decline the second planet");
        window.resolve(&mut state, content, POK, decline).unwrap();
        let choice = window
            .pending_choice(&state, content, POK)
            .expect("Gila asks how many trade goods");
        let ids: Vec<&str> = choice.options.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, ["spend|1", "spend|2", "spend|3", "decline"]);
        // Display only: what the panel needs to show votes before and after.
        assert_eq!(choice.details["kind"], "vote_trade_goods");
        assert_eq!(choice.details["outcome"], FOR);
        assert_eq!(choice.details["votes"], first_influence);
        assert_eq!(choice.details["goods"], 3);
        assert_eq!(choice.details["votes_per_good"], 2);
        assert_eq!(choice.details["card"]["title"], "Gila the Silvertongue");
        let two = choice.options[1].clone();
        window.resolve(&mut state, content, POK, two).unwrap();
        assert_eq!(state.player(&players[0]).unwrap().trade_goods, 1);
        assert_eq!(
            window.ballot.counts.get(FOR).copied(),
            Some(first_influence + 4)
        );
    }

    #[test]
    fn gila_is_not_offered_without_the_ability_or_trade_goods() {
        let (mut state, players) = game(&["a"]);
        let (first, first_influence, _second, _) = give_two_voting_planets(&mut state, &players[0]);
        state.player_mut(&players[0]).unwrap().trade_goods = 3;
        let content = ContentStore::embedded();
        let mut window = VoteWindow::new(&state, "x", for_against());
        window.open(&state, content, POK);
        let option = pick(&window, &state, FOR);
        window.resolve(&mut state, content, POK, option).unwrap();
        let option = pick(&window, &state, first.as_str());
        window.resolve(&mut state, content, POK, option).unwrap();
        let decline = window
            .pending_choice(&state, content, POK)
            .and_then(|choice| choice.options.into_iter().find(ChoiceOption::is_decline))
            .unwrap();
        window.resolve(&mut state, content, POK, decline).unwrap();
        assert_eq!(
            window.ballot.counts.get(FOR).copied(),
            Some(first_influence)
        );
        assert_eq!(state.player(&players[0]).unwrap().trade_goods, 3);
    }

    /// BF-00l deferral: a module's vote bonus can now read the content corpus, and rides on votes
    /// actually cast like the commander bonus does.
    #[test]
    fn a_module_vote_bonus_with_content_is_banked_with_the_votes() {
        let (mut state, players) = game(&["a"]);
        let (first, first_influence, _second, _) = give_two_voting_planets(&mut state, &players[0]);
        let hook = crate::factions::hooks_cards::CardHooks {
            vote_bonus_with_content: Some(|_, content, _| {
                // Reads the corpus, which the older `Hooks::vote_bonus` could not.
                i64::from(
                    content
                        .get(ContentType::Agendas, "no_such_agenda")
                        .is_none(),
                ) * 4
            }),
            ..crate::factions::hooks_cards::CardHooks::NONE
        };
        let tally = |bonus: bool| {
            let mut state = state.clone();
            let run = |state: &mut GameState| {
                let mut window = VoteWindow::new(state, "x", for_against());
                window.open(state, ContentStore::embedded(), POK);
                let option = pick(&window, state, FOR);
                window
                    .resolve(state, ContentStore::embedded(), POK, option)
                    .unwrap();
                let option = pick(&window, state, first.as_str());
                window
                    .resolve(state, ContentStore::embedded(), POK, option)
                    .unwrap();
                if let Some(done) = window
                    .pending_choice(state, ContentStore::embedded(), POK)
                    .and_then(|choice| choice.options.into_iter().find(ChoiceOption::is_decline))
                {
                    window
                        .resolve(state, ContentStore::embedded(), POK, done)
                        .unwrap();
                }
                window.ballot.counts.get(FOR).copied()
            };
            if bonus {
                crate::factions::hooks_cards::with_test_hooks(hook, || run(&mut state))
            } else {
                run(&mut state)
            }
        };
        assert_eq!(tally(false), Some(first_influence));
        assert_eq!(tally(true), Some(first_influence + 4));
    }

    #[test]
    fn obs008f2_exhausting_planets_previews_the_running_vote_total() {
        // 8.11: a second exhaust adds to the same outcome's running total, not a fresh count.
        let (mut state, players) = game(&["a"]);
        let (first, first_influence, second, second_influence) =
            give_two_voting_planets(&mut state, &players[0]);

        let mut window = VoteWindow::new(&state, "x", for_against());
        window.open(&state, ContentStore::embedded(), POK);
        let option = pick(&window, &state, FOR);
        window
            .resolve(&mut state, ContentStore::embedded(), POK, option)
            .unwrap();

        let first_option = pick(&window, &state, first.as_str());
        assert_eq!(
            first_option.preview,
            Some(Preview::certain(vec![Delta::new(
                Quantity::Votes,
                0,
                first_influence,
            )]))
        );
        window
            .resolve(&mut state, ContentStore::embedded(), POK, first_option)
            .unwrap();

        let second_option = pick(&window, &state, second.as_str());
        assert_eq!(
            second_option.preview,
            Some(Preview::certain(vec![Delta::new(
                Quantity::Votes,
                first_influence,
                first_influence + second_influence,
            )]))
        );
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

    /// Operator ruling 2026-10-07: the Deepwrought's ocean cards count as influence for voting, like
    /// any other spendable planet, through the real vote window.
    #[test]
    fn ocean_cards_count_as_influence_for_voting() {
        use crate::factions::deepwrought::testkit as dw;
        let sources = ti4_model::content_types::DEFAULT;
        let content = ContentStore::embedded();
        let mut state = dw::game();
        let (system, planet) = dw::plain_planet();
        dw::coexisting_on(&mut state, &system, &planet); // Oceanbound: gains ocean1
        let ocean = PlanetId::new("ocean1");
        assert!(votable_planets(&state, content, sources, &dw::a()).contains(&ocean));
        assert!(
            !votable_planets(&state, content, sources, &dw::b()).contains(&ocean),
            "only the holder's own card"
        );

        let mut window = VoteWindow::new(&state, "x", for_against());
        window.order = vec![dw::a()];
        window.open(&state, content, sources);
        let first = window
            .pending_choice(&state, content, sources)
            .expect("a votes");
        let outcome = first.option(FOR).expect("FOR is offered").clone();
        window
            .resolve(&mut state, content, sources, outcome)
            .expect("a votes for");
        let offered = window
            .pending_choice(&state, content, sources)
            .expect("an exhaust choice");
        let option = offered
            .option("ocean1")
            .expect("the ocean card can be exhausted for its influence")
            .clone();
        window
            .resolve(&mut state, content, sources, option)
            .expect("the ocean is exhausted");
        assert!(state.exhausted_planets.contains(&ocean), "it was exhausted");
        assert!(
            !votable_planets(&state, content, sources, &dw::a()).contains(&ocean),
            "an exhausted ocean cannot vote again"
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
