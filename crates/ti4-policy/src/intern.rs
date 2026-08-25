//! Feature names as 64-bit keys, with a side table for recovering the name.
//!
//! # Why
//!
//! A feature vector used to be `BTreeMap<String, f64>`. Every entry cost a heap allocation for
//! its key and every lookup cost a string comparison at each level of the tree. The names come
//! from a set that stops growing early — a converged Stage-2 checkpoint holds about 47,000
//! distinct names per faction — while the *instances* number in the hundreds of millions per
//! run. Naming each one once and then working with integers is the shape that fits.
//!
//! # Why a hash and not a counter
//!
//! The obvious design is a counter: hand out 0, 1, 2… as names are first seen. It is rejected
//! here because the resulting order depends on **which seeds a run happened to play first**, so
//! two runs of the same configuration would accumulate their gradient sums in different orders
//! and drift apart in the low bits for no reason anyone could reconstruct.
//!
//! A hash of the name has none of that: [`FeatureKey`] is a pure function of the name, so the
//! iteration order of a `BTreeMap<FeatureKey, _>` is fixed by the corpus rather than by history,
//! and two runs of the same configuration stay comparable. It also needs no lock, no shared
//! counter and no coordination between the thirty-two rollout workers on the hot path.
//!
//! # What this changes about results
//!
//! Hash order is not alphabetical order. Floating-point addition is not associative, so the
//! gradient sums accumulate in a different order than the string-keyed version produced and the
//! resulting weights differ in their low bits.
//!
//! This is a deliberate trade, taken explicitly. The sums are mathematically the same and
//! statistically indistinguishable, but they are **not bit-identical**: a run started before this
//! change cannot be continued and compared bit-for-bit against one started after it. Checkpoints
//! remain fully compatible — weights are stored by name, and always were.
//!
//! # Collisions
//!
//! Two names sharing a 64-bit key would silently sum into one weight. With ~50,000 names the
//! birthday probability is about 7e-11, and [`register`] asserts in debug builds that a key it is
//! asked to record does not already belong to a different name.

use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

use serde::de::{Deserialize, Deserializer};
use serde::ser::{Serialize, Serializer};

/// A feature name, as a 64-bit key. A pure function of the name.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct FeatureKey(u64);

impl FeatureKey {
    /// The key for a name. FNV-1a: no `Hasher` construction, no allocation, and stable across
    /// processes and releases, which a `DefaultHasher` is explicitly not.
    #[must_use]
    pub const fn of(name: &str) -> Self {
        let bytes = name.as_bytes();
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut index = 0;
        while index < bytes.len() {
            hash ^= bytes[index] as u64;
            hash = hash.wrapping_mul(0x0100_0000_01b3);
            index += 1;
        }
        Self(hash)
    }

    /// The key for a name given in pieces, without ever joining them.
    ///
    /// FNV-1a is a streaming hash, so folding `["a:", b, ":", c]` gives bit-for-bit the same key
    /// as hashing the string `"a:{b}:{c}"`. That is what lets the hot feature families skip
    /// formatting entirely: the name is only ever built on the first sighting of a key, to record
    /// it for later resolution.
    #[must_use]
    pub fn of_parts(parts: &[&str]) -> Self {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for part in parts {
            for byte in part.as_bytes() {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x0100_0000_01b3);
            }
        }
        Self(hash)
    }

    /// The raw key, for callers building side tables.
    #[must_use]
    pub const fn bits(self) -> u64 {
        self.0
    }

    /// Rebuild a key from stored bits.
    ///
    /// For loading a side table that recorded keys beside their names — the dense vocabulary does
    /// this so a loader can verify the key function has not changed under it. Not a way to invent
    /// a key: every key in the system is still [`Self::of`] applied to some name.
    #[must_use]
    pub const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }
}

/// Whether this thread has already recorded `key`, marking it recorded if not.
///
/// Split out of [`register`] so a caller that computed a key from pieces can skip building the
/// name at all on the overwhelmingly common path where the key is already known.
#[must_use]
pub fn first_sighting(key: FeatureKey) -> bool {
    SEEN.with(|seen| seen.borrow_mut().insert(key.0))
}

/// Record the name for a key already known to be new to this thread.
///
/// # Panics
/// If the name table's lock is poisoned; see [`register`].
pub fn record(key: FeatureKey, name: &str) {
    let mut names = NAMES.write().expect("feature name table poisoned");
    match names.entry(key.0) {
        std::collections::hash_map::Entry::Occupied(existing) => {
            debug_assert_eq!(
                existing.get().as_ref(),
                name,
                "two feature names collided on one 64-bit key"
            );
        }
        std::collections::hash_map::Entry::Vacant(slot) => {
            slot.insert(name.into());
        }
    }
}

/// Keys to the names they were made from.
///
/// Written once per *distinct* name and read only where a name is genuinely needed: applying a
/// gradient to a named weight table, and diagnostics. Never touched while features are built.
static NAMES: LazyLock<RwLock<HashMap<u64, Box<str>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

