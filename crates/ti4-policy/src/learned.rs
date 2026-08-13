//! A legality-only policy whose utility is entirely learned (M09-001, M09-002, M09-006).
//!
//! Ported from the oracle's `engine/learned_policy.py`.
//!
//! # Why this exists beside the authored bot
//!
//! [`crate::bot::ScoredBot`] decides by hand-written constants. Some of them are defensible — a
//! victory point is worth more than a trade good — and some are numbers somebody picked, like the
//! flat `6.0` this repo scores every system activation with. Nobody can validate the second kind,
//! and no amount of adding more of them turns them into the first kind.
//!
//! This module is the other answer: the engine decides what is **legal**, and everything about
//! what is *good* is a weight fitted from played games. The features are facts — the option's
//! kind, the prompt, what the seat holds, what is on the board — and never an authored score. That
//! separation is the point rather than a detail, and M09-014 exists to prove instrumented that no
//! authored utility can reach inference.
//!
//! # Shape
//!
//! - A choice is routed to one of a small number of independently learned **heads** by
//!   [`decision_head`], so learning to move ships does not disturb learning to vote.
//! - Features are hashed into a fixed number of signed buckets by [`bucket`] — the hashing trick,
//!   which keeps the weight vector a fixed size however many distinct facts turn up.
//! - A blank profile is all zeros, which scores every legal option identically and therefore plays
//!   uniformly at random. That is the honest starting point: an untrained policy should look like
//!   one.
//!
//! [`bucket`] is bit-compatible with the oracle's `_bucket`, because existing checkpoints are
//! vectors of numbers indexed by it. A hash that disagreed in one bit would load every trained
//! profile and score it as noise, and nothing would report an error.

use std::collections::BTreeMap;

use blake2::digest::consts::U8;
use blake2::{Blake2b, Digest};
use serde::{Deserialize, Serialize};
use ti4_engine::choice::Choice;

/// The hashed-policy schema this module reads and writes.
pub const SCHEMA: u32 = 2;

/// How many signed buckets a hashed profile carries by default.
pub const DEFAULT_DIMENSIONS: usize = 512;

/// The independently learned utility heads.
///
/// One head per kind of decision rather than one policy over everything: the features that decide
/// a movement are unrelated to those that decide a vote, and sharing weights between them makes
/// each one's training noise the other's signal.
pub const DECISION_HEADS: [&str; 19] = [
    "strategy",
    "secondary",
    "turn",
    "activation",
    "movement",
    "cargo",
    "landing",
    "trade",
    "tokens",
    "production",
    "payment",
    "development",
    "combat",
    "scoring",
    "agenda",
    "exploration",
    "ability",
    "transit",
    "other",
];

/// Option kinds that say nothing about what a choice is *about*.
///
/// Defensive rather than load-bearing today: none of these appears in any routing table, so
/// filtering them changes no answer. Kept because the oracle keeps it, and because the day one of
/// them gains a table entry it would silently swallow half the game — "decline" is offered in
/// nearly every window in the game.
const UNINFORMATIVE: [&str; 5] = ["decline", "done", "stop", "yes", "no"];

/// The oracle's `_HEAD_BY_KIND`, verbatim.
///
/// Taken from the running module rather than transcribed by eye, because reading it by eye got
/// four of these wrong: `strategy` routes to **secondary** and `strategy_card` to `strategy`,
/// which is the opposite of what the names suggest; `retreat` is a combat decision rather than a
/// movement one; `ready` is production; and votes are an agenda head rather than the catch-all.
fn oracle_head(kind: &str) -> Option<&'static str> {
    let head = match kind {
        "action" | "pass" => "turn",
        "activate" | "system" => "activation",
        "build" | "produce" | "ready" => "production",
        "casualty" | "combat_modifier" | "retreat" => "combat",
        "commit" => "landing",
        "load" => "cargo",
        "move" => "movement",
        "offer" | "replenish" | "transaction" => "trade",
        "pay" | "payment" => "payment",
        "pool" => "tokens",
        "research" | "technology" => "development",
        "strategy" => "secondary",
        "strategy_card" => "strategy",
        _ => return None,
    };
    Some(head)
}

