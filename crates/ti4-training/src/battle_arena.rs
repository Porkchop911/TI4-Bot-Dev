//! Pitched space battles outside a full game (ARENA-001/002): the fleet space and a lean simulator.
//!
//! The simulator models the two fleets and nothing else -- no game state, no choice windows, no
//! deciders -- which is what makes it fast enough to label fights while a predictor trains. With
//! effects off it matches `ti4_engine::combat::resolve` under the arena's declared
//! `cheapest-fresh` casualty policy (`battle_arena_probe --compare` checks this). With effects on
//! it adds what the engine does not yet model: the Jol-Nar, Letnev and L1Z1X flagships and both
//! sides' space cannon before combat. Checked against ti4calc by `battle_arena_ti4calc`.

use ti4_content::ContentStore;
use ti4_content::units::UnitType;
/// The content scope the arena reads units in: the scope games are played at (Thunder's Edge
/// included). The original six's units print the same here as under Prophecy of Kings
/// (`the_original_units_print_the_same_in_the_full_scope`).
const SOURCES: ti4_model::content_types::SourceSet = ti4_model::content_types::FULL;

/// Ship base types in composition order. Index 0 is always the fighter.
pub const SHIP_TYPES: [&str; 7] = [
    "fighter",
    "destroyer",
    "cruiser",
    "carrier",
    "dreadnought",
    "warsun",
    "flagship",
];

/// Most of each ship type a player has in their supply, in `SHIP_TYPES` order. Fighters are
/// bounded by capacity and a caller-chosen cap instead.
pub const SUPPLY: [usize; 7] = [usize::MAX, 8, 8, 4, 6, 2, 1];

/// Ship counts by base type, in `SHIP_TYPES` order.
pub type Composition = [usize; SHIP_TYPES.len()];

/// A faction and upgrade state, which together resolve each base type to a concrete unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Profile {
    pub faction: &'static str,
    pub upgraded: bool,
}

impl Profile {
    /// The six factions the bots play, and what separates each in a space battle.
    pub const CATALOGUE: [(&'static str, &'static str); 6] = [
        ("sol", "carrier carries 6; flagship carries 12"),
        ("letnev", "flagship repairs itself every combat round"),
        ("xxcha", "flagship has space cannon 3x5"),
        (
            "hacan",
            "generic ships; flagship is weak -- its trade-good bonus never applies (0 trade goods)",
        ),
        (
            "jolnar",
            "Fragile -1; flagship turns each 9 or 10 into 2 extra hits",
        ),
        (
            "l1z1x",
            "dreadnought carries 2; flagship forces hits onto non-fighters",
        ),
    ];

    /// Every faction a game can seat (the wide roster, the three Council Keleres listed apart),
    /// with and without upgrades when asked: the arena of battle feature version 8.
    #[must_use]
    pub fn wide(content: &ContentStore, upgrades: bool) -> Vec<Self> {
        let mut factions: Vec<&'static str> = Vec::new();
        for entry in ti4_engine::seating::wide_roster(content, SOURCES) {
            if entry == ti4_engine::seating::KELERES_FAMILY {
                factions.extend(ti4_engine::factions::keleres::VARIANTS);
            } else {
                factions.push(entry);
            }
        }
        let mut out = Vec::new();
        for faction in factions {
            out.push(Self {
                faction,
                upgraded: false,
            });
            if upgrades {
                out.push(Self {
                    faction,
                    upgraded: true,
                });
            }
        }
        out
    }

    /// Every catalogue faction, with and without upgrades when asked.
    #[must_use]
    pub fn all(upgrades: bool) -> Vec<Self> {
        let mut out = Vec::new();
        for (faction, _) in Self::CATALOGUE {
            out.push(Self {
                faction,
                upgraded: false,
            });
            if upgrades {
                out.push(Self {
                    faction,
                    upgraded: true,
                });
            }
        }
        out
    }

    #[must_use]
    pub fn label(self) -> String {
        format!(
            "{}{}",
            self.faction,
            if self.upgraded { "+upgrades" } else { "" }
        )
    }

    /// The unit this profile fields for a base type: the faction's own version where it has one,
    /// upgraded where the profile says so.
    #[must_use]
    pub fn unit_for(self, content: &ContentStore, base: &str) -> String {
        let own = ti4_content::units::faction_unit(content, self.faction, base, SOURCES)
            .map(|unit| unit.id().to_owned());
        let id = own.unwrap_or_else(|| base.to_owned());
        if !self.upgraded {
            return id;
        }
        ti4_content::units::unit_type(content, &id, SOURCES)
            .and_then(|unit| unit.upgrades_to().map(ToOwned::to_owned))
            .unwrap_or(id)
    }

    /// A composition as concrete unit ids with counts, empty slots dropped.
    #[must_use]
    pub fn resolve(
        self,
        content: &ContentStore,
        composition: &Composition,
    ) -> Vec<(String, usize)> {
        SHIP_TYPES
            .iter()
            .enumerate()
            .filter(|(index, _)| composition[*index] > 0)
            .map(|(index, base)| (self.unit_for(content, base), composition[index]))
            .collect()
    }
}

/// Whether the fleet's capacity carries its fighters.
#[must_use]
pub fn fighters_fit(content: &ContentStore, profile: Profile, composition: &Composition) -> bool {
    let mut capacity = 0i64;
    for (index, base) in SHIP_TYPES.iter().enumerate().skip(1) {
        let id = profile.unit_for(content, base);
        let provided =
            ti4_content::units::unit_type(content, &id, SOURCES).map_or(0, |unit| unit.capacity());
        capacity += provided * i64::try_from(composition[index]).unwrap_or(0);
    }
    i64::try_from(composition[0]).unwrap_or(i64::MAX) <= capacity
}

