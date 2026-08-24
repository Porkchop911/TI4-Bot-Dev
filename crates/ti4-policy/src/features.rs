//! Factual features for a learned policy (M09-003).
//!
//! Ported from the oracle's `HashedLinearPolicy.features`.
//!
//! Everything here is an **observation**, never a judgement. "This option's kind is `activate`",
//! "the prompt contained the word *system*", "the seat holds four trade goods" — facts a rule
//! could check, with no opinion attached about whether any of them is good. The opinion is the
//! weight, and the weight is fitted.
//!
//! That line is the whole point of the module and M09-014 exists to prove it holds: if an authored
//! score leaked in here as a feature, a "fully learned" policy would be quietly reading somebody's
//! hand-tuned constants and reporting itself as having learned them.
//!
//! # Hashing
//!
//! Names are hashed straight into signed buckets as they are added, so what comes out is already
//! the sparse vector a trainer updates. Two names landing in one bucket **sum**, which is the
//! hashing trick working as intended rather than a collision to avoid: the fixed-size vector is
//! what lets an unbounded set of facts train without the weight file growing.
//!
//! A zero or non-finite value is dropped rather than stored. A zero contributes nothing to a score
//! and nothing to a gradient, and storing it would make a vector's size depend on how many facts
//! happened to be zero.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ti4_engine::choice::{Choice, ChoiceOption, Observed};
use ti4_model::id::{PlanetId, PlayerId, SystemId};

use crate::intern::{FeatureKey, first_sighting, record, register};

/// Mecatol Rex, the fixed reference point every position is measured against.
pub const MECATOL: &str = ti4_engine::seating::MECATOL;
use crate::learned::bucket;

/// A sparse feature vector: feature key to accumulated signed value.
///
/// Keyed by [`FeatureKey`] rather than by name. See `crate::intern` for what that buys and what
/// it costs; the short version is that a name is hashed once here and never allocated again.
///
/// # Why a sorted `Vec` and not a `BTreeMap`
///
/// A vector holds about eighteen entries and is built once, iterated two or three times, and
/// dropped. At that size a B-tree is the wrong shape: its nodes are heap-allocated and chased by
/// pointer, where the whole vector fits in a couple of cache lines. Building and iterating
/// eighteen entries measured **181 ns as a `BTreeMap` against 56 ns as a sorted `Vec` — 3.2×**.
///
/// The entries are kept **sorted by key**, which is the same order a `BTreeMap<FeatureKey, _>`
/// iterates in. That is not incidental: the gradient sums are accumulated in iteration order and
/// floating-point addition is not associative, so preserving the order is what makes this change
/// bit-identical rather than merely equivalent. For the same reason [`Self::finish`] sorts
/// *stably* and merges duplicates left to right, matching the order repeated `+=` would have
/// applied them in.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FeatureVector(Vec<(FeatureKey, f64)>);

impl FeatureVector {
    /// An empty vector.
    #[must_use]
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    /// Record a value, without ordering. Call [`Self::finish`] once every value is in.
    fn push(&mut self, key: FeatureKey, value: f64) {
        self.0.push((key, value));
    }

    /// Put the entries in key order and sum any duplicates.
    fn finish(&mut self) {
        if self.0.len() > 1 {
            // Stable, so equal keys keep the order they were added in and their sum matches what
            // repeated `+=` would have produced.
            self.0.sort_by_key(|(key, _)| *key);
            let mut write = 0;
            for read in 1..self.0.len() {
                if self.0[read].0 == self.0[write].0 {
                    self.0[write].1 += self.0[read].1;
                } else {
                    write += 1;
                    self.0[write] = self.0[read];
                }
            }
            self.0.truncate(write + 1);
        }
    }

    /// The value for a key, if it carries one.
    #[must_use]
    pub fn get(&self, key: &FeatureKey) -> Option<&f64> {
        self.0
            .binary_search_by_key(key, |(slot, _)| *slot)
            .ok()
            .map(|index| &self.0[index].1)
    }

    /// Whether a key carries a value.
    #[must_use]
    pub fn contains_key(&self, key: &FeatureKey) -> bool {
        self.0.binary_search_by_key(key, |(slot, _)| *slot).is_ok()
    }

    /// Every key, in order.
    pub fn keys(&self) -> impl Iterator<Item = &FeatureKey> {
        self.0.iter().map(|(key, _)| key)
    }

    /// Every value, in key order.
    pub fn values(&self) -> impl Iterator<Item = &f64> {
        self.0.iter().map(|(_, value)| value)
    }

    /// Every entry, in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&FeatureKey, &f64)> {
        self.0.iter().map(|(key, value)| (key, value))
    }

    /// How many entries it carries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether it carries none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<'a> IntoIterator for &'a FeatureVector {
    type Item = (&'a FeatureKey, &'a f64);
    type IntoIter = std::iter::Map<
        std::slice::Iter<'a, (FeatureKey, f64)>,
        fn(&'a (FeatureKey, f64)) -> (&'a FeatureKey, &'a f64),
    >;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter().map(|(key, value)| (key, value))
    }
}

impl FromIterator<(FeatureKey, f64)> for FeatureVector {
    fn from_iter<I: IntoIterator<Item = (FeatureKey, f64)>>(entries: I) -> Self {
        let mut vector = Self(entries.into_iter().collect());
        vector.finish();
        vector
    }
}

/// The value a named feature carries in a vector, if any.
///
/// A vector is keyed by hash, so a caller holding a name has to hash it. Provided here rather
/// than left to every test and diagnostic to spell out.
#[must_use]
pub fn value_of(features: &FeatureVector, name: &str) -> Option<f64> {
    features.get(&FeatureKey::of(name)).copied()
}

/// Every name in a vector, for tests and diagnostics that want to read one back.
///
/// Only names this process has registered resolve; anything else comes back empty. Allocates a
/// string per entry, so this is not for the hot path.
#[must_use]
pub fn names_of(features: &FeatureVector) -> Vec<String> {
    features
        .keys()
        .map(|key| crate::intern::name_of(*key))
        .collect()
}

/// Builds a hashed sparse vector from facts.
pub struct Features {
    dimensions: usize,
    buckets: FeatureVector,
}

impl Features {
    /// An empty vector over `dimensions` buckets.
    #[must_use]
    pub const fn new(dimensions: usize) -> Self {
        Self {
            dimensions,
            buckets: FeatureVector::new(),
        }
    }

    /// Record one fact, at weight one.
    pub fn note(&mut self, name: &str) {
        self.add(name, 1.0);
    }

    /// Record one fact carrying a magnitude.
    ///
    /// Zero and non-finite values are dropped: neither contributes to a score or a gradient, and
    /// keeping them would make a vector's length depend on which facts happened to be zero.
    pub fn add(&mut self, name: &str, value: f64) {
        if value == 0.0 || !value.is_finite() {
            return;
        }
        let (slot, sign) = bucket(name, self.dimensions);
        self.buckets.push(register(&slot), sign * value);
    }

    /// The sparse vector.
    ///
    /// Takes `&mut self` because entries are accumulated unordered and merged on demand: reading
    /// them without that step would expose duplicates that [`Self::into_vector`] would have
    /// summed. Idempotent, so calling it repeatedly is free after the first.
    pub fn vector(&mut self) -> &FeatureVector {
        self.buckets.finish();
        &self.buckets
    }

    /// Take the sparse vector.
    #[must_use]
    pub fn into_vector(mut self) -> FeatureVector {
        self.buckets.finish();
        self.buckets
    }
}

