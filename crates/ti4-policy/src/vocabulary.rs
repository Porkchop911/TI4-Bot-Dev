//! The dense feature vocabulary: names to columns (MLP plan §4.5, decision D21).
//!
//! # Why this exists
//!
//! The linear model never needed a column index. A weight is looked up by [`FeatureKey`] in a map,
//! and a name never seen is a name with no weight — an absence that costs nothing. An MLP needs a
//! contiguous `[V, width]` matrix, so every name must map to a column and `V` must be fixed before
//! the first forward pass.
//!
//! # Why not the hashing trick
//!
//! `column = key % V` needs no vocabulary at all, and it is wrong here for a reason specific to
//! the MLP: in the linear model a wasted column costs one weight, and in the MLP it costs `width`
//! = 256. Sizing `V` so collisions are rare is what breaks it — at the 41,113 names the r6
//! champions hold, `2^18` still expects some 3,200 colliding pairs, and `2^24` buys ~50 at the
//! price of a 4.3-billion-parameter input layer. There is no setting that is both collision-free
//! and affordable, so the vocabulary is **enumerated**.
//!
//! # The ordering rule, and why it is the key rather than anything else
//!
//! Reserved out-of-vocabulary columns are allocated first, in a versioned family order, so their
//! indices never move. Every other name is assigned by ascending [`FeatureKey`]. Because the key
//! is a pure function of the name, two builds over the same set of names produce byte-identical
//! output regardless of the order the names arrived in — which is the property that lets a
//! discovery pass run in any order, on any number of threads.
//!
//! Reordering columns invalidates every weight and every Adam moment at once. That is why the
//! order is derived from the key and not from insertion, and why growth is append-only.
//!
//! # What this module does not do
//!
//! It does not discover names. Construction takes whatever names it is given; assembling the
//! corpus of names — including the §6.1 teacher-schedule replay — is M09-024b.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::intern::FeatureKey;

/// Version of the reserved out-of-vocabulary column layout.
///
/// Recorded in the manifest. Bumping it moves OOV column indices, which invalidates every weight
/// in a trained model — so it is a migration, never an edit.
pub const OOV_REGISTRY_VERSION: u32 = 1;

/// Physical capacity is rounded up to a multiple of this.
const CAPACITY_GRANULARITY: usize = 4_096;

/// Headroom over the assigned columns, so a later append has somewhere to go.
const CAPACITY_HEADROOM: f64 = 1.2;

/// The largest capacity this architecture will allocate without an explicit review.
///
/// MLP plan §4.5 fixes the expected upper bound. Exceeding it is not a thing to round up past: an
/// input layer is `V_cap × width` parameters, so the difference between 65,536 and "whatever the
/// corpus happened to need" is millions of weights nobody decided to train.
pub const CAPACITY_LIMIT: usize = 65_536;

/// The family portion of a feature name: everything before the first `:`.
///
/// `state-kind:activate:round` is in family `state-kind`; a name with no colon is its own family.
#[must_use]
pub fn family_of(name: &str) -> &str {
    name.split_once(':').map_or(name, |(family, _)| family)
}

/// The **frozen** version-1 reserved family order.
///
/// This list is data, not a derivation. An earlier draft computed it from `FEATURE_PREFIXES` and
/// `explicit_fixed_families()` and sorted the result, which made `OOV_REGISTRY_VERSION` decorative:
/// adding an ordinary feature family — something the last three packages each did — would insert
/// into the sorted order and shift every later reserved column, while the version still read 1. A
/// trained weight addressed to an old OOV index would then quietly mean something else.
///
/// So the order is written down. Adding a family is a **migration decision**, taken by bumping
/// [`OOV_REGISTRY_VERSION`] and writing a new list, not a side effect of editing a grammar. Until
/// that decision is taken, a new family's unseen names route to the global column, which is the
/// conservative direction. [`registry_matches_the_live_grammar`] fails loudly when the grammars
/// and this list disagree, so the decision cannot be skipped by accident.
const OOV_FAMILIES_V1: [&str; 38] = [
    "*-unit",
    "ability",
    "card",
    "destination",
    "faction-commodities",
    "faction-home",
    "faction-start-tech",
    "faction-start-unit",
    "faction-tech",
    "invasion",
    "kind",
    "kind-faction",
    "landing",
    "objective-count",
    "objective-met",
    "objective-need",
    "objective-progress",
    "objective-stage",
    "opponent-secrets-held",
    "option",
    "option-faction",
    "option-system",
    "origin",
    "pay",
    "payload",
    "payload-bool",
    "payload-count",
    "payload-number",
    "payload-number-kind",
    "placement",
    "production",
    "prompt-bigram",
    "prompt-kind",
    "prompt-option",
    "route",
    "state-kind",
    "state-option",
    "target",
];

