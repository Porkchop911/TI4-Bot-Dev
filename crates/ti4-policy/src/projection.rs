//! The MLP input projection (M09-024b1, Tier-C architecture ruling of 2026-08-25).
//!
//! # What this is
//!
//! The MLP does not consume the schema-4 explicit vector as emitted. It consumes a **projection**
//! of it, applied *before* vocabulary lookup: unbounded memorisation crosses are suppressed, and
//! the eight acting-seat facts are restored under a bounded bare family.
//!
//! # Why a projection rather than a change to the extractor
//!
//! The schema-4 vector is what six trained champions score with, and what M09-019b's inventory pin
//! and the legacy-subvector pin protect. Changing it to suit the MLP would move every one of those
//! at once. So the extractor is left exactly as it is and the MLP takes a different **view** of its
//! output. `explicit_choice_features` is byte-for-byte what it was; everything here is downstream
//! of it.
//!
//! # What is suppressed, and why it is a predicate rather than a list
//!
//! An **unbounded memorisation cross** is a family whose identity is the Cartesian product of two
//! free lexical identities, or of a full option identity and a state fact. Measured over the §6.1
//! schedule, three such families were 91.3% of a 203,843-name vocabulary — a dense column each, at
//! `width` weights apiece, for names seen in a vanishing fraction of games by construction.
//!
//! The ruling states this as a predicate on family *shape*, not as a list to maintain: any new
//! family with either shape is excluded by default and needs an architecture review to enter the
//! dense input. [`EXCLUDED_FAMILIES`] is the current grammar's answer to the predicate, not the
//! predicate itself.
//!
//! `state-kind` survives the predicate deliberately: its crossing axis is the bounded canonical
//! decision kind, so its columns transfer between games rather than naming one option in one board.
//!
//! # Suppressed, not routed
//!
//! An excluded name is dropped before lookup. It does **not** fall into its family's OOV column.
//! Those three reserved rows stay in the registry so that no v1 index moves, and are permanently
//! dead: never a routing destination, zeroed and masked from optimization by M09-026/M09-028.
//! Collapsing 186,088 distinct names into three columns would inject a dense signal that means
//! nothing, which is worse than dropping them.

use std::collections::BTreeSet;

use ti4_engine::choice::{Choice, ChoiceOption, Observed};
use ti4_model::id::PlayerId;

use crate::features::{FeatureVector, seat_facts};

/// The bounded bare family carrying the acting-seat facts into the MLP input.
///
/// The eight facts — round, the three token pools, goods, commodities, controlled planets and
/// technologies — are emitted by the schema-4 extractor **only** crossed, under `state-kind:` or
/// `state-option:`. Suppressing `state-option` without this would erase all eight from every
/// uniform-kind fixed-vocabulary decision, which is the `StateCross::ByOption` branch.
pub const SEAT_STATE_FAMILY: &str = "seat-state";

/// What a registered feature family is to the MLP input.
///
/// A **total** classification: every family in the frozen registry has exactly one role, and a
/// family with no role is not admitted. That direction matters. An earlier draft was a three-name
/// deny-list, which admitted anything unlisted — the opposite of the architecture ruling's
/// requirement that an unclassified family stays out until reviewed, and it silently admitted two
/// legacy-only families that the schema-4 extractor never emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FamilyRole {
    /// Gets a dense column. Its identity transfers between games.
    Transferable,
    /// An unbounded memorisation cross: the Cartesian product of two free lexical identities, or
    /// of a full option identity and a state fact. Suppressed before lookup.
    UnboundedCross,
    /// Emitted only by the legacy schema-2 hashed extractor. The MLP consumes one schema-4
    /// explicit path, so these names never occur on it — but they *do* occur in the r6 checkpoint,
    /// which is discovery source (a), so they must be rejected there rather than assumed absent.
    LegacyOnly,
}