/// Every legal fleet for a profile: hulls within supply and `max_ships`, fighters within
/// `max_fighters` and capacity.
#[must_use]
pub fn compositions(
    content: &ContentStore,
    profile: Profile,
    max_ships: usize,
    max_fighters: usize,
) -> Vec<Composition> {
    let mut out = Vec::new();
    let mut current: Composition = [0; SHIP_TYPES.len()];
    let bounds = Bounds {
        content,
        profile,
        max_ships,
        max_fighters,
    };
    walk(&bounds, 1, 0, &mut current, &mut out);
    out
}

struct Bounds<'a> {
    content: &'a ContentStore,
    profile: Profile,
    max_ships: usize,
    max_fighters: usize,
}

fn walk(
    bounds: &Bounds<'_>,
    index: usize,
    used: usize,
    current: &mut Composition,
    out: &mut Vec<Composition>,
) {
    if index == SHIP_TYPES.len() {
        if used == 0 {
            return; // a fleet of fighters alone has no capacity to carry them
        }
        for fighters in 0..=bounds.max_fighters {
            current[0] = fighters;
            if !fighters_fit(bounds.content, bounds.profile, current) {
                break; // capacity only shrinks as fighters grow
            }
            out.push(*current);
        }
        current[0] = 0;
        return;
    }
    for count in 0..=SUPPLY[index].min(bounds.max_ships - used) {
        current[index] = count;
        walk(bounds, index + 1, used + count, current, out);
    }
    current[index] = 0;
}

/// Casualty order by base type for the declared policy: cheapest first, flagship before war sun,
/// as ti4calc orders them. Sustain is taken in the same order, so dreadnoughts sustain first.
/// `battle_arena_probe` ranks the engine's options with this too, or `--compare` means nothing.
#[must_use]
pub fn hull_rank(base_type: &str) -> usize {
    match base_type {
        "fighter" => 0,
        "destroyer" => 1,
        "cruiser" => 2,
        "carrier" => 3,
        "dreadnought" => 4,
        "flagship" => 5,
        "warsun" => 6,
        _ => usize::MAX,
    }
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

#[derive(Clone, Copy)]
struct Ship {
    /// Index into the side's `names`.
    name: u16,
    hits_on: i64,
    dice: u32,
    afb_hits_on: i64,
    afb_dice: u32,
    cannon_hits_on: i64,
    cannon_dice: u32,
    sustain: bool,
    damaged: bool,
    fighter: bool,
    /// J.N.S. Hylarim: each 9 or 10, before modifiers, produces 2 additional hits.
    bonus_on_nine: bool,
    /// Arc Secundus: repaired at the start of each space combat round.
    self_repair: bool,
    /// 0.0.1 in the fleet: this ship's hits must be assigned to non-fighters if able.
    forces_non_fighters: bool,
    /// A cruiser or destroyer, which Ambush may roll for.
    ambusher: bool,
    /// A static shift to this ship's own combat rolls: the Arvicon Rex's +2 under an edict, the
    /// Bastion flagship's +1 per conquered system.
    own_bonus: i64,
    /// The flagship whose ability this is, while it stands (see [`Flagship`]).
    flagship: Flagship,
}

/// Flagships whose abilities act inside a pitched space battle, each while it stands.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Flagship {
    None,
    /// The Fourth Moon (Mentak): other players' ships in this system cannot use SUSTAIN DAMAGE.
    FourthMoon,
    /// The C'morran N'orr (Sardakk): +1 to each of its owner's other ships' combat rolls.
    Cmorran,
    /// The Salai Sai Corian (Winnu): rolls one die, hitting on 7, per opposing non-fighter ship.
    Salai,
    /// The Van Hauge (Yin): when destroyed, destroy all ships in the system.
    VanHauge,
    /// The Quetzecoatl (Argent): other players cannot use SPACE CANNON against its owner's ships.
    Quetzecoatl,
}

/// What a fight's position says about a side beyond its units: conditions a flagship reads
/// from the rest of the board, carried as encoder inputs (battle feature version 8).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SideContext {
    /// The Arvicon Rex (Mahact): the opponent's command token is not in this side's fleet pool, so
    /// the flagship rolls +2.
    pub edict: bool,
    /// The Bastion flagship: non-home systems containing a planet this side controls (+1 each).
    pub conquests: i64,
    /// The Heaven's Eye (Firmament): the opponent has a control token on one of this side's plots,
    /// so the flagship is repaired at the end of every combat round -- for the outcome, the same
    /// as the Arc Secundus's repair at the start of the next.
    pub plot_repair: bool,
}

/// One side of a fight, ships kept in casualty order so assignment is a scan.
#[derive(Clone)]
pub struct Side {
    ships: Vec<Ship>,
    modifier: i64,
    /// Mentak Ambush: at the start of a space combat, up to 2 cruisers or destroyers each roll
    /// one die against their combat value, unmodified; each success is a hit.
    ambush: bool,
    /// Argent Raid Formation: barrage hits beyond the opponent's fighters each damage one of
    /// their ships that can sustain damage.
    raid_formation: bool,
    /// Space cannon that fires before combat but takes no part in it: PDS and mechs on planets
    /// here, and guns next door whose card lets them reach. `(hits_on, dice)`.
    guns: Vec<(i64, u32)>,
    /// Unit ids in first-seen order; survivors are counted against these.
    names: Vec<String>,
}

/// Units that may join a side as guns: they shoot before combat and are never hit in it.
pub const GUN_IDS: [&str; 4] = ["pds", "pds2", "xxcha_mech", "xxcha_flagship"];