/// The families that get a reserved OOV column, in the order they are allocated.
///
/// Returns the frozen list for the current [`OOV_REGISTRY_VERSION`]. It is not recomputed from the
/// grammars — see [`OOV_FAMILIES_V1`] for why that was wrong.
#[must_use]
pub fn oov_families() -> &'static [&'static str] {
    &OOV_FAMILIES_V1
}

/// The families the live grammars say should be registered, sorted.
///
/// Only for comparison against the frozen registry. Nothing addresses a column by this.
#[must_use]
pub fn live_grammar_families() -> Vec<String> {
    let mut families: BTreeSet<String> = crate::features::FEATURE_PREFIXES
        .iter()
        .map(|prefix| prefix.trim_end_matches(':').to_owned())
        .collect();
    families.extend(
        crate::features::explicit_fixed_families()
            .iter()
            .map(|family| (*family).to_owned()),
    );
    families.insert(UNIT_SUFFIX_FAMILY.to_owned());
    families.into_iter().collect()
}

/// The column the global OOV always occupies.
///
/// Guaranteed by construction and re-checked by [`Vocabulary::validate`], so a lookup that falls
/// all the way through has a defined destination rather than a hopeful `unwrap_or(0)`.
pub const GLOBAL_OOV_COLUMN: usize = 0;

/// Stands in for every `<canonical-kind>-unit` family at once.
///
/// The suffix rule is bounded but its left half varies with the choice kind, so the families
/// cannot be listed. They share one OOV column rather than being denied one.
pub const UNIT_SUFFIX_FAMILY: &str = "*-unit";

/// The reserved column for a name whose family is not registered at all.
pub const GLOBAL_OOV: &str = "oov:*";

/// The reserved column name for one family.
#[must_use]
pub fn oov_name(family: &str) -> String {
    format!("oov:{family}")
}

/// A name that could not be assigned a column, and why.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VocabularyError {
    /// Two different names hash to one key.
    ///
    /// Not resolved by an arbitrary tie-break and not silently aliased: two names sharing a column
    /// would sum into one weight, and the model would be quietly wrong in a way no gate reads.
    #[error("feature key collision: {first:?} and {second:?} share key {key}")]
    Collision {
        key: u64,
        first: String,
        second: String,
    },
    /// The vocabulary needs more columns than this architecture allocates without review.
    #[error(
        "vocabulary needs capacity {needed}, above the {CAPACITY_LIMIT} limit ({slots} assigned)"
    )]
    OverCapacity { needed: usize, slots: usize },
    /// An append would run past the allocated capacity.
    ///
    /// Capacity is raised by an explicit migration, never implicitly: reshaping the tensor is the
    /// one thing append-only growth exists to avoid.
    #[error("appending {adding} names exceeds capacity {capacity} ({slots} already assigned)")]
    AppendOverflow {
        adding: usize,
        slots: usize,
        capacity: usize,
    },
    /// The stored layout version is not one this build understands.
    #[error(
        "slots.json declares OOV registry version {found}, but this build supports {supported}"
    )]
    UnsupportedRegistry { found: u32, supported: u32 },
    /// A reserved column is not the one the registry says belongs at that index.
    #[error("reserved column {column}: expected {expected:?}, found {found:?}")]
    ReservedLayout {
        column: usize,
        expected: String,
        found: String,
    },
    /// The stored capacity is not the capacity the sizing rule gives for these slots.
    #[error("capacity {stored} is not the {expected} the rule gives for {slots} slots")]
    CapacityMismatch {
        stored: usize,
        expected: usize,
        slots: usize,
    },
    /// A stored key is not the key of the name beside it.
    ///
    /// Either the file was edited or the key function changed. Both mean every column addresses
    /// something other than what it says it does, so it is refused rather than recomputed.
    #[error("slot {name:?} stores key {stored} but its name hashes to {computed}")]
    KeyMismatch {
        name: String,
        stored: u64,
        computed: u64,
    },
}

/// Why a stored vocabulary could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// The bytes are not a vocabulary.
    #[error("slots.json is malformed: {0}")]
    Json(#[source] serde_json::Error),
    /// The bytes parse but do not satisfy [`Vocabulary::validate`].
    #[error("slots.json is invalid: {0}")]
    Invalid(#[source] VocabularyError),
}

/// One assigned column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slot {
    /// The feature name, as UTF-8.
    pub name: String,
    /// Its key. Stored beside the name so a loader can verify the key function has not changed
    /// under the vocabulary rather than trusting that it has not.
    pub key: u64,
}