/// The frozen role of every registered family.
///
/// Written out rather than derived from `EXPLICIT_FIXED_FAMILIES`, for the same reason the OOV
/// registry is written out: deriving it would mean a family added to a grammar is admitted to the
/// dense input as a side effect of an ordinary edit. Admission is an architecture decision, and
/// `the_classification_covers_exactly_the_registry` fails when this table and the registry drift
/// so the decision cannot be skipped.
const FAMILY_ROLES: [(&str, FamilyRole); 40] = [
    ("*-unit", FamilyRole::Transferable),
    ("ability", FamilyRole::Transferable),
    ("card", FamilyRole::Transferable),
    // M09-027b. Transferable: every critic identity is bounded and means the same thing next game
    // — the round, the acting seat's economy and score, opponent counts, and the `objective_
    // progress:<family>` / `ability:<x>` / `faction_tech:<t>` tokens, all of which are corpus
    // identities rather than board ones. Admitting it is what gives `critic-state:*` names real
    // columns; without the entry the closed default routes the whole critic vector to one column
    // and `V` is a rank-1 sum (F-M09-027-3).
    ("critic-state", FamilyRole::Transferable),
    ("destination", FamilyRole::Transferable),
    ("faction-commodities", FamilyRole::Transferable),
    ("faction-home", FamilyRole::Transferable),
    ("faction-start-tech", FamilyRole::Transferable),
    ("faction-start-unit", FamilyRole::Transferable),
    ("faction-tech", FamilyRole::Transferable),
    ("invasion", FamilyRole::Transferable),
    ("kind", FamilyRole::Transferable),
    // Faction crosses are a legacy-only channel: the explicit path asserts it never emits them.
    // `option-faction` would also fail the unbounded-cross shape; `LegacyOnly` is recorded because
    // it is the reason the MLP never sees them.
    ("kind-faction", FamilyRole::LegacyOnly),
    ("landing", FamilyRole::Transferable),
    ("objective-count", FamilyRole::Transferable),
    ("objective-met", FamilyRole::Transferable),
    ("objective-need", FamilyRole::Transferable),
    ("objective-progress", FamilyRole::Transferable),
    ("objective-stage", FamilyRole::Transferable),
    ("opponent-secrets-held", FamilyRole::Transferable),
    ("option", FamilyRole::Transferable),
    ("option-faction", FamilyRole::LegacyOnly),
    ("option-system", FamilyRole::Transferable),
    ("origin", FamilyRole::Transferable),
    ("pay", FamilyRole::Transferable),
    ("payload", FamilyRole::Transferable),
    ("payload-bool", FamilyRole::Transferable),
    ("payload-count", FamilyRole::Transferable),
    ("payload-number", FamilyRole::Transferable),
    ("payload-number-kind", FamilyRole::Transferable),
    ("placement", FamilyRole::Transferable),
    ("production", FamilyRole::Transferable),
    ("prompt-bigram", FamilyRole::UnboundedCross),
    ("prompt-kind", FamilyRole::Transferable),
    ("prompt-option", FamilyRole::UnboundedCross),
    ("route", FamilyRole::Transferable),
    ("seat-state", FamilyRole::Transferable),
    ("state-kind", FamilyRole::Transferable),
    ("state-option", FamilyRole::UnboundedCross),
    ("target", FamilyRole::Transferable),
];

/// The role of a family, or `None` if it has none.
///
/// `None` is the fail-closed answer, not an error to paper over: a family nobody classified is a
/// family nobody decided to put in the model.
#[must_use]
pub fn role_of(family: &str) -> Option<FamilyRole> {
    // The `<canonical-kind>-unit` families share one registry entry, but admission is **not** a
    // suffix test. An earlier draft mapped every family ending in `-unit` to the shared role, so
    // `never-reviewed-unit:x` was admitted — reopening by suffix exactly the closed default this
    // classification exists to hold (F-M09-024b1-3). Checkpoint names are a discovery source, so an
    // arbitrary historical `*-unit` family would have entered the dense vocabulary without the
    // architecture review a new family requires.
    //
    // The approved list is pinned instead, and an unrecognised suffix family falls through to
    // `None` like any other unclassified name.
    let key = if APPROVED_UNIT_FAMILIES.contains(&family) {
        crate::vocabulary::UNIT_SUFFIX_FAMILY
    } else {
        family
    };
    FAMILY_ROLES
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, role)| *role)
}

/// The `<canonical-kind>-unit` families approved for the dense input.
///
/// Frozen, for the same reason `OOV_FAMILIES_V1` is: the left half is a canonical decision kind and
/// `canonical_feature_kind` passes unknown kinds through unchanged, so the set of families the
/// extractor *could* emit is open. The set the architecture approved is not. Adding one is a
/// review, and `every_approved_unit_family_is_emitted_by_the_grammar` fails when this list and the
/// observed families disagree.
pub const APPROVED_UNIT_FAMILIES: [&str; 5] = [
    "commit-unit",
    "load-unit",
    "move-unit",
    "produce-unit",
    "transit-unit",
];