impl Side {
    /// Build a side from unit ids and counts. `damaged` names how many ships of an id start the
    /// fight having already sustained damage; ids that cannot sustain are ignored.
    ///
    /// # Panics
    ///
    /// If a unit id is not in the content store.
    #[must_use]
    pub fn of(
        content: &ContentStore,
        fleet: &[(String, usize)],
        damaged: &[(String, usize)],
        faction: &str,
        effects: bool,
    ) -> Self {
        let l1z1x_flagship =
            effects && fleet.iter().any(|(id, n)| *n > 0 && id == "l1z1x_flagship");
        let flagship_of = |id: &str| -> Flagship {
            if !effects {
                return Flagship::None;
            }
            match id {
                "mentak_flagship" => Flagship::FourthMoon,
                "sardakk_flagship" => Flagship::Cmorran,
                "winnu_flagship" => Flagship::Salai,
                "yin_flagship" => Flagship::VanHauge,
                "argent_flagship" => Flagship::Quetzecoatl,
                _ => Flagship::None,
            }
        };
        let mut keyed: Vec<((usize, String), Ship)> = Vec::new();
        let mut names: Vec<String> = Vec::new();
        for (id, count) in fleet {
            let unit = ti4_content::units::unit_type(content, id, SOURCES).expect("unit exists");
            let name = names
                .iter()
                .position(|known| known == id)
                .unwrap_or_else(|| {
                    names.push(id.clone());
                    names.len() - 1
                });
            let ship = Ship {
                name: u16::try_from(name).expect("few unit types"),
                hits_on: unit.combat_hits_on().unwrap_or(11),
                dice: u32::try_from(unit.combat_dice()).unwrap_or(0),
                afb_hits_on: unit.afb_hits_on().unwrap_or(11),
                afb_dice: u32::try_from(unit.afb_dice()).unwrap_or(0),
                cannon_hits_on: unit.space_cannon_hits_on().unwrap_or(11),
                // The engine's `resolve` runs no space cannon, so it is an effect here too.
                cannon_dice: if effects {
                    u32::try_from(unit.space_cannon_dice()).unwrap_or(0)
                } else {
                    0
                },
                sustain: unit.sustain_damage(),
                damaged: false,
                fighter: unit.is_fighter(),
                bonus_on_nine: effects && id == "jolnar_flagship",
                self_repair: effects && id == "letnev_flagship",
                forces_non_fighters: l1z1x_flagship
                    && matches!(unit.base_type(), "flagship" | "dreadnought"),
                ambusher: matches!(unit.base_type(), "cruiser" | "destroyer"),
                own_bonus: 0,
                flagship: flagship_of(id),
            };
            let hurt = damaged
                .iter()
                .find(|(which, _)| which == id)
                .map_or(0, |(_, n)| *n);
            for copy in 0..*count {
                let mut ship = ship;
                ship.damaged = ship.sustain && copy < hurt;
                keyed.push(((hull_rank(unit.base_type()), id.clone()), ship));
            }
        }
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        Self {
            ships: keyed.into_iter().map(|(_, ship)| ship).collect(),
            guns: Vec::new(),
            names,
            modifier: modifier(faction),
            ambush: effects && faction == "mentak",
            raid_formation: effects && faction == "argent",
        }
    }

    /// The same side under a board context: the Arvicon Rex's edict and the Bastion flagship's
    /// conquests become fixed shifts to those flagships' own rolls.
    #[must_use]
    pub fn with_context(mut self, context: SideContext) -> Self {
        for ship in &mut self.ships {
            let id = self.names[usize::from(ship.name)].as_str();
            ship.own_bonus = match id {
                "mahact_flagship" if context.edict => 2,
                "bastion_flagship" => context.conquests.max(0),
                _ => 0,
            };
            if id == "firmament_flagship" {
                ship.self_repair = context.plot_repair;
            }
        }
        self
    }

    fn standing(&self, flagship: Flagship) -> bool {
        self.ships.iter().any(|ship| ship.flagship == flagship)
    }

    fn non_fighters(&self) -> usize {
        self.ships.iter().filter(|ship| !ship.fighter).count()
    }

    fn fighters(&self) -> usize {
        self.ships.iter().filter(|ship| ship.fighter).count()
    }

    /// The same side with its space cannon silenced: what `combat::resolve` fights, since the
    /// engine fires space cannon in the tactical action rather than in the combat.
    #[must_use]
    pub fn without_cannon(mut self) -> Self {
        for ship in &mut self.ships {
            ship.cannon_dice = 0;
        }
        self.guns.clear();
        self
    }

    /// The unit ids survivor counts refer to.
    #[must_use]
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// Ships per name, the counts survivors are measured against.
    #[must_use]
    pub fn fielded(&self) -> Vec<usize> {
        let mut counts = vec![0; self.names.len()];
        for ship in &self.ships {
            counts[usize::from(ship.name)] += 1;
        }
        counts
    }

    /// Add guns by unit id and count. Only their SPACE CANNON is read.
    ///
    /// # Panics
    ///
    /// If a unit id is not in the content store.
    #[must_use]
    pub fn with_guns(mut self, content: &ContentStore, guns: &[(String, usize)]) -> Self {
        for (id, count) in guns {
            let unit = ti4_content::units::unit_type(content, id, SOURCES).expect("unit exists");
            let (Some(hits_on), dice) = (unit.space_cannon_hits_on(), unit.space_cannon_dice())
            else {
                continue;
            };
            let dice = u32::try_from(dice).unwrap_or(0);
            self.guns
                .extend(std::iter::repeat_n((hits_on, dice), *count));
        }
        self
    }

    /// Hits free to land anywhere, and hits that must land on non-fighters. `opposing_hulls` is the
    /// opponent's non-fighter ships, which the Salai Sai Corian rolls one die for each.
    fn roll(&self, rng: &mut Rng, opposing_hulls: usize) -> (usize, usize) {
        let (mut free, mut forced) = (0, 0);
        let cmorran = self.standing(Flagship::Cmorran);
        for ship in &self.ships {
            let dice = if ship.flagship == Flagship::Salai {
                u32::try_from(opposing_hulls).unwrap_or(u32::MAX)
            } else {
                ship.dice
            };
            let shift = self.modifier
                + ship.own_bonus
                + i64::from(cmorran && ship.flagship != Flagship::Cmorran);
            for _ in 0..dice {
                let face = rng.die();
                let mut hits = usize::from(face + shift >= ship.hits_on);
                if ship.bonus_on_nine && face >= 9 {
                    hits += 2;
                }
                if ship.forces_non_fighters {
                    forced += hits;
                } else {
                    free += hits;
                }
            }
        }
        (free, forced)
    }