thread_local! {
    /// Keys this thread has already put in [`NAMES`], so it need not ask again.
    static SEEN: std::cell::RefCell<std::collections::HashSet<u64>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

/// Record a name so its key can be resolved back later, and return the key.
///
/// # Panics
/// If the name table's lock is poisoned, which needs another thread to have panicked while
/// holding it — at which point the table is not trustworthy anyway.
#[must_use]
pub fn register(name: &str) -> FeatureKey {
    let key = FeatureKey::of(name);
    // Thread-local first, and on the overwhelmingly common path that is the whole function: a
    // hash-set probe against keys this thread has already recorded, with no lock at all.
    //
    // Taking a read lock on the shared table instead was measured at **22% of rollout time** --
    // features are registered around 340,000 times per game and thirty-two workers sharing one
    // reader count turn a read-mostly table into cache-line ping-pong. The set converges to the
    // corpus (~50,000 keys, ~8 bytes each) within the first games and never grows again.
    if first_sighting(key) {
        record(key, name);
    }
    key
}

/// The name a key was made from, or an empty string if this process never registered it.
///
/// Allocates, so this belongs in gradient application and diagnostics — once per distinct slot
/// per update — and never in feature construction.
///
/// # Panics
/// If the name table's lock is poisoned; see [`register`].
#[must_use]
pub fn name_of(key: FeatureKey) -> String {
    NAMES
        .read()
        .expect("feature name table poisoned")
        .get(&key.0)
        .map(ToString::to_string)
        .unwrap_or_default()
}

/// How many distinct names this process has registered. Diagnostics only.
///
/// # Panics
/// If the name table's lock is poisoned; see [`register`].
#[must_use]
pub fn registered() -> usize {
    NAMES.read().expect("feature name table poisoned").len()
}

impl Serialize for FeatureKey {
    /// As the **name**, never the number: a bare key would decode against a table that may not
    /// know it, producing an artifact that reads as valid and means nothing.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&name_of(*self))
    }
}

impl<'de> Deserialize<'de> for FeatureKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        Ok(register(&name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_is_a_pure_function_of_the_name() {
        assert_eq!(
            FeatureKey::of("prompt-option:action:activate"),
            FeatureKey::of("prompt-option:action:activate")
        );
        assert_ne!(
            FeatureKey::of("kind:activate"),
            FeatureKey::of("kind:produce")
        );
    }

    #[test]
    fn a_key_from_pieces_equals_the_key_from_the_joined_name() {
        // The property the whole fast path rests on.
        for (parts, joined) in [
            (vec!["kind:", "activate"], "kind:activate"),
            (
                vec!["prompt-option:", "action", ":", "activate"],
                "prompt-option:action:activate",
            ),
            (
                vec!["state-kind:", "pay", ":", "trade_goods"],
                "state-kind:pay:trade_goods",
            ),
            (vec![""], ""),
        ] {
            assert_eq!(
                FeatureKey::of_parts(&parts),
                FeatureKey::of(joined),
                "pieces {parts:?} against {joined}"
            );
        }
    }

    #[test]
    fn a_registered_key_resolves_back_to_its_name() {
        let key = register("state-kind:pay:trade_goods");
        assert_eq!(name_of(key), "state-kind:pay:trade_goods");
    }

    #[test]
    fn ordering_is_fixed_by_the_names_not_by_when_they_were_seen() {
        // The whole reason for hashing rather than counting: two processes that meet the same
        // names in opposite orders must still iterate them identically.
        let forward: Vec<FeatureKey> = ["a:1", "b:2", "c:3"].iter().map(|n| register(n)).collect();
        let backward: Vec<FeatureKey> = ["c:3", "b:2", "a:1"].iter().map(|n| register(n)).collect();
        let one: std::collections::BTreeSet<_> = forward.into_iter().collect();
        let two: std::collections::BTreeSet<_> = backward.into_iter().collect();
        assert_eq!(
            one.into_iter().collect::<Vec<_>>(),
            two.into_iter().collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_key_serialises_as_its_name() {
        let key = register("payload-number:worth");
        let json = serde_json::to_string(&key).unwrap();
        assert_eq!(json, "\"payload-number:worth\"");
        assert_eq!(serde_json::from_str::<FeatureKey>(&json).unwrap(), key);
    }

    #[test]
    fn many_real_shaped_names_do_not_collide() {
        let mut seen: HashMap<u64, String> = HashMap::new();
        for kind in ["activate", "produce", "pay", "commit", "transaction"] {
            for token in 0..2000 {
                let name = format!("prompt-option:token{token}:{kind}");
                let key = register(&name);
                if let Some(other) = seen.insert(key.bits(), name.clone()) {
                    assert_eq!(other, name, "collision between distinct names");
                }
            }
        }
        assert_eq!(seen.len(), 10_000);
    }

    #[test]
    fn registering_is_safe_across_threads() {
        let names: Vec<String> = (0..500).map(|i| format!("race:{i}")).collect();
        let seen: Vec<Vec<FeatureKey>> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| scope.spawn(|| names.iter().map(|n| register(n)).collect::<Vec<_>>()))
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        for row in &seen {
            assert_eq!(row, &seen[0]);
        }
        assert_eq!(name_of(seen[0][7]), "race:7");
    }
}
