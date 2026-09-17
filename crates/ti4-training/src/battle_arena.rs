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
use ti4_model::POK;

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
        let own = ti4_content::units::faction_unit(content, self.faction, base, POK)
            .map(|unit| unit.id().to_owned());
        let id = own.unwrap_or_else(|| base.to_owned());
        if !self.upgraded {
            return id;
        }
        ti4_content::units::unit_type(content, &id, POK)
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
            ti4_content::units::unit_type(content, &id, POK).map_or(0, |unit| unit.capacity());
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
}

/// One side of a fight, ships kept in casualty order so assignment is a scan.
#[derive(Clone)]
pub struct Side {
    ships: Vec<Ship>,
    modifier: i64,
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
        let mut keyed: Vec<((usize, String), Ship)> = Vec::new();
        let mut names: Vec<String> = Vec::new();
        for (id, count) in fleet {
            let unit = ti4_content::units::unit_type(content, id, POK).expect("unit exists");
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
        }
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
            let unit = ti4_content::units::unit_type(content, id, POK).expect("unit exists");
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

    /// Hits free to land anywhere, and hits that must land on non-fighters.
    fn roll(&self, rng: &mut Rng) -> (usize, usize) {
        let (mut free, mut forced) = (0, 0);
        for ship in &self.ships {
            for _ in 0..ship.dice {
                let face = rng.die();
                let mut hits = usize::from(face + self.modifier >= ship.hits_on);
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

    fn absorb(&mut self, hits: usize, non_fighters_only: bool) {
        for _ in 0..hits {
            if self.ships.is_empty() {
                return;
            }
            // "If able": with no non-fighter left, the hit falls on fighters as usual.
            let restrict = non_fighters_only && self.ships.iter().any(|s| !s.fighter);
            if let Some(ship) = self
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
            self.ships.remove(at);
        }
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
    /// `Some("a")` attacker, `Some("b")` defender, `None` mutual destruction.
    pub winner: Option<&'static str>,
    pub rounds: u32,
    /// Surviving ships per entry of the side's [`Side::names`].
    pub attacker_left: Vec<usize>,
    pub defender_left: Vec<usize>,
}

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
    if !in_progress {
        // Space cannon offence: both sides roll, then both absorb.
        let a_hits = if attacker_cannon {
            a.cannon(&mut rng)
        } else {
            0
        };
        let d_hits = d.cannon(&mut rng);
        d.absorb(a_hits, false);
        a.absorb(d_hits, false);
    }
    let mut rounds = 0;
    for round in 1..=50 {
        if a.ships.is_empty() || d.ships.is_empty() {
            break;
        }
        a.repair();
        d.repair();
        if round == 1 && !in_progress {
            let (ha, hd) = (a.barrage(&mut rng), d.barrage(&mut rng));
            d.lose_fighters(ha);
            a.lose_fighters(hd);
            if a.ships.is_empty() || d.ships.is_empty() {
                break;
            }
        }
        rounds = round;
        let (a_free, a_forced) = a.roll(&mut rng);
        let (d_free, d_forced) = d.roll(&mut rng);
        d.absorb(a_forced, true);
        d.absorb(a_free, false);
        a.absorb(d_forced, true);
        a.absorb(d_free, false);
    }
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
            let unit = ti4_content::units::unit_type(content, id, POK).expect("unit exists");
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
        let unit = ti4_content::units::unit_type(content, id, POK).expect("unit exists");
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
    for round in 1..=50 {
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
