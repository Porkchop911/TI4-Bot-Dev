//! Per-faction reward terms (`plans/FACTION_REWARD_CONDITIONS_2026-10-20.md` section 4).
//!
//! Each term is a milestone with a VP-denominated weight and an optional deadline round. A milestone
//! first met after its deadline round pays nothing. Reached in time it is a potential: a once-only
//! unlock (breakthrough, commander, hero, technology) stays paid, a losable holding (Mecatol, docks,
//! units) is paid only while held. Everything here is pure shaping: `add_to_rewards` takes the
//! potential back at the horizon. The one exception is the doubled home-planet charge of some
//! factions, a kept objective like `Reward::home_loss_weight`.
//!
//! A faction's listed weights are scaled down proportionally when they sum above [`CAP_VP`]; terms
//! marked `exempt` (Letnev's breakthrough) neither count towards that sum nor scale.

use ti4_policy::Progress;
use ti4_policy::faction_measure::{FACTIONS, Q, tech_index};

/// Weight tiers in victory points (operator, 2026-10-20); scaled by `vp_weight`.
pub const H: f64 = 0.25;
/// Medium tier.
pub const M: f64 = 0.12;
/// Low tier.
pub const L: f64 = 0.05;
/// A faction's full potential, in VP, before the scale-down.
pub const CAP_VP: f64 = 1.0;

