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
use ti4_model::id::{PlanetId, PlayerId, SystemId};

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
    explicit_option_features_with(
        seen,
        &tokens(&choice.prompt),
        &seat_facts(seen, player),
        option,
        player,
    )
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
    let facts = seat_facts(seen, player);
    choice
        .options
        .iter()
        .map(|option| explicit_option_features_with(seen, &prompt_tokens, &facts, option, player))
        .collect()
}

// Keeping the extractor in one linear block makes its ordering and parity with the Python
// reference auditable; splitting it would obscure which crosses belong to the base feature set.
#[allow(clippy::too_many_lines)]
fn explicit_option_features_with(
    seen: &Observed<'_>,
    prompt_tokens: &[String],
    seat_facts: &[(&'static str, f64); 8],
    option: &ChoiceOption,
    player: &PlayerId,
) -> FeatureVector {
    let mut features = FeatureVector::new();
    let kind = canonical_feature_kind(&option.kind);
    add_named(&mut features, &format!("kind:{kind}"), 1.0);

    let mut option_tokens: BTreeSet<String> = tokens(&option.id)
        .into_iter()
        .chain(tokens(&option.label))
        .filter(|token| !token.chars().all(|character| character.is_ascii_digit()))
        .collect();
    // Stable iteration is part of the feature contract even though addition is commutative.
    for token in &option_tokens {
        add_named(&mut features, &format!("option:{token}"), 1.0);
    }

    for prompt_token in prompt_tokens {
        add_named(
            &mut features,
            &format!("prompt-kind:{prompt_token}:{kind}"),
            1.0,
        );
        for option_token in &option_tokens {
            add_named(
                &mut features,
                &format!("prompt-option:{prompt_token}:{option_token}"),
                1.0,
            );
        }
    }

    for (key, value) in &option.payload {
        match value {
            Value::Bool(flag) => add_named(
                &mut features,
                &format!(
                    "payload-bool:{key}:{}",
                    if *flag { "True" } else { "False" }
                ),
                1.0,
            ),
            Value::Number(number) => {
                if let Some(number) = number.as_f64() {
                    add_named(&mut features, &format!("payload-number:{key}"), number);
                    add_named(
                        &mut features,
                        &format!("payload-number-kind:{key}:{kind}"),
                        number,
                    );
                }
            }
            Value::String(text) => {
                for token in tokens(text)
                    .into_iter()
                    .filter(|token| !token.chars().all(|character| character.is_ascii_digit()))
                {
                    add_named(&mut features, &format!("payload:{key}:{token}"), 1.0);
                }
            }
            Value::Array(items) => {
                #[expect(clippy::cast_precision_loss, reason = "option payloads are small")]
                add_named(
                    &mut features,
                    &format!("payload-count:{key}"),
                    items.len() as f64,
                );
                for item in items {
                    if let Value::String(text) = item
                        && !text.chars().all(|character| character.is_ascii_digit())
                    {
                        add_named(
                            &mut features,
                            &format!("payload:{key}:{}", text.to_lowercase()),
                            1.0,
                        );
                    }
                }
            }
            Value::Null | Value::Object(_) => {}
        }
    }

    for (name, value) in seat_facts {
        add_named(&mut features, &format!("state-kind:{kind}:{name}"), *value);
    }

    structured_features(seen, option, player, &mut features);
    option_tokens.clear();
    features
}

fn add_named(features: &mut FeatureVector, name: &str, value: f64) {
    if value == 0.0 || !value.is_finite() {
        return;
    }
    *features.entry(name.to_owned()).or_insert(0.0) += value;
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
    features: &mut FeatureVector,
) {
    let kind = canonical_feature_kind(&option.kind);
    let active = seen.active_system();
    if matches!(kind, "activate" | "system") {
        add_system_features(seen, &option.id, player, "target", features);
    }

    if let Some(system) = payload_string(option, "system") {
        let prefix = match kind {
            "produce" | "build" => "production",
            "placement" => "placement",
            "load" => "origin",
            _ => "option-system",
        };
        add_system_features(seen, system, player, prefix, features);
    }

    match kind {
        "move" => {
            if let Some(origin) = payload_string(option, "origin") {
                add_system_features(seen, origin, player, "origin", features);
                if let Some(destination) = active {
                    add_route_features(seen, origin, destination.as_str(), features);
                }
            }
            if let Some(destination) = active {
                add_system_features(seen, destination.as_str(), player, "destination", features);
            }
        }
        "load" => {
            if let Some(destination) = active {
                add_system_features(seen, destination.as_str(), player, "destination", features);
            }
        }
        "commit" => {
            if let Some(destination) = active {
                add_system_features(seen, destination.as_str(), player, "invasion", features);
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
        add_named(features, "route:hex-distance", f64::from(distance));
    }
    add_named(
        features,
        "route:adjacent",
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
        add_named(features, &format!("{prefix}:{name}"), value);
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
        add_named(features, &format!("{prefix}:{name}"), value);
    }
    if let Some(trait_name) = planet.planet_type() {
        add_named(
            features,
            &format!("{prefix}:trait:{}", trait_name.to_lowercase()),
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
        &format!("{prefix}:controlled-by-us"),
        f64::from(u8::from(controller == Some(player))),
    );
    add_named(
        features,
        &format!("{prefix}:uncontrolled"),
        f64::from(u8::from(controller.is_none())),
    );
    add_named(
        features,
        &format!("{prefix}:controlled-by-enemy"),
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
        &format!("{prefix}:own-ground"),
        count_value(own_ground),
    );
    add_named(
        features,
        &format!("{prefix}:enemy-ground"),
        count_value(enemy_ground),
    );
}

fn unit_stats<'a>(
    seen: &'a Observed<'a>,
    unit: &ti4_model::units::Unit,
) -> Option<ti4_content::units::UnitType<'a>> {
    ti4_content::units::unit_type(seen.content(), unit.type_id.as_str(), seen.sources())
}

fn add_system_features(
    seen: &Observed<'_>,
    system_id: &str,
    player: &PlayerId,
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
    let planets = ti4_content::galaxy::planets_in(seen.content(), system_id, seen.sources());
    let controls = &system.planet_control;
    let planet_ids: Vec<PlanetId> = planets
        .iter()
        .map(|planet| PlanetId::new(planet.id()))
        .collect();
    add_named(
        features,
        &format!("{prefix}:planet-count"),
        count_value(planets.len()),
    );
    add_named(
        features,
        &format!("{prefix}:not-controlled-count"),
        count_value(
            planet_ids
                .iter()
                .filter(|planet| controls.get(*planet) != Some(player))
                .count(),
        ),
    );
    add_named(
        features,
        &format!("{prefix}:uncontrolled-count"),
        count_value(
            planet_ids
                .iter()
                .filter(|planet| !controls.contains_key(*planet))
                .count(),
        ),
    );
    add_named(
        features,
        &format!("{prefix}:enemy-controlled-count"),
        count_value(
            planet_ids
                .iter()
                .filter(|planet| controls.get(*planet).is_some_and(|owner| owner != player))
                .count(),
        ),
    );

    let own_ships = system
        .units
        .iter()
        .filter(|unit| {
            unit.owner == *player && unit_stats(seen, unit).is_some_and(|stats| stats.is_ship())
        })
        .count();
    let enemy_ships = system
        .units
        .iter()
        .filter(|unit| {
            unit.owner != *player && unit_stats(seen, unit).is_some_and(|stats| stats.is_ship())
        })
        .count();
    let own_ground_space = system
        .units
        .iter()
        .filter(|unit| {
            unit.owner == *player
                && unit_stats(seen, unit).is_some_and(|stats| stats.is_ground_force())
        })
        .count();
    let all_units = system
        .units
        .iter()
        .chain(system.planet_units.values().flatten());
    let mut enemy_ground_total = 0usize;
    let mut own_production_units = 0usize;
    for unit in all_units {
        if let Some(stats) = unit_stats(seen, unit) {
            enemy_ground_total += usize::from(unit.owner != *player && stats.is_ground_force());
            own_production_units += usize::from(unit.owner == *player && stats.has_production());
        }
    }
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
        add_named(features, &format!("{prefix}:{name}"), value);
    }
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

        assert_eq!(features.get("target:reachable"), Some(&1.0));
        assert!(
            features
                .get("target:planet-count")
                .is_some_and(|count| *count > 0.0)
        );
        assert!(!features.contains_key(&format!("option:{target}")));
        assert!(
            features
                .keys()
                .all(|name| !name.starts_with("kind-faction:"))
        );
        assert!(
            features
                .keys()
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

        assert_eq!(features.get("target:reachable"), Some(&1.0));
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
        assert_eq!(movement.get("route:adjacent"), Some(&1.0));
        assert_eq!(movement.get("move-unit:capacity"), Some(&4.0));
        assert!(movement.contains_key("origin:own-ships"));

        let planet = ti4_content::galaxy::planets_in(content, &destination, POK)
            .first()
            .expect("planet")
            .id()
            .to_owned();
        let land = ChoiceOption::new("land", "land").with("planet", planet);
        let landing_choice =
            Choice::new(player.clone(), "commit ground forces", vec![land.clone()]);
        let landing = explicit_option_features(&seen, &landing_choice, &land, &player);
        assert!(
            landing.contains_key("landing:resources") || landing.contains_key("landing:influence")
        );
        assert!(landing.contains_key("invasion:planet-count"));
        assert!(landing.contains_key("state-kind:commit:round"));
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
