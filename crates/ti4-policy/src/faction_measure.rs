//! Per-faction reward measurement: exact integer counts of the milestones the faction reward terms
//! (`ti4_training::faction_terms`) pay for, read from one seat's position. Pure; no hash iteration.
//! The table of which faction pays for what lives in the trainer; this only measures.

use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_engine::choice::Observed;
use ti4_model::content_types::ContentType;
use ti4_model::id::PlayerId;
use ti4_model::state::LeaderStatus;

/// Set by a trainer that rewards faction terms, so every other run pays nothing for the scan.
pub static MEASURE_FACTION_TERMS: AtomicBool = AtomicBool::new(false);

/// Faction aliases; a seat's `FactionProgress::faction` is the 1-based position (0 = none).
pub const FACTIONS: [&str; 32] = [
    "arborec",
    "ghost",
    "hacan",
    "jolnar",
    "l1z1x",
    "letnev",
    "mentak",
    "muaat",
    "naalu",
    "nekro",
    "saar",
    "sardakk",
    "sol",
    "winnu",
    "xxcha",
    "yin",
    "yssaril",
    "keleresm",
    "keleresx",
    "keleresa",
    "argent",
    "cabal",
    "empyrean",
    "mahact",
    "nomad",
    "naaz",
    "titans",
    "bastion",
    "deepwrought",
    "firmament",
    "obsidian",
    "crimson",
];

/// Technologies a faction term can name: `(id, alternates)`. A flag is set when the seat owns the
/// id, an alternate, or a unit upgrade whose `baseUpgrade` is the id (Advanced Carrier II is Carrier II).
pub const TECHS: [(&str, &[&str]); 27] = [
    ("gd", &[]),
    ("cv2", &[]),
    ("ff2", &[]),
    ("dn2", &[]),
    ("dd2", &[]),
    ("x89", &["x89_base", "x89c4"]),
    ("swa2", &[]),
    ("pds2", &[]),
    ("helios2", &[]),
    ("vtx", &[]),
    ("qdn", &[]),
    ("nes", &[]),
    ("sdn2", &[]),
    ("sr", &[]),
    ("asn", &[]),
    ("fl", &[]),
    ("pws2", &[]),
    ("hcf2", &[]),
    ("det", &[]),
    ("cm", &[]),
    ("ac2", &[]),
    ("se2", &[]),
    ("cr2", &[]),
    ("bs", &[]),
    ("mi", &[]),
    ("pfa", &[]),
    ("ws", &[]),
];

/// Index of a technology in [`TECHS`].
#[must_use]
pub fn tech_index(id: &str) -> Option<usize> {
    TECHS.iter().position(|(tech, _)| *tech == id)
}

/// The measured quantities (indexes into `FactionProgress::q`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Q {
    /// Breakthrough held (0/1).
    Bt,
    /// Commander no longer locked (0/1).
    Cmdr,
    /// Hero no longer locked (0/1).
    Hero,
    /// Controls Mecatol Rex (0/1).
    Mecatol,
    /// Unit-upgrade technologies owned.
    Upg,
    /// Colours (of four) in which the seat owns at least two technologies.
    Colours2,
    /// Space docks owned, on the board.
    Dock,
    /// Space docks in the home system.
    HomeDock,
    /// Space docks outside the home system.
    FarDock,
    /// PDS plus space docks (Hel-Titans count as PDS).
    Struct,
    /// Fighters on the board.
    Ftr,
    /// Fighters in the home system's space area.
    HomeFtr,
    /// Mechs on the board.
    Mech,
    /// War suns on the board.
    WarSun,
    /// Non-home planets controlled.
    Out,
    /// Action cards in hand (a public count).
    ActionCards,
    /// Units on the Cabal sheet.
    CapUnits,
    /// Valefar Assimilator tokens placed on technologies.
    Assim,
    /// Planets holding this seat's coexisting units.
    Coexist,
    /// Plot cards in play.
    Plots,
    /// The seat has flipped to the Obsidian (0/1).
    Flipped,
    /// Distinct other players with a token in the Mahact fleet pool.
    ForeignTok,
    /// Titans sleeper tokens placed.
    Sleeper,
    /// Hel-Titans on the board.
    HelTitan,
    /// Exploration cards this seat has drawn.
    Explore,
    /// Controls Mallice (0/1).
    Mallice,
}

/// Number of quantities.
pub const N_Q: usize = 26;

/// One seat's measured faction milestones.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactionProgress {
    /// 1-based position in [`FACTIONS`]; 0 when not measured.
    pub faction: u8,
    /// Quantities by [`Q`], saturated at 255.
    pub q: [u8; N_Q],
    /// Bit `i` set when the seat owns [`TECHS`]`[i]`.
    pub techs: u32,
}