/// Names to dense columns: the vocabulary itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vocabulary {
    /// Layout version of the reserved OOV columns.
    ///
    /// Fields are private and the accessors are read-only on purpose. Every invariant this type
    /// carries — the reserved prefix, the capacity rule, key/name agreement — is established at
    /// construction or at [`Self::validate`], and a `pub` field would let a caller undo any of
    /// them afterwards without passing through either.
    oov_registry_version: u32,
    /// How many leading columns are reserved OOV columns.
    oov_count: usize,
    /// Every assigned column, in index order. Index `i` is `slots[i]`.
    slots: Vec<Slot>,
    /// Physical rows allocated in the model tensor. `slots.len() <= capacity`.
    capacity: usize,
    /// Column index by key, for lookup. Rebuilt on load rather than stored.
    #[serde(skip)]
    index: BTreeMap<FeatureKey, usize>,
}

impl Vocabulary {
    /// Build a vocabulary over `names`.
    ///
    /// Reserved OOV columns come first in the versioned family order; every other name follows by
    /// ascending [`FeatureKey`]. The iteration order of `names` does not reach the output — that
    /// is the whole point, and `deterministic_under_reversed_input` pins it.
    ///
    /// # Errors
    /// [`VocabularyError::Collision`] if two distinct names share a key;
    /// [`VocabularyError::OverCapacity`] if the required capacity exceeds [`CAPACITY_LIMIT`].
    pub fn build<I, S>(names: I) -> Result<Self, VocabularyError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let reserved = oov_families();
        let mut ordered: BTreeMap<FeatureKey, String> = BTreeMap::new();
        let mut slots: Vec<Slot> = Vec::with_capacity(reserved.len() + 1);

        // The global OOV first, then one per registered family. These indices are load-bearing:
        // a trained model's weights are addressed by them, so they are allocated before anything
        // a corpus could vary.
        let push_reserved = |slots: &mut Vec<Slot>,
                             ordered: &mut BTreeMap<FeatureKey, String>,
                             name: String|
         -> Result<(), VocabularyError> {
            let key = FeatureKey::of(&name);
            if let Some(existing) = ordered.get(&key) {
                return Err(VocabularyError::Collision {
                    key: key.bits(),
                    first: existing.clone(),
                    second: name,
                });
            }
            ordered.insert(key, name.clone());
            slots.push(Slot {
                name,
                key: key.bits(),
            });
            Ok(())
        };
        push_reserved(&mut slots, &mut ordered, GLOBAL_OOV.to_owned())?;
        for family in reserved {
            push_reserved(&mut slots, &mut ordered, oov_name(family))?;
        }
        let oov_count = slots.len();

        // Everything else, keyed. A `BTreeMap` over the key *is* the ordering rule.
        let mut assigned: BTreeMap<FeatureKey, String> = BTreeMap::new();
        for name in names {
            let name = name.as_ref();
            let key = FeatureKey::of(name);
            if let Some(existing) = ordered.get(&key).or_else(|| assigned.get(&key)) {
                if existing != name {
                    return Err(VocabularyError::Collision {
                        key: key.bits(),
                        first: existing.clone(),
                        second: name.to_owned(),
                    });
                }
                continue;
            }
            assigned.insert(key, name.to_owned());
        }
        for (key, name) in assigned {
            slots.push(Slot {
                name,
                key: key.bits(),
            });
        }