/// The oracle's `_OTHER_SPLIT_BY_KIND`, verbatim.
///
/// Consulted only after [`oracle_head`] has been tried against every kind on offer, which is the
/// precedence the oracle uses: these are the kinds schema 5 split out of the catch-all head, and a
/// choice that also offers a first-class kind belongs with that one.
fn oracle_other_head(kind: &str) -> Option<&'static str> {
    let head = match kind {
        "ability" | "action_card" | "breakthrough" | "leader" => "ability",
        "agenda" | "quash" | "speaker" | "tiebreak" | "vote" | "vote_planet" => "agenda",
        "annex" | "explore" | "frontier" | "relic" => "exploration",
        "score" => "scoring",
        "transit" => "transit",
        _ => return None,
    };
    Some(head)
}

/// Kinds this engine raises that the oracle's tables do not name.
///
/// Kept apart from the ported tables rather than merged into them, so the ported ones stay
/// checkable against their source. Each lands on the head its oracle counterpart uses: this engine
/// says `land` where the oracle says `commit`, and splits some of the oracle's kinds finer.
fn local_head(kind: &str) -> Option<&'static str> {
    let head = match kind {
        "land" => "landing", // the oracle's `commit`
        "ground_casualty" | "reaction" | "retreat_to" | "sustain" => "combat",
        "place" => "production",
        "spend" => "payment",
        "ready_technology" => "development",
        "open_transaction" | "answer" => "trade",
        "discard" | "return" | "remove" => "scoring",
        _ => return None,
    };
    Some(head)
}

/// Route one legal choice to one independently learned utility head.
///
/// Kinds first, prompt second. A prompt is free text an engine change can reword, so it decides
/// only what the kinds could not.
#[must_use]
pub fn decision_head(choice: &Choice) -> &'static str {
    let mut kinds: Vec<&str> = choice
        .options
        .iter()
        .map(|option| option.kind.as_str())
        .collect();
    kinds.sort_unstable();
    kinds.dedup();

    let meaningful: Vec<&str> = kinds
        .iter()
        .copied()
        .filter(|kind| !UNINFORMATIVE.contains(kind))
        .collect();
    let ordered = if meaningful.is_empty() {
        &kinds
    } else {
        &meaningful
    };
    // Three passes in the oracle's order: first-class kinds, then the kinds schema 5 split out of
    // the catch-all, then this engine's own. A choice offering both a first-class kind and a split
    // one belongs with the first-class one, which is why these are not one table.
    for table in [oracle_head, oracle_other_head, local_head] {
        for kind in ordered {
            if let Some(head) = table(kind) {
                return head;
            }
        }
    }

    let prompt = choice.prompt.to_lowercase();
    if prompt.contains("secondary") || prompt.contains("strategy token") {
        return "secondary";
    }
    if prompt == "movement" {
        return "movement";
    }
    if prompt.contains("produce") || prompt.contains("build") {
        return "production";
    }
    if prompt.contains("payment") || prompt.contains("spend") || prompt.contains("pay ") {
        return "payment";
    }
    if prompt.contains("trade") || prompt.contains("transaction") {
        return "trade";
    }
    if prompt.contains("combat") || prompt.contains("hit") || prompt.contains("retreat") {
        return "combat";
    }
    "other"
}

/// One feature name's bucket and sign (the hashing trick).
///
/// Bit-compatible with the oracle's `_bucket`: blake2b with an eight-byte digest, the first four
/// bytes little-endian modulo the dimension count, and the sign taken from the low bit of the
/// fifth. Every trained checkpoint is a vector indexed by exactly this, so a disagreement in one
/// bit would silently score every existing profile as noise.
///
/// # Panics
/// Never. `dimensions` of zero is treated as one, because a profile with no buckets cannot be
/// built by [`blank_profile`] and a caller passing zero here wants the degenerate case, not a
/// division fault.
#[must_use]
pub fn bucket(name: &str, dimensions: usize) -> (String, f64) {
    let digest = Blake2b::<U8>::digest(name.as_bytes());
    let index = u32::from_le_bytes([digest[0], digest[1], digest[2], digest[3]]) as usize
        % dimensions.max(1);
    let sign = if digest[4] & 1 == 1 { 1.0 } else { -1.0 };
    (format!("h{index:04}"), sign)
}

