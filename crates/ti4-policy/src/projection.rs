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

/// Families excluded from the dense input under the current grammar.
///
/// The answer to the predicate, not the predicate. See [`is_unbounded_cross`].
pub const EXCLUDED_FAMILIES: [&str; 3] = ["prompt-bigram", "prompt-option", "state-option"];

/// Whether a family is an unbounded memorisation cross.
///
/// The three names are the current grammar's members. A family added later with either shape —
/// two free lexical identities crossed, or a full option identity crossed with a state fact — is
/// excluded by default and requires an architecture review before it may be admitted, which is
/// why `is_unbounded_cross` is consulted at one place and `EXCLUDED_FAMILIES` is not spread
/// around the crate.
#[must_use]
pub fn is_unbounded_cross(family: &str) -> bool {
    EXCLUDED_FAMILIES.contains(&family)
}

/// Whether a feature name survives the projection into the dense input.
#[must_use]
pub fn admits(name: &str) -> bool {
    !is_unbounded_cross(crate::vocabulary::family_of(name))
}

/// The acting-seat facts under the bare family, for one position.
///
/// Option-invariant by construction — the same eight values on every option of a choice — which is
/// the point: MLP plan §4.1's nonlinear per-option trunk can let them interact with option facts,
/// where a linear head would see a constant and ignore them.
#[must_use]
pub fn seat_state_facts(seen: &Observed<'_>, player: &PlayerId) -> Vec<(String, f64)> {
    seat_facts(seen, player)
        .into_iter()
        .map(|(name, value)| (format!("{SEAT_STATE_FAMILY}:{name}"), value))
        .collect()
}

/// Project one already-extracted vector into the MLP input.
///
/// Suppression happens here, before any vocabulary lookup, so an excluded name never reaches a
/// column at all.
fn project_vector(vector: &FeatureVector, seat_state: &[(String, f64)]) -> FeatureVector {
    let kept = vector
        .iter()
        .filter(|(key, _)| admits(&crate::intern::name_of(**key)))
        .map(|(key, value)| (*key, *value));
    // The bare seat facts are a restatement of a position fact, not a second contribution to an
    // existing column: their family is disjoint from everything the extractor emits, so the
    // duplicate-summing in `from_pairs` cannot reach them.
    // `register`, not `FeatureKey::of`: the key alone puts a value in the vector but leaves it
    // nameless, so `names_of` resolves it to an empty string and the discovery pass that builds
    // the vocabulary from names would never see it. The agreement test between projected names
    // and projected vectors is what catches that.
    let added = seat_state
        .iter()
        .map(|(name, value)| (crate::intern::register(name), *value));
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
) -> Vec<FeatureVector> {
    let seat_state = seat_state_facts(seen, player);
    crate::features::explicit_choice_features(seen, choice, player, held_secrets)
        .iter()
        .map(|vector| project_vector(vector, &seat_state))
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
) -> FeatureVector {
    let seat_state = seat_state_facts(seen, player);
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

        let after = mlp_choice_features(&seen, &choice, &player, &[]);
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
        // choice on `ByOption`, where the eight facts previously rode `state-option:` alone; the
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

        let expected = seat_state_facts(&seen, &player);
        assert_eq!(expected.len(), 8, "all eight acting-seat facts");
        // Non-vacuity: at least one of them is non-zero in this position, so an all-zero vector
        // could not pass the comparison below.
        assert!(
            expected.iter().any(|(_, value)| *value != 0.0),
            "the fixture position has no non-zero seat fact"
        );

        for vector in &mlp_choice_features(&seen, &choice, &player, &[]) {
            for (name, value) in &expected {
                assert_eq!(
                    crate::features::value_of(vector, name),
                    Some(*value),
                    "{name} is missing from a ByOption option after projection"
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

        let expected = seat_state_facts(&seen, &player);
        for choice in [&mixed, &none] {
            for vector in &mlp_choice_features(&seen, choice, &player, &[]) {
                for (name, value) in &expected {
                    assert_eq!(
                        crate::features::value_of(vector, name),
                        Some(*value),
                        "{name} missing under {:?}",
                        crate::features::state_cross(choice)
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
        let _ = mlp_choice_features(&seen, &choice, &player, &[]);
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
        let projected = mlp_choice_features(&seen, &mixed, &player, &[]);
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

        for vector in &mlp_choice_features(&seen, &choice, &player, &held) {
            for name in crate::features::names_of(vector) {
                assert!(
                    !name.contains("mlp"),
                    "{name}: an opponent secret alias reached the MLP input"
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
        from_names.extend(seat_state_facts(&seen, &player).into_iter().map(|(n, _)| n));

        let from_vectors: BTreeSet<String> = mlp_choice_features(&seen, &choice, &player, &[])
            .iter()
            .flat_map(crate::features::names_of)
            .collect();

        assert!(!from_vectors.is_empty());
        assert_eq!(from_names, from_vectors);
    }
}