/// A checkable condition on one snapshot.
#[derive(Debug, Clone, Copy)]
pub enum Cond {
    /// A measured quantity is at least `n`.
    Q(Q, u8),
    /// Owns a technology (by [`ti4_policy::faction_measure::TECHS`] id).
    Tech(&'static str),
    /// Infantry (as the opening gate counts them) at least `n`.
    Inf(i64),
    /// Capacity ships at least `n`.
    Cap(i64),
    /// Fracture planets held at least `n`.
    Frac(i64),
    /// Holds Styx.
    Styx,
    /// Relics held at least `n`.
    Relic(i64),
    /// Technologies gained since setup at least `n`.
    Gained(i64),
    /// Every condition holds.
    All(&'static [Cond]),
    /// Some condition holds.
    Any(&'static [Cond]),
}

impl Cond {
    /// Whether the snapshot meets this condition.
    #[must_use]
    pub fn met(&self, p: &Progress) -> bool {
        let f = &p.faction_terms;
        match *self {
            Self::Q(q, n) => f.get(q) >= n,
            Self::Tech(id) => tech_index(id).is_some_and(|i| f.has_tech(i)),
            Self::Inf(n) => p.infantry >= n,
            Self::Cap(n) => p.capacity_ships >= n,
            Self::Frac(n) => p.fracture_planets >= n,
            Self::Styx => p.holds_styx,
            Self::Relic(n) => p.relics >= n,
            Self::Gained(n) => p.technologies_gained >= n,
            Self::All(all) => all.iter().all(|c| c.met(p)),
            Self::Any(any) => any.iter().any(|c| c.met(p)),
        }
    }
}

/// One paid milestone.
#[derive(Debug, Clone, Copy)]
pub struct Term {
    /// Short id, for logs and the replayer.
    pub id: &'static str,
    /// The milestone.
    pub cond: Cond,
    /// Weight in VP, before the scale-down.
    pub weight: f64,
    /// Last round it can be reached in; `None` = any round.
    pub deadline: Option<u32>,
    /// Cannot be lost once reached (paid on the latch, not on the current state).
    pub once: bool,
    /// Outside the cap and the scale-down.
    pub exempt: bool,
}

const fn t(id: &'static str, cond: Cond, weight: f64, deadline: Option<u32>, once: bool) -> Term {
    Term {
        id,
        cond,
        weight,
        deadline,
        once,
        exempt: false,
    }
}
// Once-only milestones and losable holdings.
const fn o(id: &'static str, cond: Cond, w: f64, d: Option<u32>) -> Term {
    t(id, cond, w, d, true)
}
const fn h(id: &'static str, cond: Cond, w: f64, d: Option<u32>) -> Term {
    t(id, cond, w, d, false)
}

const fn tech(id: &'static str, w: f64, d: u32) -> Term {
    o(id, Cond::Tech(id), w, Some(d))
}
const fn bt(w: f64, d: Option<u32>) -> Term {
    o("bt", Cond::Q(Q::Bt, 1), w, d)
}
const fn cmdr(w: f64, d: Option<u32>) -> Term {
    o("cmdr", Cond::Q(Q::Cmdr, 1), w, d)
}
const fn hero(w: f64, d: u32) -> Term {
    o("hero", Cond::Q(Q::Hero, 1), w, Some(d))
}
const fn mr(w: f64, d: Option<u32>) -> Term {
    h("mr", Cond::Q(Q::Mecatol, 1), w, d)
}
const fn styx(w: f64, d: u32) -> Term {
    h("styx", Cond::Styx, w, Some(d))
}
const fn dock(n: u8, w: f64, d: u32) -> Term {
    h("dock", Cond::Q(Q::Dock, n), w, Some(d))
}
const fn fdock(w: f64, d: u32) -> Term {
    h("fdock", Cond::Q(Q::FarDock, 1), w, Some(d))
}
const fn inf(n: i64, w: f64, d: Option<u32>) -> Term {
    h("inf", Cond::Inf(n), w, d)
}
const fn cap(n: i64, w: f64, d: Option<u32>) -> Term {
    h("cap", Cond::Cap(n), w, d)
}
const fn out(n: u8, w: f64, d: u32) -> Term {
    h("out", Cond::Q(Q::Out, n), w, Some(d))
}

/// A faction's terms. `home_doubled`: its home-planet charge is doubled (kept, not shaping).
#[derive(Debug, Clone, Copy)]
pub struct Row {
    /// Faction alias (the Keleres row serves all three variants, the Firmament row the Obsidian).
    pub faction: &'static str,
    /// Home-planet charge doubled (operator: Cabal, Hacan, Muaat, Winnu).
    pub home_doubled: bool,
    /// The terms.
    pub terms: &'static [Term],
}

const fn row(faction: &'static str, home_doubled: bool, terms: &'static [Term]) -> Row {
    Row {
        faction,
        home_doubled,
        terms,
    }
}

/// Letnev's breakthrough: 0.5 VP by the end of round one, exempt from the scale-down.
const LETNEV_BT: Term = Term {
    exempt: true,
    ..bt(0.5, Some(1))
};

/// The table. Breakthrough weights are the `bt(..)` entries, one per faction; the unguided ones were
/// set by the operator on 2026-10-20 (plan section 5, Q1).
pub const TABLE: &[Row] = &[
    row(
        "arborec",
        false,
        &[
            bt(H, Some(1)),
            mr(M, None),
            styx(M, 4),
            inf(8, L, None),
            h("mech", Cond::Q(Q::Mech, 1), L, Some(2)),
            tech("x89", L, 3),
        ],
    ),
    row(
        "argent",
        false,
        &[
            mr(M, Some(2)),
            tech("swa2", M, 3),
            o("pds2", Cond::Tech("pds2"), L, None),
            cmdr(M, Some(2)),
            h(
                "fwd",
                Cond::Any(&[Cond::Q(Q::FarDock, 1), Cond::Q(Q::Struct, 3)]),
                L,
                None,
            ),
            // No breakthrough term (operator, 2026-10-20).
        ],
    ),
    row(
        "bastion",
        false,
        &[
            dock(2, M, 2),
            tech("helios2", H, 3),
            inf(6, M, Some(1)),
            out(3, M, 1),
            mr(M, None),
            bt(L, None),
        ],
    ),
    row(
        "cabal",
        true,
        &[
            dock(2, H, 1),
            dock(3, M, 3),
            tech("vtx", M, 2),
            o(
                "cv2+ff2",
                Cond::All(&[Cond::Tech("cv2"), Cond::Tech("ff2")]),
                M,
                Some(3),
            ),
            h("capt", Cond::Q(Q::CapUnits, 3), L, None),
            cmdr(L, None),
            bt(M, Some(2)),
            h("frac", Cond::Frac(1), M, Some(3)),
        ],
    ),
    row(
        "crimson",
        false,
        &[
            cmdr(H, None),
            mr(M, Some(3)),
            styx(M, 4),
            o(
                "cv2|ff2",
                Cond::Any(&[Cond::Tech("cv2"), Cond::Tech("ff2")]),
                M,
                Some(3),
            ),
            cap(3, M, Some(2)),
        ],
    ),
    row(
        "deepwrought",
        false,
        &[
            h("coexist", Cond::Q(Q::Coexist, 3), H, Some(2)),
            bt(M, Some(1)),
            o(
                "cv2|dn2",
                Cond::Any(&[Cond::Tech("cv2"), Cond::Tech("dn2")]),
                M,
                Some(3),
            ),
            tech("x89", L, 4),
        ],
    ),
    row(
        "empyrean",
        false,
        &[
            fdock(M, 2),
            tech("gd", M, 3),
            tech("cv2", M, 3),
            tech("ff2", M, 3),
            bt(L, Some(2)),
            o("explore1", Cond::Q(Q::Explore, 1), L, Some(1)),
            o("explore2", Cond::Q(Q::Explore, 2), L, Some(2)),
            o("explore3", Cond::Q(Q::Explore, 3), L, Some(3)),
            styx(M, 4),
            cap(2, M, None),
        ],
    ),
    row(
        "firmament",
        false,
        &[
            h("plots", Cond::Q(Q::Plots, 2), H, Some(2)),
            hero(M, 3),
            o("flipped", Cond::Q(Q::Flipped, 1), M, Some(3)),
        ],
    ),
    row(
        "ghost",
        false,
        &[
            h("mallice", Cond::Q(Q::Mallice, 1), M, Some(1)),
            cap(2, M, Some(1)),
            tech("sr", M, 1),
            tech("cv2", M, 2),
            cmdr(M, Some(3)),
            bt(H, Some(2)),
            hero(L, 4),
        ],
    ),
    row(
        "hacan",
        true,
        &[
            tech("gd", M, 1),
            tech("cv2", M, 2),
            tech("qdn", H, 3),
            h("hdock", Cond::Q(Q::HomeDock, 2), M, Some(1)),
            h("hftr", Cond::Q(Q::HomeFtr, 4), M, None),
            bt(L, None),
            hero(L, 3),
        ],
    ),
    // No breakthrough term (operator, 2026-10-20).
    row(
        "jolnar",
        false,
        &[
            inf(4, M, Some(1)),
            h("upg2", Cond::Q(Q::Upg, 2), M, Some(3)),
            h("upg3", Cond::Q(Q::Upg, 3), M, Some(5)),
            o("colours", Cond::Q(Q::Colours2, 3), M, Some(4)),
            tech("pds2", L, 2),
            hero(L, 4),
        ],
    ),
    row(
        "keleres",
        false,
        &[
            bt(H, Some(2)),
            fdock(M, 1),
            tech("asn", H, 3),
            tech("gd", M, 4),
            tech("cv2", M, 4),
            tech("x89", M, 4),
            mr(M, None),
            inf(8, L, None),
        ],
    ),
    row(
        "l1z1x",
        false,
        &[
            tech("gd", M, 2),
            tech("sdn2", M, 3),
            tech("cv2", M, 3),
            styx(H, 4),
            tech("x89", L, 4),
            bt(M, Some(2)),
        ],
    ),
    row(
        "letnev",
        false,
        &[
            LETNEV_BT,
            mr(H, Some(2)),
            h("frac", Cond::Frac(1), H, Some(3)),
            tech("dd2", M, 2),
            tech("gd", M, 4),
            tech("nes", M, 4),
            tech("cv2", M, 4),
            inf(5, L, None),
        ],
    ),
    row(
        "mahact",
        false,
        &[
            h("foreign", Cond::Q(Q::ForeignTok, 2), H, Some(3)),
            bt(M, Some(2)),
            h("relic1", Cond::Relic(1), M, Some(3)),
            h("relic2", Cond::Relic(2), L, Some(4)),
            tech("gd", M, 4),
            tech("cv2", M, 4),
            tech("fl", M, 4),
            tech("x89", M, 4),
            mr(M, Some(3)),
            styx(M, 4),
        ],
    ),
    row(
        "mentak",
        false,
        &[tech("cv2", H, 2), cap(2, M, Some(1)), bt(M, Some(2))],
    ),
    row(
        "muaat",
        true,
        &[
            h("ws2", Cond::Q(Q::WarSun, 2), H, Some(2)),
            tech("pws2", M, 3),
            mr(M, Some(3)),
            bt(H, Some(2)), // operator, 2026-10-20
            hero(M, 4),
        ],
    ),
    row(
        "naalu",
        false,
        &[
            cap(2, M, Some(1)),
            h("ftr6", Cond::Q(Q::Ftr, 6), M, Some(3)),
            tech("hcf2", H, 3),
            cmdr(M, Some(2)),
            bt(L, None),
        ],
    ),
    row(
        "naaz",
        false,
        &[
            h("mech3", Cond::Q(Q::Mech, 3), M, Some(2)),
            o("explore4", Cond::Q(Q::Explore, 4), M, Some(3)),
            h("relic", Cond::Relic(1), M, Some(4)),
            fdock(M, 2),
            o(
                "gd+pfa+x89",
                Cond::All(&[Cond::Tech("gd"), Cond::Tech("pfa"), Cond::Tech("x89")]),
                M,
                Some(4),
            ),
            h("frac", Cond::Frac(1), L, Some(3)),
            bt(L, None),
        ],
    ),
    row(
        "nekro",
        false,
        &[
            h(
                "inf4+cap2",
                Cond::All(&[Cond::Inf(4), Cond::Cap(2)]),
                M,
                Some(1),
            ),
            h("assim", Cond::Q(Q::Assim, 1), M, Some(2)),
            o("gained3", Cond::Gained(3), H, Some(4)),
            h("ac3", Cond::Q(Q::ActionCards, 3), L, None),
            bt(L, Some(3)), // operator
        ],
    ),
    row(
        "nomad",
        false,
        &[
            tech("gd", M, 2),
            tech("cv2", M, 3),
            out(4, M, 2),
            mr(M, Some(2)),
            h("mech2", Cond::Q(Q::Mech, 2), L, None),
            bt(L, None),
        ],
    ),
    row(
        "ralnel",
        false,
        &[
            dock(2, M, 2),
            h("struct5", Cond::Q(Q::Struct, 5), M, Some(3)),
            tech("det", M, 3),
            tech("gd", M, 3),
            tech("cv2", M, 3),
            mr(M, Some(3)),
            inf(5, L, None),
            bt(L, None),
        ],
    ),
    // No breakthrough term (operator, 2026-10-20).
    row(
        "saar",
        false,
        &[
            dock(2, H, 1),
            dock(3, M, 3),
            tech("cm", H, 2),
            tech("cv2", M, 3),
            out(5, M, 2),
            mr(M, Some(2)),
        ],
    ),
    row(
        "sardakk",
        false,
        &[
            out(5, H, 2),
            tech("det", M, 1),
            tech("gd", M, 3),
            tech("cv2", M, 3),
            inf(8, M, None),
            bt(L, None),
        ],
    ),
    row(
        "sol",
        false,
        &[
            tech("gd", M, 1),
            tech("ac2", H, 2),
            cap(3, M, Some(2)),
            inf(8, M, None),
            cmdr(M, Some(3)),
            mr(M, Some(2)),
            styx(M, 4),
            bt(H, Some(2)),
        ],
    ),
    row(
        "titans",
        false,
        &[
            tech("se2", H, 2),
            h(
                "hel+struct",
                Cond::All(&[Cond::Q(Q::HelTitan, 2), Cond::Q(Q::Struct, 5)]),
                M,
                Some(3),
            ),
            h("sleeper", Cond::Q(Q::Sleeper, 2), M, Some(2)),
            mr(M, Some(2)),
            styx(M, 3),
            bt(L, None),
        ],
    ),
    row(
        "winnu",
        true,
        &[
            h(
                "cap2+ftr4",
                Cond::All(&[Cond::Cap(2), Cond::Q(Q::Ftr, 4)]),
                M,
                Some(2),
            ),
            tech("det", M, 1),
            tech("gd", M, 3),
            tech("cv2", M, 3),
            mr(M, Some(4)),
            cmdr(M, Some(4)),
            bt(H, Some(2)),
            hero(H, 4), // bt: operator, "essential"
        ],
    ),
    row(
        "xxcha",
        false,
        &[
            out(4, M, 2),
            cmdr(M, Some(3)),
            h("ws1", Cond::Q(Q::WarSun, 1), M, Some(4)),
            tech("cr2", L, 3),
            bt(H, Some(2)),
        ],
    ), // bt: operator
    row(
        "yin",
        false,
        &[
            tech("gd", M, 3),
            tech("cv2", M, 3),
            mr(M, None),
            styx(M, 4),
            cmdr(L, None),
            bt(H, Some(2)),
        ],
    ),
    row(
        "yssaril",
        false,
        &[
            h("ac7", Cond::Q(Q::ActionCards, 7), M, Some(3)),
            o(
                "bs+mi",
                Cond::All(&[Cond::Tech("bs"), Cond::Tech("mi")]),
                M,
                Some(2),
            ),
            tech("gd", M, 3),
            h("mech2", Cond::Q(Q::Mech, 2), L, None), // No breakthrough term (operator, 2026-10-20).
        ],
    ),
];

/// The row serving a seat's faction alias, if it has one.
#[must_use]
pub fn row_for_alias(alias: &str) -> Option<&'static Row> {
    let name = match alias {
        "keleresm" | "keleresx" | "keleresa" => "keleres",
        "obsidian" => "firmament",
        other => other,
    };
    TABLE.iter().find(|r| r.faction == name)
}

/// The row for `Progress::faction_terms.faction` (1-based into [`FACTIONS`]).
#[must_use]
pub fn row_for(faction: u8) -> Option<&'static Row> {
    usize::from(faction)
        .checked_sub(1)
        .and_then(|i| FACTIONS.get(i))
        .and_then(|a| row_for_alias(a))
}

/// Factor applied to every non-exempt term of a row: 1, or less when the weights sum above [`CAP_VP`].
#[must_use]
pub fn scale_of(row: &Row) -> f64 {
    let sum: f64 = row
        .terms
        .iter()
        .filter(|t| !t.exempt)
        .map(|t| t.weight)
        .sum();
    if sum > CAP_VP { CAP_VP / sum } else { 1.0 }
}

/// One term's state at the end of a snapshot sequence, for display.
#[derive(Debug, Clone, PartialEq)]
pub struct TermStatus {
    /// Term id.
    pub id: &'static str,
    /// Effective weight in VP after the scale-down.
    pub weight_vp: f64,
    /// Deadline round.
    pub deadline: Option<u32>,
    /// Once-only (cannot be lost).
    pub once: bool,
    /// Reached in time at some snapshot.
    pub earned: bool,
    /// The condition holds now.
    pub met_now: bool,
    /// VP of potential currently standing for this term.
    pub paying_vp: f64,
}

/// Walk `snapshots` in order and return, per snapshot, each term's `(earned, paying)` state.
fn walk(snapshots: &[Progress]) -> Vec<(Option<&'static Row>, Vec<(bool, bool)>)> {
    let mut latch: Vec<bool> = Vec::new();
    let mut current: Option<&'static Row> = None;
    snapshots
        .iter()
        .map(|p| {
            let row = row_for(p.faction_terms.faction);
            if row.map(|r| r.faction) != current.map(|r| r.faction) {
                current = row;
                latch = vec![false; row.map_or(0, |r| r.terms.len())];
            }
            let states = row.map_or_else(Vec::new, |r| {
                r.terms
                    .iter()
                    .zip(latch.iter_mut())
                    .map(|(term, earned)| {
                        let met = term.cond.met(p);
                        if met && term.deadline.is_none_or(|d| p.round_number <= d) {
                            *earned = true;
                        }
                        (*earned, *earned && (term.once || met))
                    })
                    .collect()
            });
            (row, states)
        })
        .collect()
}

/// The faction-term potential of each snapshot, in VP (the faction scale-down applied).
#[must_use]
pub fn potentials(snapshots: &[Progress]) -> Vec<f64> {
    walk(snapshots)
        .into_iter()
        .map(|(row, states)| {
            row.map_or(0.0, |r| {
                let scale = scale_of(r);
                r.terms
                    .iter()
                    .zip(states)
                    .filter(|(_, (_, paying))| *paying)
                    .map(|(t, _)| t.weight * if t.exempt { 1.0 } else { scale })
                    .sum()
            })
        })
        .collect()
}

/// Per-term status after the last snapshot (what the replayer shows for a seat).
#[must_use]
pub fn status(snapshots: &[Progress]) -> Vec<TermStatus> {
    let walked = walk(snapshots);
    let (Some((Some(row), states)), Some(last)) = (walked.last(), snapshots.last()) else {
        return Vec::new();
    };
    let scale = scale_of(row);
    row.terms
        .iter()
        .zip(states)
        .map(|(t, (earned, paying))| {
            let weight_vp = t.weight * if t.exempt { 1.0 } else { scale };
            TermStatus {
                id: t.id,
                weight_vp,
                deadline: t.deadline,
                once: t.once,
                earned: *earned,
                met_now: t.cond.met(last),
                paying_vp: if *paying { weight_vp } else { 0.0 },
            }
        })
        .collect()
}

/// Add the faction terms to per-step `rewards` (one shorter than `snapshots`): potential
/// differences in reward units (`vp_weight * scale` per VP), the shaping taken back at the horizon,
/// plus the kept doubled home charge (`home_loss_weight` once more for [`Row::home_doubled`]).
pub fn add_to_rewards(
    snapshots: &[Progress],
    per_vp: f64,
    home_loss_weight: f64,
    rewards: &mut [f64],
) {
    let pot = potentials(snapshots);
    for (i, slot) in rewards.iter_mut().enumerate() {
        if let (Some(a), Some(b)) = (pot.get(i), pot.get(i + 1)) {
            *slot += per_vp * (b - a);
        }
        if let (Some(a), Some(b)) = (snapshots.get(i), snapshots.get(i + 1)) {
            *slot += home_potential(b, home_loss_weight) - home_potential(a, home_loss_weight);
        }
    }
    if let (Some(slot), Some(end)) = (rewards.last_mut(), pot.last()) {
        *slot -= per_vp * end;
    }
}

/// The kept doubled home charge: `-home_loss_weight` per lost home planet for the doubled factions.
#[must_use]
pub fn home_potential(p: &Progress, home_loss_weight: f64) -> f64 {
    if row_for(p.faction_terms.faction).is_some_and(|r| r.home_doubled) {
        #[expect(
            clippy::cast_precision_loss,
            reason = "home planet counts are single digits"
        )]
        let lost = p.home_planets_lost as f64;
        -home_loss_weight * lost
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_policy::faction_measure::{FactionProgress, N_Q};