/// A fitted profile: the weights, and enough metadata to refuse the wrong one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    /// The schema this profile was written against.
    pub schema: u32,
    /// Always `fully_learned` for this module. Present so a profile carrying authored weights
    /// cannot be loaded here by accident.
    pub mode: String,
    /// A human-readable name.
    pub name: String,
    /// The faction this profile plays.
    pub faction: String,
    /// The learned part.
    pub learned: Learned,
}

/// The weights, one independently learned set per decision head.
///
/// **A divergence from the oracle's schema 2, and a deliberate one.** That schema carries a single
/// flat weight vector; its *trainer* keys statistics and updates by `(faction, head)` against a
/// per-head layout. Carrying one flat vector here would mean every head's update landing on every
/// other head's weights, which is the thing the heads exist to prevent — learning to move ships
/// would drift the weights that decide votes, and neither would converge.
///
/// Importing an oracle schema-2 checkpoint therefore needs a migration that copies its flat vector
/// into each head. That is M09-015's job and is named here rather than discovered when the first
/// checkpoint scores as noise.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Learned {
    /// One weight set per head in [`DECISION_HEADS`].
    pub heads: BTreeMap<String, Head>,
}

/// One head's fitted weights and how sharply it commits to them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Head {
    /// Bucket name to weight. Buckets absent from the map score zero.
    pub weights: BTreeMap<String, f64>,
    /// How sharply to prefer the best option. One is the fitted default.
    pub temperature: f64,
}

impl Head {
    /// An untrained head: every bucket zero.
    #[must_use]
    pub fn blank(dimensions: usize) -> Self {
        Self {
            weights: (0..dimensions.max(1))
                .map(|index| (format!("h{index:04}"), 0.0))
                .collect(),
            temperature: 1.0,
        }
    }

    /// Score an already-hashed sparse vector: the dot product with these weights.
    ///
    /// The signs are baked into the vector when it is built, so this is a plain dot product and
    /// not a second hashing.
    #[must_use]
    pub fn score_vector(&self, features: &BTreeMap<String, f64>) -> f64 {
        features
            .iter()
            .map(|(slot, value)| self.weights.get(slot).copied().unwrap_or(0.0) * value)
            .sum()
    }
}

/// Why a profile was refused.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ProfileError {
    /// Written against a schema this module does not read.
    #[error("profile schema {found} is not the {SCHEMA} this module reads")]
    Schema {
        /// The schema found on the profile.
        found: u32,
    },
    /// Not a fully learned profile.
    #[error("profile mode {found:?} is not \"fully_learned\"")]
    Mode {
        /// The mode found on the profile.
        found: String,
    },
    /// Built for a different faction than the seat asking for it.
    #[error("profile is for faction {found:?}, not {wanted:?}")]
    Faction {
        /// The faction the profile names.
        found: String,
        /// The faction the seat plays.
        wanted: String,
    },
    /// A head that no choice can route to, or a head every choice needs and nothing carries.
    #[error("profile is missing a weight set for the {head:?} head")]
    MissingHead {
        /// The head with no weights.
        head: String,
    },
    /// No weights at all.
    #[error("profile carries no weights")]
    Empty,
    /// A weight or the temperature is not a finite number.
    #[error("profile carries a non-finite {what}")]
    NotFinite {
        /// Which value was not finite.
        what: String,
    },
    /// A temperature at or below zero, which makes the softmax undefined.
    #[error("profile temperature {found} must be above zero")]
    Temperature {
        /// The temperature found.
        found: f64,
    },
}

/// An untrained profile: every head, every bucket zero.
///
/// Scores every legal option identically, so it plays uniformly at random. That is deliberate — an
/// untrained policy should be visibly untrained, rather than inheriting a shape from somewhere.
#[must_use]
pub fn blank_profile(faction: &str, dimensions: usize) -> Profile {
    Profile {
        schema: SCHEMA,
        mode: "fully_learned".to_owned(),
        name: format!("blank-learned-{faction}"),
        faction: faction.to_owned(),
        learned: Learned {
            heads: DECISION_HEADS
                .iter()
                .map(|head| ((*head).to_owned(), Head::blank(dimensions)))
                .collect(),
        },
    }
}