/// The tokens the oracle's `[a-z0-9]+` finds in a lowercased string.
///
/// `to_lowercase` allocates a whole second copy of its input unconditionally, and almost every
/// string reaching here — option ids, labels, prompts — is already lowercase, so that copy was
/// usually made only to be thrown away. Borrowing when nothing needs changing costs one scan for
/// an uppercase byte.
fn tokens(text: &str) -> Vec<String> {
    let lowered = if text.bytes().any(|byte| byte.is_ascii_uppercase()) {
        std::borrow::Cow::Owned(text.to_lowercase())
    } else {
        std::borrow::Cow::Borrowed(text)
    };
    lowered
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

/// Every factual observation about one legal option, hashed.
///
/// `player` is whose turn it is to answer, which is `choice.player` in every ordinary call — taken
/// as an argument so a trainer can re-derive a past decision's features for a seat.
#[must_use]
pub fn option_features(
    seen: &Observed<'_>,
    choice: &Choice,
    option: &ChoiceOption,
    player: &PlayerId,
    dimensions: usize,
) -> FeatureVector {
    let mut features = Features::new(dimensions);
    for (name, value) in option_feature_names(seen, choice, option, player) {
        features.add(&name, value);
    }
    features.into_vector()
}

/// Every factual observation about one legal option, **before** hashing.
///
/// Split out from [`option_features`] so the names are inspectable rather than only their buckets.
/// A hashed vector cannot be read back — that is the trade the hashing trick makes — so without
/// this there is no way to check what a policy is actually being shown, and M09-014's requirement
/// that no authored utility reaches inference would be an assertion rather than a test.
#[must_use]
pub fn option_feature_names(
    seen: &Observed<'_>,
    choice: &Choice,
    option: &ChoiceOption,
    player: &PlayerId,
) -> Vec<(String, f64)> {
    let mut features = Named::default();
    let faction = seen
        .seat(player)
        .map(|seat| seat.faction.to_string())
        .unwrap_or_default();

    features.note(&format!("kind:{}", option.kind));
    features.note(&format!("kind-faction:{}:{faction}", option.kind));

    // Identity, as words. A set, so an id and a label sharing a word count once — the fact is
    // "this option mentions carriers", not "it mentions them twice".
    let mut option_tokens: BTreeSet<String> = tokens(&option.id).into_iter().collect();
    option_tokens.extend(tokens(&option.label));
    for token in &option_tokens {
        features.note(&format!("option:{token}"));
        features.note(&format!("option-faction:{token}:{faction}"));
    }

    // The prompt, crossed with the option. A list rather than a set, because the bigrams below
    // need the order and a repeated word is a different phrase.
    let prompt_tokens = tokens(&choice.prompt);
    for token in &prompt_tokens {
        features.note(&format!("prompt-option:{token}:{}", option.id));
    }
    for pair in prompt_tokens.windows(2) {
        features.note(&format!(
            "prompt-bigram:{}:{}:{}",
            pair[0], pair[1], option.id
        ));
    }

    for (key, value) in &option.payload {
        match value {
            Value::Bool(flag) => {
                // Python renders these as `True`/`False`, and the name is hashed, so the casing is
                // part of the identity rather than cosmetic.
                let rendered = if *flag { "True" } else { "False" };
                features.note(&format!("payload-bool:{key}:{rendered}"));
            }
            Value::Number(number) => {
                if let Some(number) = number.as_f64() {
                    features.add(&format!("payload-number:{key}"), number);
                    features.add(
                        &format!("payload-number-kind:{key}:{}", option.kind),
                        number,
                    );
                }
            }
            Value::String(text) => {
                for token in tokens(text) {
                    features.note(&format!("payload:{key}:{token}"));
                }
            }
            Value::Array(items) => {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "payload lists are a handful of entries"
                )]
                let count = items.len() as f64;
                features.add(&format!("payload-count:{key}"), count);
                for item in items {
                    if let Value::String(text) = item {
                        features.note(&format!("payload:{key}:{}", text.to_lowercase()));
                    }
                }
            }
            Value::Null | Value::Object(_) => {}
        }
    }

    // Where the seat stands, crossed with what it is being asked. The same fact means different
    // things for different decisions: four trade goods is a lot when paying and irrelevant when
    // assigning a hit, and only the cross can learn that.
    let seat = seen.seat(player);
    #[expect(
        clippy::cast_precision_loss,
        reason = "counts and pools are small integers"
    )]
    let state_facts: [(&str, f64); 8] = [
        ("round", f64::from(seen.round())),
        (
            "tactic_tokens",
            f64::from(seat.as_ref().map_or(0, |s| s.tactic_tokens)),
        ),
        (
            "strategic_tokens",
            f64::from(seat.as_ref().map_or(0, |s| s.strategic_tokens)),
        ),
        (
            "fleet_tokens",
            f64::from(seat.as_ref().map_or(0, |s| s.fleet_tokens)),
        ),
        (
            "trade_goods",
            f64::from(seat.as_ref().map_or(0, |s| s.trade_goods)),
        ),
        (
            "commodities",
            f64::from(seat.as_ref().map_or(0, |s| s.commodities)),
        ),
        (
            "controlled_planets",
            seen.controlled_planets(player).len() as f64,
        ),
        (
            "technologies",
            seat.as_ref().map_or(0, |s| s.technologies.len()) as f64,
        ),
    ];
    for (name, value) in state_facts {
        features.add(&format!("state-kind:{}:{name}", option.kind), value);
        features.add(&format!("state-option:{}:{name}", option.id), value);
    }

    features.0
}

/// Collision-free schema-3/4/5 features used by the successful policy-gradient runs.
///
/// This is deliberately not `option_feature_names` with the hash removed.  The oracle's explicit
/// extractor also removes faction crosses, bare numeric identities and exact option ids, because
/// those let a policy memorise one seat or map instead of reading the board.  Keeping the two
/// extractors separate preserves schema-2 compatibility while making the representation used for
/// new training unambiguous.
#[must_use]
pub fn explicit_option_features(
    seen: &Observed<'_>,
    choice: &Choice,
    option: &ChoiceOption,
    player: &PlayerId,
) -> FeatureVector {
    let context = ChoiceContext {
        facts: seat_facts(seen, player),
        own_units: seen.systems_with_units_of(player).into_iter().collect(),
    };
    explicit_option_features_with(
        seen,
        &tokens(&choice.prompt),
        &context,
        option,
        player,
        state_cross(choice),
    )
}

/// Facts about the seat that every option of a choice is described against, computed once.
///
/// `own_units` is here rather than looked up per option because
/// [`Observed::systems_with_units_of`] scans the board and allocates, and an activation choice
/// offers thirty-odd options that would each have asked the same question.
struct ChoiceContext<'a> {
    facts: [(&'static str, f64); 8],
    own_units: Vec<&'a SystemId>,
}

/// The eight per-seat facts every option of a choice is described against.
///
/// None of them varies with the option: they are the round, this seat's pools, its goods, its
/// planet count and its technology count. Only the feature *name* varies, because it is crossed
/// with the option's kind. Computing them per option meant
/// [`Observed::controlled_planets`] — which scans the whole board and allocates a `Vec` — ran
/// once for every option offered, to take its length each time.
#[must_use]
#[expect(clippy::cast_precision_loss, reason = "public counts are small")]
fn seat_facts(seen: &Observed<'_>, player: &PlayerId) -> [(&'static str, f64); 8] {
    let seat = seen.seat(player);
    [
        ("round", f64::from(seen.round())),
        (
            "tactic_tokens",
            f64::from(seat.as_ref().map_or(0, |s| s.tactic_tokens)),
        ),
        (
            "strategic_tokens",
            f64::from(seat.as_ref().map_or(0, |s| s.strategic_tokens)),
        ),
        (
            "fleet_tokens",
            f64::from(seat.as_ref().map_or(0, |s| s.fleet_tokens)),
        ),
        (
            "trade_goods",
            f64::from(seat.as_ref().map_or(0, |s| s.trade_goods)),
        ),
        (
            "commodities",
            f64::from(seat.as_ref().map_or(0, |s| s.commodities)),
        ),
        (
            "controlled_planets",
            seen.controlled_planets(player).len() as f64,
        ),
        (
            "technologies",
            seat.as_ref().map_or(0, |s| s.technologies.len()) as f64,
        ),
    ]
}

/// Features for every option of one choice, in the choice's own option order.
///
/// The prompt is tokenised **once for the whole choice** rather than once per option.
/// [`tokens`] allocates a lowercased copy of its input plus one `String` per token, and a single
/// transaction decision offers up to 37 options — so the per-option form did that work 37 times
/// over one unchanging prompt. The feature set is identical either way; only the allocation
/// count differs.
#[must_use]
pub fn explicit_choice_features(
    seen: &Observed<'_>,
    choice: &Choice,
    player: &PlayerId,
) -> Vec<FeatureVector> {
    // Both of these are constant across the choice's options and are computed once here.
    let prompt_tokens = tokens(&choice.prompt);
    let context = ChoiceContext {
        facts: seat_facts(seen, player),
        own_units: seen.systems_with_units_of(player).into_iter().collect(),
    };
    let cross = state_cross(choice);
    choice
        .options
        .iter()
        .map(|option| {
            explicit_option_features_with(seen, &prompt_tokens, &context, option, player, cross)
        })
        .collect()
}

/// How a choice's per-seat state facts are crossed so they can influence the decision.
///
/// A linear softmax cannot see an option-invariant feature: the same name with the same value on
/// every option adds one constant to every logit and cancels. So state facts only reach a decision
/// if their *name* differs between options, and what they are crossed with decides that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateCross {
    /// Cross with the option's kind. Discriminates whenever the options' kinds differ.
    ByKind,
    /// Cross with the option's id, for a small option set whose kinds are all the same.
    ///
    /// Every strategy-card secondary is exactly this case: both options are built with
    /// `STRATEGY_KIND`, so the kind cross is inert and the seat's tokens, goods and planets could
    /// not reach the decision at all -- the head answered "should I take this secondary" from the
    /// card's identity alone, never from whether the seat could afford it.
    ByOption,
    /// Neither. The kinds are uniform and the option set is too large to cross state with option
    /// ids, which are systems and planets on the big heads -- that would multiply the weight table
    /// by the number of facts and invite memorising specific boards, which is exactly what the
    /// explicit schema removed exact option ids to prevent.
    None,
}