    fn snap(alias: &str, round: u32, f: impl FnOnce(&mut Progress)) -> Progress {
        let id = FACTIONS.iter().position(|a| *a == alias).expect("faction") + 1;
        let mut p = Progress {
            round_number: round,
            faction_terms: FactionProgress {
                faction: u8::try_from(id).expect("id"),
                q: [0; N_Q],
                techs: 0,
            },
            ..Progress::default()
        };
        f(&mut p);
        p
    }
    fn set(p: &mut Progress, q: Q, n: u8) {
        p.faction_terms.q[q as usize] = n;
    }

    #[test]
    fn every_faction_has_a_row_and_rows_fit_the_cap() {
        for alias in FACTIONS {
            let row = row_for_alias(alias).unwrap_or_else(|| panic!("no row for {alias}"));
            let scaled: f64 = row
                .terms
                .iter()
                .filter(|t| !t.exempt)
                .map(|t| t.weight * scale_of(row))
                .sum();
            assert!(scaled <= CAP_VP + 1e-9, "{alias} {scaled}");
            for term in row.terms {
                if let Cond::Tech(id) = term.cond {
                    assert!(tech_index(id).is_some(), "{alias}: unknown tech {id}");
                }
            }
        }
        assert!(row_for(0).is_none());
    }

    #[test]
    fn cap_scales_proportionally_and_letnev_bt_is_exempt() {
        let letnev = row_for_alias("letnev").expect("row");
        let sum: f64 = letnev
            .terms
            .iter()
            .filter(|t| !t.exempt)
            .map(|t| t.weight)
            .sum();
        assert!(sum > CAP_VP);
        assert!((scale_of(letnev) - CAP_VP / sum).abs() < 1e-12);
        let p = snap("letnev", 1, |p| set(p, Q::Bt, 1));
        assert!(
            (potentials(&[p])[0] - 0.5).abs() < 1e-12,
            "bt pays its full 0.5 VP"
        );
    }

