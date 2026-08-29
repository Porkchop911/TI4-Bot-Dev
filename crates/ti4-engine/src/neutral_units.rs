//! Neutral units (Thunder's Edge).
//!
//! A force that fights but is not a player. The Fracture places them, and other effects can too.
//!
//! > **2.** If a player has ships in a space area that contains neutral units, they will resolve a
//! > space combat against those units.
//! >
//! > **3.** If a player has ground forces on a planet that contains neutral units, they will
//! > resolve a ground combat against those units.
//! >
//! > **4.** Any player other than the active player may roll the combat and unit ability dice
//! > during a combat involving neutral units.
//! >
//! > **5.** Neutral units will always use each and every unit ability that they can.
//! >
//! > **6.** Neutral units do not take turns, own technology, or draw action cards, and so on. They
//! > cannot retreat from combat.
//! >
//! > **7.** When a hit is assigned against a group of neutral units, it is assigned to the unit that
//! > is lowest on the neutral unit reference card.
//! >
//! > **7a.** Hits produced against neutral units may only be assigned to eligible unit types, as
//! > usual.
//! >
//! > **9.** Neutral units are considered to be another player's ships for abilities and other game
//! > effects. However, there is no neutral player.
//!
//! # Rule 9 is the whole design
//!
//! "Another player's ships, but there is no neutral player" is satisfied by giving them a
//! [`NEUTRAL`] owner that is a perfectly ordinary [`PlayerId`] — so every ownership comparison in
//! the engine treats them as someone else's units without knowing they exist — while never adding
//! that id to `state.players` or `state.seating_order`. Anything that iterates seats therefore skips
//! them automatically, which is exactly rule 6.
//!
//! # The roster is transcribed content, not invented
//!
//! Rule 7 orders hits by "the neutral unit reference card", and the rules text says only that "the
//! combat values and unit abilities are found on the neutral unit reference card". The card is now
//! transcribed into `units.json` as eleven records tagged `faction: "neutral"`, each carrying its
//! printed position as `cardOrder` (1 at the top). This is the one place the corpus deliberately
//! diverges from the oracle it was copied from, and `loader.rs` says so where the counts are pinned.
//!
//! Rule 7 wants the unit *lowest* on the card, so [`roster`] sorts by `cardOrder` descending. The
//! two-column bottom block makes fighter-versus-mech and PDS-versus-infantry ordering ambiguous by
//! eye; it is never consulted, because a hit is either a space hit or a ground hit and no unit is
//! eligible for both. The orderings that are consulted — ships among themselves, ground forces among
//! themselves — are each a single column and unambiguous.
//!
//! [`missing_content`] and [`can_place`] remain: they now guard against the records being removed or
//! a source scope that excludes Thunder's Edge, rather than against a gap in the corpus.

use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{PlayerId, UnitTypeId};
use ti4_model::state::GameState;

/// The owner id carried by every neutral unit.
///
/// A real `PlayerId` so ownership comparisons work unchanged (rule 9), never seated so nothing
/// iterating players finds it (rule 6). No faction may use this id.
pub const NEUTRAL: &str = "neutral";

/// Whether this owner is the neutral force rather than a seated player.
#[must_use]
pub fn is_neutral(player: &PlayerId) -> bool {
    player.as_str() == NEUTRAL
}

/// The neutral owner id.
#[must_use]
pub fn owner() -> PlayerId {
    PlayerId::new(NEUTRAL)
}

/// Whether a seated player exists under the neutral id, which would break rule 9.
///
/// Rule 9 says there is no neutral player. If one were ever seated, "another player's ships" would
/// become "a player's ships" and turn order, scoring and elimination would all acquire a phantom
/// seat. Checked rather than assumed.
#[must_use]
pub fn is_seated(state: &GameState) -> bool {
    state.players.iter().any(|seat| is_neutral(&seat.id))
}

