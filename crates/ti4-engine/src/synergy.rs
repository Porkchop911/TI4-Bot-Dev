//! Synergy (Thunder's Edge).
//!
//! A breakthrough grants synergy between two technology colours, letting components of one be
//! treated as the other.
//!
//! > **1.** Certain abilities, primarily breakthroughs, grant synergy between two colors of
//! > technology, allowing components of one color to be treated as the other for some game effects.
//! >
//! > **2.** When researching a technology, a player may treat each technology they own and/or each
//! > technology speciality on a planet they control that matches one color of their synergy as any
//! > color of that synergy.
//! >
//! > **3.** When scoring an objective, a player may treat each technology they own that matches one
//! > color of their synergy as any color of that synergy.
//! >
//! > **4.** These choices are made individually for each technology and/or each technology
//! > speciality.
//! >
//! > **6.** Each technology and/or each technology speciality can only be one color of technology at
//! > any given point in time.
//! >
//! > **8.** A technology and/or each technology speciality may be treated as different colors of
//! > technologies at different points in time.
//!
//! # Why this is a query and not stored state
//!
//! Rules 4, 6 and 8 together say the assignment is made per component, must be consistent at any one
//! moment, and may differ at another. Storing a chosen colour would violate 8; asking at each check
//! satisfies all three, because a check is exactly "a given point in time".
//!
//! # Why counting is enough
//!
//! A synergy pair makes its two colours interchangeable *within the pair*. A holding of `a` of
//! colour A and `b` of colour B can fill any split of a requirement asking `na` of A and `nb` of B,
//! so the pair is satisfiable exactly when `a + b >= na + nb`. No search is needed, and no
//! assignment has to be materialised — which is the whole content of rules 4 and 6 for two colours.
//! Colours outside the pair are unaffected and check as they always did.

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;

/// The two colours a player's breakthrough makes interchangeable, if they have one.
///
/// Read from the breakthrough record's `synergy` field. A breakthrough without one — the rules note
/// Nekro as the exception — yields `None`, as does a player who has not gained theirs.
#[must_use]
pub fn pair(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Option<[String; 2]> {
    let alias = state.player(player)?.breakthrough.as_ref()?;
    let record = content
        .from_sources(ContentType::Breakthroughs, sources)
        .find(|record| record.text("alias") == Some(alias.as_str()))?;
    let colours = record.strings("synergy");
    let [first, second] = colours.as_slice() else {
        return None;
    };
    Some([
        (*first).to_ascii_uppercase(),
        (*second).to_ascii_uppercase(),
    ])
}

/// Whether a colour is one of the two this player's synergy joins.
#[must_use]
pub fn joins(pair: Option<&[String; 2]>, colour: &str) -> bool {
    pair.is_some_and(|[a, b]| a == colour || b == colour)
}

/// Whether `holdings` satisfy `needs`, with a synergy pair pooled (rules 2, 3, 4, 6).
///
/// `shortfall_allowance` is the faction waiver budget applied across the whole requirement, which
/// exists independently of synergy; it is threaded through so the two interact once rather than
/// twice. Returns whether the requirement is met.
#[must_use]
pub fn satisfies(
    needs: &BTreeMap<&'static str, usize>,
    holdings: &BTreeMap<&'static str, usize>,
    pair: Option<&[String; 2]>,
    mut waivable: usize,
) -> bool {
    let held = |colour: &str| -> usize { holdings.get(colour).copied().unwrap_or(0) };

    // The pooled pair, checked once against the sum of what it is asked for.
    if let Some([a, b]) = pair {
        let wanted: usize = needs
            .iter()
            .filter(|(colour, _)| *colour == a || *colour == b)
            .map(|(_, need)| *need)
            .sum();
        if wanted > 0 {
            let pooled = held(a) + held(b);
            if pooled < wanted {
                let short = wanted - pooled;
                if waivable < short {
                    return false;
                }
                waivable -= short;
            }
        }
    }

    needs.iter().all(|(colour, need)| {
        if joins(pair, colour) {
            return true; // already accounted for above
        }
        let have = held(colour);
        if have >= *need {
            return true;
        }
        let short = need - have;
        if waivable >= short {
            waivable -= short;
            return true;
        }
        false
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(pairs: &[(&'static str, usize)]) -> BTreeMap<&'static str, usize> {
        pairs.iter().copied().collect()
    }

    fn biotic_cybernetic() -> [String; 2] {
        ["BIOTIC".to_owned(), "CYBERNETIC".to_owned()]
    }

    #[test]
    fn a_synergy_colour_stands_in_for_its_partner() {
        // Rule 2: two biotic satisfy a requirement for two cybernetic.
        let needs = counts(&[("CYBERNETIC", 2)]);
        let holdings = counts(&[("BIOTIC", 2)]);
        assert!(satisfies(&needs, &holdings, Some(&biotic_cybernetic()), 0));
    }

    #[test]
    fn the_pair_is_pooled_not_doubled() {
        // Rule 6: each component is only one colour at a time. One biotic cannot be both the
        // biotic and the cybernetic a requirement asks for.
        let needs = counts(&[("BIOTIC", 1), ("CYBERNETIC", 1)]);
        let one = counts(&[("BIOTIC", 1)]);
        assert!(
            !satisfies(&needs, &one, Some(&biotic_cybernetic()), 0),
            "one technology cannot fill both slots"
        );
        let two = counts(&[("BIOTIC", 2)]);
        assert!(satisfies(&needs, &two, Some(&biotic_cybernetic()), 0));
    }

    #[test]
    fn colours_outside_the_pair_are_untouched() {
        let needs = counts(&[("WARFARE", 1)]);
        let holdings = counts(&[("BIOTIC", 3)]);
        assert!(
            !satisfies(&needs, &holdings, Some(&biotic_cybernetic()), 0),
            "synergy joins two colours, not all four"
        );
    }

    #[test]
    fn without_a_breakthrough_nothing_changes() {
        let needs = counts(&[("CYBERNETIC", 2)]);
        let holdings = counts(&[("BIOTIC", 2)]);
        assert!(!satisfies(&needs, &holdings, None, 0));
    }

    #[test]
    fn the_waiver_budget_still_applies_across_both() {
        let needs = counts(&[("CYBERNETIC", 2), ("WARFARE", 1)]);
        let holdings = counts(&[("BIOTIC", 1)]);
        // Short one in the pair and one outside it: two waivers cover exactly that.
        assert!(satisfies(&needs, &holdings, Some(&biotic_cybernetic()), 2));
        assert!(!satisfies(&needs, &holdings, Some(&biotic_cybernetic()), 1));
    }

    #[test]
    fn the_corpus_gives_every_trained_faction_a_synergy() {
        // Not a rule, a data check: phase 4 assumes each of the six carries one.
        let content = ti4_content::ContentStore::embedded();
        let sources = ti4_model::content_types::DEFAULT;
        for faction in ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"] {
            let record = content
                .from_sources(ContentType::Breakthroughs, sources)
                .find(|record| record.text("faction") == Some(faction))
                .unwrap_or_else(|| panic!("{faction} has a breakthrough"));
            assert_eq!(
                record.strings("synergy").len(),
                2,
                "{faction}'s breakthrough must name two synergy colours"
            );
        }
    }
}