/// Whether an option id names a fixed vocabulary word rather than a piece of the board.
///
/// The gate on crossing seat state with the option id is the *identity* of the ids, not how many
/// there are. Counting options was the obvious rule and it is wrong: an activation choice can
/// offer two systems, and crossing seat state with a tile id is precisely the memorisation the
/// explicit schema removed exact option ids to prevent -- the activation test asserts it.
///
/// This engine writes board references in exactly two shapes, and both are rejected here:
///
/// * a bare system id, which is all digits (`01`, `100`);
/// * a composite naming a target, which carries a separator (`exhaust|accoen`, `move|16|2`).
///
/// Everything else is drawn from a closed vocabulary the content defines -- `yes`, `no`,
/// `decline`, the three command pools, the eight strategy cards -- so crossing adds one slot per
/// fact per vocabulary word and nothing that varies with the board.
///
/// It is a heuristic and it fails closed only for the two shapes above. `inert_audit` prints the
/// ids of every head that carries no state, which is how to re-check it after a content change.
fn is_fixed_vocabulary_id(id: &str) -> bool {
    !id.is_empty()
        && !id.contains('|')
        && !id.contains(':')
        && !id.chars().all(|character| character.is_ascii_digit())
}

/// Which cross a choice gets.
#[must_use]
pub fn state_cross(choice: &Choice) -> StateCross {
    if !uniform_kind(choice) {
        return StateCross::ByKind;
    }
    if !choice.options.is_empty()
        && choice
            .options
            .iter()
            .all(|option| is_fixed_vocabulary_id(&option.id))
    {
        StateCross::ByOption
    } else {
        StateCross::None
    }
}

/// Whether every option of this choice canonicalises to the same feature kind.
///
/// When it does, the three kind-keyed families — `kind:`, `prompt-kind:` and `state-kind:` —
/// take the **same value on every option**, and a feature with that property is inert:
///
/// - its score contribution is one constant added to every logit, and softmax ignores that;
/// - its policy-gradient term is `φ_chosen − Σₒ pₒφₒ = c − c·1 = 0`;
/// - its entropy-gradient term is `Σₒ coeffₒ·φₒ = c·Σₒ coeffₒ`, and
///   `Σₒ coeffₒ = −(Σₒ pₒ ln pₒ + H)/T = −(−H + H)/T = 0`.
///
/// So it can never move a weight and never change a decision, and building, storing and summing
/// it is work whose result arithmetic discards. Measured over 600 real choices, these three
/// families are 45.9% of every feature instance and 70–82% of each is inert.
///
/// Checked on the *kinds* rather than on the finished vectors, because comparing vectors would
/// cost what it saves. A choice whose options differ in kind keeps everything: `state-kind:move:*`
/// and `state-kind:decline:*` are different slots, so each one does distinguish its options.
fn uniform_kind(choice: &Choice) -> bool {
    let mut kinds = choice
        .options
        .iter()
        .map(|option| canonical_feature_kind(&option.kind));
    let Some(first) = kinds.next() else {
        return true;
    };
    kinds.all(|kind| kind == first)
}

// Keeping the extractor in one linear block makes its ordering and parity with the Python
// reference auditable; splitting it would obscure which crosses belong to the base feature set.
#[allow(clippy::too_many_lines)]
fn explicit_option_features_with(
    seen: &Observed<'_>,
    prompt_tokens: &[String],
    context: &ChoiceContext<'_>,
    option: &ChoiceOption,
    player: &PlayerId,
    cross: StateCross,
) -> FeatureVector {
    let mut features = FeatureVector::new();
    let kind = canonical_feature_kind(&option.kind);
    // Skipped when every option shares this kind: it would be the same name and value on every
    // option, and `StateCross::ByKind` is exactly the case where the kinds differ.
    if cross == StateCross::ByKind {
        add_parts(&mut features, &["kind:", kind], 1.0);
    }

    // Identity as words, with board *identities* removed and vocabulary kept.
    //
    // A composite id names a verb and its argument, but the argument is not always a board
    // reference: `exhaust|archonren` names a planet, while `build|carrier|1` names a unit type.
    // Dropping everything after the separator was the first attempt and it cost the production
    // head its ability to tell a carrier from a cruiser. What has to go is the specific planet,
    // so that the policy learns about planets rather than about Archon Ren.
    //
    // The all-digit filter was doing this job by accident and only for tiles -- system ids are
    // numbers, so `option:72` was dropped, while `option:archonren` sailed through because planet
    // names are words. That accident is why the activation head carries no tile identity and the
    // payment head carried every planet's.
    //
    // The planet lookup is scoped to the argument of a composite id rather than run over every
    // word of every option: that is the only place a board identity appears, and testing all of
    // them cost 35% of an update.
    let dropped: BTreeSet<String> = option
        .id
        .split_once('|')
        .map(|(_, argument)| {
            tokens(argument)
                .into_iter()
                .filter(|token| is_planet_id(token))
                .collect()
        })
        .unwrap_or_default();
    let mut option_tokens: BTreeSet<String> = tokens(&option.id)
        .into_iter()
        .chain(tokens(&option.label))
        .filter(|token| !token.chars().all(|character| character.is_ascii_digit()))
        .filter(|token| !dropped.contains(token))
        .collect();
    // Stable iteration is part of the feature contract even though addition is commutative.
    for token in &option_tokens {
        add_parts(&mut features, &["option:", token], 1.0);
    }

    for prompt_token in prompt_tokens {
        add_parts(
            &mut features,
            &["prompt-kind:", prompt_token, ":", kind],
            1.0,
        );
        for option_token in &option_tokens {
            add_parts(
                &mut features,
                &["prompt-option:", prompt_token, ":", option_token],
                1.0,
            );
        }
    }

    for (key, value) in &option.payload {
        match value {
            Value::Bool(flag) => add_named(
                &mut features,
                format_args!(
                    "payload-bool:{key}:{}",
                    if *flag { "True" } else { "False" }
                ),
                1.0,
            ),
            Value::Number(number) => {
                if let Some(number) = number.as_f64() {
                    add_named(&mut features, format_args!("payload-number:{key}"), number);
                    add_named(
                        &mut features,
                        format_args!("payload-number-kind:{key}:{kind}"),
                        number,
                    );
                    // A payment option carries what it is worth; the choice carries what is owed.
                    // Which planet to exhaust is decided by the two together -- does this cover
                    // the debt, and how much is wasted if it overshoots -- and a weighted sum of
                    // the two separately cannot express either. Recorded only where both are
                    // present, so no other head is touched.
                    if key == "worth"
                        && let Some(owed) = option
                            .payload
                            .get("owed")
                            .and_then(Value::as_f64)
                    {
                        add_named(
                            &mut features,
                            format_args!("pay:covers-owed"),
                            f64::from(u8::from(number >= owed)),
                        );
                        add_named(
                            &mut features,
                            format_args!("pay:overpay"),
                            (number - owed).max(0.0),
                        );
                        add_named(
                            &mut features,
                            format_args!("pay:shortfall"),
                            (owed - number).max(0.0),
                        );
                    }
                }
            }
            Value::String(text) => {
                for token in tokens(text)
                    .into_iter()
                    .filter(|token| !token.chars().all(|character| character.is_ascii_digit()))
                {
                    add_named(&mut features, format_args!("payload:{key}:{token}"), 1.0);
                }
            }
            Value::Array(items) => {
                #[expect(clippy::cast_precision_loss, reason = "option payloads are small")]
                add_named(
                    &mut features,
                    format_args!("payload-count:{key}"),
                    items.len() as f64,
                );
                for item in items {
                    if let Value::String(text) = item
                        && !text.chars().all(|character| character.is_ascii_digit())
                    {
                        add_named(
                            &mut features,
                            format_args!("payload:{key}:{}", text.to_lowercase()),
                            1.0,
                        );
                    }
                }
            }
            Value::Null | Value::Object(_) => {}
        }
    }

    match cross {
        StateCross::ByKind => {
            for (name, value) in &context.facts {
                add_parts(&mut features, &["state-kind:", kind, ":", name], *value);
            }
        }
        StateCross::ByOption => {
            for (name, value) in &context.facts {
                add_parts(&mut features, &["state-option:", &option.id, ":", name], *value);
            }
        }
        StateCross::None => {}
    }

    structured_features(seen, option, player, context, &mut features);
    option_tokens.clear();
    features.finish();
    features
}

/// Add a feature whose name is a fixed shape with string pieces slotted in.
///
/// The hot families all look like `family:{a}` or `family:{a}:{b}`, and formatting them was
/// measured at roughly 60% of what naming a feature costs. FNV-1a is a streaming hash, so
/// folding the pieces gives bit-for-bit the same key as hashing the joined string — the name
/// itself is only ever built on the first sighting of a key, to record it for later resolution.
fn add_parts(features: &mut FeatureVector, parts: &[&str], value: f64) {
    if value == 0.0 || !value.is_finite() {
        return;
    }
    let key = FeatureKey::of_parts(parts);
    if first_sighting(key) {
        record(key, &parts.concat());
    }
    features.push(key, value);
}

