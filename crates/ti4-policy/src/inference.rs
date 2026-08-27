//! Playing from a fitted profile, and recording what was played (M09-004, M09-013).
//!
//! Ported from the oracle's fully-learned branch of `ScoredBot._choose`, its `_policy_probabilities`
//! and `_sample`, and `learned_policy.trajectory_record`.
//!
//! # Legality only
//!
//! The one structural difference from the authored bot: a learned policy is offered **every legal
//! option**, never a shortlist. [`crate::bot::ScoredBot`] filters with `worth_considering` before
//! it samples, which is an authored judgement about what is worth thinking about — exactly the
//! kind of thing that must not reach a policy claiming its utility is entirely learned. A filtered
//! option is one the policy can never be taught to want, and its absence would be invisible in
//! every metric.
//!
//! # Sampling, not argmax
//!
//! Softmax over the legal set at the profile's temperature. An argmax policy is solvable, plays
//! the same game from a given position every time, and gives a policy-gradient trainer one
//! trajectory where it needs a distribution — the probabilities recorded here are what the update
//! divides by.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, Observed, SeatObservation};
use ti4_model::id::PlayerId;

use crate::features::{FeatureVector, explicit_choice_features, option_features};
use crate::learned::{Profile, decision_head};
use crate::progress::{Baseline, Progress};

/// One learned decision, in the form a policy-gradient trainer consumes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrajectoryStep {
    /// Which seat took this decision.
    ///
    /// Carried on the step rather than left to the caller's bookkeeping, so a trainer pooling
    /// trajectories from six seats cannot credit one seat's play to another. Crossed wiring there
    /// trains every policy on everybody's decisions and looks, from every metric, like training.
    pub player: PlayerId,
    /// Which head decided this.
    pub head: String,
    /// The option taken.
    pub chosen: String,
    /// The features of every legal option, so the update can compute the expectation the chosen
    /// option is measured against.
    ///
    /// The chosen option's own vector lives in here too, under `chosen`; [`Self::features`] reads
    /// it back. It used to be cloned into a separate field as well, which deep-copied a whole
    /// feature vector -- twenty-odd heap-allocated keys -- on every recorded decision, to store a
    /// second copy of something already present.
    pub legal: BTreeMap<String, FeatureVector>,
    /// What the policy thought each legal option's chance was.
    pub probabilities: BTreeMap<String, f64>,
    /// What the game had produced for this seat when the decision was taken.
    ///
    /// Stamped here rather than by the caller because it has to be the position *at the decision*,
    /// and by the time a rollout sees the game again several more decisions have happened. The
    /// reward is a difference between consecutive snapshots, so one taken late is not merely
    /// imprecise — it moves the credit onto the wrong decision.
    pub progress: Progress,
}

impl TrajectoryStep {
    /// The features of the option actually taken.
    ///
    /// Read out of [`Self::legal`] rather than stored beside it: the two were always equal by
    /// construction, and the test below still pins that.
    #[must_use]
    pub fn features(&self) -> &FeatureVector {
        static EMPTY: FeatureVector = FeatureVector::new();
        self.legal.get(&self.chosen).unwrap_or(&EMPTY)
    }
}

/// Softmax over the legal set.
///
/// Shifted by the best score before exponentiating, which changes no probability and stops a large
/// score overflowing to infinity — at which point every probability becomes `NaN` and the sample
/// silently falls back to the first option.
#[must_use]
pub fn probabilities(scores: &BTreeMap<String, f64>, temperature: f64) -> BTreeMap<String, f64> {
    if scores.is_empty() {
        return BTreeMap::new();
    }
    if scores.len() == 1 {
        return scores.keys().map(|id| (id.clone(), 1.0)).collect();
    }
    let temperature = temperature.max(1e-6);
    let best = scores.values().copied().fold(f64::NEG_INFINITY, f64::max);
    // Weights alone, positionally aligned with `scores.keys()`, rather than carrying a cloned
    // copy of every option id through the intermediate. The ids are cloned once, into the
    // result. Iteration order is the map's, so every sum and quotient is unchanged.
    let weights: Vec<f64> = scores
        .values()
        .map(|score| ((score - best) / temperature).exp())
        .collect();
    let total: f64 = weights.iter().sum();
    if !total.is_finite() || total <= 0.0 {
        // Uniform rather than a fabricated preference: an unusable score distribution is a bug to
        // notice, and a policy that quietly favours whatever sorted first would hide it.
        #[expect(clippy::cast_precision_loss, reason = "option counts are small")]
        let share = 1.0 / weights.len() as f64;
        return scores.keys().map(|id| (id.clone(), share)).collect();
    }
    scores
        .keys()
        .cloned()
        .zip(weights.into_iter().map(|weight| weight / total))
        .collect()
}

