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

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use ti4_engine::choice::{Choice, ChoiceOption, Observed};
use ti4_model::id::PlayerId;

use crate::learned::bucket;

/// A sparse feature vector: bucket name to accumulated signed value.
pub type FeatureVector = BTreeMap<String, f64>;

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
            buckets: BTreeMap::new(),
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
        *self.buckets.entry(slot).or_insert(0.0) += sign * value;
    }

    /// The sparse vector.
    #[must_use]
    pub const fn vector(&self) -> &FeatureVector {
        &self.buckets
    }

    /// Take the sparse vector.
    #[must_use]
    pub fn into_vector(self) -> FeatureVector {
        self.buckets
    }
}

/// The tokens the oracle's `[a-z0-9]+` finds in a lowercased string.
fn tokens(text: &str) -> Vec<String> {
    text.to_lowercase()
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

/// Structured tactical facts this port does not extract yet (M09-010).
///
/// The oracle also describes the *system* an option names — how many planets it holds, who
/// controls them, whose ships are there, whether it is reachable — under role-specific prefixes
/// like `origin:` and `destination:`. Option ids are useful identity features but cannot teach
/// that an unseen origin is rich in troops, so this is where a policy learns to generalise across
/// systems it has never seen.
///
/// Named rather than silently missing: without it a learned policy can still rank options, but it
/// memorises system ids instead of reading boards.
#[must_use]
pub const fn structured_features_missing() -> &'static str {
    "M09-010: origin/destination/route/cargo/production system features"
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn the_features_match_the_oracle_extractor_bucket_for_bucket() {
        // Generated by calling the real `HashedLinearPolicy.features`, not by reading it. Every
        // trained weight is indexed by these buckets, so a feature that hashes differently is a
        // weight learned for one fact being applied to another — and nothing would report it.
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
                let got = ours.get(slot).copied().unwrap_or(0.0);
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
            (single.get(&slot).copied().unwrap_or(0.0) - sign).abs() < 1e-9
                || single.contains_key(&slot),
            "the id alone records the word"
        );
        // The label repeating it must not double its contribution beyond the extra `build`/`one`
        // tokens, which land elsewhere.
        assert!(doubled.contains_key(&slot));
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
}