        let capacity = capacity_for(slots.len())?;
        let mut vocabulary = Self {
            oov_registry_version: OOV_REGISTRY_VERSION,
            oov_count,
            slots,
            capacity,
            index: BTreeMap::new(),
        };
        vocabulary.reindex();
        Ok(vocabulary)
    }

    /// Rebuild the key-to-column map. Called after construction and after loading.
    ///
    /// Private: it is not a repair, and calling it on a vocabulary whose slots were changed behind
    /// the type's back would produce a consistent index over an invalid layout.
    fn reindex(&mut self) {
        self.index = self
            .slots
            .iter()
            .enumerate()
            .map(|(column, slot)| (FeatureKey::from_bits(slot.key), column))
            .collect();
    }

    /// How many columns are assigned. The logical size.
    #[must_use]
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    /// The reserved-layout version this vocabulary was built under.
    #[must_use]
    pub const fn oov_registry_version(&self) -> u32 {
        self.oov_registry_version
    }

    /// How many leading columns are reserved OOV columns.
    #[must_use]
    pub const fn oov_count(&self) -> usize {
        self.oov_count
    }

    /// Every assigned column, in index order.
    #[must_use]
    pub fn slots(&self) -> &[Slot] {
        &self.slots
    }

    /// Rows allocated in the tensor. The physical size.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Rows allocated but not yet assigned. These and their Adam moments are zero.
    #[must_use]
    pub const fn free_rows(&self) -> usize {
        self.capacity - self.slots.len()
    }

    /// The column a name contributes to.
    ///
    /// An unassigned name is **not dropped**. It contributes to its family's OOV column, or to the
    /// global OOV if the family is unregistered. Dropping silently would make an unknown
    /// `option:` word indistinguishable from its absence — exactly the case where the policy
    /// should be uncertain rather than confident.
    #[must_use]
    pub fn column_of(&self, name: &str) -> usize {
        if let Some(column) = self.index.get(&FeatureKey::of(name)) {
            return *column;
        }
        let family = family_of(name);
        let reserved = if family.ends_with("-unit") {
            oov_name(UNIT_SUFFIX_FAMILY)
        } else {
            oov_name(family)
        };
        // The global column is at a known index, guaranteed at construction and re-checked by
        // `validate`, so a lookup that falls all the way through lands somewhere defined rather
        // than on a hopeful `unwrap_or(0)` that would silently alias column 0 to whatever happened
        // to be there.
        self.index
            .get(&FeatureKey::of(&reserved))
            .copied()
            .unwrap_or(GLOBAL_OOV_COLUMN)
    }

    /// Whether a name has a column of its own rather than falling back to an OOV column.
    #[must_use]
    pub fn is_assigned(&self, name: &str) -> bool {
        self.index.contains_key(&FeatureKey::of(name))
    }

    /// Append newly discovered names into unused preallocated rows.
    ///
    /// Append-only: existing columns are never reordered or reused, so every trained weight and
    /// every Adam moment keeps its meaning. New names are assigned in ascending [`FeatureKey`]
    /// **within this batch**, which makes the result a function of the batch's contents rather
    /// than of the order workers reported them. Returns how many columns were added.
    ///
    /// # Errors
    /// [`VocabularyError::AppendOverflow`] if the batch would run past [`Self::capacity`], and
    /// [`VocabularyError::Collision`] if a new name collides with an assigned one.
    pub fn append<I, S>(&mut self, names: I) -> Result<usize, VocabularyError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut batch: BTreeMap<FeatureKey, String> = BTreeMap::new();
        for name in names {
            let name = name.as_ref();
            let key = FeatureKey::of(name);
            if let Some(column) = self.index.get(&key) {
                if self.slots[*column].name != name {
                    return Err(VocabularyError::Collision {
                        key: key.bits(),
                        first: self.slots[*column].name.clone(),
                        second: name.to_owned(),
                    });
                }
                continue;
            }
            if let Some(existing) = batch.get(&key) {
                if existing != name {
                    return Err(VocabularyError::Collision {
                        key: key.bits(),
                        first: existing.clone(),
                        second: name.to_owned(),
                    });
                }
                continue;
            }
            batch.insert(key, name.to_owned());
        }
        if batch.is_empty() {
            return Ok(0);
        }
        if self.slots.len() + batch.len() > self.capacity {
            return Err(VocabularyError::AppendOverflow {
                adding: batch.len(),
                slots: self.slots.len(),
                capacity: self.capacity,
            });
        }
        let added = batch.len();
        let first_new = self.slots.len();
        for (key, name) in batch {
            self.slots.push(Slot {
                name,
                key: key.bits(),
            });
        }
        for (offset, slot) in self.slots[first_new..].iter().enumerate() {
            self.index
                .insert(FeatureKey::from_bits(slot.key), first_new + offset);
        }
        Ok(added)
    }

    /// Serialize to the canonical `slots.json` bytes.
    ///
    /// Pretty-printed with a trailing newline so a diff of two builds is readable, and so the
    /// byte-identity check compares something a human can inspect when it fails.
    ///
    /// # Errors
    /// Propagates any `serde_json` failure.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        Ok(text)
    }

    /// Load from `slots.json` bytes, rebuilding the lookup index and checking it.
    ///
    /// # Errors
    /// [`LoadError::Json`] for malformed input, or [`LoadError::Invalid`] if the file does not
    /// satisfy [`Self::validate`].
    pub fn from_json(text: &str) -> Result<Self, LoadError> {
        let mut vocabulary: Self = serde_json::from_str(text).map_err(LoadError::Json)?;
        vocabulary.reindex();
        vocabulary.validate().map_err(LoadError::Invalid)?;
        Ok(vocabulary)
    }

    /// Check the invariants a `slots.json` must satisfy before anything trusts its columns.
    ///
    /// A vocabulary this module built satisfies these by construction. A vocabulary read off disk
    /// has not been built by this module — it has been *stored*, possibly by another version, and
    /// the properties every trained weight depends on are worth one pass to confirm rather than to
    /// assume:
    ///
    /// * every stored key is the key of its own name, so a change to the key function is caught
    ///   here rather than by a model that silently addresses the wrong columns;
    /// * no two columns share a key, since two names summing into one weight is wrong in a way no
    ///   downstream gate reads; and
    /// * the assigned columns fit the recorded capacity.
    ///
    /// # Errors
    /// [`VocabularyError::Collision`], [`VocabularyError::KeyMismatch`], or
    /// [`VocabularyError::AppendOverflow`] when the assigned columns exceed capacity.
    pub fn validate(&self) -> Result<(), VocabularyError> {
        // 1. Fail closed on a layout this build does not know. An unrecognised version means the
        //    reserved columns below are somebody else's, and nothing here can tell which.
        if self.oov_registry_version != OOV_REGISTRY_VERSION {
            return Err(VocabularyError::UnsupportedRegistry {
                found: self.oov_registry_version,
                supported: OOV_REGISTRY_VERSION,
            });
        }

        // 2. The reserved prefix is checked element by element, not by length. A reordered or
        //    substituted reserved column is exactly the corruption that would silently re-point
        //    every trained OOV weight, and it preserves the count.
        let families = oov_families();
        if self.oov_count != families.len() + 1 {
            return Err(VocabularyError::ReservedLayout {
                column: self.oov_count,
                expected: format!("{} reserved columns", families.len() + 1),
                found: format!("{}", self.oov_count),
            });
        }
        if self.slots.len() < self.oov_count {
            return Err(VocabularyError::ReservedLayout {
                column: self.slots.len(),
                expected: format!("at least {} columns", self.oov_count),
                found: format!("{}", self.slots.len()),
            });
        }
        if self.slots[GLOBAL_OOV_COLUMN].name != GLOBAL_OOV {
            return Err(VocabularyError::ReservedLayout {
                column: GLOBAL_OOV_COLUMN,
                expected: GLOBAL_OOV.to_owned(),
                found: self.slots[GLOBAL_OOV_COLUMN].name.clone(),
            });
        }
        for (offset, family) in families.iter().enumerate() {
            let column = offset + 1;
            let expected = oov_name(family);
            if self.slots[column].name != expected {
                return Err(VocabularyError::ReservedLayout {
                    column,
                    expected,
                    found: self.slots[column].name.clone(),
                });
            }
        }

        // 3. Capacity is not a free field. It is a function of the assigned count, and it carries
        //    the granularity and the architecture limit with it.
        let expected_capacity = capacity_for(self.slots.len())?;
        if self.capacity != expected_capacity {
            return Err(VocabularyError::CapacityMismatch {
                stored: self.capacity,
                expected: expected_capacity,
                slots: self.slots.len(),
            });
        }

        let mut seen: BTreeMap<FeatureKey, &str> = BTreeMap::new();
        for slot in &self.slots {
            let computed = FeatureKey::of(&slot.name);
            if computed.bits() != slot.key {
                return Err(VocabularyError::KeyMismatch {
                    name: slot.name.clone(),
                    stored: slot.key,
                    computed: computed.bits(),
                });
            }
            // Two columns on one key is a defect whether the names differ or not: distinct names
            // would sum into one weight, and a repeated name means two columns claim to be the
            // same feature. Neither is recoverable by picking one.
            if let Some(first) = seen.insert(computed, &slot.name) {
                return Err(VocabularyError::Collision {
                    key: slot.key,
                    first: first.to_owned(),
                    second: slot.name.clone(),
                });
            }
        }
        if self.slots.len() > self.capacity {
            return Err(VocabularyError::AppendOverflow {
                adding: 0,
                slots: self.slots.len(),
                capacity: self.capacity,
            });
        }
        Ok(())
    }
}