/// Whether a family is an unbounded memorisation cross.
#[must_use]
pub fn is_unbounded_cross(family: &str) -> bool {
    role_of(family) == Some(FamilyRole::UnboundedCross)
}

/// Every family that can never be a routing destination on the MLP path.
///
/// Both non-transferable roles: the unbounded crosses, suppressed by shape, and the legacy-only
/// channels, which the schema-4 extractor does not emit at all. Their reserved rows are retained so
/// no v1 index moves, and are dead — five of them, not the three the crosses alone would give.
#[must_use]
pub fn inactive_families() -> Vec<&'static str> {
    FAMILY_ROLES
        .iter()
        .filter(|(_, role)| *role != FamilyRole::Transferable)
        .map(|(name, _)| *name)
        .collect()
}

/// Whether a feature name survives the projection into the dense input.
///
/// Closed by default: admitted only if its family is classified `Transferable`.
#[must_use]
pub fn admits(name: &str) -> bool {
    role_of(crate::vocabulary::family_of(name)) == Some(FamilyRole::Transferable)
}

thread_local! {
    /// Memoised admission, keyed by the interned key.
    ///
    /// Admission is a pure function of the name, and [`crate::intern::FeatureKey`] is a pure
    /// function of the name, so the answer for a key never changes and caching it is sound.
    ///
    /// Thread-local rather than a shared map: a decider runs on one thread for the life of a game,
    /// so this needs no lock at all — and a lock is most of what was being paid.
    static ADMITTED: std::cell::RefCell<std::collections::HashMap<crate::intern::FeatureKey, bool>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// [`admits`] for a caller that already holds the key, memoised.
///
/// # Why this exists
///
/// `project_vector` ran `admits(&name_of(key))` for **every feature of every option of every
/// decision**: a global `RwLock` read, a `String` allocation, a `split_once`, and then a linear
/// scan of the forty `FAMILY_ROLES` entries comparing strings. At roughly forty features across
/// twenty options that is eight hundred locks and allocations and up to thirty-two thousand string
/// comparisons for one decision.
///
/// M09-029 measured the consequence: feature extraction alone cost more than the entire linear
/// game it was being compared against, so the throughput gate was about to charge the architecture
/// for a memoisable string lookup.
#[must_use]
pub fn admits_key(key: crate::intern::FeatureKey) -> bool {
    ADMITTED.with(|cache| {
        if let Some(known) = cache.borrow().get(&key) {
            return *known;
        }
        let verdict = admits(&crate::intern::name_of(key));
        cache.borrow_mut().insert(key, verdict);
        verdict
    })
}

/// The acting-seat facts under the bare family, for one position.
///
/// Option-invariant by construction — the same eight values on every option of a choice — which is
/// the point: MLP plan §4.1's nonlinear per-option trunk can let them interact with option facts,
/// where a linear head would see a constant and ignore them.
#[must_use]
pub fn seat_state_facts(
    seen: &Observed<'_>,
    player: &PlayerId,
    baseline: crate::progress::Baseline,
) -> Vec<(String, f64)> {
    let mut facts: Vec<(String, f64)> = seat_facts(seen, player)
        .into_iter()
        .map(|(name, value)| (format!("{SEAT_STATE_FAMILY}:{name}"), value))
        .collect();
    facts.extend(opening_facts(seen, player, baseline));
    facts
}

/// What the opening bar asks for, and where this seat stands against it.
///
/// The reward has always known these; the policy did not. `seat_facts` offers *absolute* controlled
/// planets and nothing about gains since setup, distinct systems, units built, or how far short each
/// component is — so a decision could not be conditioned on "I hold two systems and this activation
/// must reach a third", or "I have the planets and still owe a unit". The two measured failure
/// classes are exactly those two sentences.
///
/// Both the level and the deficit are emitted. They are redundant given the requirement, but the
/// requirement is a constant the model would have to learn to subtract, and a deficit that reaches
/// zero is a far easier thing to condition on than a level that reaches three.
///
/// # Concentration
///
/// Ship and ground spread are here for the same reason. Failed openings finish with forces in fewer
/// systems than cleared ones, and a seat cannot see that about itself from any existing fact. What
/// is offered is the count, not a reward for raising it: paying directly for spread would teach
/// ships to disperse to no purpose, while a fact lets the policy learn when spread is worth having.
///
/// `outside-active` is the resource a spreading decision actually spends — forces not already
/// committed to the system being resolved. It is the difference between "I have three carriers" and
/// "I have three carriers *left to send somewhere else*".
fn opening_facts(
    seen: &Observed<'_>,
    player: &PlayerId,
    baseline: crate::progress::Baseline,
) -> Vec<(String, f64)> {
    let requirement = ti4_engine::opening::DEFAULT_REQUIREMENT;
    let progress = crate::progress::measure(seen, player, baseline);

    let controlled = seen.controlled_planets(player);
    let controlled_systems: std::collections::BTreeSet<_> =
        controlled.iter().map(|(system, _)| *system).collect();
    let ship_systems = seen.systems_with_units_of(player);
    let active = seen.active_system();

    #[expect(
        clippy::cast_precision_loss,
        reason = "planet, system and unit counts are single digits"
    )]
    let deficit = |held: i64, bar: usize| -> f64 {
        let bar = i64::try_from(bar).unwrap_or(i64::MAX);
        (bar - held).max(0) as f64
    };
    #[expect(
        clippy::cast_precision_loss,
        reason = "planet, system and unit counts are single digits"
    )]
    let level = |value: i64| -> f64 { value as f64 };
    #[expect(
        clippy::cast_precision_loss,
        reason = "system counts are single digits"
    )]
    let count = |value: usize| -> f64 { value as f64 };

    vec![
        // Where this seat stands on each part of the bar.
        (
            format!("{SEAT_STATE_FAMILY}:opening-planets-gained"),
            level(progress.planets_gained),
        ),
        (
            format!("{SEAT_STATE_FAMILY}:opening-systems"),
            level(progress.systems),
        ),
        (
            format!("{SEAT_STATE_FAMILY}:opening-units-gained"),
            level(progress.units_gained),
        ),
        // And how much of each is still owed. Zero means that part is done.
        (
            format!("{SEAT_STATE_FAMILY}:opening-planets-needed"),
            deficit(progress.planets_gained, requirement.planets_gained),
        ),
        (
            format!("{SEAT_STATE_FAMILY}:opening-systems-needed"),
            deficit(progress.systems, requirement.systems),
        ),
        (
            format!("{SEAT_STATE_FAMILY}:opening-units-needed"),
            deficit(progress.units_gained, requirement.units_gained),
        ),
        // Where the forces are, which is what decides whether the deficits above can still close.
        (
            format!("{SEAT_STATE_FAMILY}:opening-ship-systems"),
            count(ship_systems.len()),
        ),
        (
            format!("{SEAT_STATE_FAMILY}:opening-controlled-systems"),
            count(controlled_systems.len()),
        ),
        (
            format!("{SEAT_STATE_FAMILY}:opening-ship-systems-outside-active"),
            count(
                ship_systems
                    .iter()
                    .filter(|system| active.is_none_or(|current| **system != current))
                    .count(),
            ),
        ),
        (
            format!("{SEAT_STATE_FAMILY}:opening-controlled-systems-outside-active"),
            count(
                controlled_systems
                    .iter()
                    .filter(|system| active.is_none_or(|current| **system != current))
                    .count(),
            ),
        ),
    ]
}

