//! Fleet strength: cheap descriptors of a fleet, and a matchup comparison between two.
//!
//! A shortlist heuristic, not the odds. The descriptors are intrinsic to one fleet; the matchup
//! comparison depends on both, because each side's opening fire (space cannon, and barrage up to
//! the other side's fighters) comes off the other side first. The arena predictor prices fights;
//! this only decides which fleets are worth pricing.
//!
//! Calibration against the lean arena is in `strength_index_probe`
//! (`plans/ASTRA_ACTIVATION_REWORK_2026-09-17.md`): the matchup comparison names the favourite in
//! 97% of fights outside a 40–60% win band, and is barely better than a coin on close fights.

use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;

/// What a fleet brings to a space combat, before meeting anyone.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Descriptors {
    /// Expected hits per combat round.
    pub firepower: f64,
    /// Hits the fleet can absorb, sustain included.
    pub durability: f64,
    /// Fighters in the fleet.
    pub fighters: f64,
    /// Expected anti-fighter-barrage hits in round 1.
    pub barrage: f64,
    /// Expected space-cannon hits before combat.
    pub cannon: f64,
    /// Transport capacity.
    pub capacity: f64,
    /// Resource cost.
    pub cost: f64,
}

/// The chance one die hits `hits_on` with a roll shift of `modifier`.
#[must_use]
pub fn hit_chance(hits_on: i64, modifier: i64) -> f64 {
    let needed = hits_on - modifier;
    (f64::from(i32::try_from(11 - needed).unwrap_or(0)) / 10.0).clamp(0.0, 1.0)
}

/// A faction's shift to every combat roll.
#[must_use]
pub fn modifier(faction: &str) -> i64 {
    match faction {
        "jolnar" => -1,
        "sardakk" => 1,
        _ => 0,
    }
}

/// Descriptors of `fleet` (unit id, count) for `faction`. `damaged` names ships that have already
/// used SUSTAIN DAMAGE. Units the content does not know are skipped.
#[must_use]
pub fn descriptors(
    content: &ContentStore,
    sources: SourceSet,
    faction: &str,
    fleet: &[(String, usize)],
    damaged: &[(String, usize)],
) -> Descriptors {
    let shift = modifier(faction);
    let mut out = Descriptors::default();
    for (id, count) in fleet {
        let Some(unit) = ti4_content::units::unit_type(content, id, sources) else {
            continue;
        };
        #[expect(clippy::cast_precision_loss, reason = "fleet counts are small")]
        let n = *count as f64;
        #[expect(clippy::cast_precision_loss, reason = "dice counts are small")]
        let dice = unit.combat_dice() as f64;
        let mut per_ship = dice
            * unit
                .combat_hits_on()
                .map_or(0.0, |on| hit_chance(on, shift));
        if id == "jolnar_flagship" {
            // Each 9 or 10 before modifiers adds 2 hits.
            per_ship += dice * 0.2 * 2.0;
        }
        out.firepower += n * per_ship;
        let hurt = damaged
            .iter()
            .find(|(which, _)| which == id)
            .map_or(0, |(_, k)| (*k).min(*count));
        #[expect(clippy::cast_precision_loss, reason = "fleet counts are small")]
        let fresh = (count - hurt) as f64;
        let sustain = f64::from(u8::from(unit.sustain_damage()));
        let mut hp = n + fresh * sustain;
        if id == "letnev_flagship" {
            hp += n; // repairs every round: roughly one more hit over a fight
        }
        out.durability += hp;
        out.cost += n * unit.cost();
        #[expect(clippy::cast_precision_loss, reason = "dice and capacity are small")]
        {
            out.cannon += n
                * unit.space_cannon_dice() as f64
                * unit
                    .space_cannon_hits_on()
                    .map_or(0.0, |on| hit_chance(on, shift));
            out.barrage += n
                * unit.afb_dice() as f64
                * unit.afb_hits_on().map_or(0.0, |on| hit_chance(on, shift));
            out.capacity += n * unit.capacity() as f64;
        }
        if unit.is_fighter() {
            out.fighters += n;
        }
    }
    out
}

/// Lanchester square law after the opening volleys: `own`'s firepower times durability once
/// `enemy`'s space cannon, and its barrage up to `own`'s fighters, have come off, with firepower
/// falling in the same proportion.
#[must_use]
pub fn strength_after_opening(own: &Descriptors, enemy: &Descriptors) -> f64 {
    let lost = enemy.cannon + enemy.barrage.min(own.fighters);
    let left = (own.durability - lost).max(0.0);
    let scale = if own.durability > 0.0 {
        left / own.durability
    } else {
        0.0
    };
    own.firepower * scale * left
}

/// `ln(S_own / S_enemy)` after opening fire: above zero favours `own`. An empty side is a large
/// negative (or positive) number, never infinite.
#[must_use]
pub fn matchup(own: &Descriptors, enemy: &Descriptors) -> f64 {
    let (a, b) = (
        strength_after_opening(own, enemy),
        strength_after_opening(enemy, own),
    );
    ((a + 1e-3) / (b + 1e-3)).ln()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fleet(entries: &[(&str, usize)]) -> Vec<(String, usize)> {
        entries
            .iter()
            .map(|(id, n)| ((*id).to_owned(), *n))
            .collect()
    }

    #[test]
    fn a_dreadnought_outweighs_a_destroyer_and_damage_costs_durability() {
        let content = ContentStore::embedded();
        let pok = ti4_model::POK;
        let dread = descriptors(content, pok, "hacan", &fleet(&[("dreadnought", 1)]), &[]);
        let hurt = descriptors(
            content,
            pok,
            "hacan",
            &fleet(&[("dreadnought", 1)]),
            &fleet(&[("dreadnought", 1)]),
        );
        let destroyer = descriptors(content, pok, "hacan", &fleet(&[("destroyer", 1)]), &[]);
        assert!((dread.durability - 2.0).abs() < 1e-9);
        assert!((hurt.durability - 1.0).abs() < 1e-9);
        assert!(matchup(&dread, &destroyer) > 0.0);
        assert!(matchup(&destroyer, &dread) < 0.0);
        assert!(destroyer.barrage > 0.0, "destroyers carry a barrage");
    }

    #[test]
    fn jolnar_rolls_worse_with_the_same_ships() {
        let content = ContentStore::embedded();
        let ships = fleet(&[("cruiser", 2)]);
        let hacan = descriptors(content, ti4_model::POK, "hacan", &ships, &[]);
        let jolnar = descriptors(content, ti4_model::POK, "jolnar", &ships, &[]);
        assert!(jolnar.firepower < hacan.firepower);
        assert!((matchup(&hacan, &hacan)).abs() < 1e-9);
    }
}