/// Physical capacity for `slots` assigned columns: the next multiple of 4,096 at or above
/// `1.2 × slots`.
///
/// # Errors
/// [`VocabularyError::OverCapacity`] above [`CAPACITY_LIMIT`] — the package stops for an explicit
/// architecture review rather than silently allocating a larger model.
pub fn capacity_for(slots: usize) -> Result<usize, VocabularyError> {
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "slot counts are far below 2^53 and the result is rounded up anyway"
    )]
    let wanted = (slots as f64 * CAPACITY_HEADROOM).ceil() as usize;
    let needed = wanted.div_ceil(CAPACITY_GRANULARITY) * CAPACITY_GRANULARITY;
    if needed > CAPACITY_LIMIT {
        return Err(VocabularyError::OverCapacity { needed, slots });
    }
    Ok(needed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small, realistic name set: two legacy families, two M09-021/022/023 families, and one
    /// name in no registered family at all.
    fn sample() -> Vec<String> {
        [
            "kind:activate",
            "option:mecatol",
            "objective-need:planets:4",
            "ability:versatile",
            "faction-commodities",
            "opponent-secrets-held:2",
            "commit-unit:cost",
            "wholly-unregistered:thing",
        ]
        .iter()
        .map(|name| (*name).to_owned())
        .collect()
    }

    #[test]
    fn reserved_columns_come_first_and_do_not_move_when_the_corpus_changes() {
        // The load-bearing property of the whole layout: a trained model addresses OOV columns by
        // index, so those indices must be a function of the registry version and nothing else.
        // Two vocabularies over different corpora must agree on every reserved column.
        let small = Vocabulary::build(sample()).expect("builds");
        let large = Vocabulary::build(
            sample()
                .into_iter()
                .chain((0..500).map(|n| format!("option:filler{n}"))),
        )
        .expect("builds");

        assert!(small.oov_count > 1, "the registry is not empty");
        assert_eq!(small.oov_count, large.oov_count);
        assert_eq!(
            small.slots[..small.oov_count],
            large.slots[..large.oov_count],
            "reserved columns moved when the corpus grew"
        );
        assert_eq!(
            small.slots[0].name, GLOBAL_OOV,
            "the global OOV is column 0"
        );
        assert!(
            large.slot_count() > small.slot_count(),
            "the fixture must actually differ in size"
        );
    }

    #[test]
    fn assignment_is_deterministic_under_reversed_input() {
        // MLP plan section 4.5 requires the construction to run twice over reversed input and
        // produce byte-identical output. Comparing the serialized bytes rather than the structure
        // is deliberate: `slots.json` is what a checkpoint hashes.
        let forward = Vocabulary::build(sample()).expect("builds");
        let mut reversed_names = sample();
        reversed_names.reverse();
        let reversed = Vocabulary::build(reversed_names).expect("builds");

        assert_eq!(
            forward.to_json().expect("json"),
            reversed.to_json().expect("json"),
            "input order reached the output"
        );
        // Non-vacuity: the two inputs really were different orders.
        let mut names = sample();
        names.reverse();
        assert_ne!(
            names,
            sample(),
            "the fixture must be order-sensitive to begin with"
        );
    }

    #[test]
    fn names_are_assigned_in_ascending_key_order_after_the_reserved_block() {
        let vocabulary = Vocabulary::build(sample()).expect("builds");
        let assigned = &vocabulary.slots[vocabulary.oov_count..];
        assert_eq!(assigned.len(), sample().len(), "every name got a column");
        let keys: Vec<u64> = assigned.iter().map(|slot| slot.key).collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(keys, sorted, "assignment is not in ascending key order");
    }

    #[test]
    fn unseen_names_reach_their_family_oov_then_the_global_one() {
        // Section 4.5: an unseen name is not dropped. Dropping would make an unknown `option:`
        // word indistinguishable from its absence — the case where the policy should be uncertain
        // rather than confident.
        let vocabulary = Vocabulary::build(sample()).expect("builds");

        let known = vocabulary.column_of("option:mecatol");
        assert!(vocabulary.is_assigned("option:mecatol"));
        assert!(
            known >= vocabulary.oov_count,
            "an assigned name used an OOV column"
        );

        let unseen_in_family = vocabulary.column_of("option:never-seen-before");
        assert!(!vocabulary.is_assigned("option:never-seen-before"));
        assert_eq!(
            unseen_in_family,
            vocabulary.column_of(&oov_name("option")),
            "an unseen name missed its family OOV"
        );
        assert_ne!(unseen_in_family, known, "it landed on the known column");

        // The bounded `<kind>-unit` suffix rule shares one column, since the left half varies.
        assert_eq!(
            vocabulary.column_of("commit-unit:sustained"),
            vocabulary.column_of(&oov_name(UNIT_SUFFIX_FAMILY)),
            "a suffix-rule family missed the shared OOV"
        );

        // A family nobody registered falls all the way through to the global column.
        assert_eq!(
            vocabulary.column_of("no-such-family:at-all"),
            vocabulary.column_of(GLOBAL_OOV),
            "an unregistered family missed the global OOV"
        );
    }

    #[test]
    fn capacity_is_the_next_multiple_of_4096_above_a_fifth_more_than_the_slots() {
        assert_eq!(capacity_for(1).expect("small"), 4_096);
        assert_eq!(
            capacity_for(4_096).expect("exact"),
            8_192,
            "1.2x pushes past the boundary"
        );
        // The r6 champions hold 41,113 names; with the reserved block that is what this
        // architecture will actually allocate. Recorded here so the number is pinned rather than
        // recomputed by hand later.
        let realistic = 41_113
            + Vocabulary::build(Vec::<String>::new())
                .expect("builds")
                .oov_count;
        let allocated = capacity_for(realistic).expect("under the limit");
        assert_eq!(allocated, 53_248);
        assert!(
            allocated < CAPACITY_LIMIT,
            "the r6 corpus fits without a review"
        );
    }

    #[test]
    fn a_vocabulary_too_large_to_allocate_stops_rather_than_rounding_up() {
        // Section 4.5: above the limit the package stops for an explicit architecture review
        // rather than silently allocating a larger model.
        let error = capacity_for(60_000).expect_err("60,000 slots needs 72,000 rows");
        match error {
            VocabularyError::OverCapacity { needed, slots } => {
                assert_eq!(slots, 60_000);
                assert!(needed > CAPACITY_LIMIT, "the fixture must exceed the limit");
            }
            other => panic!("wrong error: {other:?}"),
        }
    }

    #[test]
    fn append_is_key_ordered_within_the_batch_and_moves_nothing() {
        let mut vocabulary = Vocabulary::build(sample()).expect("builds");
        let before = vocabulary.slots.clone();
        let capacity_before = vocabulary.capacity();

        let added = vocabulary
            .append(["option:zeta", "option:alpha", "kind:brand-new"])
            .expect("fits");
        assert_eq!(added, 3);
        assert_eq!(
            vocabulary.slots[..before.len()],
            before[..],
            "an existing column moved"
        );
        assert_eq!(
            vocabulary.capacity(),
            capacity_before,
            "capacity is not raised implicitly"
        );

        let appended: Vec<u64> = vocabulary.slots[before.len()..]
            .iter()
            .map(|slot| slot.key)
            .collect();
        let mut sorted = appended.clone();
        sorted.sort_unstable();
        assert_eq!(appended, sorted, "the batch was not key-ordered");

        // Idempotent: appending what is already assigned adds nothing.
        assert_eq!(vocabulary.append(["option:zeta"]).expect("no-op"), 0);
        assert!(vocabulary.is_assigned("option:zeta"));
    }

    #[test]
    fn appending_past_capacity_is_refused_rather_than_reshaping() {
        let mut vocabulary = Vocabulary::build(sample()).expect("builds");
        let room = vocabulary.free_rows();
        assert!(room > 0, "the fixture must have room to overflow");
        let too_many: Vec<String> = (0..=room).map(|n| format!("option:overflow{n}")).collect();

        let error = vocabulary
            .append(too_many.clone())
            .expect_err("one more than fits");
        match error {
            VocabularyError::AppendOverflow {
                adding, capacity, ..
            } => {
                assert_eq!(adding, room + 1);
                assert_eq!(capacity, vocabulary.capacity());
            }
            other => panic!("wrong error: {other:?}"),
        }
        // Refused, not partially applied.
        assert_eq!(
            vocabulary.free_rows(),
            room,
            "a refused append still changed the vocabulary"
        );

        // And exactly filling it is allowed, so the boundary is off-by-one-proof.
        let exact: Vec<String> = too_many.into_iter().take(room).collect();
        assert_eq!(vocabulary.append(exact).expect("exact fit"), room);
        assert_eq!(vocabulary.free_rows(), 0);
    }

    #[test]
    fn a_stored_file_with_two_columns_on_one_key_is_refused() {
        // A real 64-bit FNV-1a collision cannot be constructed in a test, so the collision branch
        // is reached the way it is actually reachable end to end: a `slots.json` that claims one.
        // The loader must refuse it rather than pick a column, because two names summing into one
        // weight is wrong in a way no downstream gate reads.
        let mut vocabulary = Vocabulary::build(sample()).expect("builds");
        let victim = vocabulary.slots[vocabulary.oov_count].clone();
        vocabulary.slots.push(Slot {
            name: victim.name.clone(),
            key: victim.key,
        });

        let text = vocabulary.to_json().expect("json");
        let error = Vocabulary::from_json(&text).expect_err("a duplicated key must be refused");
        assert!(
            matches!(error, LoadError::Invalid(VocabularyError::Collision { .. })),
            "wrong error: {error}"
        );
    }

    #[test]
    fn a_stored_key_that_does_not_match_its_name_is_refused() {
        // Guards the promise `Slot::key` makes. If the key function ever changes under a stored
        // vocabulary, every column addresses something other than what it says it does; that is
        // caught here rather than by a model quietly reading the wrong weights.
        let mut vocabulary = Vocabulary::build(sample()).expect("builds");
        vocabulary.slots[vocabulary.oov_count].key ^= 1;

        let text = vocabulary.to_json().expect("json");
        let error = Vocabulary::from_json(&text).expect_err("a wrong key must be refused");
        assert!(
            matches!(
                error,
                LoadError::Invalid(VocabularyError::KeyMismatch { .. })
            ),
            "wrong error: {error}"
        );
    }

    #[test]
    fn a_round_trip_through_json_preserves_every_column_and_its_lookups() {
        let vocabulary = Vocabulary::build(sample()).expect("builds");
        let loaded = Vocabulary::from_json(&vocabulary.to_json().expect("json")).expect("loads");
        assert_eq!(loaded.slots, vocabulary.slots);
        assert_eq!(loaded.capacity(), vocabulary.capacity());
        assert_eq!(loaded.oov_count, vocabulary.oov_count);
        // The index is rebuilt rather than stored, so lookups must survive the trip.
        for name in sample() {
            assert_eq!(loaded.column_of(&name), vocabulary.column_of(&name));
        }
        assert_eq!(
            loaded.column_of("option:never-seen"),
            vocabulary.column_of("option:never-seen")
        );
    }

    #[test]
    fn every_registered_family_has_exactly_one_reserved_column() {
        let vocabulary = Vocabulary::build(Vec::<String>::new()).expect("builds");
        let families = oov_families();
        assert_eq!(
            vocabulary.oov_count,
            families.len() + 1,
            "the reserved block is not the registry plus the global column"
        );
        for family in families {
            let column = vocabulary.column_of(&oov_name(family));
            assert!(
                column < vocabulary.oov_count,
                "{family} has no reserved column"
            );
        }
    }

    #[test]
    fn the_frozen_registry_matches_the_live_grammar() {
        // F-M09-024a-1. The registry is frozen data, so it can fall behind the grammars it was
        // written from — and falling behind silently is the whole failure mode: a new family whose
        // unseen names quietly pool into the global column, or worse, a "fix" that re-derives the
        // order and moves every reserved index under a version that still reads 1.
        //
        // This test is the forcing function. When it fails, the correct response is a migration —
        // bump OOV_REGISTRY_VERSION and write a new frozen list — not an edit to OOV_FAMILIES_V1
        // in place, which would move existing columns under a version promising they never move.
        let frozen: Vec<String> = oov_families().iter().map(|f| (*f).to_owned()).collect();
        let live = live_grammar_families();
        assert_eq!(
            frozen, live,
            "the feature grammars and the frozen OOV registry disagree. Do not edit \
             OOV_FAMILIES_V1 in place: that moves reserved columns under a version that promises \
             they never move. Bump OOV_REGISTRY_VERSION and add a new frozen list."
        );
    }

    #[test]
    fn a_stored_file_from_an_unknown_registry_version_is_refused() {
        let mut vocabulary = Vocabulary::build(sample()).expect("builds");
        vocabulary.oov_registry_version = OOV_REGISTRY_VERSION + 1;
        let error = Vocabulary::from_json(&vocabulary.to_json().expect("json"))
            .expect_err("an unknown layout must be refused");
        assert!(
            matches!(
                error,
                LoadError::Invalid(VocabularyError::UnsupportedRegistry { .. })
            ),
            "wrong error: {error}"
        );
    }

    #[test]
    fn a_reordered_reserved_prefix_is_refused_even_though_the_count_is_right() {
        // The count-preserving corruption: swap two reserved columns. Every trained OOV weight
        // would silently change meaning, and a length check would not notice.
        let mut vocabulary = Vocabulary::build(sample()).expect("builds");
        vocabulary.slots.swap(1, 2);
        let error = Vocabulary::from_json(&vocabulary.to_json().expect("json"))
            .expect_err("a reordered reserved prefix must be refused");
        assert!(
            matches!(
                error,
                LoadError::Invalid(VocabularyError::ReservedLayout { .. })
            ),
            "wrong error: {error}"
        );
    }

    #[test]
    fn a_missing_global_oov_column_is_refused() {
        let mut vocabulary = Vocabulary::build(sample()).expect("builds");
        let stolen = vocabulary.slots[vocabulary.oov_count].clone();
        vocabulary.slots[GLOBAL_OOV_COLUMN] = stolen;
        let error = Vocabulary::from_json(&vocabulary.to_json().expect("json"))
            .expect_err("a missing global OOV must be refused");
        match error {
            LoadError::Invalid(VocabularyError::ReservedLayout { column, .. }) => {
                assert_eq!(column, GLOBAL_OOV_COLUMN);
            }
            other => panic!("wrong error: {other}"),
        }
    }

    #[test]
    fn a_wrong_reserved_count_is_refused() {
        let mut vocabulary = Vocabulary::build(sample()).expect("builds");
        vocabulary.oov_count += 1;
        let error = Vocabulary::from_json(&vocabulary.to_json().expect("json"))
            .expect_err("a wrong reserved count must be refused");
        assert!(
            matches!(
                error,
                LoadError::Invalid(VocabularyError::ReservedLayout { .. })
            ),
            "wrong error: {error}"
        );
    }

    #[test]
    fn a_stored_capacity_that_the_rule_does_not_give_is_refused() {
        // Three ways to get a wrong capacity, all refused: not a multiple of the granularity,
        // above the architecture limit, and simply not the value the rule produces.
        for bad in [4_000_usize, 70_000, 8_192] {
            let mut vocabulary = Vocabulary::build(sample()).expect("builds");
            assert_ne!(vocabulary.capacity, bad, "the fixture must actually differ");
            vocabulary.capacity = bad;
            let error = Vocabulary::from_json(&vocabulary.to_json().expect("json"))
                .expect_err("a wrong capacity must be refused");
            assert!(
                matches!(
                    error,
                    LoadError::Invalid(
                        VocabularyError::CapacityMismatch { .. }
                            | VocabularyError::OverCapacity { .. }
                    )
                ),
                "wrong error for capacity {bad}: {error}"
            );
        }
    }
}