/// The neutral unit roster, lowest on the reference card first (rule 7 order).
///
/// Read from content records marked with the neutral faction. Empty until that content exists.
#[must_use]
pub fn roster(content: &ContentStore, sources: SourceSet) -> Vec<UnitTypeId> {
    // Ordered by the printed card, lowest first, because rule 7 assigns a hit to the unit lowest on
    // it. `cardOrder` is the printed position with 1 at the top, so this is descending.
    let mut found: Vec<(i64, UnitTypeId)> = content
        .from_sources(ContentType::Units, sources)
        .filter(|record| record.text("faction") == Some(NEUTRAL))
        .filter_map(|record| {
            let order = record.int("cardOrder")?;
            record.id().map(|id| (order, UnitTypeId::new(id)))
        })
        .collect();
    found.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    found.into_iter().map(|(_, id)| id).collect()
}

/// Whether the neutral unit reference card is absent from the corpus.
///
/// While this is true, neutral units cannot be placed and [`can_place`] refuses. The Fracture
/// (phase 5) depends on this being resolved.
#[must_use]
pub fn missing_content(content: &ContentStore, sources: SourceSet) -> bool {
    roster(content, sources).is_empty()
}

/// Whether neutral units may be placed at all.
///
/// # Errors
/// [`NeutralError::NoReferenceCard`] while the roster is absent.
pub fn can_place(content: &ContentStore, sources: SourceSet) -> Result<(), NeutralError> {
    if missing_content(content, sources) {
        return Err(NeutralError::NoReferenceCard);
    }
    Ok(())
}

/// Rule 6: neutral units cannot retreat.
#[must_use]
pub const fn may_retreat() -> bool {
    false
}

/// Rule 5: neutral units always use every unit ability they can.
///
/// Stated as a function so a caller offering an optional ability to a combatant has something to
/// ask, rather than each site re-deciding what "always" means.
#[must_use]
pub fn uses_every_ability(player: &PlayerId) -> bool {
    is_neutral(player)
}

/// Which of a group's units takes the next hit (rules 7, 7a).
///
/// Ordered by the reference card, restricted to types the hit may legally be assigned to. Returns
/// `None` when nothing is eligible, or while the roster is absent.
#[must_use]
pub fn next_casualty<'a>(
    order: &[UnitTypeId],
    present: &'a [ti4_model::units::Unit],
    eligible: impl Fn(&ti4_model::units::Unit) -> bool,
) -> Option<&'a ti4_model::units::Unit> {
    order.iter().find_map(|kind| {
        present
            .iter()
            .find(|unit| &unit.type_id == kind && eligible(unit))
    })
}

/// Something that cannot be done while neutral units are unmodelled.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NeutralError {
    /// The corpus carries no neutral unit records.
    #[error(
        "the neutral unit reference card is not in this corpus: no unit records carry the neutral \
         faction, so their roster, combat values and hit order are unknown"
    )]
    NoReferenceCard,
}

#[cfg(test)]
mod tests {
    use ti4_model::content_types::DEFAULT as ALL_SOURCES;
    use ti4_model::units::Unit;

    use super::*;

    #[test]
    fn the_neutral_force_is_not_a_seated_player() {
        // Rule 9: another player's ships, but no neutral player.
        let state = crate::fixtures::game(&["a", "b"]);
        assert!(!is_seated(&state), "no seat may carry the neutral id");
        assert!(is_neutral(&owner()));
        assert!(!is_neutral(&PlayerId::new("a")));
    }

    #[test]
    fn neutral_units_are_owned_by_someone_else_as_far_as_the_engine_can_tell() {
        // The point of rule 9: existing ownership comparisons need no special case.
        let mut state = crate::fixtures::game(&["a"]);
        let system = ti4_model::id::SystemId::new("19");
        let a = PlayerId::new("a");
        state
            .system_mut(&system)
            .units
            .push(Unit::new(UnitTypeId::new("cruiser"), owner()));

        assert!(
            state.system_state(&system).units_of(&a).is_empty(),
            "a neutral ship is not player a's"
        );
        assert_eq!(
            state.system_state(&system).units_of(&owner()).len(),
            1,
            "and it is owned by somebody"
        );
    }