    fn barrage(&self, rng: &mut Rng) -> usize {
        let mut hits = 0;
        for ship in &self.ships {
            for _ in 0..ship.afb_dice {
                if rng.die() >= ship.afb_hits_on {
                    hits += 1;
                }
            }
        }
        hits
    }

    /// Space cannon is not a combat roll, so Fragile does not apply.
    fn cannon(&self, rng: &mut Rng) -> usize {
        let mut hits = 0;
        let ships = self
            .ships
            .iter()
            .map(|ship| (ship.cannon_hits_on, ship.cannon_dice));
        for (hits_on, dice) in ships.chain(self.guns.iter().copied()) {
            for _ in 0..dice {
                if rng.die() >= hits_on {
                    hits += 1;
                }
            }
        }
        hits
    }

    fn repair(&mut self) {
        for ship in &mut self.ships {
            if ship.self_repair {
                ship.damaged = false;
            }
        }
    }

    fn lose_fighters(&mut self, mut hits: usize) {
        let mut index = 0;
        while hits > 0 && index < self.ships.len() {
            if self.ships[index].fighter {
                self.ships.remove(index);
                hits -= 1;
            } else {
                index += 1;
            }
        }
    }

    /// Take `hits` under the declared casualty policy. `may_sustain` is false while the opponent's
    /// Fourth Moon stands. Returns whether the Van Hauge was destroyed, which destroys every ship
    /// in the system: the caller empties both sides.
    fn absorb(&mut self, hits: usize, non_fighters_only: bool, may_sustain: bool) -> bool {
        for _ in 0..hits {
            if self.ships.is_empty() {
                return false;
            }
            // "If able": with no non-fighter left, the hit falls on fighters as usual.
            let restrict = non_fighters_only && self.ships.iter().any(|s| !s.fighter);
            if may_sustain
                && let Some(ship) = self
                    .ships
                    .iter_mut()
                    .find(|s| (!restrict || !s.fighter) && s.sustain && !s.damaged)
            {
                ship.damaged = true;
                continue;
            }
            let at = self
                .ships
                .iter()
                .position(|s| (!restrict || !s.fighter) && !s.damaged)
                .or_else(|| self.ships.iter().position(|s| !restrict || !s.fighter))
                .unwrap_or(0);
            let lost = self.ships.remove(at);
            if lost.flagship == Flagship::VanHauge {
                return true;
            }
        }
        false
    }

    /// Raid Formation: each excess barrage hit damages one undamaged ship that can sustain damage,
    /// the largest first (the Argent player chooses; the arena declares the largest).
    fn raid(&mut self, excess: usize) {
        for _ in 0..excess {
            let Some(ship) = self
                .ships
                .iter_mut()
                .rev()
                .find(|s| !s.fighter && s.sustain && !s.damaged)
            else {
                return;
            };
            ship.damaged = true;
        }
    }

    /// Ambush: the (up to) two cruisers or destroyers with the best combat value each roll once,
    /// unmodified.
    fn ambush_hits(&self, rng: &mut Rng) -> usize {
        if !self.ambush {
            return 0;
        }
        let mut values: Vec<i64> = self
            .ships
            .iter()
            .filter(|ship| ship.ambusher)
            .map(|ship| ship.hits_on)
            .collect();
        values.sort_unstable();
        values
            .into_iter()
            .take(2)
            .map(|value| usize::from(rng.die() >= value))
            .sum()
    }
}

/// splitmix64: fast, and seeded per fight so a panel is reproducible.
pub struct Rng(u64);