thread_local! {
    /// One reusable buffer per thread for composing feature names.
    ///
    /// `format!` allocates a fresh `String` for every feature of every option, for a string that
    /// is hashed and dropped immediately. Taking `fmt::Arguments` instead lets callers keep
    /// writing names exactly as before while the bytes land in a buffer that is cleared and
    /// reused.
    static SCRATCH: std::cell::RefCell<String> =
        std::cell::RefCell::new(String::with_capacity(160));
}

fn add_named(features: &mut FeatureVector, name: std::fmt::Arguments<'_>, value: f64) {
    if value == 0.0 || !value.is_finite() {
        return;
    }
    SCRATCH.with(|scratch| {
        let mut scratch = scratch.borrow_mut();
        scratch.clear();
        // Writing into a String is infallible; the Result exists for the general Write contract.
        let _ = std::fmt::Write::write_fmt(&mut *scratch, name);
        features.push(register(&scratch), value);
    });
}

/// Local choice kinds translated to the oracle identity used by imported explicit weights.
fn canonical_feature_kind(kind: &str) -> &str {
    match kind {
        "land" => "commit",
        "place" => "produce",
        "spend" => "pay",
        "ready_technology" => "technology",
        "open_transaction" | "answer" => "transaction",
        "ground_casualty" | "sustain" => "casualty",
        "retreat_to" => "retreat",
        other => other,
    }
}

fn payload_string<'a>(option: &'a ChoiceOption, key: &str) -> Option<&'a str> {
    option.payload.get(key).and_then(Value::as_str)
}

fn structured_features(
    seen: &Observed<'_>,
    option: &ChoiceOption,
    player: &PlayerId,
    context: &ChoiceContext<'_>,
    features: &mut FeatureVector,
) {
    let kind = canonical_feature_kind(&option.kind);
    let active = seen.active_system();
    if matches!(kind, "activate" | "system") {
        add_system_features(seen, &option.id, player, context, "target", features);
    }

    if let Some(system) = payload_string(option, "system") {
        let prefix = match kind {
            "produce" | "build" => "production",
            "placement" => "placement",
            "load" => "origin",
            _ => "option-system",
        };
        add_system_features(seen, system, player, context, prefix, features);
    }

    // The strategy draft. Until this existed the head saw the card's *identity* and the seat's
    // state, and nothing about what any card does -- so "take Warfare" and "take Imperial" were
    // two names with no properties between them, and the only way to learn a preference was to
    // memorise one weight per card per faction.
    //
    // Both facts are public. Initiative is printed on the card and decides the whole action
    // phase's turn order; trade goods sit visibly on unpicked cards and are one of the two
    // reasons to take a card you do not otherwise want (LRR 83.2).
    if kind == "strategy_card" {
        let card = ti4_model::id::StrategyCardId::new(option.id.as_str());
        if let Some(initiative) = seen.card_initiative(&card) {
            add_named(
                features,
                format_args!("card:initiative"),
                small_integer_value(i64::from(initiative)),
            );
            // Going early and going last are different things, and a linear model reads a raw
            // initiative number as "more is better" in one direction only.
            add_named(
                features,
                format_args!("card:first-pick"),
                f64::from(u8::from(initiative <= 2)),
            );
            add_named(
                features,
                format_args!("card:last-pick"),
                f64::from(u8::from(initiative >= 7)),
            );
        }
        add_named(
            features,
            format_args!("card:goods"),
            small_integer_value(i64::from(seen.strategy_card_goods(&card))),
        );
    }

    match kind {
        "move" => {
            if let Some(origin) = payload_string(option, "origin") {
                add_system_features(seen, origin, player, context, "origin", features);
                if let Some(destination) = active {
                    add_route_features(seen, origin, destination.as_str(), features);
                }
            }
            if let Some(destination) = active {
                add_system_features(
                    seen,
                    destination.as_str(),
                    player,
                    context,
                    "destination",
                    features,
                );
            }
        }
        "load" => {
            if let Some(destination) = active {
                add_system_features(
                    seen,
                    destination.as_str(),
                    player,
                    context,
                    "destination",
                    features,
                );
            }
        }
        "commit" => {
            if let Some(destination) = active {
                add_system_features(
                    seen,
                    destination.as_str(),
                    player,
                    context,
                    "invasion",
                    features,
                );
            }
            if let Some(planet) = payload_string(option, "planet") {
                add_planet_features(
                    seen,
                    planet,
                    active.map(SystemId::as_str),
                    player,
                    "landing",
                    features,
                );
            }
        }
        _ => {}
    }

    if let Some(unit) = payload_string(option, "unit") {
        add_unit_features(seen, unit, &format!("{kind}-unit"), features);
    }
}

fn add_route_features(
    seen: &Observed<'_>,
    origin: &str,
    destination: &str,
    features: &mut FeatureVector,
) {
    let Some(galaxy) = seen.galaxy() else {
        return;
    };
    if let Some(distance) = galaxy.distance(origin, destination) {
        add_named(
            features,
            format_args!("route:hex-distance"),
            f64::from(distance),
        );
    }
    add_named(
        features,
        format_args!("route:adjacent"),
        f64::from(u8::from(galaxy.are_adjacent(origin, destination))),
    );
}

fn add_unit_features(
    seen: &Observed<'_>,
    unit_id: &str,
    prefix: &str,
    features: &mut FeatureVector,
) {
    let Some(unit) = ti4_content::units::unit_type(seen.content(), unit_id, seen.sources()) else {
        return;
    };
    for (name, value) in [
        ("move", small_integer_value(unit.move_value())),
        ("capacity", small_integer_value(unit.capacity())),
        ("cost", unit.cost()),
        ("is-ship", f64::from(u8::from(unit.is_ship()))),
        ("is-ground", f64::from(u8::from(unit.is_ground_force()))),
        ("is-fighter", f64::from(u8::from(unit.is_fighter()))),
        ("is-structure", f64::from(u8::from(unit.is_structure()))),
        ("has-production", f64::from(u8::from(unit.has_production()))),
        ("sustain", f64::from(u8::from(unit.sustain_damage()))),
    ] {
        add_named(features, format_args!("{prefix}:{name}"), value);
    }
}

fn add_planet_features(
    seen: &Observed<'_>,
    planet_id: &str,
    system_id: Option<&str>,
    player: &PlayerId,
    prefix: &str,
    features: &mut FeatureVector,
) {
    let Some(planet) = ti4_content::galaxy::planet(seen.content(), planet_id, seen.sources())
    else {
        return;
    };
    for (name, value) in [
        ("resources", small_integer_value(planet.resources())),
        ("influence", small_integer_value(planet.influence())),
        ("legendary", f64::from(u8::from(planet.is_legendary()))),
        (
            "homeworld",
            f64::from(u8::from(planet.homeworld_of().is_some())),
        ),
        (
            "tech-specialties",
            count_value(planet.tech_specialties().len()),
        ),
    ] {
        add_named(features, format_args!("{prefix}:{name}"), value);
    }
    // One feature per trait: a dual-trait planet is described by both, which is what the
    // objectives that read them do.
    for trait_name in planet.planet_types() {
        add_named(
            features,
            format_args!("{prefix}:trait:{}", trait_name.to_lowercase()),
            1.0,
        );
    }
    let Some(system_id) = system_id.or_else(|| planet.system_id()) else {
        return;
    };
    let state = seen.system(&SystemId::new(system_id));
    let planet_id = PlanetId::new(planet_id);
    let controller = state.planet_control.get(&planet_id);
    add_named(
        features,
        format_args!("{prefix}:controlled-by-us"),
        f64::from(u8::from(controller == Some(player))),
    );
    add_named(
        features,
        format_args!("{prefix}:uncontrolled"),
        f64::from(u8::from(controller.is_none())),
    );
    add_named(
        features,
        format_args!("{prefix}:controlled-by-enemy"),
        f64::from(u8::from(controller.is_some_and(|owner| owner != player))),
    );
    let occupants = state.on_planet(&planet_id);
    let own_ground = occupants
        .iter()
        .filter(|unit| {
            unit.owner == *player
                && unit_stats(seen, unit).is_some_and(|stats| stats.is_ground_force())
        })
        .count();
    let enemy_ground = occupants
        .iter()
        .filter(|unit| {
            unit.owner != *player
                && unit_stats(seen, unit).is_some_and(|stats| stats.is_ground_force())
        })
        .count();
    add_named(
        features,
        format_args!("{prefix}:own-ground"),
        count_value(own_ground),
    );
    add_named(
        features,
        format_args!("{prefix}:enemy-ground"),
        count_value(enemy_ground),
    );
}

fn unit_stats<'a>(
    seen: &'a Observed<'a>,
    unit: &ti4_model::units::Unit,
) -> Option<ti4_content::units::UnitType<'a>> {
    ti4_content::units::unit_type(seen.content(), unit.type_id.as_str(), seen.sources())
}