    #[test]
    fn deadline_gates_and_losable_holdings_step_down() {
        // Sol mr M@R2: reached in round 3 pays nothing; reached in round 2 pays while held.
        let held = |p: &mut Progress| set(p, Q::Mecatol, 1);
        let late = [snap("sol", 3, |_| {}), snap("sol", 3, held)];
        assert!(potentials(&late).iter().all(|v| *v == 0.0));
        let on_time = [
            snap("sol", 2, |_| {}),
            snap("sol", 2, held),
            snap("sol", 3, |_| {}),
            snap("sol", 3, held),
        ];
        let pot = potentials(&on_time);
        assert!(
            pot[1] > 0.0 && pot[2] == 0.0 && (pot[3] - pot[1]).abs() < 1e-12,
            "{pot:?}"
        );
        // Once-only: a commander stays paid after the flag drops.
        let cm = |p: &mut Progress| set(p, Q::Cmdr, 1);
        let once = potentials(&[snap("sol", 3, cm), snap("sol", 4, |_| {})]);
        assert!(once[0] > 0.0 && (once[0] - once[1]).abs() < 1e-12);
    }

    #[test]
    fn shaping_is_taken_back_at_the_horizon_but_home_is_kept() {
        let snaps = [
            snap("hacan", 1, |_| {}),
            snap("hacan", 2, |p| {
                set(p, Q::Bt, 1);
                p.home_planets_lost = 1;
            }),
        ];
        let mut rewards = [0.0];
        add_to_rewards(&snaps, 6.0, 0.5, &mut rewards);
        // The shaping potential is paid on entry and taken back on the same (final) slot; only the
        // kept home charge of one planet at the doubled extra 0.5 remains.
        assert!((rewards[0] + 0.5).abs() < 1e-12, "{rewards:?}");
        let mut plain = [0.0];
        add_to_rewards(&[snaps[0], snaps[0]], 6.0, 0.5, &mut plain);
        assert!(plain[0].abs() < 1e-12);
        // A non-doubled faction pays no home term.
        let other = snap("sol", 2, |p| p.home_planets_lost = 1);
        assert!(home_potential(&other, 0.5).abs() < 1e-12);
    }