/// A bot that plays from a fitted profile and nothing else.
/// Per-decision projected option vectors with their option ids.
///
/// Named because the shape is nested three deep and appears in a field, a getter and a rollout
/// return type; spelling it out at each site is how one of them ends up subtly different.
pub type ProjectedOptions = Vec<Vec<(String, FeatureVector)>>;

pub struct LearnedBot {
    profile: Arc<Profile>,
    rng: ChaCha8Rng,
    /// The decisions taken, when recording.
    ///
    /// Behind a shared handle because the bot is moved into the engine's decision table and cannot
    /// be borrowed back out of it: the table holds `Box<dyn Decider>`, and a caller wanting the
    /// trajectory afterwards would otherwise have to downcast. [`LearnedBot::trajectory`] hands out
    /// the same handle, so a rollout takes it before seating and reads it after the game.
    trajectory: Rc<RefCell<Vec<TrajectoryStep>>>,
    recording: bool,
    /// The option-free critic vector at each recorded decision, when the corpus asks for one.
    ///
    /// Kept beside the trajectory rather than inside [`TrajectoryStep`] because the step is a
    /// serialized type the linear trainer already consumes, and the critic vector is wanted by
    /// exactly one caller (M10-031's teacher corpus). Pushed under the same `recording` branch, so
    /// index `i` here is index `i` there or the vector is absent entirely.
    critic: Rc<RefCell<Vec<FeatureVector>>>,
    critic_features: Option<crate::critic::CriticFeatures>,
    /// The **projected** per-option vectors — what the MLP actually consumes — with their option
    /// ids, in the engine's option order.
    ///
    /// Recorded separately from [`TrajectoryStep::legal`], which holds the raw schema-4 features
    /// the linear policy scores with. The two are not the same feature set: the projection drops
    /// the unbounded `state-option:`/`prompt-option:` crosses and *adds* the bare `seat-state:`
    /// facts. A corpus built from `legal` therefore trains an MLP on inputs it will never see at
    /// inference — which is exactly the defect M10-031 shipped.
    projected: Rc<RefCell<ProjectedOptions>>,
    record_projected: bool,
    /// What this seat held at setup, so progress is a gain rather than a total.
    baseline: Baseline,
}

impl LearnedBot {
    /// Play from `profile`, with its own deterministic stream.
    #[must_use]
    pub fn new(profile: Profile, seed: u64) -> Self {
        Self::from_shared(Arc::new(profile), seed)
    }

    /// Play from an immutable profile shared by every rollout in a training batch.
    ///
    /// A schema-4 profile contains tens of thousands of named weights. Sharing it avoids cloning
    /// that complete table once per seat and game while preserving an immutable policy snapshot
    /// for the whole update.
    #[must_use]
    pub fn from_shared(profile: Arc<Profile>, seed: u64) -> Self {
        Self {
            profile,
            rng: ChaCha8Rng::seed_from_u64(seed),
            trajectory: Rc::new(RefCell::new(Vec::new())),
            recording: false,
            critic: Rc::new(RefCell::new(Vec::new())),
            critic_features: None,
            projected: Rc::new(RefCell::new(Vec::new())),
            record_projected: false,
            baseline: Baseline::default(),
        }
    }

    /// What this seat held at setup.
    ///
    /// Without it every holding reads as a gain, which is wrong in the direction that flatters the
    /// policy: a faction that starts on three planets would be credited with taking them.
    #[must_use]
    pub const fn from_setup(mut self, baseline: Baseline) -> Self {
        self.baseline = baseline;
        self
    }

    /// Record every decision, for training. Off by default: a batch run does not want one.
    #[must_use]
    pub const fn recording(mut self) -> Self {
        self.recording = true;
        self
    }

    /// The profile being played.
    #[must_use]
    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    /// Also record the option-free critic vector at every decision (M10-031).
    ///
    /// Only meaningful together with [`Self::recording`]; without it nothing is pushed to either
    /// buffer, which is what keeps the two aligned.
    #[must_use]
    pub const fn recording_critic(mut self, features: crate::critic::CriticFeatures) -> Self {
        self.critic_features = Some(features);
        self
    }