/// Project one already-extracted vector into the MLP input.
///
/// Suppression happens here, before any vocabulary lookup, so an excluded name never reaches a
/// column at all.
fn project_vector(
    vector: &FeatureVector,
    seat_state: &[(crate::intern::FeatureKey, f64)],
) -> FeatureVector {
    let kept = vector
        .iter()
        .filter(|(key, _)| admits_key(**key))
        .map(|(key, value)| (*key, *value));
    // The bare seat facts are a restatement of a position fact, not a second contribution to an
    // existing column: their family is disjoint from everything the extractor emits, so the
    // duplicate-summing in `from_pairs` cannot reach them.
    // `register`, not `FeatureKey::of`: the key alone puts a value in the vector but leaves it
    // nameless, so `names_of` resolves it to an empty string and the discovery pass that builds
    // the vocabulary from names would never see it. The agreement test between projected names
    // and projected vectors is what catches that.
    // Already interned by the caller, once per choice. Registering per option meant a lock and a
    // hash for each of the eight seat facts on every one of the legal options (M09-029).
    let added = seat_state.iter().copied();
    FeatureVector::from_pairs(kept.chain(added))
}

/// Every option of a choice, as the MLP sees it.
///
/// Takes the same inputs as [`crate::features::explicit_choice_features`], including the explicit
/// held-secret records, so the hidden-information boundary is exactly the one M09-021 established:
/// live play passes the acting seat's own cards, bound at ask time; offline contexts compute them
/// on state they already hold.
#[must_use]
pub fn mlp_choice_features(
    seen: &Observed<'_>,
    choice: &Choice,
    player: &PlayerId,
    held_secrets: &[ti4_engine::objectives::CardProgress],
    baseline: crate::progress::Baseline,
) -> Vec<FeatureVector> {
    let seat_state = interned_seat_state(seen, player, baseline);
    crate::features::explicit_choice_features(seen, choice, player, held_secrets)
        .iter()
        .map(|vector| project_vector(vector, &seat_state))
        .collect()
}