    #[test]
    fn mid_game_steps_carry_the_potential_and_unmeasured_seats_pay_nothing() {
        let snaps = [
            snap("sol", 1, |_| {}),
            snap("sol", 1, |p| set(p, Q::Bt, 1)),
            snap("sol", 2, |p| set(p, Q::Bt, 1)),
        ];
        let mut rewards = [0.0, 0.0];
        add_to_rewards(&snaps, 6.0, 0.0, &mut rewards);
        assert!(
            rewards[0] > 0.0 && rewards[1] < 0.0,
            "paid on reaching, taken back at the end"
        );
        assert!((rewards[0] + rewards[1]).abs() < 1e-12);
        let none = [Progress::default(), Progress::default()];
        let mut r = [0.0];
        add_to_rewards(&none, 6.0, 1.0, &mut r);
        assert_eq!(r, [0.0]);
        assert!(status(&none).is_empty());
    }

    #[test]
    fn off_is_bit_identical_and_on_changes_returns() {
        use crate::reward::{Episode, Reward, Stage, returns};
        let steps = vec![
            snap("sol", 1, |_| {}),
            snap("sol", 2, |p| set(p, Q::Bt, 1)),
            snap("sol", 2, |p| set(p, Q::Bt, 1)),
        ];
        let episode = Episode {
            final_progress: snap("sol", 3, |p| set(p, Q::Bt, 1)),
            steps,
            cleared: true,
            shortfall: 0.0,
            traded_goods: 0.0,
            strategy_card_plays: std::collections::BTreeMap::new(),
        };
        let base = Reward::for_stage(Stage::Two);
        let mut off = base.clone();
        off.faction_terms_weight = 0.0;
        let a = returns(&episode, &base);
        let b = returns(&episode, &off);
        assert!(a.iter().zip(&b).all(|(x, y)| x.to_bits() == y.to_bits()));
        // Faction fields alone, with the weight off, equal the same game without them.
        let mut bare = episode.clone();
        for p in bare
            .steps
            .iter_mut()
            .chain(std::iter::once(&mut bare.final_progress))
        {
            p.faction_terms = FactionProgress::default();
        }
        let c = returns(&bare, &base);
        assert!(a.iter().zip(&c).all(|(x, y)| x.to_bits() == y.to_bits()));
        let mut on = base;
        on.faction_terms_weight = 1.0;
        let d = returns(&episode, &on);
        assert!(d.iter().zip(&a).any(|(x, y)| (x - y).abs() > 1e-9));
    }
}