    #[test]
    fn neutral_units_never_retreat() {
        assert!(!may_retreat(), "rule 6");
        assert!(uses_every_ability(&owner()), "rule 5");
        assert!(!uses_every_ability(&PlayerId::new("a")));
    }

    #[test]
    fn hits_go_to_the_lowest_eligible_unit_on_the_card() {
        // Rules 7 and 7a, against a supplied order: the roster itself is content this corpus does
        // not have, but the ordering rule is testable without it.
        let order = vec![
            UnitTypeId::new("infantry"),
            UnitTypeId::new("fighter"),
            UnitTypeId::new("cruiser"),
        ];
        let present = vec![
            Unit::new(UnitTypeId::new("cruiser"), owner()),
            Unit::new(UnitTypeId::new("fighter"), owner()),
        ];

        let taken = next_casualty(&order, &present, |_| true).expect("something is eligible");
        assert_eq!(
            taken.type_id,
            UnitTypeId::new("fighter"),
            "the fighter is lower on the card than the cruiser"
        );

        // 7a: only eligible types. With fighters ineligible the hit moves up the card.
        let taken = next_casualty(&order, &present, |unit| {
            unit.type_id != UnitTypeId::new("fighter")
        })
        .expect("the cruiser is eligible");
        assert_eq!(taken.type_id, UnitTypeId::new("cruiser"));
    }

    #[test]
    fn the_reference_card_is_in_the_corpus_lowest_first() {
        // Rule 7 assigns hits to the unit lowest on the card, so the roster reads bottom-up.
        let content = ti4_content::ContentStore::embedded();
        assert!(!missing_content(content, ALL_SOURCES));
        assert!(can_place(content, ALL_SOURCES).is_ok());

        let order: Vec<String> = roster(content, ALL_SOURCES)
            .into_iter()
            .map(|id| id.as_str().to_owned())
            .collect();
        assert_eq!(
            order,
            vec![
                "neutral_spacedock",
                "neutral_pds",
                "neutral_infantry",
                "neutral_mech",
                "neutral_fighter",
                "neutral_destroyer",
                "neutral_cruiser",
                "neutral_carrier",
                "neutral_dreadnought",
                "neutral_warsun",
                "neutral_flagship",
            ]
        );
    }

    #[test]
    fn the_transcribed_stats_match_the_printed_card() {
        // Pins the transcription itself. Every number here was read off the reference sheet; if a
        // record is edited, this says so rather than letting a silent change ride.
        let content = ti4_content::ContentStore::embedded();
        let types = ti4_content::units::catalogue(content, ALL_SOURCES);
        let stat = |id: &str| types.get(id).expect("transcribed").clone();

        let flagship = stat("neutral_flagship");
        assert_eq!((flagship.combat_hits_on(), flagship.combat_dice()), (Some(7), 2));
        assert_eq!(flagship.capacity(), 3);
        assert!(flagship.sustain_damage());

        let warsun = stat("neutral_warsun");
        assert_eq!((warsun.combat_hits_on(), warsun.combat_dice()), (Some(3), 3));
        assert_eq!(warsun.bombard_hits_on(), Some(3));

        let destroyer = stat("neutral_destroyer");
        assert_eq!((destroyer.combat_hits_on(), destroyer.combat_dice()), (Some(8), 1));

        let infantry = stat("neutral_infantry");
        assert_eq!(infantry.combat_hits_on(), Some(8));
        assert!(infantry.is_ground_force());

        let pds = stat("neutral_pds");
        assert!(pds.planetary_shield());
        assert!(!pds.is_ship());
    }

    #[test]
    fn a_scope_without_thunders_edge_has_no_neutral_units() {
        // The guard still does something: neutral units are Thunder's Edge content.
        let content = ti4_content::ContentStore::embedded();
        assert!(missing_content(content, ti4_model::content_types::POK));
        assert_eq!(
            can_place(content, ti4_model::content_types::POK),
            Err(NeutralError::NoReferenceCard)
        );
    }
}