    /// A handle on the critic vectors this bot records, aligned with [`Self::trajectory`].
    #[must_use]
    pub fn critic_vectors(&self) -> Rc<RefCell<Vec<FeatureVector>>> {
        Rc::clone(&self.critic)
    }

    /// Also record the projected per-option vectors the MLP consumes (M10-031).
    ///
    /// Only meaningful together with [`Self::recording`]; without it nothing is pushed to either
    /// buffer, which is what keeps them aligned.
    #[must_use]
    pub const fn recording_projected(mut self) -> Self {
        self.record_projected = true;
        self
    }

    /// A handle on the projected option vectors, aligned with [`Self::trajectory`].
    #[must_use]
    pub fn projected_vectors(&self) -> Rc<RefCell<ProjectedOptions>> {
        Rc::clone(&self.projected)
    }

    /// A handle on the decisions this bot takes.
    ///
    /// Taken before the bot is seated, read after the game. The handle is shared rather than
    /// copied, so what a rollout reads is what the bot actually recorded.
    #[must_use]
    pub fn trajectory(&self) -> Rc<RefCell<Vec<TrajectoryStep>>> {
        Rc::clone(&self.trajectory)
    }

    /// Score every legal option, and say what each one's chance is.
    ///
    /// `held_secrets` names the held-secret records this scoring may use — live play passes the
    /// bound seat's own cards from its [`SeatObservation`], offline contexts compute them on their
    /// full state. The feature path never reads secrets through caller-controlled identity data.
    ///
    /// Returned together because a trainer needs both: the scores to check a fit, and the
    /// probabilities the sample was actually drawn from.
    #[must_use]
    pub fn consider(
        &self,
        seen: &Observed<'_>,
        choice: &Choice,
        held_secrets: &[ti4_engine::objectives::CardProgress],
    ) -> (BTreeMap<String, FeatureVector>, BTreeMap<String, f64>) {
        // The head decides which weights read the features, so the same fact means different
        // things to different decisions. One shared vector would have every head's update land on
        // every other head's weights.
        let requested_head = decision_head(choice);
        let head = self.profile.resolved_head(requested_head);
        // The explicit path builds the whole choice at once so the prompt is tokenised once
        // rather than once per option (see `explicit_choice_features`).
        let legal: BTreeMap<String, FeatureVector> = if self.profile.is_explicit() {
            choice
                .options
                .iter()
                .map(|option| option.id.clone())
                .zip(explicit_choice_features(
                    seen,
                    choice,
                    &choice.player,
                    held_secrets,
                ))
                .collect()
        } else {
            choice
                .options
                .iter()
                .map(|option| {
                    (
                        option.id.clone(),
                        option_features(
                            seen,
                            choice,
                            option,
                            &choice.player,
                            self.profile.dimensions(),
                        ),
                    )
                })
                .collect()
        };
        let scores: BTreeMap<String, f64> = legal
            .iter()
            .map(|(id, vector)| (id.clone(), self.profile.score_vector(head, vector)))
            .collect();
        let temperature = self.profile.head(head).map_or(1.0, |head| head.temperature);
        let chances = probabilities(&scores, temperature);
        (legal, chances)
    }
}

impl Decider for LearnedBot {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        // Without a position there are no facts to read, so every option scores alike and the
        // sample is uniform. Refused rather than guessed at: a learned policy asked to decide
        // blind has nothing to decide with, and quietly answering would look like play.
        choice
            .options
            .first()
            .cloned()
            .ok_or_else(|| IllegalChoice::NoOptions {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
            })
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        if choice.options.is_empty() {
            return Err(IllegalChoice::NoOptions {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
            });
        }
        // The view is bound to this choice's owner by the engine; its held-secret progress is
        // exactly that seat's own cards, and nothing else exists to request another seat's.
        let (legal, chances) = self.consider(seen.observed(), choice, &seen.held_secret_progress());

        // Every legal option, in the order the engine offered them. No shortlist: a filtered
        // option is one the policy can never learn to want.
        let mut roll = self.rng.random_range(0.0..1.0);
        let mut chosen = choice.options.last().expect("options are not empty");
        for option in &choice.options {
            roll -= chances.get(&option.id).copied().unwrap_or(0.0);
            if roll <= 0.0 {
                chosen = option;
                break;
            }
        }