impl Rng {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n`, for `n` below 2^32.
    pub fn below(&mut self, n: usize) -> usize {
        let n = u64::try_from(n).unwrap_or(u64::MAX);
        usize::try_from(((self.next_u64() >> 32) * n) >> 32).unwrap_or(0)
    }

    fn die(&mut self) -> i64 {
        1 + i64::try_from(self.below(10)).unwrap_or(0)
    }
}

/// Winner (`Some("a")` attacker, `Some("b")` defender, `None` mutual destruction) and rounds.
#[must_use]
pub fn fight(attacker: &Side, defender: &Side, seed: u64) -> (Option<&'static str>, u32) {
    fight_with(attacker, defender, seed, true)
}

/// As [`fight`], choosing whether the attacker's own space cannon fires before combat.
///
/// Both sides fire (ti4calc agrees). The engine currently lets only non-active players fire,
/// which is an engine bug; `false` reproduces it.
#[must_use]
pub fn fight_with(
    attacker: &Side,
    defender: &Side,
    seed: u64,
    attacker_cannon: bool,
) -> (Option<&'static str>, u32) {
    let outcome = fight_outcome(attacker, defender, seed, attacker_cannon);
    (outcome.winner, outcome.rounds)
}

/// How a fight ended, and what was left of each side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// `Some("a")` attacker, `Some("b")` defender, `None` mutual destruction or, with
    /// [`Outcome::unresolved`], a space fight stopped at the round cap.
    pub winner: Option<&'static str>,
    pub rounds: u32,
    /// Surviving ships per entry of the side's [`Side::names`].
    pub attacker_left: Vec<usize>,
    pub defender_left: Vec<usize>,
    /// Both sides still had units when the round cap ([`ROUND_CAP`]) stopped the fight. A space
    /// fight reports it as `winner: None`, the same as mutual destruction, so this is how the two
    /// are told apart; a ground fight reports it as a failed invasion.
    pub unresolved: bool,
}

/// Rounds after which a fight is stopped unresolved.
pub const ROUND_CAP: u32 = 50;

/// As [`fight_with`], also counting survivors.
#[must_use]
pub fn fight_outcome(
    attacker: &Side,
    defender: &Side,
    seed: u64,
    attacker_cannon: bool,
) -> Outcome {
    fight_core(attacker, defender, seed, attacker_cannon, false)
}

/// A fight already under way, as it stands when retreats are announced: space cannon and the
/// round-1 barrage are behind it (LRR 78.3 precedes 78.4), so neither fires again.
#[must_use]
pub fn fight_in_progress(attacker: &Side, defender: &Side, seed: u64) -> Outcome {
    fight_core(attacker, defender, seed, false, true)
}

fn fight_core(
    attacker: &Side,
    defender: &Side,
    seed: u64,
    attacker_cannon: bool,
    in_progress: bool,
) -> Outcome {
    let mut rng = Rng::new(seed);
    let (mut a, mut d) = (attacker.clone(), defender.clone());
    let defender_had_ships = !d.ships.is_empty();
    // Whether each side may sustain: not while the other side's Fourth Moon stands.
    let sustains = |a: &Side, d: &Side| {
        (
            !d.standing(Flagship::FourthMoon),
            !a.standing(Flagship::FourthMoon),
        )
    };
    // The Van Hauge destroyed: every ship in the system goes with it.
    let blast = |a: &mut Side, d: &mut Side| {
        a.ships.clear();
        d.ships.clear();
    };
    if !in_progress {
        // Space cannon offence: both sides roll, then both absorb. Nobody may fire on a side
        // whose Quetzecoatl stands.
        let a_hits = if attacker_cannon && !d.standing(Flagship::Quetzecoatl) {
            a.cannon(&mut rng)
        } else {
            0
        };
        let d_hits = if a.standing(Flagship::Quetzecoatl) {
            0
        } else {
            d.cannon(&mut rng)
        };
        let (a_may, d_may) = sustains(&a, &d);
        let blown = d.absorb(a_hits, false, d_may) | a.absorb(d_hits, false, a_may);
        if blown {
            blast(&mut a, &mut d);
        }
    }
    let mut rounds = 0;
    for round in 1..=ROUND_CAP {
        if a.ships.is_empty() || d.ships.is_empty() {
            break;
        }
        a.repair();
        d.repair();
        if round == 1 && !in_progress {
            // "At the start of a space combat" (Ambush), then the barrage.
            let (ha, hd) = (a.ambush_hits(&mut rng), d.ambush_hits(&mut rng));
            let (a_may, d_may) = sustains(&a, &d);
            if d.absorb(ha, false, d_may) | a.absorb(hd, false, a_may) {
                blast(&mut a, &mut d);
                break;
            }
            if a.ships.is_empty() || d.ships.is_empty() {
                break;
            }
            let (ha, hd) = (a.barrage(&mut rng), d.barrage(&mut rng));
            let (a_fighters, d_fighters) = (a.fighters(), d.fighters());
            d.lose_fighters(ha);
            a.lose_fighters(hd);
            if a.raid_formation {
                d.raid(ha.saturating_sub(d_fighters));
            }
            if d.raid_formation {
                a.raid(hd.saturating_sub(a_fighters));
            }
            if a.ships.is_empty() || d.ships.is_empty() {
                break;
            }
        }
        rounds = round;
        let (a_free, a_forced) = a.roll(&mut rng, d.non_fighters());
        let (d_free, d_forced) = d.roll(&mut rng, a.non_fighters());
        // The defender takes its casualties first, then the attacker -- the engine's order -- so a
        // Fourth Moon destroyed by the attacker's hits no longer stops the attacker sustaining.
        let d_may = sustains(&a, &d).1;
        let mut blown = d.absorb(a_forced, true, d_may);
        blown |= d.absorb(a_free, false, d_may);
        let a_may = sustains(&a, &d).0;
        blown |= a.absorb(d_forced, true, a_may);
        blown |= a.absorb(d_free, false, a_may);
        if blown {
            blast(&mut a, &mut d);
        }
    }
    let unresolved = !a.ships.is_empty() && !d.ships.is_empty();
    let winner = match (a.ships.is_empty(), d.ships.is_empty()) {
        (false, true) => Some("a"),
        (true, false) => Some("b"),
        // Guns alone cannot be destroyed: an attacker they wipe out simply failed.
        (true, true) if !defender_had_ships => Some("b"),
        _ => None,
    };
    Outcome {
        winner,
        rounds,
        attacker_left: a.fielded(),
        defender_left: d.fielded(),
        unresolved,
    }
}

/// A ground force in the lean invasion.
#[derive(Clone, Copy)]
struct Trooper {
    name: u16,
    hits_on: i64,
    dice: u32,
    sustain: bool,
    damaged: bool,
    cost: f64,
    infantry: bool,
    /// The unit's own SPACE CANNON (the Xxcha mech): it fires in space cannon defense only while
    /// the unit stands.
    gun: Option<(i64, u32)>,
}

/// One side of a ground combat on one planet: its ground forces, and what fires for it outside
/// the combat rounds.
#[derive(Clone)]
pub struct GroundSide {
    troops: Vec<Trooper>,
    names: Vec<String>,
    modifier: i64,
    /// Shield Paling (the Jol-Nar mech) on the planet: infantry ignore Fragile.
    infantry_ignore_modifier: bool,
    /// Space cannon defense (defender) as `(hits_on, dice)`.
    guns: Vec<(i64, u32)>,
    /// Bombardment dice `(hits_on, dice)` fired before landing, when the planet allows it.
    bombard: Vec<(i64, u32)>,
    /// Harrow: the bombardment dice fired again after every round (L1Z1X, unshielded planet).
    harrow: Vec<(i64, u32)>,
}

impl GroundSide {
    /// Ground forces by unit id; `damaged` names mechs that start having sustained.
    ///
    /// # Panics
    ///
    /// If a unit id is not in the content store.
    #[must_use]
    pub fn of(
        content: &ContentStore,
        forces: &[(String, usize)],
        damaged: &[(String, usize)],
        faction: &str,
    ) -> Self {
        let mut names: Vec<String> = Vec::new();
        let mut troops = Vec::new();
        for (id, count) in forces {
            let unit = ti4_content::units::unit_type(content, id, SOURCES).expect("unit exists");
            let name = names
                .iter()
                .position(|known| known == id)
                .unwrap_or_else(|| {
                    names.push(id.clone());
                    names.len() - 1
                });
            let hurt = damaged
                .iter()
                .find(|(which, _)| which == id)
                .map_or(0, |(_, n)| *n);
            for copy in 0..*count {
                troops.push(Trooper {
                    name: u16::try_from(name).expect("few unit types"),
                    hits_on: unit.combat_hits_on().unwrap_or(11),
                    dice: u32::try_from(unit.combat_dice()).unwrap_or(0),
                    sustain: unit.sustain_damage(),
                    damaged: unit.sustain_damage() && copy < hurt,
                    cost: unit.cost(),
                    infantry: unit.base_type() == "infantry",
                    gun: unit
                        .space_cannon_hits_on()
                        .map(|on| (on, u32::try_from(unit.space_cannon_dice()).unwrap_or(0))),
                });
            }
        }
        let jolnar_mech =
            faction == "jolnar" && forces.iter().any(|(id, n)| *n > 0 && id == "jolnar_mech");
        let mut side = Self {
            troops,
            names,
            modifier: modifier(faction),
            infantry_ignore_modifier: jolnar_mech,
            guns: Vec::new(),
            bombard: Vec::new(),
            harrow: Vec::new(),
        };
        side.sort();
        side
    }

    /// Casualty order, as the engine's ground hit takes them: cheapest first, a damaged unit
    /// before a fresh one of the same cost, then by unit id.
    fn sort(&mut self) {
        let names = self.names.clone();
        self.troops.sort_by(|a, b| {
            a.cost
                .total_cmp(&b.cost)
                .then(b.damaged.cmp(&a.damaged))
                .then(names[usize::from(a.name)].cmp(&names[usize::from(b.name)]))
        });
    }

    /// Add space cannon defense by structure id (PDS, PDS II). Ground forces with SPACE CANNON
    /// (the Xxcha mech) already carry theirs and fire only while they stand.
    #[must_use]
    pub fn with_defense_guns(mut self, content: &ContentStore, guns: &[(String, usize)]) -> Self {
        self.guns.extend(dice_of(content, guns, |unit| {
            unit.space_cannon_hits_on()
                .map(|on| (on, unit.space_cannon_dice()))
        }));
        self
    }

    /// Add the invading side's bombardment by unit id. `before_landing` fires once before the
    /// forces land; `harrow` fires again after every round.
    #[must_use]
    pub fn with_bombardment(
        mut self,
        content: &ContentStore,
        units: &[(String, usize)],
        before_landing: bool,
        harrow: bool,
    ) -> Self {
        let dice = dice_of(content, units, |unit| {
            unit.bombard_hits_on().map(|on| (on, unit.bombard_dice()))
        });
        if before_landing {
            self.bombard.extend(dice.iter().copied());
        }
        if harrow {
            self.harrow.extend(dice);
        }
        self
    }

    /// The unit ids survivor counts refer to.
    #[must_use]
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// Ground forces per name.
    #[must_use]
    pub fn fielded(&self) -> Vec<usize> {
        let mut counts = vec![0; self.names.len()];
        for troop in &self.troops {
            counts[usize::from(troop.name)] += 1;
        }
        counts
    }

    fn roll(&self, rng: &mut Rng) -> usize {
        let mut hits = 0;
        for troop in &self.troops {
            let shift = if troop.infantry && self.infantry_ignore_modifier && self.modifier < 0 {
                0
            } else {
                self.modifier
            };
            for _ in 0..troop.dice {
                if rng.die() + shift >= troop.hits_on {
                    hits += 1;
                }
            }
        }
        hits
    }

    fn absorb(&mut self, hits: usize) {
        for _ in 0..hits {
            if let Some(troop) = self
                .troops
                .iter_mut()
                .find(|troop| troop.sustain && !troop.damaged)
            {
                troop.damaged = true;
                continue;
            }
            if self.troops.is_empty() {
                return;
            }
            // Mechs may have taken damage since the last sort: damaged before fresh at equal cost.
            self.sort();
            self.troops.remove(0);
        }
    }
}

fn dice_of(
    content: &ContentStore,
    units: &[(String, usize)],
    read: impl Fn(UnitType<'_>) -> Option<(i64, i64)>,
) -> Vec<(i64, u32)> {
    let mut out = Vec::new();
    for (id, count) in units {
        let unit = ti4_content::units::unit_type(content, id, SOURCES).expect("unit exists");
        if let Some((on, dice)) = read(unit) {
            let dice = u32::try_from(dice).unwrap_or(0);
            out.extend(std::iter::repeat_n((on, dice), *count));
        }
    }
    out
}

fn volley(dice: &[(i64, u32)], rng: &mut Rng) -> usize {
    let mut hits = 0;
    for (on, count) in dice {
        for _ in 0..*count {
            if rng.die() >= *on {
                hits += 1;
            }
        }
    }
    hits
}

/// A ground combat on one planet (LRR 49): the invader's bombardment if any, space cannon
/// defense against the landed forces, then simultaneous rounds with Harrow after each.
///
/// The winner is `Some("a")` when the invader takes the planet and `Some("b")` when the defender
/// holds it, including when both sides are wiped out (49.5d); never `None`.
#[must_use]
pub fn ground_fight(attacker: &GroundSide, defender: &GroundSide, seed: u64) -> Outcome {
    let mut rng = Rng::new(seed);
    let (mut a, mut d) = (attacker.clone(), defender.clone());
    let hits = volley(&a.bombard, &mut rng);
    d.absorb(hits);
    let standing: Vec<(i64, u32)> = d
        .guns
        .iter()
        .copied()
        .chain(d.troops.iter().filter_map(|troop| troop.gun))
        .collect();
    let hits = volley(&standing, &mut rng);
    a.absorb(hits);
    let mut rounds = 0;
    for round in 1..=ROUND_CAP {
        if a.troops.is_empty() || d.troops.is_empty() {
            break;
        }
        rounds = round;
        let (ha, hd) = (a.roll(&mut rng), d.roll(&mut rng));
        d.absorb(ha);
        a.absorb(hd);
        let harrow = volley(&a.harrow, &mut rng);
        d.absorb(harrow);
    }
    let unresolved = !a.troops.is_empty() && !d.troops.is_empty();
    let winner = if !a.troops.is_empty() && d.troops.is_empty() {
        Some("a")
    } else {
        Some("b")
    };
    Outcome {
        winner,
        rounds,
        attacker_left: a.fielded(),
        defender_left: d.fielded(),
        unresolved,
    }
}

/// FNV-1a, for split keys that must not depend on the platform's hasher.
#[must_use]
pub fn fnv(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hand_count_of_fleets_holds() {
        // 8 ships, 16 fighters, supply limits, generic capacities with a capacity-3 flagship:
        // 10,719 without a flagship plus 8,163 with one.
        let content = ContentStore::embedded();
        let hacan = Profile {
            faction: "hacan",
            upgraded: false,
        };
        assert_eq!(compositions(content, hacan, 8, 16).len(), 18_882);
    }

    #[test]
    fn a_fight_nobody_can_win_is_unresolved_not_mutual_destruction() {
        // Space docks roll no combat dice, so neither side can ever score a hit.
        let content = ContentStore::embedded();
        let docks = vec![("spacedock".to_owned(), 1)];
        let a = Side::of(content, &docks, &[], "hacan", true);
        let d = Side::of(content, &docks, &[], "sol", true);
        let outcome = fight_outcome(&a, &d, 3, true);
        assert!(outcome.unresolved);
        assert_eq!(outcome.winner, None);
        assert_eq!(outcome.rounds, ROUND_CAP);

        let fleet = vec![("dreadnought".to_owned(), 2)];
        let strong = Side::of(content, &fleet, &[], "hacan", true);
        let weak = Side::of(content, &[("fighter".to_owned(), 1)], &[], "sol", true);
        assert!(!fight_outcome(&strong, &weak, 3, true).unresolved);
    }

    #[test]
    fn a_lone_fleet_beats_nothing_and_damage_is_applied() {
        let content = ContentStore::embedded();
        let fleet = vec![("dreadnought".to_owned(), 2)];
        let hurt = Side::of(
            content,
            &fleet,
            &[("dreadnought".to_owned(), 1)],
            "hacan",
            true,
        );
        assert_eq!(hurt.ships.iter().filter(|s| s.damaged).count(), 1);
        let empty = Side::of(content, &[], &[], "hacan", true);
        assert_eq!(fight(&hurt, &empty, 7).0, Some("a"));
    }
}

#[cfg(test)]
mod wide_tests {
    //! The version-8 arena: every seatable faction, and the space-combat effects the engine's
    //! comparison probe cannot reach (timing-window abilities) or reaches only statistically.
    use super::*;

    fn fleet(units: &[(&str, usize)]) -> Vec<(String, usize)> {
        units.iter().map(|(id, n)| ((*id).to_owned(), *n)).collect()
    }

    fn side(units: &[(&str, usize)], faction: &str) -> Side {
        Side::of(ContentStore::embedded(), &fleet(units), &[], faction, true)
    }

    /// Attacker win rate over `n` seeds.
    fn win_rate(a: &Side, d: &Side, n: u64) -> f64 {
        let wins = (0..n)
            .filter(|seed| fight_outcome(a, d, *seed, true).winner == Some("a"))
            .count();
        #[expect(clippy::cast_precision_loss, reason = "small counts")]
        let rate = wins as f64 / n as f64;
        rate
    }

    #[test]
    fn the_original_units_print_the_same_in_the_full_scope() {
        let content = ContentStore::embedded();
        let ids = [
            "carrier",
            "carrier2",
            "cruiser",
            "cruiser2",
            "destroyer",
            "destroyer2",
            "dreadnought",
            "dreadnought2",
            "fighter",
            "fighter2",
            "hacan_flagship",
            "jolnar_flagship",
            "l1z1x_dreadnought",
            "l1z1x_dreadnought2",
            "l1z1x_flagship",
            "letnev_flagship",
            "sol_carrier",
            "sol_carrier2",
            "sol_flagship",
            "warsun",
            "xxcha_flagship",
            "pds",
            "pds2",
            "xxcha_mech",
            "infantry",
            "infantry2",
        ];
        for id in ids {
            let old = ti4_content::units::unit_type(content, id, ti4_model::POK).unwrap();
            let new = ti4_content::units::unit_type(content, id, SOURCES).unwrap();
            let stats = |u: &UnitType| {
                (
                    u.combat_hits_on(),
                    u.combat_dice(),
                    u.afb_hits_on(),
                    u.afb_dice(),
                    u.space_cannon_hits_on(),
                    u.space_cannon_dice(),
                    u.sustain_damage(),
                    u.capacity(),
                )
            };
            assert_eq!(stats(&old), stats(&new), "{id}");
        }
    }

    #[test]
    fn every_seatable_faction_fields_legal_fleets() {
        let content = ContentStore::embedded();
        let profiles = Profile::wide(content, true);
        assert!(profiles.len() >= 60, "{} profiles", profiles.len());
        for profile in profiles {
            let fleets = compositions(content, profile, 2, 2);
            assert!(!fleets.is_empty(), "{}", profile.label());
            for composition in fleets.iter().take(50) {
                let units = profile.resolve(content, composition);
                let a = Side::of(content, &units, &[], profile.faction, true);
                let d = side(&[("cruiser", 2)], "hacan");
                let _ = fight_outcome(&a, &d, 1, true);
            }
        }
    }

    #[test]
    fn the_van_hauge_takes_every_ship_with_it() {
        // A lone Van Hauge against a lone dreadnought: whenever the flagship dies, so does the
        // dreadnought -- the defender never loses while the attacker survives.
        let yin = side(&[("yin_flagship", 1)], "yin");
        let dread = side(&[("dreadnought", 1)], "hacan");
        let mut blasts = 0;
        for seed in 0..400 {
            let outcome = fight_outcome(&dread, &yin, seed, true);
            if outcome.defender_left.iter().sum::<usize>() == 0 {
                assert_eq!(
                    outcome.attacker_left.iter().sum::<usize>(),
                    0,
                    "seed {seed}"
                );
                assert_eq!(outcome.winner, None);
                blasts += 1;
            }
        }
        assert!(blasts > 0, "the flagship never died in 400 fights");
    }

    #[test]
    fn the_fourth_moon_stops_the_opponent_sustaining() {
        let content = ContentStore::embedded();
        let dreads = side(&[("dreadnought", 2)], "hacan");
        let with = side(&[("mentak_flagship", 1)], "mentak");
        let without = Side::of(
            content,
            &fleet(&[("mentak_flagship", 1)]),
            &[],
            "hacan",
            false,
        );
        assert!(win_rate(&dreads, &with, 4000) < win_rate(&dreads, &without, 4000) - 0.05);
    }

    #[test]
    fn the_cmorran_lifts_its_owners_other_ships() {
        let content = ContentStore::embedded();
        // A close fight (about 53% without it), so the +1 to the cruisers shows.
        let enemy = side(&[("cruiser", 6)], "hacan");
        let fleet_ids = fleet(&[("sardakk_flagship", 1), ("cruiser", 3)]);
        let on = Side::of(content, &fleet_ids, &[], "sardakk", true);
        let off = Side::of(content, &fleet_ids, &[], "sardakk", false);
        assert!(win_rate(&on, &enemy, 4000) > win_rate(&off, &enemy, 4000) + 0.03);
    }

    #[test]
    fn the_salai_rolls_a_die_per_opposing_hull() {
        let winnu = side(&[("winnu_flagship", 1)], "winnu");
        let carrier = side(&[("carrier", 1)], "hacan");
        // No dice printed: against a carrier it rolls one die a round and usually wins.
        assert!(win_rate(&winnu, &carrier, 2000) > 0.4);
        let fighters_only = side(&[("fighter", 3)], "hacan");
        assert!(
            win_rate(&winnu, &fighters_only, 500).abs() < f64::EPSILON,
            "fighters are not hulls, so it never rolls"
        );
    }

    #[test]
    fn the_quetzecoatl_silences_space_cannon_against_its_owner() {
        let content = ContentStore::embedded();
        let argent = side(&[("argent_flagship", 1)], "argent");
        let guns =
            Side::of(content, &[], &[], "hacan", true).with_guns(content, &fleet(&[("pds2", 4)]));
        assert!((win_rate(&argent, &guns, 300) - 1.0).abs() < f64::EPSILON);
        let hacan = side(&[("hacan_flagship", 1)], "hacan");
        assert!(
            win_rate(&hacan, &guns, 2000) < 1.0,
            "an ordinary flagship can be shot"
        );
    }

    #[test]
    fn ambush_and_raid_formation_fire_only_for_their_factions() {
        let content = ContentStore::embedded();
        let enemy = side(&[("destroyer", 3)], "hacan");
        let cruisers = fleet(&[("cruiser", 2)]);
        let mentak = Side::of(content, &cruisers, &[], "mentak", true);
        let plain = Side::of(content, &cruisers, &[], "hacan", true);
        assert!(win_rate(&mentak, &enemy, 4000) > win_rate(&plain, &enemy, 4000) + 0.05);

        // Raid Formation: barrage hits beyond the fighters damage sustaining ships.
        let destroyers = fleet(&[("argent_destroyer2", 4)]);
        let argent = Side::of(content, &destroyers, &[], "argent", true);
        let neutral = Side::of(content, &destroyers, &[], "hacan", true);
        let target = side(&[("dreadnought", 2)], "letnev");
        assert!(win_rate(&argent, &target, 4000) > win_rate(&neutral, &target, 4000));
    }

    #[test]
    fn board_context_shifts_the_conditional_flagships() {
        // Two cruisers: a fight each flagship can win or lose (66%, 10%, 66% without context).
        let enemy = side(&[("cruiser", 2)], "hacan");
        let mahact = side(&[("mahact_flagship", 1)], "mahact");
        let edict = mahact.clone().with_context(SideContext {
            edict: true,
            ..SideContext::default()
        });
        assert!(win_rate(&edict, &enemy, 4000) > win_rate(&mahact, &enemy, 4000) + 0.05);

        let bastion = side(&[("bastion_flagship", 1)], "bastion");
        let conquered = bastion.clone().with_context(SideContext {
            conquests: 4,
            ..SideContext::default()
        });
        assert!(win_rate(&conquered, &enemy, 4000) > win_rate(&bastion, &enemy, 4000) + 0.05);

        let firmament = side(&[("firmament_flagship", 1)], "firmament");
        let puppeted = firmament.clone().with_context(SideContext {
            plot_repair: true,
            ..SideContext::default()
        });
        assert!(win_rate(&puppeted, &enemy, 4000) > win_rate(&firmament, &enemy, 4000) + 0.05);
    }
}