impl FactionProgress {
    /// A quantity.
    #[must_use]
    pub const fn get(&self, q: Q) -> u8 {
        self.q[q as usize]
    }
    /// Whether technology `index` of [`TECHS`] is owned.
    #[must_use]
    pub const fn has_tech(&self, index: usize) -> bool {
        self.techs >> index & 1 == 1
    }
}

fn sat(n: usize) -> u8 {
    u8::try_from(n).unwrap_or(u8::MAX)
}

fn types_of<'a>(content: &'a ContentStore, id: &str) -> Vec<&'a str> {
    content
        .get(ContentType::Technologies, id)
        .map(|r| r.strings("types"))
        .unwrap_or_default()
}

/// Measure `me`. Zeroed (faction 0) for an unknown seat.
#[must_use]
pub fn measure(seen: &Observed<'_>, me: &PlayerId) -> FactionProgress {
    let mut out = FactionProgress::default();
    let (Some(seat), Some(player)) = (seen.seat(me), seen.probe_state().player(me)) else {
        return out;
    };
    let faction = seat.faction.as_str();
    out.faction = FACTIONS
        .iter()
        .position(|f| *f == faction)
        .map_or(0, |i| sat(i + 1));
    let content = seen.content();
    let q = &mut out.q;
    let mut set = |k: Q, n: usize| q[k as usize] = sat(n);

    // Leaders: Locked is the only state before the unlock (Purged heroes were unlocked first).
    let unlocked = |kind: &str| {
        seat.leaders
            .iter()
            .any(|(id, status)| id.as_str().contains(kind) && *status != LeaderStatus::Locked)
    };
    set(Q::Bt, usize::from(seat.breakthrough.is_some()));
    set(Q::Cmdr, usize::from(unlocked("commander")));
    set(Q::Hero, usize::from(unlocked("hero")));
    set(Q::Flipped, usize::from(faction == "obsidian"));

    // Technologies.
    for (i, (id, alts)) in TECHS.iter().enumerate() {
        let owned = seat.technologies.iter().any(|held| {
            let held = held.as_str();
            held == *id
                || alts.contains(&held)
                || content
                    .get(ContentType::Technologies, held)
                    .and_then(|r| r.text("baseUpgrade"))
                    == Some(*id)
        });
        out.techs |= u32::from(owned) << i;
    }
    let (mut upg, mut colours) = (0, BTreeMap::<&str, usize>::new());
    for tech in seat.technologies {
        let types = types_of(content, tech.as_str());
        if types.contains(&"UNITUPGRADE") {
            upg += 1;
        }
        for colour in ["PROPULSION", "BIOTIC", "WARFARE", "CYBERNETIC"] {
            if types.contains(&colour) {
                *colours.entry(colour).or_insert(0) += 1;
            }
        }
    }
    set(Q::Upg, upg);
    set(Q::Colours2, colours.values().filter(|n| **n >= 2).count());

    // Planets.
    let controlled = seen.controlled_planets(me);
    let home = ti4_content::factions::get(content, faction)
        .map(|f| f.home_planets())
        .unwrap_or_default();
    set(
        Q::Out,
        controlled
            .iter()
            .filter(|(_, p)| !home.contains(&p.as_str()))
            .count(),
    );
    set(
        Q::Mecatol,
        usize::from(
            controlled
                .iter()
                .any(|(_, p)| ti4_engine::seating::is_mecatol_planet(p.as_str())),
        ),
    );
    set(
        Q::Mallice,
        usize::from(controlled.iter().any(|(_, p)| p.as_str() == "mallice")),
    );

    // Board units.
    let catalogue = ti4_content::units::catalogue(content, seen.sources());
    let (mut dock, mut home_dock, mut far_dock, mut pds, mut hel) = (0, 0, 0, 0, 0);
    let (mut ftr, mut home_ftr, mut mech, mut ws) = (0, 0, 0, 0);
    for (sid, system) in seen.board() {
        let at_home = player.home_system.as_ref() == Some(sid);
        for (unit, in_space) in system
            .units
            .iter()
            .map(|u| (u, true))
            .chain(system.planet_units.values().flatten().map(|u| (u, false)))
            .filter(|(u, _)| &u.owner == me)
        {
            let Some(kind) = catalogue.get(unit.type_id.as_str()) else {
                continue;
            };
            match kind.base_type() {
                "spacedock" => {
                    dock += 1;
                    if at_home {
                        home_dock += 1
                    } else {
                        far_dock += 1
                    }
                }
                "pds" => {
                    pds += 1;
                    hel += usize::from(unit.type_id.as_str().starts_with("titans_pds"));
                }
                "mech" => mech += 1,
                "warsun" => ws += 1,
                _ => {}
            }
            if kind.is_fighter() {
                ftr += 1;
                home_ftr += usize::from(at_home && in_space);
            }
        }
    }
    set(Q::Dock, dock);
    set(Q::HomeDock, home_dock);
    set(Q::FarDock, far_dock);
    set(Q::Struct, dock + pds);
    set(Q::Ftr, ftr);
    set(Q::HomeFtr, home_ftr);
    set(Q::Mech, mech);
    set(Q::WarSun, ws);
    set(Q::HelTitan, hel);

    // Counts kept on the sheet or in public faction rows.
    set(Q::ActionCards, seat.action_cards_held);
    set(Q::CapUnits, player.captured_units.len());
    set(Q::Assim, player.assimilated_technologies.len());
    set(Q::Plots, player.plots.len());
    let mut foreign = seen.mahact_fleet_pool_owners(me);
    foreign.retain(|owner| owner != me);
    foreign.sort();
    foreign.dedup();
    set(Q::ForeignTok, foreign.len());
    let mine = format!("{me}|");
    set(
        Q::Sleeper,
        seen.faction_marks_with_prefix("titans:sleeper:")
            .iter()
            .filter(|(_, v)| v.starts_with(&mine))
            .count(),
    );
    set(
        Q::Coexist,
        seen.board()
            .values()
            .map(|b| {
                b.coexisting
                    .iter()
                    .filter(|(planet, who)| {
                        who.contains(me) && !b.on_planet_of(planet, me).is_empty()
                    })
                    .count()
            })
            .sum(),
    );
    set(
        Q::Explore,
        seen.probe_state()
            .exploration_log
            .iter()
            .filter(|r| &r.player == me)
            .count(),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_engine::fixtures::{game, hub_from, plain_systems, put};
    use ti4_model::POK;
    use ti4_model::id::{BreakthroughId, FactionId, LeaderId, SystemId, TechnologyId};

    fn me() -> PlayerId {
        PlayerId::new("a")
    }

    #[test]
    fn counts_and_flags_are_exact() {
        let ids = plain_systems(7);
        let hub = hub_from(&ids);
        let mut state = game(&["a", "b"]);
        let read = |s: &ti4_model::state::GameState| {
            measure(
                &Observed::new(s, ContentStore::embedded(), POK, Some(&hub.galaxy)),
                &me(),
            )
        };
        state.player_mut(&me()).expect("seat").faction = FactionId::new("titans");
        let base = read(&state);
        assert_eq!(base.faction, 27);
        assert_eq!(base.q[Q::Dock as usize], 0);

        let seat = state.player_mut(&me()).expect("seat");
        seat.technologies.insert(TechnologyId::new("ac2")); // counts as cv2
        seat.technologies.insert(TechnologyId::new("gd"));
        seat.technologies.insert(TechnologyId::new("fl"));
        seat.technologies.insert(TechnologyId::new("x89_base"));
        seat.leaders
            .insert(LeaderId::new("titanscommander"), LeaderStatus::Unlocked);
        seat.leaders
            .insert(LeaderId::new("titanshero"), LeaderStatus::Locked);
        seat.plots = vec!["a".into(), "b".into()];
        seat.breakthrough = Some(BreakthroughId::new("titansbt"));
        let system = SystemId::new(hub.outer[0].clone());
        put(&mut state, &system, "spacedock", &me(), 1);
        put(&mut state, &system, "fighter", &me(), 3);
        put(&mut state, &system, "warsun", &me(), 1);
        let got = read(&state);
        let has = |name: &str| got.has_tech(tech_index(name).expect("known"));
        assert!(has("cv2") && has("ac2") && has("gd") && has("x89") && !has("dn2"));
        assert_eq!(got.get(Q::Upg), 1);
        assert_eq!(got.get(Q::Cmdr), 1);
        assert_eq!(got.get(Q::Hero), 0);
        assert_eq!(
            (got.get(Q::Plots), got.get(Q::Dock), got.get(Q::FarDock)),
            (2, 1, 1)
        );
        assert_eq!(
            (got.get(Q::Ftr), got.get(Q::WarSun), got.get(Q::Struct)),
            (3, 1, 1)
        );
        assert_eq!(
            (got.get(Q::Colours2), got.get(Q::Bt)),
            (1, 1),
            "gd + fl are two propulsion techs"
        );
    }
}