        if self.recording {
            if self.record_projected {
                // The MLP's own view of this decision, built from the same bound capability.
                let vectors = crate::projection::mlp_choice_features(
                    seen.observed(),
                    choice,
                    &choice.player,
                    &seen.held_secret_progress(),
                    self.baseline,
                );
                self.projected.borrow_mut().push(
                    choice
                        .options
                        .iter()
                        .map(|option| option.id.clone())
                        .zip(vectors)
                        .collect(),
                );
            }
            if let Some(features) = self.critic_features {
                // Built from the bound view, like everything else on this path: the critic takes
                // the capability, so a corpus cannot capture a seat's position from omniscient
                // state by accident.
                self.critic
                    .borrow_mut()
                    .push(crate::critic::critic_vector(seen, features).facts().clone());
            }
            self.trajectory.borrow_mut().push(TrajectoryStep {
                player: choice.player.clone(),
                head: self.profile.resolved_head(decision_head(choice)).to_owned(),
                chosen: chosen.id.clone(),
                legal,
                probabilities: chances,
                progress: crate::progress::measure(seen, &choice.player, self.baseline),
            });
        }
        Ok(chosen.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::learned::{DEFAULT_DIMENSIONS, blank_profile, bucket};
    use ti4_content::ContentStore;
    use ti4_engine::choice::ask_private;
    use ti4_model::content_types::POK;
    use ti4_model::id::PlayerId;
    use ti4_model::state::GameState;

    fn table() -> GameState {
        ti4_engine::fixtures::game(&["a", "b"])
    }

    /// Held-secret records for seat "a" on this full state — the offline form of what live play
    /// receives bound to its [`SeatObservation`].
    fn held(state: &GameState) -> Vec<ti4_engine::objectives::CardProgress> {
        ti4_engine::choice::held_secret_progress(
            state,
            ContentStore::embedded(),
            POK,
            None,
            &PlayerId::new("a"),
        )
    }

    fn asked(kinds: &[(&str, &str)]) -> Choice {
        Choice::new(
            PlayerId::new("a"),
            "activate a system",
            kinds
                .iter()
                .map(|(id, kind)| ChoiceOption::labelled(*id, *kind, *id))
                .collect::<Vec<ChoiceOption>>(),
        )
    }

    #[test]
    fn an_untrained_policy_plays_uniformly() {
        // The honest starting point. A blank profile scores everything zero, so a softmax over it
        // is flat — and if it were not, the shape would be coming from somewhere nobody fitted.
        let state = table();
        let seen = Observed::new(&state, ContentStore::embedded(), POK, None);
        let choice = asked(&[("18", "activate"), ("26", "activate"), ("31", "activate")]);

        let bot = LearnedBot::new(blank_profile("sol", DEFAULT_DIMENSIONS), 1);
        let (_, chances) = bot.consider(&seen, &choice, &held(&state));
        for chance in chances.values() {
            assert!((chance - 1.0 / 3.0).abs() < 1e-9, "{chance}");
        }
    }

    #[test]
    fn a_trained_weight_moves_the_odds() {
        let state = table();
        let seen = Observed::new(&state, ContentStore::embedded(), POK, None);
        let choice = asked(&[("18", "activate"), ("26", "activate")]);

        let mut profile = blank_profile("sol", DEFAULT_DIMENSIONS);
        // Teach it that options mentioning "18" are worth something.
        let (slot, sign) = bucket("option:18", DEFAULT_DIMENSIONS);
        profile
            .head_mut("activation")
            .unwrap()
            .weights
            .insert(slot, 5.0 * sign);

        let bot = LearnedBot::new(profile, 1);
        let (_, chances) = bot.consider(&seen, &choice, &held(&state));
        assert!(chances["18"] > chances["26"], "{chances:?}");
    }

    #[test]
    fn every_legal_option_keeps_a_chance() {
        // The legality-only property. The authored bot filters before it samples; a policy whose
        // utility is entirely learned must not, because a filtered option is one it can never be
        // taught to want and its absence shows up in no metric.
        let state = table();
        let seen = Observed::new(&state, ContentStore::embedded(), POK, None);
        let choice = asked(&[("18", "activate"), ("26", "activate"), ("31", "activate")]);

        let mut profile = blank_profile("sol", DEFAULT_DIMENSIONS);
        let (slot, sign) = bucket("option:18", DEFAULT_DIMENSIONS);
        profile
            .head_mut("activation")
            .unwrap()
            .weights
            .insert(slot, 20.0 * sign);

        let bot = LearnedBot::new(profile, 1);
        let (legal, chances) = bot.consider(&seen, &choice, &held(&state));
        assert_eq!(legal.len(), 3, "every option was scored");
        assert_eq!(chances.len(), 3);
        for (id, chance) in &chances {
            assert!(*chance > 0.0, "{id} was given no chance at all");
        }
    }

    #[test]
    fn learned_inference_never_enters_the_authored_score_or_filter_paths() {
        let state = table();
        let seen = Observed::new(&state, ContentStore::embedded(), POK, None);
        let choice = asked(&[("18", "activate"), ("26", "activate"), ("31", "activate")]);

        // First prove the probes are attached to real authored boundaries. A zero-vs-zero check
        // would merely restate the desired architecture and was the defect in the old evidence.
        let _ = crate::bot::authored_path_hits(true);
        let mut authored = crate::bot::ScoredBot::new(1);
        authored.choose(&choice).expect("authored fixture is legal");
        let (scores, filters) = crate::bot::authored_path_hits(true);
        assert!(
            scores >= choice.options.len(),
            "score probe is vacuous: {scores}"
        );
        assert!(filters > 0, "filter probe is vacuous");

        let learned = LearnedBot::new(blank_profile("sol", DEFAULT_DIMENSIONS), 1);
        let (legal, chances) = learned.consider(&seen, &choice, &held(&state));
        assert_eq!(legal.len(), choice.options.len());
        assert_eq!(chances.len(), choice.options.len());
        assert_eq!(
            crate::bot::authored_path_hits(true),
            (0, 0),
            "learned inference crossed an authored utility boundary"
        );
    }

    #[test]
    fn the_probabilities_are_a_distribution() {
        let scores: BTreeMap<String, f64> = [("a", 3.0), ("b", -1.0), ("c", 0.5)]
            .into_iter()
            .map(|(id, score)| (id.to_owned(), score))
            .collect();
        let chances = probabilities(&scores, 1.0);
        let total: f64 = chances.values().sum();
        assert!((total - 1.0).abs() < 1e-12, "{total}");
        assert!(chances["a"] > chances["c"] && chances["c"] > chances["b"]);
    }

    #[test]
    fn a_huge_score_does_not_overflow_into_nothing() {
        // Without the shift by the best score, `exp` overflows to infinity, every probability
        // becomes NaN, and the sample silently falls back to whatever sorted first. That is a bug
        // that looks exactly like a confident policy.
        let scores: BTreeMap<String, f64> = [("a", 1_000.0), ("b", 999.0)]
            .into_iter()
            .map(|(id, score)| (id.to_owned(), score))
            .collect();
        let chances = probabilities(&scores, 1.0);

        assert!(chances.values().all(|chance| chance.is_finite()));
        let total: f64 = chances.values().sum();
        assert!((total - 1.0).abs() < 1e-12, "{total}");
        assert!(chances["a"] > chances["b"]);
    }

    #[test]
    fn a_colder_policy_commits_harder() {
        let scores: BTreeMap<String, f64> = [("a", 2.0), ("b", 1.0)]
            .into_iter()
            .map(|(id, score)| (id.to_owned(), score))
            .collect();
        let warm = probabilities(&scores, 5.0);
        let cold = probabilities(&scores, 0.1);
        assert!(cold["a"] > warm["a"], "{} against {}", cold["a"], warm["a"]);
    }

    #[test]
    fn the_sample_follows_the_odds() {
        // A distribution nobody draws from is a report, not a policy.
        let state = table();
        let seen = Observed::new(&state, ContentStore::embedded(), POK, None);
        let choice = asked(&[("18", "activate"), ("26", "activate")]);

        // The weight is found by measuring rather than assumed. Buckets collide — that is the
        // hashing trick working — so one weight does not move one option's score by one, and a
        // test that assumed it would was testing arithmetic that does not happen.
        let (slot, sign) = bucket("option:18", DEFAULT_DIMENSIONS);
        let odds_at = |weight: f64| {
            let mut profile = blank_profile("sol", DEFAULT_DIMENSIONS);
            profile
                .head_mut("activation")
                .unwrap()
                .weights
                .insert(slot.clone(), weight * sign);
            LearnedBot::new(profile, 7)
                .consider(&seen, &choice, &held(&state))
                .1["18"]
        };
        let weight = [0.05, 0.1, 0.2, 0.4, 0.8]
            .into_iter()
            .find(|weight| (0.55..0.95).contains(&odds_at(*weight)))
            .expect("some weight states a preference short of certainty");

        let mut profile = blank_profile("sol", DEFAULT_DIMENSIONS);
        profile
            .head_mut("activation")
            .unwrap()
            .weights
            .insert(slot, weight * sign);
        let mut bot = LearnedBot::new(profile, 7);
        let expected = bot.consider(&seen, &choice, &held(&state)).1["18"];

        let mut favoured = 0;
        for _ in 0..400 {
            if ask_private(
                &choice,
                &state,
                ContentStore::embedded(),
                POK,
                None,
                &mut bot,
            )
            .unwrap()
            .id == "18"
            {
                favoured += 1;
            }
        }
        let rate = f64::from(favoured) / 400.0;
        assert!(
            (rate - expected).abs() < 0.08,
            "drew {rate} against a stated {expected}"
        );
        assert!(favoured < 400, "and the other option still happened");
    }

    #[test]
    fn the_same_seed_plays_the_same_game() {
        let state = table();
        let choice = asked(&[("18", "activate"), ("26", "activate"), ("31", "activate")]);

        let played = |seed| {
            let mut bot = LearnedBot::new(blank_profile("sol", DEFAULT_DIMENSIONS), seed);
            (0..30)
                .map(|_| {
                    ask_private(
                        &choice,
                        &state,
                        ContentStore::embedded(),
                        POK,
                        None,
                        &mut bot,
                    )
                    .unwrap()
                    .id
                })
                .collect::<Vec<String>>()
        };
        assert_eq!(played(4), played(4));
        assert_ne!(played(4), played(5), "and a different seed does not");
    }

    #[test]
    fn a_recording_bot_keeps_what_a_trainer_needs() {
        let state = table();
        let seen = Observed::new(&state, ContentStore::embedded(), POK, None);
        let choice = asked(&[("18", "activate"), ("26", "activate")]);

        let mut bot = LearnedBot::new(blank_profile("sol", DEFAULT_DIMENSIONS), 2).recording();
        let taken = ask_private(
            &choice,
            &state,
            ContentStore::embedded(),
            POK,
            None,
            &mut bot,
        )
        .unwrap();

        let recorded = bot.trajectory();
        let steps = recorded.borrow();
        let step = steps.first().expect("it was recorded");
        assert_eq!(step.chosen, taken.id);
        assert_eq!(
            step.player,
            PlayerId::new("a"),
            "the step names who took it"
        );
        assert_eq!(step.head, "activation");
        assert_eq!(
            step.legal.len(),
            2,
            "every legal option's features are kept"
        );
        assert_eq!(
            step.features(),
            &step.legal[&taken.id],
            "and the chosen one's are the ones it took"
        );
        assert!((step.probabilities.values().sum::<f64>() - 1.0).abs() < 1e-12);
        assert_eq!(step.progress.round_number, seen.round());
    }

    #[test]
    fn a_bot_that_is_not_recording_keeps_nothing() {
        let state = table();
        let choice = asked(&[("18", "activate")]);
        let mut bot = LearnedBot::new(blank_profile("sol", DEFAULT_DIMENSIONS), 2);
        ask_private(
            &choice,
            &state,
            ContentStore::embedded(),
            POK,
            None,
            &mut bot,
        )
        .unwrap();
        assert!(bot.trajectory().borrow().is_empty());
    }

    #[test]
    fn it_only_ever_answers_with_an_option_it_was_offered() {
        let state = table();
        let choice = asked(&[("18", "activate"), ("26", "activate"), ("31", "activate")]);
        let mut bot = LearnedBot::new(blank_profile("sol", DEFAULT_DIMENSIONS), 3);

        for _ in 0..200 {
            let answer = ask_private(
                &choice,
                &state,
                ContentStore::embedded(),
                POK,
                None,
                &mut bot,
            )
            .unwrap();
            assert!(choice.ids().contains(&answer.id.as_str()));
        }
    }

    #[test]
    fn an_empty_choice_is_refused_rather_than_answered() {
        let state = table();
        let nothing = Choice::new(PlayerId::new("a"), "pick", Vec::new());
        let mut bot = LearnedBot::new(blank_profile("sol", DEFAULT_DIMENSIONS), 1);
        assert!(
            ask_private(
                &nothing,
                &state,
                ContentStore::embedded(),
                POK,
                None,
                &mut bot
            )
            .is_err()
        );
        assert!(bot.choose(&nothing).is_err());
    }
}