impl Profile {
    /// Check a profile before it is trusted to decide anything.
    ///
    /// Refused rather than repaired. A profile that is wrong in any of these ways is a training
    /// artifact from somewhere else, and quietly coercing it into shape would make a policy that
    /// scores nonsense look like one that plays badly.
    ///
    /// # Errors
    /// [`ProfileError`] naming what was wrong.
    pub fn validate(&self, faction: Option<&str>) -> Result<(), ProfileError> {
        if self.schema != SCHEMA {
            return Err(ProfileError::Schema { found: self.schema });
        }
        if self.mode != "fully_learned" {
            return Err(ProfileError::Mode {
                found: self.mode.clone(),
            });
        }
        if let Some(wanted) = faction
            && self.faction != wanted
        {
            return Err(ProfileError::Faction {
                found: self.faction.clone(),
                wanted: wanted.to_owned(),
            });
        }
        if self.learned.heads.is_empty() {
            return Err(ProfileError::Empty);
        }
        // Every head a choice can route to must exist, or the decisions that route there score
        // zero for ever and the policy silently plays them at random.
        for head in DECISION_HEADS {
            if !self.learned.heads.contains_key(head) {
                return Err(ProfileError::MissingHead {
                    head: head.to_owned(),
                });
            }
        }
        for (name, head) in &self.learned.heads {
            if head.weights.is_empty() {
                return Err(ProfileError::Empty);
            }
            if let Some((slot, _)) = head.weights.iter().find(|(_, weight)| !weight.is_finite()) {
                return Err(ProfileError::NotFinite {
                    what: format!("weight {slot} of head {name}"),
                });
            }
            if !head.temperature.is_finite() {
                return Err(ProfileError::NotFinite {
                    what: format!("temperature of head {name}"),
                });
            }
            if head.temperature <= 0.0 {
                return Err(ProfileError::Temperature {
                    found: head.temperature,
                });
            }
        }
        Ok(())
    }

    /// How many buckets each head carries.
    #[must_use]
    pub fn dimensions(&self) -> usize {
        self.learned
            .heads
            .values()
            .next()
            .map_or(0, |head| head.weights.len())
    }

    /// One head's weights, or the catch-all when it is not carried.
    #[must_use]
    pub fn head(&self, head: &str) -> Option<&Head> {
        self.learned
            .heads
            .get(head)
            .or_else(|| self.learned.heads.get("other"))
    }

    /// One head's weights, for the trainer to update.
    pub fn head_mut(&mut self, head: &str) -> Option<&mut Head> {
        self.learned.heads.get_mut(head)
    }

    /// Score an already-hashed sparse vector against one head.
    #[must_use]
    pub fn score_vector(&self, head: &str, features: &BTreeMap<String, f64>) -> f64 {
        self.head(head)
            .map_or(0.0, |head| head.score_vector(features))
    }