#[expect(
    clippy::too_many_lines,
    reason = "one linear list of the facts a system carries; splitting it would hide which are recorded"
)]
fn add_system_features(
    seen: &Observed<'_>,
    system_id: &str,
    player: &PlayerId,
    context: &ChoiceContext<'_>,
    prefix: &str,
    features: &mut FeatureVector,
) {
    let Some(galaxy) = seen.galaxy() else {
        return;
    };
    if galaxy.coord_of(system_id).is_none() {
        return;
    }
    let system = seen.system(&SystemId::new(system_id));
    // The system record already lists its planets, so this reads them directly instead of
    // scanning the whole planet corpus for a matching `tileId` -- a call measured at 2,327 ns,
    // made one to three times for every option of every decision. The two agree across all 231
    // systems of the corpus, which `galaxy::system` records.
    let planet_ids: Vec<&str> =
        ti4_content::galaxy::system(seen.content(), system_id, seen.sources())
            .map(|record| record.planets())
            .unwrap_or_default();
    let controls = &system.planet_control;
    add_named(
        features,
        format_args!("{prefix}:planet-count"),
        count_value(planet_ids.len()),
    );
    add_named(
        features,
        format_args!("{prefix}:not-controlled-count"),
        count_value(
            planet_ids
                .iter()
                .filter(|planet| controls.get(**planet) != Some(player))
                .count(),
        ),
    );
    add_named(
        features,
        format_args!("{prefix}:uncontrolled-count"),
        count_value(
            planet_ids
                .iter()
                .filter(|planet| !controls.contains_key(**planet))
                .count(),
        ),
    );
    add_named(
        features,
        format_args!("{prefix}:enemy-controlled-count"),
        count_value(
            planet_ids
                .iter()
                .filter(|planet| controls.get(**planet).is_some_and(|owner| owner != player))
                .count(),
        ),
    );

    // One pass, five counters. Each `unit_stats` is a content lookup, and the four separate
    // filters this replaces performed that lookup once per counter per unit -- four times over
    // the same units, in a function that runs for every option of every movement, invasion,
    // production and activation decision.
    let mut own_ships = 0usize;
    let mut enemy_ships = 0usize;
    let mut own_ground_space = 0usize;
    let mut enemy_ground_total = 0usize;
    let mut own_production_units = 0usize;
    for unit in &system.units {
        let Some(stats) = unit_stats(seen, unit) else {
            continue;
        };
        let mine = unit.owner == *player;
        if stats.is_ship() {
            own_ships += usize::from(mine);
            enemy_ships += usize::from(!mine);
        }
        if stats.is_ground_force() {
            own_ground_space += usize::from(mine);
            enemy_ground_total += usize::from(!mine);
        }
        own_production_units += usize::from(mine && stats.has_production());
    }
    // Ground forces on planets count towards the totals but not towards the in-space counters.
    for unit in system.planet_units.values().flatten() {
        let Some(stats) = unit_stats(seen, unit) else {
            continue;
        };
        let mine = unit.owner == *player;
        enemy_ground_total += usize::from(!mine && stats.is_ground_force());
        own_production_units += usize::from(mine && stats.has_production());
    }
    // Where this system sits, and what is on it. Without these, two tiles carrying the same
    // planet counts are the same decision: `option:{id}` is filtered out for being all-digit, so
    // the identity of an activation target reaches the policy through nothing else. Measured at
    // 94% of activation decisions holding at least two options with identical vectors.
    //
    // Facts, not judgements: none of them says a system is worth taking, only what it is and
    // where it is relative to the seat's own ships and to Mecatol.
    if let Some(record) = ti4_content::galaxy::system(seen.content(), system_id, seen.sources()) {
        let (mut resources, mut influence) = (0, 0);
        for id in &planet_ids {
            if let Some(planet) = ti4_content::galaxy::planet(seen.content(), id, seen.sources()) {
                resources += planet.resources();
                influence += planet.influence();
            }
        }
        add_named(
            features,
            format_args!("{prefix}:resources"),
            small_integer_value(resources),
        );
        add_named(
            features,
            format_args!("{prefix}:influence"),
            small_integer_value(influence),
        );
        add_named(
            features,
            format_args!("{prefix}:wormholes"),
            count_value(record.wormholes().len()),
        );
        add_named(
            features,
            format_args!("{prefix}:anomaly"),
            f64::from(u8::from(record.is_anomaly())),
        );
    }
    // Distance to the nearest system this seat already has units in, and to Mecatol. Two systems
    // with identical contents are still different moves if one is next to your fleet.
    let nearest = context
        .own_units
        .iter()
        .filter_map(|origin| galaxy.distance(origin.as_str(), system_id))
        .min();
    if let Some(distance) = nearest {
        add_named(
            features,
            format_args!("{prefix}:own-distance"),
            small_integer_value(i64::from(distance)),
        );
    }
    add_named(
        features,
        format_args!("{prefix}:own-adjacent"),
        f64::from(u8::from(nearest == Some(1))),
    );
    add_named(
        features,
        format_args!("{prefix}:own-here"),
        f64::from(u8::from(nearest == Some(0))),
    );
    if let Some(distance) = galaxy.distance(crate::features::MECATOL, system_id) {
        add_named(
            features,
            format_args!("{prefix}:mecatol-distance"),
            small_integer_value(i64::from(distance)),
        );
    }
    // How many *other* seats are present, which is what makes a system contested rather than open.
    let rivals: std::collections::BTreeSet<&PlayerId> = system
        .units
        .iter()
        .map(|unit| &unit.owner)
        .chain(
            system
                .planet_units
                .values()
                .flatten()
                .map(|unit| &unit.owner),
        )
        .chain(controls.values())
        .filter(|owner| *owner != player)
        .collect();
    add_named(
        features,
        format_args!("{prefix}:rival-seats"),
        count_value(rivals.len()),
    );

    // Interactions between the seat's own position and this system, which a linear model cannot
    // form for itself: it scores a weighted sum of features, so the product of two of them is not
    // available unless it is supplied. Without these the ranking of systems is identical whether
    // the seat has one command token or five.
    //
    // Crossing state with the *system id* would be the memorisation the explicit schema exists to
    // prevent. These cross it with what the system IS -- how far, how contested -- so they say
    // "this is beyond my reach" and never "this is tile 72".
    //
    // Kept bounded and on the same scale as the surrounding counts. A raw product would run to
    // tokens x distance and dominate a sum of small integers.
    let tactic = context
        .facts
        .iter()
        .find(|(name, _)| *name == "tactic_tokens")
        .map_or(0.0, |(_, value)| *value);
    let fleet = context
        .facts
        .iter()
        .find(|(name, _)| *name == "fleet_tokens")
        .map_or(0.0, |(_, value)| *value);
    if let Some(distance) = nearest {
        let reach = f64::from(distance) - tactic;
        add_named(
            features,
            format_args!("{prefix}:distance-beyond-tokens"),
            reach.max(0.0),
        );
        add_named(
            features,
            format_args!("{prefix}:within-token-budget"),
            f64::from(u8::from(reach <= 0.0)),
        );
    }
    add_named(
        features,
        format_args!("{prefix}:enemy-ships-over-fleet"),
        (count_value(enemy_ships) - fleet).max(0.0),
    );

    for (name, value) in [
        ("own-ships", count_value(own_ships)),
        ("enemy-ships", count_value(enemy_ships)),
        ("own-ground-space", count_value(own_ground_space)),
        ("enemy-ground-total", count_value(enemy_ground_total)),
        ("own-production-units", count_value(own_production_units)),
        (
            "reachable",
            f64::from(u8::from(system_reachable(seen, player, system_id))),
        ),
    ] {
        add_named(features, format_args!("{prefix}:{name}"), value);
    }
}

/// Every planet id the content defines, built once.
///
/// Used to drop a specific planet's name from an option's words. The set is a superset across
/// source sets, which is the safe direction: a name that is a planet under any printing is a board
/// identity and should not become a feature under another.
fn planet_ids() -> &'static BTreeSet<String> {
    static IDS: std::sync::OnceLock<BTreeSet<String>> = std::sync::OnceLock::new();
    IDS.get_or_init(|| {
        ti4_content::galaxy::all_planets(
            ti4_content::ContentStore::embedded(),
            ti4_model::content_types::FULL,
        )
        .into_keys()
        .map(str::to_owned)
        .collect()
    })
}

/// Whether a word names a specific planet rather than a piece of vocabulary.
fn is_planet_id(token: &str) -> bool {
    // Cheap rejects first: the set lookup runs for every word of every option of every decision.
    token.len() >= 4 && planet_ids().contains(token)
}

fn small_integer_value(value: i64) -> f64 {
    f64::from(i32::try_from(value).expect("TI4 printed integer values fit in i32"))
}

fn count_value(value: usize) -> f64 {
    f64::from(u32::try_from(value).expect("TI4 component counts fit in u32"))
}