/// The bare seat facts, interned once for the whole choice.
///
/// `register` rather than `FeatureKey::of`: the key alone puts a value in the vector but leaves it
/// nameless, so `names_of` resolves it to an empty string and the discovery pass that builds the
/// vocabulary from names would never see it. The agreement test between projected names and
/// projected vectors is what catches that.
fn interned_seat_state(
    seen: &Observed<'_>,
    player: &PlayerId,
    baseline: crate::progress::Baseline,
) -> Vec<(crate::intern::FeatureKey, f64)> {
    seat_state_facts(seen, player, baseline)
        .into_iter()
        .map(|(name, value)| (crate::intern::register(&name), value))
        .collect()
}

/// One option, as the MLP sees it.
#[must_use]
pub fn mlp_option_features(
    seen: &Observed<'_>,
    choice: &Choice,
    option: &ChoiceOption,
    player: &PlayerId,
    held_secrets: &[ti4_engine::objectives::CardProgress],
    baseline: crate::progress::Baseline,
) -> FeatureVector {
    let seat_state = interned_seat_state(seen, player, baseline);
    let vector =
        crate::features::explicit_option_features(seen, choice, option, player, held_secrets);
    project_vector(&vector, &seat_state)
}

/// The projection applied to a set of discovered names.
///
/// M09-024b2's discovery pass runs through this so the vocabulary it builds and the vectors the
/// model is fed agree by construction rather than by two lists being kept in step.
#[must_use]
pub fn project_names<I, S>(names: I) -> BTreeSet<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    names
        .into_iter()
        .filter(|name| admits(name.as_ref()))
        .map(|name| name.as_ref().to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_engine::choice::ChoiceOption;
    use ti4_model::content_types::POK;
    use ti4_model::id::FactionId;

    fn position() -> (ti4_model::state::GameState, PlayerId) {
        let player = PlayerId::new("a");
        let mut state = ti4_engine::fixtures::game(&["a", "b"]);
        state.round = 3;
        {
            let seat = state.player_mut(&player).unwrap();
            seat.faction = FactionId::new("sol");
            seat.tactic_tokens = 2;
            seat.strategic_tokens = 1;
            seat.trade_goods = 5;
        }
        (state, player)
    }

    /// A uniform-kind choice whose option ids are fixed-vocabulary, so it crosses `ByOption` —
    /// the branch that carried the seat facts before the projection suppressed `state-option`.
    fn by_option_choice(player: &PlayerId) -> Choice {
        let options: Vec<ChoiceOption> = ["pok2diplomacy", "pok3politics"]
            .iter()
            .map(|id| ChoiceOption::new(*id, "strategy_card"))
            .collect();
        Choice::new(player.clone(), "choose a strategy card", options)
    }

    #[test]
    fn the_projection_suppresses_every_unbounded_cross() {
        let content = ti4_content::ContentStore::embedded();
        let (state, player) = position();
        let seen = Observed::new(&state, content, POK, None);
        let choice = by_option_choice(&player);

        // Non-vacuity: the unprojected vector really does carry excluded families, or suppressing
        // them proves nothing.
        let before = crate::features::explicit_choice_features(&seen, &choice, &player, &[]);
        let excluded_before: Vec<String> = before
            .iter()
            .flat_map(crate::features::names_of)
            .filter(|name| !admits(name))
            .collect();
        assert!(
            !excluded_before.is_empty(),
            "the fixture emits no excluded families: nothing to suppress"
        );

        let after = mlp_choice_features(
            &seen,
            &choice,
            &player,
            &[],
            crate::progress::Baseline::default(),
        );
        for vector in &after {
            for name in crate::features::names_of(vector) {
                assert!(
                    admits(&name),
                    "{name} survived the projection: its family is an unbounded cross"
                );
            }
        }
    }

    #[test]
    fn the_seat_facts_survive_by_option_under_the_bare_family() {
        // The correction the ruling required. `state_cross` puts a uniform-kind fixed-vocabulary
        // choice on `ByOption`, where the seat facts previously rode `state-option:` alone; the
        // bare family is what keeps them after suppression.
        let content = ti4_content::ContentStore::embedded();
        let (state, player) = position();
        let seen = Observed::new(&state, content, POK, None);
        let choice = by_option_choice(&player);
        assert_eq!(
            crate::features::state_cross(&choice),
            crate::features::StateCross::ByOption,
            "the fixture must be a ByOption choice"
        );

        let expected = seat_state_facts(&seen, &player, crate::progress::Baseline::default());
        assert_eq!(
            expected.len(),
            18,
            "eight general seat facts and ten opening-progress ones"
        );
        // Named rather than counted alone. A count catches a fact that vanished; it does not catch
        // a fact that was renamed, and a renamed feature is a new column with no trained weight
        // behind it — which looks like nothing at all from outside.
        for wanted in [
            "opening-planets-gained",
            "opening-systems",
            "opening-units-gained",
            "opening-planets-needed",
            "opening-systems-needed",
            "opening-units-needed",
            "opening-ship-systems",
            "opening-controlled-systems",
            "opening-ship-systems-outside-active",
            "opening-controlled-systems-outside-active",
        ] {
            let name = format!("{SEAT_STATE_FAMILY}:{wanted}");
            assert!(
                expected.iter().any(|(emitted, _)| *emitted == name),
                "{name} is not emitted"
            );
        }
        // Non-vacuity: at least one of them is non-zero in this position, so an all-zero vector
        // could not pass the comparison below.
        assert!(
            expected.iter().any(|(_, value)| *value != 0.0),
            "the fixture position has no non-zero seat fact"
        );

        for vector in &mlp_choice_features(
            &seen,
            &choice,
            &player,
            &[],
            crate::progress::Baseline::default(),
        ) {
            for (name, value) in &expected {
                assert_eq!(
                    crate::features::value_of(vector, name),
                    Some(*value),
                    "{name} is missing from a ByOption option after projection",
                );
            }
        }
    }

    #[test]
    fn the_seat_facts_are_present_under_every_crossing_mode() {
        // ByOption is the branch the ruling named, but the family is emitted unconditionally: a
        // fact that appears only on some decisions is a fact the trunk cannot rely on.
        let content = ti4_content::ContentStore::embedded();
        let (state, player) = position();
        let seen = Observed::new(&state, content, POK, None);

        let mixed = Choice::new(
            player.clone(),
            "act",
            vec![
                ChoiceOption::labelled("move|x", "move", "move"),
                ChoiceOption::labelled("decline", "decline", "decline"),
            ],
        );
        let none = Choice::new(
            player.clone(),
            "produce a unit",
            vec![
                ChoiceOption::labelled("produce|fighter@18", "production", "a fighter"),
                ChoiceOption::labelled("produce|scout@19", "production", "a scout"),
            ],
        );
        assert_eq!(
            crate::features::state_cross(&mixed),
            crate::features::StateCross::ByKind
        );
        assert_eq!(
            crate::features::state_cross(&none),
            crate::features::StateCross::None
        );

        let expected = seat_state_facts(&seen, &player, crate::progress::Baseline::default());
        for choice in [&mixed, &none] {
            for vector in &mlp_choice_features(
                &seen,
                choice,
                &player,
                &[],
                crate::progress::Baseline::default(),
            ) {
                for (name, value) in &expected {
                    assert_eq!(
                        crate::features::value_of(vector, name),
                        Some(*value),
                        "{name} missing under {:?}",
                        crate::features::state_cross(choice),
                    );
                }
            }
        }
    }

    #[test]
    fn the_schema_four_vector_is_untouched() {
        // The projection is a view. If it changed what the extractor emits, six trained champions
        // and two pinned inventories would all move at once.
        let content = ti4_content::ContentStore::embedded();
        let (state, player) = position();
        let seen = Observed::new(&state, content, POK, None);
        let choice = by_option_choice(&player);

        let before = crate::features::explicit_choice_features(&seen, &choice, &player, &[]);
        let _ = mlp_choice_features(
            &seen,
            &choice,
            &player,
            &[],
            crate::progress::Baseline::default(),
        );
        let after = crate::features::explicit_choice_features(&seen, &choice, &player, &[]);
        assert_eq!(before, after, "the extractor's output moved");

        // And the projection adds nothing to it: no `seat-state:` name is in the schema-4 vector.
        for vector in &after {
            for name in crate::features::names_of(vector) {
                assert!(
                    !name.starts_with(SEAT_STATE_FAMILY),
                    "{name}: the bare seat family leaked into the schema-4 vector"
                );
            }
        }
    }

    #[test]
    fn the_projection_keeps_state_kind_and_the_bounded_families() {
        // The predicate excludes a shape, not everything crossed. `state-kind` crosses on the
        // bounded canonical decision kind, so it transfers between games and stays.
        let content = ti4_content::ContentStore::embedded();
        let (state, player) = position();
        let seen = Observed::new(&state, content, POK, None);
        let mixed = Choice::new(
            player.clone(),
            "act",
            vec![
                ChoiceOption::labelled("move|x", "move", "move"),
                ChoiceOption::labelled("decline", "decline", "decline"),
            ],
        );
        let projected = mlp_choice_features(
            &seen,
            &mixed,
            &player,
            &[],
            crate::progress::Baseline::default(),
        );
        assert!(
            projected
                .iter()
                .flat_map(crate::features::names_of)
                .any(|name| name.starts_with("state-kind:")),
            "state-kind was suppressed; the predicate excludes a shape, not all crosses"
        );
    }

    #[test]
    fn opponent_secrets_do_not_survive_the_projection_either() {
        // The projection only ever removes names and adds public seat facts, so it cannot widen
        // the hidden-information boundary. Asserted rather than argued, since "it only removes"
        // is the kind of claim that stops being true when someone adds a branch.
        let content = ti4_content::ContentStore::embedded();
        let mut state = ti4_engine::fixtures::game(&["a", "b"]);
        state
            .player_mut(&PlayerId::new("b"))
            .unwrap()
            .secret_objectives = vec![ti4_model::id::SecretObjectiveId::new("mlp")];
        let player = PlayerId::new("a");
        let seen = Observed::new(&state, content, POK, None);
        let choice = by_option_choice(&player);
        let held = ti4_engine::choice::held_secret_progress(&state, content, POK, None, &player);

        for vector in &mlp_choice_features(
            &seen,
            &choice,
            &player,
            &held,
            crate::progress::Baseline::default(),
        ) {
            for name in crate::features::names_of(vector) {
                assert!(
                    !name.contains("mlp"),
                    "{name}: an opponent secret alias reached the MLP input",
                );
            }
        }
    }

    #[test]
    fn projecting_a_name_set_agrees_with_projecting_a_vector() {
        // M09-024b2 builds the vocabulary from projected *names* while the model is fed projected
        // *vectors*. If those two disagreed, the vocabulary would be missing columns the model
        // asks for, or holding columns nothing ever fills.
        let content = ti4_content::ContentStore::embedded();
        let (state, player) = position();
        let seen = Observed::new(&state, content, POK, None);
        let choice = by_option_choice(&player);

        let raw: Vec<String> =
            crate::features::explicit_choice_features(&seen, &choice, &player, &[])
                .iter()
                .flat_map(crate::features::names_of)
                .collect();
        let mut from_names = project_names(raw);
        from_names.extend(
            seat_state_facts(&seen, &player, crate::progress::Baseline::default())
                .into_iter()
                .map(|(n, _)| n),
        );

        let from_vectors: BTreeSet<String> = mlp_choice_features(
            &seen,
            &choice,
            &player,
            &[],
            crate::progress::Baseline::default(),
        )
        .iter()
        .flat_map(crate::features::names_of)
        .collect();

        assert!(!from_vectors.is_empty());
        assert_eq!(from_names, from_vectors);
    }

    #[test]
    fn the_classification_covers_exactly_the_registry() {
        // The forcing function for admission. If a family enters a grammar without a role, this
        // fails and the correct response is an architecture decision about whether it belongs in
        // the dense input — not adding it to the table to make the test green.
        let classified: BTreeSet<&str> = FAMILY_ROLES.iter().map(|(name, _)| *name).collect();
        let registered: BTreeSet<&str> =
            crate::vocabulary::oov_families().iter().copied().collect();
        assert_eq!(
            classified, registered,
            "a registered family has no MLP role, or a role names a family nobody registers. \
             Admission is an architecture decision: classify it deliberately, do not default it."
        );
        assert_eq!(FAMILY_ROLES.len(), 40, "one role per registered family");
    }

    #[test]
    fn an_unclassified_family_is_not_admitted() {
        // Closed by default. The earlier deny-list admitted anything unlisted, which is the
        // opposite of what the ruling requires of a new family.
        assert_eq!(role_of("no-such-family-was-ever-registered"), None);
        assert!(!admits("no-such-family-was-ever-registered:whatever"));
        assert!(!admits("brand-new-cross:a:b"));
        // And a name with no family separator at all.
        assert!(!admits("bare-unclassified-name"));
    }

    #[test]
    fn legacy_only_checkpoint_names_are_rejected() {
        // `kind-faction` and `option-faction` never occur on the schema-4 explicit path, but they
        // *do* occur in the r6 checkpoint, which is discovery source (a). Admitting them would
        // carry roughly 6,188 stale columns into the layout and could reproduce the contaminated
        // capacity instead of the corrected single-path one.
        for family in ["kind-faction", "option-faction"] {
            assert_eq!(role_of(family), Some(FamilyRole::LegacyOnly), "{family}");
        }
        let from_checkpoint = [
            "kind-faction:strategy_card:sol",
            "option-faction:pok2diplomacy:letnev",
            "prompt-bigram:choose:a:card",
            "prompt-option:starpoint:xanhact",
            "state-option:holy_planet_of_ixth:strategic_tokens",
        ];
        let kept = project_names(from_checkpoint);
        assert!(
            kept.is_empty(),
            "names no schema-4 decision can emit survived the projection: {kept:?}"
        );

        // Non-vacuity: the same call keeps a transferable name, so the assertion above is about
        // these families rather than about `project_names` rejecting everything.
        assert_eq!(
            project_names(["objective-met:sar"]).len(),
            1,
            "the projection rejected a transferable name"
        );
    }

    #[test]
    fn every_inactive_family_is_reported_and_every_other_is_live() {
        // Five, not three: the two legacy-only channels are as unreachable from the MLP runtime
        // path as the three crosses, and M09-026/M09-028 must zero and mask all five reserved rows.
        let inactive = inactive_families();
        assert_eq!(
            inactive.len(),
            5,
            "three crosses plus two legacy-only channels"
        );
        for family in [
            "prompt-bigram",
            "prompt-option",
            "state-option",
            "kind-faction",
            "option-faction",
        ] {
            assert!(
                inactive.contains(&family),
                "{family} is not reported inactive"
            );
            assert!(!admits(&format!("{family}:anything")));
        }
        for (family, role) in &FAMILY_ROLES {
            let listed = inactive.contains(family);
            assert_eq!(
                listed,
                *role != FamilyRole::Transferable,
                "{family} is misreported: role {role:?}, listed inactive {listed}"
            );
        }
    }

    #[test]
    fn every_approved_unit_family_resolves_and_no_other_does() {
        // F-M09-024b1-3. The five approved families share one registry entry and one role; an
        // unapproved `-unit` family is unclassified and therefore denied, because admission is a
        // pinned list rather than a suffix test. Checkpoint names are a discovery source, so a
        // suffix test would let an arbitrary historical family into the dense vocabulary.
        for family in APPROVED_UNIT_FAMILIES {
            assert_eq!(
                role_of(family),
                Some(FamilyRole::Transferable),
                "{family} is approved but unclassified"
            );
            assert!(admits(&format!("{family}:cost")), "{family} was denied");
        }
        for family in [
            "never-reviewed-unit",
            "sneaky-unit",
            "-unit",
            "faction-start-unit-unit",
        ] {
            assert_eq!(role_of(family), None, "{family} resolved a role");
            assert!(
                !admits(&format!("{family}:anything")),
                "{family} was admitted by suffix"
            );
        }
        // The fixed family that merely looks like one keeps its own role.
        assert_eq!(
            role_of("faction-start-unit"),
            Some(FamilyRole::Transferable)
        );
    }
}