    /// Score one feature vector by name: hashes each name, then takes the dot product.
    #[must_use]
    pub fn score(&self, head: &str, features: &[(String, f64)]) -> f64 {
        let dimensions = self.dimensions();
        let Some(head) = self.head(head) else {
            return 0.0;
        };
        features
            .iter()
            .map(|(name, value)| {
                let (slot, sign) = bucket(name, dimensions);
                head.weights.get(&slot).copied().unwrap_or(0.0) * sign * value
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use ti4_engine::choice::ChoiceOption;
    use ti4_model::id::PlayerId;

    #[derive(Deserialize)]
    struct GoldenBucket {
        name: String,
        dimensions: usize,
        bucket: String,
        sign: f64,
    }

    #[test]
    fn the_hash_agrees_with_the_oracle_bucket_for_bucket() {
        // Every trained checkpoint is a vector indexed by this hash. A disagreement in one bit
        // would load every existing profile and score it as noise, with nothing reporting an
        // error — so this is a golden corpus rather than a property test.
        let corpus: Vec<GoldenBucket> =
            serde_json::from_str(include_str!("../tests/golden_buckets.json"))
                .expect("the golden corpus parses");
        assert!(corpus.len() >= 40, "a corpus worth having");

        for row in corpus {
            let (slot, sign) = bucket(&row.name, row.dimensions);
            assert_eq!(
                (slot.as_str(), sign),
                (row.bucket.as_str(), row.sign),
                "bucket({:?}, {})",
                row.name,
                row.dimensions
            );
        }
    }

    #[test]
    fn a_bucket_always_lands_inside_the_profile() {
        // The hashing trick's one hard requirement: an unseen feature must still index a weight
        // that exists, or inference panics on data it has never seen — which is all data.
        for dimensions in [1usize, 7, 512, 4096] {
            let blank = blank_profile("sol", dimensions);
            for name in ["never_seen_before", "", "another", "x"] {
                let (slot, _) = bucket(name, dimensions);
                assert!(
                    blank.head("turn").unwrap().weights.contains_key(&slot),
                    "{slot} is outside a {dimensions}-bucket profile"
                );
            }
        }
    }

    #[test]
    fn an_untrained_policy_scores_every_option_the_same() {
        // It should be visibly untrained rather than inheriting a shape from somewhere.
        let blank = blank_profile("sol", DEFAULT_DIMENSIONS);
        let one = blank.score(
            "movement",
            &[("kind=move".to_owned(), 1.0), ("distance".to_owned(), 3.0)],
        );
        let other = blank.score("movement", &[("kind=land".to_owned(), 1.0)]);
        assert!((one - other).abs() < f64::EPSILON);
        assert!(one.abs() < f64::EPSILON);
    }

    #[test]
    fn a_trained_weight_moves_the_score_in_the_direction_of_its_sign() {
        let mut profile = blank_profile("sol", DEFAULT_DIMENSIONS);
        let feature = "kind=score".to_owned();
        let (slot, sign) = bucket(&feature, DEFAULT_DIMENSIONS);
        profile
            .head_mut("scoring")
            .unwrap()
            .weights
            .insert(slot, 2.0);

        let scored = profile.score("scoring", &[(feature, 1.0)]);
        assert!((scored - 2.0 * sign).abs() < 1e-12, "{scored}");
    }

    #[test]
    fn a_feature_value_scales_its_contribution() {
        let mut profile = blank_profile("sol", DEFAULT_DIMENSIONS);
        let feature = "planets".to_owned();
        let (slot, _) = bucket(&feature, DEFAULT_DIMENSIONS);
        profile
            .head_mut("activation")
            .unwrap()
            .weights
            .insert(slot, 1.0);

        let one = profile.score("activation", &[(feature.clone(), 1.0)]);
        let three = profile.score("activation", &[(feature, 3.0)]);
        assert!((three - 3.0 * one).abs() < 1e-12);
    }

    #[test]
    fn a_blank_profile_validates_and_a_broken_one_is_refused() {
        let blank = blank_profile("sol", 16);
        assert_eq!(blank.validate(Some("sol")), Ok(()));

        assert!(matches!(
            blank.validate(Some("hacan")),
            Err(ProfileError::Faction { .. })
        ));

        let mut wrong_schema = blank_profile("sol", 16);
        wrong_schema.schema = 99;
        assert!(matches!(
            wrong_schema.validate(None),
            Err(ProfileError::Schema { found: 99 })
        ));

        let mut authored = blank_profile("sol", 16);
        authored.mode = "authored".to_owned();
        assert!(matches!(
            authored.validate(None),
            Err(ProfileError::Mode { .. })
        ));

        let mut empty = blank_profile("sol", 16);
        empty.learned.heads.clear();
        assert!(matches!(empty.validate(None), Err(ProfileError::Empty)));

        let mut headless = blank_profile("sol", 16);
        headless.learned.heads.remove("movement");
        assert!(
            matches!(
                headless.validate(None),
                Err(ProfileError::MissingHead { .. })
            ),
            "a head no weights carry decides its choices at random for ever"
        );
    }

    #[test]
    fn a_profile_that_cannot_produce_a_softmax_is_refused_before_it_decides_anything() {
        // A non-finite weight or a temperature at zero makes every score NaN or the softmax
        // undefined. Caught at load, because at inference it looks like a policy playing badly.
        let mut infinite = blank_profile("sol", 16);
        infinite
            .head_mut("turn")
            .unwrap()
            .weights
            .insert("h0000".to_owned(), f64::NAN);
        assert!(matches!(
            infinite.validate(None),
            Err(ProfileError::NotFinite { .. })
        ));

        let mut frozen = blank_profile("sol", 16);
        frozen.head_mut("turn").unwrap().temperature = 0.0;
        assert!(matches!(
            frozen.validate(None),
            Err(ProfileError::Temperature { .. })
        ));
    }

    fn choice_of(prompt: &str, kinds: &[&str]) -> Choice {
        Choice::new(
            PlayerId::new("a"),
            prompt,
            kinds
                .iter()
                .enumerate()
                .map(|(index, kind)| ChoiceOption::new(format!("o{index}"), *kind))
                .collect::<Vec<ChoiceOption>>(),
        )
    }

    #[test]
    fn a_choice_routes_to_the_head_that_learns_it() {
        assert_eq!(
            decision_head(&choice_of("pick", &["activate"])),
            "activation"
        );
        assert_eq!(decision_head(&choice_of("movement", &["move"])), "movement");
        assert_eq!(decision_head(&choice_of("pick", &["score"])), "scoring");
        assert_eq!(decision_head(&choice_of("pick", &["casualty"])), "combat");
        assert_eq!(decision_head(&choice_of("pick", &["pay"])), "payment");

        // The four this got wrong when it was transcribed by eye rather than taken from the
        // oracle. Every one of them reads backwards from its name.
        assert_eq!(
            decision_head(&choice_of("pick", &["strategy"])),
            "secondary",
            "`strategy` is following somebody else's card"
        );
        assert_eq!(
            decision_head(&choice_of("pick", &["strategy_card"])),
            "strategy",
            "`strategy_card` is choosing your own"
        );
        assert_eq!(
            decision_head(&choice_of("pick", &["retreat"])),
            "combat",
            "retreating is a combat decision, not a movement one"
        );
        assert_eq!(decision_head(&choice_of("pick", &["ready"])), "production");
        assert_eq!(decision_head(&choice_of("pick", &["vote"])), "agenda");
    }

    #[test]
    fn declining_does_not_decide_which_head_a_choice_belongs_to() {
        // "Decline" is offered in nearly every window. Routing on it would put half the game in
        // one head and teach that head nothing about any of it.
        assert_eq!(
            decision_head(&choice_of("commit ground forces", &["decline", "land"])),
            "landing"
        );
        assert_eq!(
            decision_head(&choice_of("score an objective", &["decline", "score"])),
            "scoring"
        );
    }

    #[test]
    fn a_choice_of_nothing_but_declining_still_lands_somewhere() {
        // It must route, not panic: every legal choice needs a head, including the degenerate
        // ones. The prompt decides when the kinds cannot.
        assert_eq!(
            decision_head(&choice_of("pok1leadership secondary", &["decline"])),
            "secondary"
        );
        assert_eq!(
            decision_head(&choice_of("movement", &["decline"])),
            "movement"
        );
        assert_eq!(decision_head(&choice_of("whatever", &["decline"])), "other");
    }

    #[test]
    fn an_unknown_kind_falls_through_to_the_prompt_and_then_to_other() {
        assert_eq!(
            decision_head(&choice_of("produce a unit", &["no_such_kind"])),
            "production"
        );
        assert_eq!(
            decision_head(&choice_of("nothing familiar", &["no_such_kind"])),
            "other"
        );
    }

    #[test]
    fn every_head_a_kind_routes_to_is_a_head_that_exists() {
        // A typo in the routing table would send a whole kind of decision to a head no profile
        // carries weights for, and it would train and infer as silence.
        for kind in [
            "strategy",
            "strategy_card",
            "action",
            "activate",
            "move",
            "load",
            "land",
            "commit",
            "transaction",
            "pool",
            "produce",
            "pay",
            "research",
            "casualty",
            "score",
            "vote",
            "explore",
            "leader",
            "transit",
            "agenda",
        ] {
            let head = decision_head(&choice_of("x", &[kind]));
            assert!(DECISION_HEADS.contains(&head), "{kind} routed to {head}");
        }
    }

    #[derive(Deserialize)]
    struct GoldenHead {
        kind: Option<String>,
        prompt: Option<String>,
        head: String,
    }

    /// Kinds this engine raises that the oracle has no name for, and where each is sent instead.
    ///
    /// A ledger rather than a lookup. The oracle routes every one of these to its catch-all head
    /// because it has never seen them; sending them somewhere better is a deliberate divergence,
    /// and listing them here is what makes it deliberate. A kind that drifts out of agreement
    /// without being added shows up as a failure in the golden test below.
    const LOCAL_DIVERGENCES: [(&str, &str); 13] = [
        ("land", "landing"), // the oracle's `commit`
        ("ground_casualty", "combat"),
        ("sustain", "combat"),
        ("reaction", "combat"),
        ("place", "production"),
        ("spend", "payment"),
        ("ready_technology", "development"),
        ("open_transaction", "trade"),
        ("answer", "trade"),
        ("retreat_to", "combat"),
        ("remove", "scoring"),
        ("return", "scoring"),
        ("discard", "scoring"),
    ];

    #[test]
    fn routing_agrees_with_the_oracle_wherever_the_oracle_has_an_opinion() {
        // Generated by calling the oracle's `decision_head`, not by reading it. Reading it got
        // four routes wrong — `strategy` and `strategy_card` are swapped relative to what their
        // names suggest, `retreat` is combat, `ready` is production, and votes are agenda.
        let corpus: Vec<GoldenHead> =
            serde_json::from_str(include_str!("../tests/golden_heads.json"))
                .expect("the golden corpus parses");
        assert!(corpus.len() >= 40, "a corpus worth having");

        let mut diverged: Vec<&str> = Vec::new();
        for row in &corpus {
            let asked = row.kind.as_ref().map_or_else(
                || choice_of(row.prompt.as_deref().unwrap_or(""), &["decline"]),
                |kind| choice_of("pick one", &[kind.as_str(), "decline"]),
            );
            let ours = decision_head(&asked);

            let Some(kind) = row.kind.as_deref() else {
                assert_eq!(ours, row.head, "prompt {:?}", row.prompt);
                continue;
            };
            if ours == row.head {
                continue;
            }
            // A disagreement is only allowed where the oracle had no opinion to disagree with.
            assert_eq!(
                row.head, "other",
                "{kind} routes to {ours} here and {} in the oracle",
                row.head
            );
            let expected = LOCAL_DIVERGENCES
                .iter()
                .find(|(name, _)| *name == kind)
                .map(|(_, head)| *head);
            assert_eq!(
                expected,
                Some(ours),
                "{kind} diverges from the oracle without being listed as a divergence"
            );
            diverged.push(kind);
        }

        // Every listed divergence must still be one. An entry for a kind that now agrees with the
        // oracle is a stale note that would hide a real change later.
        let mut listed: Vec<&str> = LOCAL_DIVERGENCES.iter().map(|(kind, _)| *kind).collect();
        listed.sort_unstable();
        diverged.sort_unstable();
        let covered: Vec<&str> = listed
            .iter()
            .copied()
            .filter(|kind| corpus.iter().any(|row| row.kind.as_deref() == Some(*kind)))
            .collect();
        assert_eq!(
            diverged, covered,
            "the divergence ledger and the corpus disagree"
        );
    }

    #[test]
    fn a_profile_round_trips_through_json() {
        // Checkpoints are files. A profile that cannot be read back is a training run lost.
        let profile = blank_profile("jolnar", 32);
        let text = serde_json::to_string(&profile).unwrap();
        let read: Profile = serde_json::from_str(&text).unwrap();
        assert_eq!(read, profile);
        assert_eq!(read.validate(Some("jolnar")), Ok(()));
    }
}