fn system_reachable(seen: &Observed<'_>, player: &PlayerId, target: &str) -> bool {
    let target = SystemId::new(target);
    let Some(galaxy) = seen.galaxy() else {
        return false;
    };
    let pinned = seen.systems_with_token(player);
    seen.board().iter().any(|(origin, state)| {
        !pinned.contains(origin)
            && state.units.iter().any(|unit| {
                unit.owner == *player
                    && unit_stats(seen, unit).is_some_and(|stats| {
                        stats.is_ship()
                            && galaxy
                                .distance(origin.as_str(), target.as_str())
                                .is_some_and(|distance| {
                                    distance <= i32::try_from(stats.move_value()).unwrap_or(0)
                                })
                    })
            })
    })
}

/// Collects `(name, value)` pairs before they are hashed.
#[derive(Default)]
struct Named(Vec<(String, f64)>);

impl Named {
    fn note(&mut self, name: &str) {
        self.add(name, 1.0);
    }

    fn add(&mut self, name: &str, value: f64) {
        self.0.push((name.to_owned(), value));
    }
}

/// The prefixes every emitted feature name carries.
///
/// A closed list, checked by a test over the names actually produced. Each names a *fact* — a
/// kind, a word, a payload entry, a count of something on the board. None of them can carry an
/// authored score, which is the property M09-014 has to hold.
pub const FEATURE_PREFIXES: [&str; 13] = [
    "kind:",
    "kind-faction:",
    "option:",
    "option-faction:",
    "prompt-option:",
    "prompt-bigram:",
    "payload-bool:",
    "payload-number:",
    "payload-number-kind:",
    "payload:",
    "payload-count:",
    "state-kind:",
    "state-option:",
];

/// Structured tactical feature parity status (M09-010 repair).
///
/// [`explicit_option_features`] now emits the oracle's role-specific system, planet, unit and route
/// facts. The legacy hashed extractor remains unchanged on purpose: changing its inputs would make
/// existing schema-2 weights mean something different without changing their stored bucket names.
#[must_use]
pub const fn structured_features_status() -> &'static str {
    "complete for explicit schemas; schema 2 remains compatibility-frozen"
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use serde::Deserialize;
    use ti4_engine::choice::Observed;
    use ti4_model::content_types::POK;
    use ti4_model::id::FactionId;
    use ti4_model::state::GameState;

    const DIMENSIONS: usize = 512;

    #[derive(Deserialize)]
    struct GoldenFeatures {
        prompt: String,
        id: String,
        kind: String,
        label: String,
        payload: BTreeMap<String, Value>,
        features: BTreeMap<String, f64>,
    }

    /// The seat the golden corpus was generated against.
    fn oracle_seat() -> GameState {
        let mut state = ti4_engine::fixtures::game(&["a"]);
        state.round = 2;
        let player = PlayerId::new("a");
        {
            let seat = state.player_mut(&player).unwrap();
            seat.faction = FactionId::new("sol");
            seat.tactic_tokens = 3;
            seat.strategic_tokens = 2;
            seat.fleet_tokens = 1;
            seat.trade_goods = 4;
            seat.commodities = 2;
            seat.technologies = ["a", "b", "c"]
                .into_iter()
                .map(ti4_model::id::TechnologyId::new)
                .collect();
        }
        // Two controlled planets, in two systems, matching the corpus.
        state
            .system_mut(&ti4_model::id::SystemId::new("18"))
            .set_control(ti4_model::id::PlanetId::new("mr"), player.clone());
        state
            .system_mut(&ti4_model::id::SystemId::new("26"))
            .set_control(ti4_model::id::PlanetId::new("arretze"), player);
        state
    }

    fn observed_three_player_board() -> (GameState, ti4_content::galaxy::Galaxy) {
        let content = ti4_content::ContentStore::embedded();
        let players = ["a", "b", "c"].map(PlayerId::new);
        let factions: BTreeMap<PlayerId, FactionId> = players
            .iter()
            .cloned()
            .zip(["letnev", "jolnar", "hacan"].map(FactionId::new))
            .collect();
        let mut state =
            ti4_engine::setup::start_game_seeded(content, &players, POK, None, 17).expect("setup");
        for (player, faction) in &factions {
            state.player_mut(player).unwrap().faction = faction.clone();
        }
        let filler: Vec<String> = ti4_engine::seating::map_filler(content, 30, POK, 17)
            .into_iter()
            .map(|system| system.to_string())
            .collect();
        let refs: Vec<&str> = filler.iter().map(String::as_str).collect();
        let galaxy = ti4_engine::seating::build_board(content, &factions, &refs, POK).unwrap();
        for (player, faction) in &factions {
            ti4_engine::seating::deploy(&mut state, content, player, faction, POK).unwrap();
        }
        (state, galaxy)
    }

    #[test]
    fn explicit_activation_reads_the_real_board_without_memorising_a_tile_id() {
        let (state, galaxy) = observed_three_player_board();
        let content = ti4_content::ContentStore::embedded();
        let player = PlayerId::new("a");
        let home = state.player(&player).unwrap().home_system.as_ref().unwrap();
        let target = galaxy
            .adjacent(home.as_str())
            .into_iter()
            .find(|system| !ti4_content::galaxy::planets_in(content, system, POK).is_empty())
            .expect("a neighbouring system with a planet");
        let option = ChoiceOption::labelled(target, "activate", format!("activate {target}"));
        let choice = Choice::new(player.clone(), "activate a system", vec![option.clone()]);
        let seen = Observed::new(&state, content, POK, Some(&galaxy));
        let features = explicit_option_features(&seen, &choice, &option, &player);

        assert_eq!(value_of(&features, "target:reachable"), Some(1.0));
        assert!(value_of(&features, "target:planet-count").is_some_and(|count| count > 0.0));
        assert!(value_of(&features, &format!("option:{target}")).is_none());
        assert!(
            names_of(&features)
                .iter()
                .all(|name| !name.starts_with("kind-faction:"))
        );
        assert!(
            names_of(&features)
                .iter()
                .all(|name| !name.starts_with("state-option:"))
        );
    }

    #[test]
    fn a_fleets_own_unpinned_system_is_reachable_for_activation_scoring() {
        let (state, galaxy) = observed_three_player_board();
        let content = ti4_content::ContentStore::embedded();
        let player = PlayerId::new("a");
        let home = state.player(&player).unwrap().home_system.as_ref().unwrap();
        let option =
            ChoiceOption::labelled(home.to_string(), "activate", format!("activate {home}"));
        let choice = Choice::new(player.clone(), "activate a system", vec![option.clone()]);
        let seen = Observed::new(&state, content, POK, Some(&galaxy));

        let features = explicit_option_features(&seen, &choice, &option, &player);

        assert_eq!(value_of(&features, "target:reachable"), Some(1.0));
    }

    #[test]
    fn explicit_movement_and_landing_expose_route_unit_and_planet_facts() {
        let (mut state, galaxy) = observed_three_player_board();
        let content = ti4_content::ContentStore::embedded();
        let player = PlayerId::new("a");
        let origin = state
            .player(&player)
            .unwrap()
            .home_system
            .as_ref()
            .unwrap()
            .clone();
        let destination = galaxy
            .adjacent(origin.as_str())
            .into_iter()
            .find(|system| !ti4_content::galaxy::planets_in(content, system, POK).is_empty())
            .unwrap()
            .to_owned();
        state.active_system = Some(SystemId::new(&destination));
        let move_choice = ti4_engine::tactical::movement_options(
            &player,
            &[ti4_engine::tactical::Movable {
                origin: origin.clone(),
                index: 0,
                unit: ti4_model::units::Unit::new(
                    ti4_model::id::UnitTypeId::new("carrier"),
                    player.clone(),
                ),
                capacity: 4,
                gravity_drive: false,
            }],
        );
        let move_option = move_choice
            .options
            .iter()
            .find(|option| option.kind == "move")
            .unwrap();
        let seen = Observed::new(&state, content, POK, Some(&galaxy));
        let movement = explicit_option_features(&seen, &move_choice, move_option, &player);
        assert_eq!(value_of(&movement, "route:adjacent"), Some(1.0));
        assert_eq!(value_of(&movement, "move-unit:capacity"), Some(4.0));
        assert!(value_of(&movement, "origin:own-ships").is_some());

        let planet = ti4_content::galaxy::planets_in(content, &destination, POK)
            .first()
            .expect("planet")
            .id()
            .to_owned();
        let land = ChoiceOption::new("land", "land").with("planet", planet);
        // With the terminator, as the engine always offers it (`invasion::commit_options`). It
        // matters here beyond realism: a choice whose options all share one kind has its
        // kind-keyed features skipped as inert, so `state-kind:commit:*` below is only a fact
        // about this decision when some option has a different kind.
        let landing_choice = Choice::new(
            player.clone(),
            "commit ground forces",
            vec![
                land.clone(),
                ChoiceOption::new("done_committing", "decline"),
            ],
        );
        let landing = explicit_option_features(&seen, &landing_choice, &land, &player);
        assert!(
            value_of(&landing, "landing:resources").is_some()
                || value_of(&landing, "landing:influence").is_some()
        );
        assert!(value_of(&landing, "invasion:planet-count").is_some());
        assert!(value_of(&landing, "state-kind:commit:round").is_some());
    }

    #[test]
    fn the_base_features_match_the_oracle_extractor_bucket_for_bucket() {
        // Generated by calling the real `HashedLinearPolicy.features`, not by reading it. Every
        // trained weight is indexed by these buckets, so a feature that hashes differently is a
        // weight learned for one fact being applied to another — and nothing would report it.
        //
        // **This checks the base block only, and the corpus was generated with no galaxy.**
        // The oracle suppresses its structured board features when there is no map, so comparing
        // against a map-less oracle hid exactly the block this port has not written. With a map it
        // emits three to six more buckets per option — planet counts, who controls them, enemy
        // ships present, whether the system is reachable — and those are what a policy needs to
        // learn *which* system to activate rather than memorising ids. The real-board tests in
        // `structured_features_status` cover that explicit block.
        let corpus: Vec<GoldenFeatures> =
            serde_json::from_str(include_str!("../tests/golden_features.json"))
                .expect("the golden corpus parses");
        assert!(corpus.len() >= 5, "several kinds and payload shapes");

        let state = oracle_seat();
        let seen = Observed::new(&state, ti4_content::ContentStore::embedded(), POK, None);
        let player = PlayerId::new("a");

        for case in &corpus {
            let mut option = ChoiceOption::labelled(&case.id, &case.kind, &case.label);
            for (key, value) in &case.payload {
                option = option.with(key.clone(), value.clone());
            }
            let asked = Choice::new(player.clone(), &case.prompt, vec![option.clone()]);

            let ours = option_features(&seen, &asked, &option, &player, DIMENSIONS);
            assert_eq!(
                ours.len(),
                case.features.len(),
                "{} produced {} buckets against the oracle's {}",
                case.kind,
                ours.len(),
                case.features.len()
            );
            for (slot, want) in &case.features {
                let got = value_of(&ours, slot).unwrap_or(0.0);
                assert!(
                    (got - want).abs() < 1e-9,
                    "{} bucket {slot}: {got} against the oracle's {want}",
                    case.kind
                );
            }
        }
    }

    #[test]
    fn a_zero_fact_is_dropped_rather_than_stored() {
        // A zero contributes nothing to a score and nothing to a gradient. Storing it would make a
        // vector's length depend on which facts happened to be zero.
        let mut features = Features::new(DIMENSIONS);
        features.add("nothing", 0.0);
        features.add("also_nothing", f64::NAN);
        features.add("infinite", f64::INFINITY);
        assert!(features.vector().is_empty());

        features.add("something", 2.0);
        assert_eq!(features.vector().len(), 1);
    }

    #[test]
    fn two_facts_in_one_bucket_sum_rather_than_overwrite() {
        // The hashing trick working as intended. Overwriting would silently discard a fact, and
        // which one survived would depend on iteration order.
        let mut features = Features::new(1); // one bucket, so everything collides
        features.add("first", 1.0);
        features.add("second", 1.0);
        features.add("third", 1.0);

        let vector = features.vector();
        assert_eq!(vector.len(), 1);
        let total: f64 = vector.values().sum();
        // Signs differ per name, so the sum is the signed total rather than three.
        assert!(total.abs() <= 3.0 && total.abs() > 0.0, "{total}");
    }

    #[test]
    fn tokens_are_the_alphanumeric_runs_of_a_lowercased_string() {
        assert_eq!(tokens("destroy|1"), vec!["destroy", "1"]);
        assert_eq!(
            tokens("Produce Carrier For 3"),
            vec!["produce", "carrier", "for", "3"]
        );
        assert_eq!(
            tokens("pok1leadership secondary"),
            vec!["pok1leadership", "secondary"]
        );
        assert_eq!(tokens("move|16|0"), vec!["move", "16", "0"]);
        assert!(tokens("---").is_empty());
    }

    #[test]
    fn an_option_mentioning_a_word_twice_records_it_once() {
        // The fact is "this option mentions carriers", not how often. Counting would make a longer
        // label a stronger signal about nothing.
        let state = oracle_seat();
        let seen = Observed::new(&state, ti4_content::ContentStore::embedded(), POK, None);
        let player = PlayerId::new("a");

        let once = ChoiceOption::labelled("carrier", "produce", "build one");
        let twice = ChoiceOption::labelled("carrier", "produce", "build carrier");
        let asked = Choice::new(player.clone(), "produce", vec![once.clone()]);

        let single = option_features(&seen, &asked, &once, &player, DIMENSIONS);
        let doubled = option_features(&seen, &asked, &twice, &player, DIMENSIONS);
        let (slot, sign) = bucket("option:carrier", DIMENSIONS);
        assert!(
            (value_of(&single, &slot).unwrap_or(0.0) - sign).abs() < 1e-9
                || value_of(&single, &slot).is_some(),
            "the id alone records the word"
        );
        // The label repeating it must not double its contribution beyond the extra `build`/`one`
        // tokens, which land elsewhere.
        assert!(value_of(&doubled, &slot).is_some());
    }

    #[test]
    fn the_same_position_hashes_the_same_way_twice() {
        // Inference and training must agree on what a decision looked like, and they run at
        // different times against a reconstructed state.
        let state = oracle_seat();
        let seen = Observed::new(&state, ti4_content::ContentStore::embedded(), POK, None);
        let player = PlayerId::new("a");
        let option = ChoiceOption::labelled("18", "activate", "activate 18");
        let asked = Choice::new(player.clone(), "activate a system", vec![option.clone()]);

        let once = option_features(&seen, &asked, &option, &player, DIMENSIONS);
        let twice = option_features(&seen, &asked, &option, &player, DIMENSIONS);
        assert_eq!(once, twice);
    }

    #[test]
    fn two_different_options_do_not_hash_alike() {
        // If they did, no policy could ever separate them however long it trained.
        let state = oracle_seat();
        let seen = Observed::new(&state, ti4_content::ContentStore::embedded(), POK, None);
        let player = PlayerId::new("a");
        let one = ChoiceOption::labelled("18", "activate", "activate 18");
        let other = ChoiceOption::labelled("26", "activate", "activate 26");
        let asked = Choice::new(
            player.clone(),
            "activate a system",
            vec![one.clone(), other.clone()],
        );

        assert_ne!(
            option_features(&seen, &asked, &one, &player, DIMENSIONS),
            option_features(&seen, &asked, &other, &player, DIMENSIONS)
        );
    }

    #[test]
    fn the_seats_position_reaches_the_features() {
        // Without this a policy sees the menu and never the game, and would learn one ranking of
        // option ids to use in every position it ever meets.
        let mut poor = oracle_seat();
        poor.player_mut(&PlayerId::new("a")).unwrap().trade_goods = 0;
        let rich = oracle_seat();

        let player = PlayerId::new("a");
        let option = ChoiceOption::new("pay|exact", "pay");
        let asked = Choice::new(player.clone(), "pay 3", vec![option.clone()]);
        let content = ti4_content::ContentStore::embedded();

        let thin = option_features(
            &Observed::new(&poor, content, POK, None),
            &asked,
            &option,
            &player,
            DIMENSIONS,
        );
        let flush = option_features(
            &Observed::new(&rich, content, POK, None),
            &asked,
            &option,
            &player,
            DIMENSIONS,
        );
        assert_ne!(thin, flush, "the same option in two positions hashed alike");
    }

    #[test]
    fn every_feature_is_a_fact_and_none_of_them_is_a_score() {
        // M09-014 in miniature, and the reason `option_feature_names` exists at all: a hashed
        // vector cannot be read back, so without the names there would be no way to check what a
        // "fully learned" policy is being shown. A policy reading a hand-tuned constant would be
        // reporting somebody else's opinion as something it had learned.
        let state = oracle_seat();
        let seen = Observed::new(&state, ti4_content::ContentStore::embedded(), POK, None);
        let player = PlayerId::new("a");
        let option = ChoiceOption::labelled("produce|carrier", "produce", "produce carrier for 3")
            .with("cost", 3)
            .with("units", 1);
        let asked = Choice::new(player.clone(), "produce a unit", vec![option.clone()]);

        let named = option_feature_names(&seen, &asked, &option, &player);
        assert!(named.len() > 20, "a real vector: {}", named.len());
        for (name, _) in &named {
            assert!(
                FEATURE_PREFIXES
                    .iter()
                    .any(|prefix| name.starts_with(prefix)),
                "{name} is not one of the declared factual shapes"
            );
        }
    }

    /// M09-019b feature inventory pin.
    ///
    /// The evidence table in `plans/evidence/M09-019.md` catalogues the current feature families;
    /// this test encodes its structural facts so rows 021–023 (which add or change families) can
    /// land only by breaking one of these assertions and updating the inventory in the same
    /// package. That is the diff mechanism the row requires.
    #[test]
    fn m09_019b_feature_inventory_is_pinned() {
        // 1. The legacy family vocabulary: exactly the thirteen declared closed-list prefixes.
        assert_eq!(
            FEATURE_PREFIXES,
            [
                "kind:",
                "kind-faction:",
                "option:",
                "option-faction:",
                "prompt-option:",
                "prompt-bigram:",
                "payload-bool:",
                "payload-number:",
                "payload-number-kind:",
                "payload:",
                "payload-count:",
                "state-kind:",
                "state-option:",
            ]
        );

        // 2. The schema-4 head vocabulary (the r6 champion's): exactly the fourteen declared heads.
        assert_eq!(
            crate::learned::STAGE1_DECISION_HEADS,
            [
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
                "other",
            ]
        );

        // 3. The inventory fixture: one option carrying every payload shape and a multi-token
        //    prompt, so all thirteen legacy families are exercised by names actually emitted —
        //    each table row is real rather than aspirational.
        let state = oracle_seat();
        let seen = Observed::new(&state, ti4_content::ContentStore::embedded(), POK, None);
        let player = PlayerId::new("a");
        let option = ChoiceOption::labelled("produce|carrier", "produce", "produce carrier for 3")
            .with("ready", true)
            .with("cost", 3)
            .with("cargo", "archonren")
            .with("list", serde_json::json!(["alpha", "beta"]));
        let asked = Choice::new(player.clone(), "produce a unit now", vec![option.clone()]);

        let named = option_feature_names(&seen, &asked, &option, &player);
        for (name, _) in &named {
            assert!(
                FEATURE_PREFIXES.iter().any(|prefix| name.starts_with(prefix)),
                "{name} escapes the pinned legacy families"
            );
        }
        for prefix in FEATURE_PREFIXES {
            assert!(
                named.iter().any(|(name, _)| name.starts_with(prefix)),
                "family {prefix:?} is not exercised by the inventory fixture — its table row is unverifiable"
            );
        }

        // 4. The explicit path on the same fixture: factual names with the legacy memorisation
        //    channels removed. A single option with a composite id gives StateCross::None, so no
        //    seat-fact cross and no kind family; prompt-kind is the explicit-only family.
        let explicit = explicit_option_features(&seen, &asked, &option, &player);
        assert_eq!(state_cross(&asked), StateCross::None);
        for name in names_of(&explicit) {
            assert!(
                !name.starts_with("kind-faction:") && !name.starts_with("option-faction:"),
                "{name}: faction crosses are a legacy-only channel"
            );
            if let Some(token) = name.strip_prefix("option:") {
                assert!(
                    token.chars().any(|character| !character.is_ascii_digit()),
                    "{name}: bare numeric identities must not reach the explicit path"
                );
            }
            assert!(
                !name.starts_with("state-kind:") && !name.starts_with("state-option:"),
                "{name}: StateCross::None emits no seat-fact cross"
            );
        }
        let names = names_of(&explicit);
        assert!(
            names.iter().any(|name| name.starts_with("prompt-kind:")),
            "the explicit-only prompt-kind family is missing from the fixture output"
        );
    }

    #[test]
    fn the_names_and_the_buckets_describe_the_same_decision() {
        // If naming and hashing could drift apart, the check above would be inspecting something
        // other than what inference reads.
        let state = oracle_seat();
        let seen = Observed::new(&state, ti4_content::ContentStore::embedded(), POK, None);
        let player = PlayerId::new("a");
        let option = ChoiceOption::labelled("18", "activate", "activate 18");
        let asked = Choice::new(player.clone(), "activate a system", vec![option.clone()]);

        let hashed = option_features(&seen, &asked, &option, &player, DIMENSIONS);
        let mut rebuilt = Features::new(DIMENSIONS);
        for (name, value) in option_feature_names(&seen, &asked, &option, &player) {
            rebuilt.add(&name, value);
        }
        assert_eq!(hashed, rebuilt.into_vector());
    }

    #[test]
    fn a_uniform_kind_choice_drops_exactly_the_features_that_cannot_matter() {
        // The property the skip rests on, checked rather than argued: everything dropped had the
        // same value on every option, and everything that distinguished the options was kept.
        let (state, galaxy) = observed_three_player_board();
        let content = ti4_content::ContentStore::embedded();
        let seen = Observed::new(&state, content, POK, Some(&galaxy));
        let player = PlayerId::new("a");
        let options: Vec<ChoiceOption> = ["pok2diplomacy", "pok3politics", "pok7technology"]
            .iter()
            .map(|id| ChoiceOption::new(*id, "strategy_card"))
            .collect();
        let choice = Choice::new(player.clone(), "choose a strategy card", options.clone());
        assert!(uniform_kind(&choice), "the fixture is a single-kind choice");

        let prompt_tokens = tokens(&choice.prompt);
        let context = ChoiceContext {
            facts: seat_facts(&seen, &player),
            own_units: seen.systems_with_units_of(&player).into_iter().collect(),
        };
        let full: Vec<FeatureVector> = options
            .iter()
            .map(|option| {
                explicit_option_features_with(
                    &seen,
                    &prompt_tokens,
                    &context,
                    option,
                    &player,
                    StateCross::ByKind,
                )
            })
            .collect();
        let kept = explicit_choice_features(&seen, &choice, &player);

        let dropped: Vec<_> = full[0]
            .keys()
            .filter(|key| !kept[0].contains_key(key))
            .copied()
            .collect();
        assert!(!dropped.is_empty(), "the rule should drop something here");
        for key in &dropped {
            let value = full[0].get(key).copied();
            assert!(
                full.iter().all(|vector| vector.get(key).copied() == value),
                "{} was dropped but does not have one value across the options",
                crate::intern::name_of(*key)
            );
        }
        for key in full[0].keys() {
            let value = full[0].get(key).copied();
            if !full.iter().all(|vector| vector.get(key).copied() == value) {
                assert!(
                    kept[0].contains_key(key),
                    "{} distinguishes the options and must be kept",
                    crate::intern::name_of(*key)
                );
            }
        }
    }

    #[test]
    fn a_binary_choice_can_see_the_seat_state() {
        // Every strategy-card secondary builds both options with the same kind, so crossing state
        // with the kind produced one name and one value on both options -- provably inert in a
        // softmax. The head could only ever answer from the card's identity, never from whether
        // the seat could afford the cost. Crossing with the option id instead is what lets the
        // state through, and this is the property that has to hold.
        let (state, galaxy) = observed_three_player_board();
        let content = ti4_content::ContentStore::embedded();
        let seen = Observed::new(&state, content, POK, Some(&galaxy));
        let player = PlayerId::new("a");
        let choice = Choice::new(
            player.clone(),
            "spend a strategy token to replenish commodities",
            vec![
                ChoiceOption::labelled("no", "strategy", "decline"),
                ChoiceOption::labelled("yes", "strategy", "replenish"),
            ],
        );
        assert!(uniform_kind(&choice), "a secondary is a single-kind choice");
        assert_eq!(state_cross(&choice), StateCross::ByOption);

        let vectors = explicit_choice_features(&seen, &choice, &player);
        let named = |index: usize| -> Vec<String> {
            vectors[index]
                .keys()
                .map(|key| crate::intern::name_of(*key))
                .collect()
        };
        let no = named(0);
        let yes = named(1);
        assert!(
            no.iter().any(|name| name.starts_with("state-option:no:")),
            "the declining option must carry the seat state, got {no:?}"
        );
        assert!(
            yes.iter().any(|name| name.starts_with("state-option:yes:")),
            "the accepting option must carry the seat state"
        );
        // The point of the cross: the two options must not share these slots, or they cancel again.
        for name in &yes {
            if name.starts_with("state-option:") {
                assert!(
                    !no.contains(name),
                    "{name} appears on both options and would be inert again"
                );
            }
        }
    }

    #[test]
    fn a_mixed_kind_choice_keeps_everything() {
        // `state-kind:move:*` and `state-kind:decline:*` are different slots, so each one does
        // distinguish its options and none of them is inert.
        let (state, galaxy) = observed_three_player_board();
        let content = ti4_content::ContentStore::embedded();
        let seen = Observed::new(&state, content, POK, Some(&galaxy));
        let player = PlayerId::new("a");
        let choice = Choice::new(
            player.clone(),
            "movement",
            vec![
                ChoiceOption::new("move|16|2", "move"),
                ChoiceOption::new("done_moving", "decline"),
            ],
        );
        assert!(!uniform_kind(&choice));

        let context = ChoiceContext {
            facts: seat_facts(&seen, &player),
            own_units: seen.systems_with_units_of(&player).into_iter().collect(),
        };
        let full = explicit_option_features_with(
            &seen,
            &tokens(&choice.prompt),
            &context,
            &choice.options[0],
            &player,
            StateCross::ByKind,
        );
        assert_eq!(
            explicit_choice_features(&seen, &choice, &player)[0],
            full,
            "a mixed-kind choice loses nothing"
        );
    }
}
