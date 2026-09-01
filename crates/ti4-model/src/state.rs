//! Game state.
//!
//! Ported from the oracle's `engine/state.py`. There, state is an immutable frozen
//! dataclass and every change returns a new `GameState`. Here it is an owned value with
//! `&mut self` mutators and `Clone`: a caller that needs the old state keeps a clone, which
//! is the same guarantee without rebuilding the whole structure on every field write.
//!
//! Two idioms from the oracle are load-bearing and are carried over deliberately.
//!
//! **Duration-scoped effects store the sequence number they were played in**, not a flag.
//! `combat_bonus_round`, `move_bonus_activation`, `free_production_use` and the rest hold a
//! value of the matching monotonic counter on [`GameState`]. A flag would have to be cleared
//! by some later step, and a combat or a tactical action can end down several paths — one
//! side wiped out, a retreat — so the flag leaks out of the paths that do not clear it. A
//! sequence number lapses on its own the moment the counter moves.
//!
//! **Equality ignores the derived and bookkeeping fields.** See [`GameState`]'s `PartialEq`.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::id::{
    ActionCardId, BreakthroughId, FactionId, LeaderId, ObjectiveId, PlanetId, PlayerId, RelicId,
    SecretObjectiveId, StrategyCardId, SystemId, TechnologyId, UnitTypeId,
};
use crate::units::Unit;

/// Game phase. Determines who resolves first in a window (LRR 1.19, 1.20).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Strategy,
    Action,
    Status,
    Agenda,
}

impl Phase {
    /// Strategy and agenda phases order by speaker, clockwise (LRR 1.20). Every other
    /// phase orders by initiative from the active player (LRR 1.19).
    #[must_use]
    pub const fn uses_speaker_order(self) -> bool {
        matches!(self, Self::Strategy | Self::Agenda)
    }
}

impl Default for Phase {
    /// A game begins in its first strategy phase.
    fn default() -> Self {
        Self::Strategy
    }
}

/// Command tokens a faction has, total (LRR 20.4). Eight start on the command sheet and the rest
/// are in reinforcements; a token is either on the sheet or on the board, never both.
pub const TOKENS_PER_FACTION: i32 = 16;

/// The lifecycle state of a leader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LeaderStatus {
    Locked,
    Readied,
    Exhausted,
    Unlocked,
    Purged,
}

/// Which pool a gained command token may go into (LRR 20, 52.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenPool {
    Tactic,
    Fleet,
    Strategic,
}

impl TokenPool {
    /// Every pool, in the oracle's `Player.POOLS` order.
    pub const ALL: [Self; 3] = [Self::Tactic, Self::Fleet, Self::Strategic];
}

/// Something a seat *did*, as opposed to something a seat *has*.
///
/// Thirteen secret objectives are written against an event rather than a position — "destroy
/// another player's war sun", "win a combat in an anomaly", "be the last player to pass". No
/// snapshot of the board can answer any of them: the war sun is gone, the combat left nothing
/// behind that records who won it, and passing order is not a thing the board holds. Without a
/// ledger those cards are undecidable, which is why they were unimplemented.
///
/// One variant per card, rather than a general event log, because the requirement is the only
/// consumer and a log would invite a second one with different needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Feat {
    /// Destroy Their Greatest Ship: destroyed another player's war sun or flagship.
    DestroyedACapitalShip,
    /// Make an Example of Their World: bombardment took the last ground forces off a planet.
    BombardedOutTheLastGroundForces,
    /// Turn Their Fleets to Dust: space cannon offense took the last non-fighter ships.
    SpaceCannonTookTheLastNonFighters,
    /// Fight with Precision: anti-fighter barrage took the last fighters.
    BarrageTookTheLastFighters,
    /// Spark a Rebellion: won a combat against the player with the most victory points.
    WonAgainstThePointsLeader,
    /// Unveil Flagship: won a space combat beside a flagship that survived it.
    WonBesideASurvivingFlagship,
    /// Betray a Friend: won a combat against a player whose promissory note was held at the
    /// start of the tactical action.
    WonAgainstANoteHolder,
    /// Brave the Void: won a combat in an anomaly.
    WonInAnAnomaly,
    /// Darken the Skies: won a combat in another player's home system.
    WonInARivalHome,
    /// Demonstrate Your Power: held three or more non-fighter ships in the active system when a
    /// space combat ended.
    HeldThreeShipsAfterASpaceCombat,
    /// Become a Martyr: lost control of a planet in a home system.
    LostAHomePlanet,
    /// Drive the Debate: elected by an agenda, in person or by a controlled planet.
    ElectedByAnAgenda,
    /// Prove Endurance: was the last player to pass in a game round.
    LastToPass,
}

/// A monotonically allocated occurrence for a secret-objective trigger.
///
/// A turn is too broad a scope: one tactical action can contain anti-fighter barrage,
/// space-cannon offense, space combat, ground combat, bombardment, and control loss.
/// The engine allocates an occurrence at the concrete rules boundary and records a feat against
/// that value, so a later event cannot reuse an earlier event's proof.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct FeatOccurrence(pub u64);

// ─── SystemState ───────────────────────────────────────────────────────────────

/// What is in a system's space area, and on its planets.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemState {
    /// Units in the space area.
    pub units: Vec<Unit>,
    /// Players who have placed a command token here.
    pub command_tokens: BTreeSet<PlayerId>,
    /// Planet to controlling player (LRR 78.7c, 49.5). Excluded from equality.
    pub planet_control: BTreeMap<PlanetId, PlayerId>,
    /// Planet to the units standing on it. Ground forces are always on a planet or in a
    /// system's space area (LRR 43.1), never anywhere else. Excluded from equality.
    pub planet_units: BTreeMap<PlanetId, Vec<Unit>>,
    /// Planet to the players coexisting on it, one controller aside (Thunder's Edge coexistence 2).
    ///
    /// Coexistence is not derivable from occupancy. Two players with ground forces on one planet
    /// normally means a ground combat that has not happened yet; coexistence is the state a
    /// specific effect puts them in, in which combat is *not* triggered. So it is recorded rather
    /// than inferred. The controller is never listed here: they are in `planet_control`.
    ///
    /// Excluded from equality, like the other two planet maps.
    #[serde(default)]
    pub coexisting: BTreeMap<PlanetId, BTreeSet<PlayerId>>,
    /// Planets destroyed outright, whose planet cards have been purged (the Stellar Converter).
    ///
    /// The planet stays in the galaxy -- the tile is still on the table and still a system you can
    /// move into -- but there is no longer a card to take, so it can never be controlled or landed
    /// on again. Recorded on the system rather than globally because a planet belongs to a system,
    /// and because the two operations that would resurrect it, `set_control` and `land`, are both
    /// here: guarding the chokepoints is what makes the destruction stick without every reader of
    /// the galaxy having to remember to ask.
    ///
    /// Excluded from equality, like the other three planet maps.
    #[serde(default)]
    pub purged_planets: BTreeSet<PlanetId>,
}

/// Mirrors the oracle's `compare=False` on `planet_control` and `planet_units`.
impl PartialEq for SystemState {
    fn eq(&self, other: &Self) -> bool {
        self.units == other.units && self.command_tokens == other.command_tokens
    }
}

impl SystemState {
    #[must_use]
    pub fn controls_a_planet(&self, player: &PlayerId) -> bool {
        self.planet_control.values().any(|p| p == player)
    }

    #[must_use]
    pub fn on_planet(&self, planet: &PlanetId) -> &[Unit] {
        self.planet_units.get(planet).map_or(&[], Vec::as_slice)
    }

    #[must_use]
    pub fn on_planet_of(&self, planet: &PlanetId, owner: &PlayerId) -> Vec<&Unit> {
        self.on_planet(planet)
            .iter()
            .filter(|u| &u.owner == owner)
            .collect()
    }

    #[must_use]
    pub fn planet_owners(&self, planet: &PlanetId) -> BTreeSet<&PlayerId> {
        self.on_planet(planet).iter().map(|u| &u.owner).collect()
    }

    /// Move units out of the space area and onto a planet (LRR 49.2a).
    ///
    /// A destroyed planet is not landed on: there is nothing left to land on. The units stay in
    /// the space area rather than being quietly destroyed with it.
    pub fn land(&mut self, planet: &PlanetId, units: &[Unit]) {
        if self.purged_planets.contains(planet) {
            return;
        }
        self.remove(units);
        self.planet_units
            .entry(planet.clone())
            .or_default()
            .extend_from_slice(units);
    }

    pub fn remove_from_planet(&mut self, planet: &PlanetId, units: &[Unit]) {
        if let Some(standing) = self.planet_units.get_mut(planet) {
            remove_each(standing, units);
        }
    }

    /// Swap one unit on a planet for another, e.g. marking it as having sustained damage.
    pub fn replace_planet_unit(&mut self, planet: &PlanetId, old: &Unit, new: Unit) -> bool {
        let Some(standing) = self.planet_units.get_mut(planet) else {
            return false;
        };
        replace_first(standing, old, new)
    }

    pub fn set_control(&mut self, planet: PlanetId, player: PlayerId) {
        if self.purged_planets.contains(&planet) {
            return; // no card to take: the planet was destroyed
        }
        self.planet_control.insert(planet, player);
    }

    /// Destroy a planet: its units, its control, its coexistence.
    ///
    /// The attachments and the planet card itself are purged by the caller, which is the only
    /// party that can reach `GameState`.
    pub fn purge_planet(&mut self, planet: &PlanetId) {
        self.planet_units.remove(planet);
        self.planet_control.remove(planet);
        self.coexisting.remove(planet);
        self.purged_planets.insert(planet.clone());
    }

    #[must_use]
    pub fn units_of(&self, owner: &PlayerId) -> Vec<&Unit> {
        self.units.iter().filter(|u| &u.owner == owner).collect()
    }

    #[must_use]
    pub fn owners(&self) -> BTreeSet<&PlayerId> {
        self.units.iter().map(|u| &u.owner).collect()
    }

    #[must_use]
    pub fn has_units_of(&self, owner: &PlayerId) -> bool {
        self.units.iter().any(|u| &u.owner == owner)
    }

    pub fn add(&mut self, units: &[Unit]) {
        self.units.extend_from_slice(units);
    }

    /// Remove one occurrence of each named unit from the space area.
    pub fn remove(&mut self, units: &[Unit]) {
        remove_each(&mut self.units, units);
    }

    pub fn place_token(&mut self, player: PlayerId) {
        self.command_tokens.insert(player);
    }

    /// Swap one unit for another, e.g. marking it as having sustained damage.
    pub fn replace_unit(&mut self, old: &Unit, new: Unit) -> bool {
        replace_first(&mut self.units, old, new)
    }
}

/// Remove one occurrence of each element, like Python's `list.remove` in a loop.
///
/// Units are interchangeable values with no identity, so "remove this unit" means "remove
/// one unit like this" — removing every match would destroy a whole stack.
fn remove_each(from: &mut Vec<Unit>, units: &[Unit]) {
    for unit in units {
        if let Some(index) = from.iter().position(|u| u == unit) {
            from.remove(index);
        }
    }
}

fn replace_first(units: &mut [Unit], old: &Unit, new: Unit) -> bool {
    if let Some(index) = units.iter().position(|u| u == old) {
        units[index] = new;
        return true;
    }
    false
}

// ─── Player ────────────────────────────────────────────────────────────────────

/// One player's state.
///
/// Fields are grouped as the oracle groups them. The many `Option<u32>` fields are
/// duration-scoped effects: see the module documentation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub faction: FactionId,
    /// Physical home assigned at setup. Usually derived from `faction`; stored explicitly
    /// so tournament replicas of one faction can occupy distinct legal home systems
    /// without all claiming the same planet.
    pub home_system: Option<SystemId>,
    pub home_planets: Vec<PlanetId>,

    // -- strategy cards ---------------------------------------------------------
    /// Strategy cards held this round, kept sorted by initiative number, lowest first.
    ///
    /// A collection rather than a single card because a three-player game deals two cards
    /// per player. The ordering is not cosmetic: [`Self::strategy_card`] reads element zero
    /// as the initiative card, so whatever assigns this must sort it.
    /// [`GameState::deal_strategy_card`] is the only thing that should.
    pub strategy_cards: Vec<StrategyCardId>,
    /// Held cards already spent on their strategic action. A card is exhausted by taking
    /// that action, and each held card exhausts separately.
    pub exhausted_strategy_cards: BTreeSet<StrategyCardId>,

    // -- economy ----------------------------------------------------------------
    /// Command token pools (LRR 20). Tokens are gained into a pool of choice (52.4).
    pub tactic_tokens: i32,
    pub fleet_tokens: i32,
    pub strategic_tokens: i32,
    pub passed: bool,
    pub victory_points: i32,
    /// Spendable as one resource or one influence each (LRR 75.3).
    pub trade_goods: i32,
    /// Commodities become trade goods only when traded away (LRR 21).
    pub commodities: i32,

    // -- cards and technology ---------------------------------------------------
    /// Unscored secret objectives held, hidden from other players (LRR 61.17).
    pub secret_objectives: Vec<SecretObjectiveId>,
    /// Technology aliases owned (LRR 90.1).
    pub technologies: BTreeSet<TechnologyId>,
    /// Owned technologies currently exhausted. A technology card exhausts like a planet
    /// does and readies in the status phase; without this a card saying "exhaust this
    /// card" could be used every turn for ever.
    pub exhausted_technologies: BTreeSet<TechnologyId>,
    /// Action cards in hand (LRR 2.3). Ordered, since cards are chosen by index.
    pub action_cards: Vec<ActionCardId>,
    /// The faction breakthrough, once earned from the Thunder's Edge expedition.
    pub breakthrough: Option<BreakthroughId>,
    /// Relic fragments by trait, awaiting purge for a relic (LRR 35.9). Not compared.
    pub relic_fragments: BTreeMap<String, i32>,
    /// Relics held faceup in the play area. Cannot be traded (LRR 73.4).
    pub relics: Vec<RelicId>,
    /// Relic cards currently exhausted, readying with the rest at the status phase. The
    /// set exists for the relics that exhaust to use themselves: Heart of Ixth spends
    /// one use on a die result and then waits for the readying step; without it the
    /// same card would bend every die in the game.
    #[serde(default)]
    pub exhausted_relics: BTreeSet<RelicId>,
    /// Exploration cards placed faceup in the play area, which carry their own ACTION.
    ///
    /// Two Enigmatic Device cards say "place this card faceup in your play area" and then print an
    /// ACTION. They are not relics -- they do not count for a relic objective and no relic effect
    /// reaches them -- so they are held separately rather than borrowing `relics` for the ride.
    #[serde(default)]
    pub exploration_cards: Vec<String>,
    /// Firmament plot cards, represented by the control token on each facedown card.
    pub plots: Vec<String>,
    /// Secret aliases scored as plots. They do not count against the secret limit.
    pub plot_objectives: BTreeSet<SecretObjectiveId>,
    /// A failed Silver Flame roll permanently forbids public-objective scoring.
    pub public_objectives_forbidden: bool,
    /// Leader lifecycle states. Not compared.
    pub leaders: BTreeMap<LeaderId, LeaderStatus>,

    // -- duration-scoped effects -------------------------------------------------
    /// Letnev's hero: the round during which fleet supply is limited by neither laws nor
    /// the fleet pool. The card says "during this game round", so this holds the round
    /// number rather than a flag, and is cleared when that round ends.
    pub fleet_supply_unlimited_until: Option<u32>,
    /// Morale Boost: the [`GameState::combat_round_seq`] during which this player's combat
    /// rolls get +1.
    pub combat_bonus_round: Option<u32>,
    /// Extra votes this seat casts on the agenda numbered here (Distinguished Councilor, Bribery).
    ///
    /// Scoped by `agenda_seq` for the same reason `combat_bonus_round` is scoped by the round: the
    /// cards say "that outcome", meaning this agenda, and an unscoped bonus would follow the seat
    /// for the rest of the game.
    #[serde(default)]
    pub extra_votes_agenda: Option<(u32, i64)>,
    /// Hack Election: the [`GameState::agenda_seq`] of the agenda whose vote this seat takes
    /// the last seat of — "During this agenda, you vote last." Scoped by `agenda_seq` for
    /// the same reason `extra_votes_agenda` is: `reveal_agenda` bumps the counter before
    /// its window opens, so the marker binds to the vote the reveal produced (including a
    /// Veto replacement, which is voted on in the same cycle) and expires at the next
    /// reveal without any cleanup.
    #[serde(default)]
    pub hack_votes_last_agenda: Option<u32>,

    /// Rout ("At the start of the 'Announce Retreats' step of space combat, if you are the
    /// defender: your opponent must announce a retreat, if able."): the
    /// [`GameState::combat_round_seq`] the card was played into, i.e. the round whose
    /// Announcing step the opponent's retreat was forced in. Scopes itself the way
    /// `extra_votes_agenda` does: the counter only moves forward, so a marker from an earlier
    /// round (or an earlier combat) can never match again.
    #[serde(default)]
    pub rout_round: Option<u32>,

    /// Waylay ("Before you roll dice for ANTI-FIGHTER BARRAGE: hits from this roll are
    /// produced against all ships (not just fighters)."): the [`GameState::combat_round_seq`
    /// ] the card was played into, i.e. the round whose barrage roll may target every ship.
    #[serde(default)]
    pub waylay_barrage_round: Option<u32>,
    /// Hits this seat may cancel before assigning them, in the combat round numbered here
    /// (Shields Holding). Consumed as they are cancelled.
    #[serde(default)]
    pub cancel_hits_round: Option<(u32, usize)>,
    /// This seat may not retreat during the combat round numbered here (Intercept).
    #[serde(default)]
    pub retreat_barred_round: Option<u32>,
    /// Evelyn `DeLouis` and Viscount Unlenn: the combat round in which one of this player's
    /// units rolls an extra die.
    pub extra_die_round: Option<u32>,
    /// Unit type selected for that extra die. Identical copies are strategically
    /// equivalent, but different unit types have different combat values, so this stays a
    /// real policy choice.
    pub extra_die_unit: Option<UnitTypeId>,
    /// Letnev Munitions Reserves: the combat round whose dice may be rerolled after paying
    /// two trade goods at that round's opening window.
    pub munitions_round: Option<u32>,
    /// Harrugh Gefhara: the [`GameState::production_seq`] whose unit costs are zero.
    /// Scoped so the hero cannot quietly make every later use free as well.
    pub free_production_use: Option<u32>,
    /// Spatial Conduit Cylinders: the activation during which the active system counts as
    /// adjacent to every system holding this player's units.
    pub conduit_activation: Option<u32>,
    /// Flank Speed: the [`GameState::activation_seq`] during which this player's ships each
    /// get +1 move.
    pub move_bonus_activation: Option<u32>,
    /// Gravity Drive may raise the move of exactly one ship per tactical action. Records
    /// the activation in which that allowance was spent; unlike Flank Speed it must not
    /// improve every later ship in the same movement step.
    pub gravity_drive_used_activation: Option<u32>,
    /// Magen Defense Grid (original printing): the combat round during which this player's
    /// ground forces are barred from rolling. Kept on the affected player so simultaneous
    /// combats cannot leak the suppression to another planet.
    pub ground_roll_suppressed_round: Option<u32>,
    /// Duranium Armor may not repair a unit that sustained in the same combat round.
    /// Counts by unit id distinguish two identical hulls without inventing identities.
    pub sustained_damage_round: Option<u32>,
    pub sustained_damage_types: Vec<UnitTypeId>,
    /// Agency Supply Network is once per action rather than once per production use.
    pub agency_supply_used_turn: Option<u32>,
    /// Fleet Logistics allows a second action each turn. A turn is identified by
    /// [`GameState::turn_seq`] rather than by round: every player normally takes several
    /// turns in one action phase, and the allowance refreshes for each of them.
    pub fleet_logistics_used_turn: Option<u32>,
    /// Nav Suite: the activation during which this player ignores anomaly effects.
    pub anomalies_ignored_activation: Option<u32>,
    /// In The Silence Of Space: the activation it was played in, and the one system whose
    /// ships may move through other players' ships. Both or neither.
    pub silence_activation: Option<u32>,
    pub silence_system: Option<SystemId>,
    /// Fighter Prototype: one entry per copy, each the [`GameState::combat_round_seq`] of the
    /// round in which each of this player's fighters' combat rolls gets +2. A Vec rather than
    /// an `Option` because two copies of the card stack, and the round-robin reaction window
    /// lets a player play both.
    #[serde(default)]
    pub fighter_bonus_round: Vec<u32>,
    /// Bunker: one entry per copy, each the [`GameState::activation_seq`] of the tactical
    /// action whose invasion applies -4 to every BOMBARDMENT roll made against planets this
    /// player controls. An invasion belongs to exactly one activation, so the activation is
    /// the invasion's identity.
    #[serde(default)]
    pub bunker_invasion: Vec<u32>,
    /// War Machine: one entry per copy, each the [`GameState::activation_seq`] of the
    /// production step that gains +4 total PRODUCTION value and -1 combined unit cost (five
    /// faces of budget in the engine's budget model). Production happens once per tactical
    /// action, so the activation scopes the marker to that step.
    #[serde(default)]
    pub war_machine_use: Vec<u32>,
    /// Blitz: one entry per copy, each the [`GameState::activation_seq`] of the invasion in
    /// which that player's non-fighter ships without BOMBARDMENT roll BOMBARDMENT 6. An
    /// invasion belongs to exactly one activation, so the marker lapses when the next
    /// tactical action begins.
    #[serde(default)]
    pub blitz_invasion: Vec<u32>,
    /// Disable: one entry per copy, each the [`GameState::activation_seq`] of the invasion in
    /// which that player's opponents' PDS units lose PLANETARY SHIELD and SPACE CANNON. As
    /// with Blitz, the activation scopes the marker to the invasion it was played in.
    #[serde(default)]
    pub disable_invasion: Vec<u32>,
    /// Solar Flare: one entry per copy, each the [`GameState::activation_seq`] of the tactical
    /// action whose movement step no opponent's SPACE CANNON may fire at this player's ships.
    /// The engine's cannon step is the one that belongs to that action, so the activation
    /// scopes the marker to it.
    #[serde(default)]
    pub solar_flare: Vec<u32>,
    /// Lost Star Chart: one entry per copy, each the [`GameState::activation_seq`] of the
    /// tactical action in which systems containing both an alpha and a beta wormhole are
    /// adjacent to each other. The card names this tactical action, so the activation scopes
    /// the marker to it.
    #[serde(default)]
    pub lost_star: Vec<u32>,
    /// The Dominus Orb: activations during which this player's command tokens do not pin their
    /// ships. Held per activation, like `lost_star`, because the card is purged into one tactical
    /// action and must not loosen the next one.
    #[serde(default)]
    pub dominus_orb: Vec<u32>,
    /// Political Stability: this player returned no strategy cards in the status phase
    /// that set it, keeps the cards it was holding, and skips choosing cards in the
    /// strategy phase that follows. The marker survives through the agenda phase and the
    /// draft — that is its point — and is cleared when the action phase of that round
    /// begins, which is when the retained cards next go back out the door (the following
    /// round's status phase).
    #[serde(default)]
    pub stability: bool,
    /// Extreme Duress: the seat under duress, and the seat whose card put it there. Set
    /// at the start of the target's turn; a strategic action lifts it quietly, and any
    /// other action the target takes triggers the punishment, which discards the
    /// target's action cards, hands every trade good to the holder, and shows the secret
    /// objectives. Passing is not an action, so it neither triggers nor lifts it.
    #[serde(default)]
    pub duress_by: Option<PlayerId>,

    // -- returning and captured units ---------------------------------------------
    /// Generic Infantry II casualties waiting on their technology card.
    pub infantry_returning: i32,
    /// Units waiting on unit-upgrade technology cards (Infantry II, Letani Warrior II,
    /// Crimson Legionnaire II). The concrete type is retained because faction upgrades
    /// return their own plastic rather than generic infantry.
    pub technology_units_returning: Vec<UnitTypeId>,
    /// Spec Ops II units that succeeded on their destruction roll, waiting until the start
    /// of this player's next turn.
    pub spec_ops_returning: i32,
    /// Units physically captured from opponents, as `(owner, unit type)`. Vortex and Cabal
    /// effects remove these from the victim's available reinforcements.
    pub captured_units: Vec<(PlayerId, UnitTypeId)>,
    /// Technology copied by each Nekro Valefar card: card alias to source technology.
    /// Not compared.
    pub assimilated_technologies: BTreeMap<String, TechnologyId>,
    /// Event-scoped feat evidence for secret-objective timing.
    #[serde(default)]
    pub event_feats: Vec<(Feat, FeatOccurrence)>,
}

/// Mirrors the oracle's `compare=False` on `relic_fragments`, `leaders`, and
/// `assimilated_technologies`.
impl PartialEq for Player {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.faction == other.faction
            && self.home_system == other.home_system
            && self.home_planets == other.home_planets
            && self.strategy_cards == other.strategy_cards
            && self.exhausted_strategy_cards == other.exhausted_strategy_cards
            && self.tactic_tokens == other.tactic_tokens
            && self.fleet_tokens == other.fleet_tokens
            && self.strategic_tokens == other.strategic_tokens
            && self.passed == other.passed
            && self.victory_points == other.victory_points
            && self.trade_goods == other.trade_goods
            && self.commodities == other.commodities
            && self.secret_objectives == other.secret_objectives
            && self.technologies == other.technologies
            && self.exhausted_technologies == other.exhausted_technologies
            && self.action_cards == other.action_cards
            && self.breakthrough == other.breakthrough
            && self.relics == other.relics
            && self.exhausted_relics == other.exhausted_relics
            && self.plots == other.plots
            && self.plot_objectives == other.plot_objectives
            && self.public_objectives_forbidden == other.public_objectives_forbidden
            && self.fleet_supply_unlimited_until == other.fleet_supply_unlimited_until
            && self.combat_bonus_round == other.combat_bonus_round
            && self.extra_votes_agenda == other.extra_votes_agenda
            && self.hack_votes_last_agenda == other.hack_votes_last_agenda
            && self.rout_round == other.rout_round
            && self.waylay_barrage_round == other.waylay_barrage_round
            && self.cancel_hits_round == other.cancel_hits_round
            && self.retreat_barred_round == other.retreat_barred_round
            && self.extra_die_round == other.extra_die_round
            && self.extra_die_unit == other.extra_die_unit
            && self.munitions_round == other.munitions_round
            && self.free_production_use == other.free_production_use
            && self.conduit_activation == other.conduit_activation
            && self.move_bonus_activation == other.move_bonus_activation
            && self.gravity_drive_used_activation == other.gravity_drive_used_activation
            && self.ground_roll_suppressed_round == other.ground_roll_suppressed_round
            && self.sustained_damage_round == other.sustained_damage_round
            && self.sustained_damage_types == other.sustained_damage_types
            && self.agency_supply_used_turn == other.agency_supply_used_turn
            && self.fleet_logistics_used_turn == other.fleet_logistics_used_turn
            && self.anomalies_ignored_activation == other.anomalies_ignored_activation
            && self.silence_activation == other.silence_activation
            && self.silence_system == other.silence_system
            && self.fighter_bonus_round == other.fighter_bonus_round
            && self.bunker_invasion == other.bunker_invasion
            && self.war_machine_use == other.war_machine_use
            && self.blitz_invasion == other.blitz_invasion
            && self.disable_invasion == other.disable_invasion
            && self.solar_flare == other.solar_flare
            && self.lost_star == other.lost_star
            && self.dominus_orb == other.dominus_orb
            && self.stability == other.stability
            && self.duress_by == other.duress_by
            && self.infantry_returning == other.infantry_returning
            && self.technology_units_returning == other.technology_units_returning
            && self.spec_ops_returning == other.spec_ops_returning
            && self.captured_units == other.captured_units
            // Event-scoped feat evidence gates secret-scoring eligibility, so it is part of the
            // canonical projection (M07-021): a direct-vs-stepped divergence in feat evidence must
            // fail state equality even when no objective has scored yet.
            && self.event_feats == other.event_feats
    }
}

impl Player {
    /// A player at the start of a game, with the opening command tokens (LRR 20.1).
    #[must_use]
    pub fn new(id: PlayerId) -> Self {
        Self {
            id,
            faction: FactionId::new("generic"),
            home_system: None,
            home_planets: Vec::new(),
            strategy_cards: Vec::new(),
            exhausted_strategy_cards: BTreeSet::new(),
            tactic_tokens: 3,
            fleet_tokens: 3,
            strategic_tokens: 2,
            passed: false,
            victory_points: 0,
            trade_goods: 0,
            commodities: 0,
            secret_objectives: Vec::new(),
            technologies: BTreeSet::new(),
            exhausted_technologies: BTreeSet::new(),
            action_cards: Vec::new(),
            breakthrough: None,
            relic_fragments: BTreeMap::new(),
            relics: Vec::new(),
            exhausted_relics: BTreeSet::new(),
            exploration_cards: Vec::new(),
            plots: Vec::new(),
            plot_objectives: BTreeSet::new(),
            public_objectives_forbidden: false,
            leaders: BTreeMap::new(),
            fleet_supply_unlimited_until: None,
            combat_bonus_round: None,
            extra_votes_agenda: None,
            hack_votes_last_agenda: None,
            rout_round: None,
            waylay_barrage_round: None,
            cancel_hits_round: None,
            retreat_barred_round: None,
            extra_die_round: None,
            extra_die_unit: None,
            munitions_round: None,
            free_production_use: None,
            conduit_activation: None,
            move_bonus_activation: None,
            gravity_drive_used_activation: None,
            ground_roll_suppressed_round: None,
            sustained_damage_round: None,
            sustained_damage_types: Vec::new(),
            agency_supply_used_turn: None,
            fleet_logistics_used_turn: None,
            anomalies_ignored_activation: None,
            silence_activation: None,
            silence_system: None,
            fighter_bonus_round: Vec::new(),
            bunker_invasion: Vec::new(),
            war_machine_use: Vec::new(),
            blitz_invasion: Vec::new(),
            disable_invasion: Vec::new(),
            solar_flare: Vec::new(),
            lost_star: Vec::new(),
            dominus_orb: Vec::new(),
            stability: false,
            duress_by: None,
            infantry_returning: 0,
            technology_units_returning: Vec::new(),
            spec_ops_returning: 0,
            captured_units: Vec::new(),
            assimilated_technologies: BTreeMap::new(),
            event_feats: Vec::new(),
        }
    }

    /// The card this player's initiative is read from: the lowest-numbered held.
    ///
    /// LRR 83.3 orders players by initiative number, and a player holding two cards acts on
    /// the lower of them.
    #[must_use]
    pub fn strategy_card(&self) -> Option<&StrategyCardId> {
        self.strategy_cards.first()
    }

    /// Every held card spent. The gate on passing (LRR 3.3).
    ///
    /// False when no card is held. A vacuous `all()` would read true and let a cardless
    /// player pass.
    #[must_use]
    pub fn strategy_card_exhausted(&self) -> bool {
        !self.strategy_cards.is_empty()
            && self
                .strategy_cards
                .iter()
                .all(|c| self.exhausted_strategy_cards.contains(c))
    }

    /// Held cards whose strategic action is still available, in initiative order.
    #[must_use]
    pub fn unused_strategy_cards(&self) -> Vec<&StrategyCardId> {
        self.strategy_cards
            .iter()
            .filter(|c| !self.exhausted_strategy_cards.contains(*c))
            .collect()
    }

    #[must_use]
    pub fn has_unused_strategy_card(&self) -> bool {
        !self.unused_strategy_cards().is_empty()
    }

    /// Tokens in one pool.
    #[must_use]
    pub const fn tokens(&self, pool: TokenPool) -> i32 {
        match pool {
            TokenPool::Tactic => self.tactic_tokens,
            TokenPool::Fleet => self.fleet_tokens,
            TokenPool::Strategic => self.strategic_tokens,
        }
    }

    /// Add to one pool *without* the reinforcement cap. Prefer [`GameState::gain_token`].
    ///
    /// A faction has sixteen command tokens and no more: LRR 20.4 limits a player to what is in
    /// their reinforcements, and a token is either on the board or on the command sheet. Counting
    /// the board needs the board, which a `Player` cannot see -- so the capped entry point is on
    /// `GameState` and this one is for the paths that give a token *back* (negative counts) or that
    /// have already done the arithmetic.
    pub const fn gain_token_uncapped(&mut self, pool: TokenPool, count: i32) {
        match pool {
            TokenPool::Tactic => self.tactic_tokens += count,
            TokenPool::Fleet => self.fleet_tokens += count,
            TokenPool::Strategic => self.strategic_tokens += count,
        }
    }

    /// Spend one token from a pool, or report that the pool is empty.
    ///
    /// A pool cannot go negative: running out of command tokens is a real constraint that
    /// shapes the whole action phase, and an unchecked decrement silently removes it.
    pub const fn spend_token(&mut self, pool: TokenPool) -> bool {
        let available = match pool {
            TokenPool::Tactic => &mut self.tactic_tokens,
            TokenPool::Fleet => &mut self.fleet_tokens,
            TokenPool::Strategic => &mut self.strategic_tokens,
        };
        if *available <= 0 {
            return false;
        }
        *available -= 1;
        true
    }

    /// Total command tokens across all three pools.
    #[must_use]
    pub const fn total_tokens(&self) -> i32 {
        self.tactic_tokens + self.fleet_tokens + self.strategic_tokens
    }
}

// ─── GameState ─────────────────────────────────────────────────────────────────

/// A promise made in a transaction: promiser, partner, and what was promised.
pub type PromiseKey = (PlayerId, PlayerId, String);

/// Serialises the promise map as a sequence rather than an object.
///
/// JSON object keys must be strings, and a promise is keyed by a triple. Flattening the
/// key into one string would need an escaping scheme and could collide with a promise whose
/// text contains the separator, so the map travels as a list of records instead. `BTreeMap`
/// iteration is sorted, so the encoding is canonical.
mod promise_map {
    use super::{PlayerId, PromiseKey};
    use serde::de::Deserialize as _;
    use serde::{Deserializer, Serialize as _, Serializer};
    use std::collections::BTreeMap;

    #[derive(serde::Serialize, serde::Deserialize)]
    struct Record {
        promiser: PlayerId,
        partner: PlayerId,
        promise: String,
        /// `None` while the promise is still outstanding.
        kept: Option<bool>,
    }

    pub(super) fn serialize<S: Serializer>(
        promises: &BTreeMap<PromiseKey, Option<bool>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let records: Vec<Record> = promises
            .iter()
            .map(|((promiser, partner, promise), kept)| Record {
                promiser: promiser.clone(),
                partner: partner.clone(),
                promise: promise.clone(),
                kept: *kept,
            })
            .collect();
        records.serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<PromiseKey, Option<bool>>, D::Error> {
        Ok(Vec::<Record>::deserialize(deserializer)?
            .into_iter()
            .map(|r| ((r.promiser, r.partner, r.promise), r.kept))
            .collect())
    }
}

/// One die roll of a unit, held between the roll and the window that may reroll it.
///
/// Fire Team, Scramble Frequency, and Aglnlan Oln all act on dice that have been rolled but
/// not yet applied. The faces are retained from the moment of the roll so a reroll can re-draw
/// specific dice and the roll site can recompute the hits before any unit is removed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerollEntry {
    /// The unit type that made this roll.
    pub unit: String,
    /// The planet the roll was made on or against (ground combat, bombardment);
    /// `None` for a system-wide roll.
    pub planet: Option<PlanetId>,
    /// The value this roll hits on, if it was a hit roll at all.
    pub hits_on: Option<u32>,
    /// The current faces; a reroll replaces specific positions in place.
    pub faces: Vec<u32>,
    /// The positions replaced by some reroll, whichever ability made it. Crown of
    /// Thalnos destroys units whose *reroll* produced no hit, which the faces alone
    /// cannot say: a die showing 4 is a hit or a miss depending on what it replaced.
    #[serde(default)]
    pub rerolled: BTreeSet<usize>,
    /// Per-die result adjustments that survive rerolls: Thalnos adds 1 to each die it
    /// rerolls, and the adjustment dies with the die when a later reroll replaces it.
    #[serde(default)]
    pub deltas: BTreeMap<usize, i8>,
    /// The unit types pooled into this roll, and how many of each. Fleet and ground
    /// rolls pool dice by combat value, so one entry can be several units' dice at once;
    /// destruction rules that name "the units that rerolled" read this.
    #[serde(default)]
    pub unit_types: BTreeMap<String, u32>,
}

/// Every roll one player made at one moment, the set a timing window can still reroll.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerollSet {
    /// What was rolled: `"ground"`, `"bombardment"`, `"space_cannon"`, or
    /// `"anti_fighter_barrage"`.
    pub kind: String,
    /// The system the roll was made in.
    pub system: SystemId,
    /// One entry per die roll the player made.
    pub rolls: Vec<RerollEntry>,
}

impl RerollEntry {
    /// The hits this entry currently produces from its (possibly rerolled) faces.
    ///
    /// A Thalnos +1 on a die counts in the total: the card applies +1 to the *results*,
    /// and results are what remove units.
    #[must_use]
    pub fn hits(&self) -> usize {
        self.hits_on.map_or(0, |on| {
            self.faces
                .iter()
                .enumerate()
                .filter(|(index, face)| {
                    let adjusted =
                        i64::from(**face) + self.deltas.get(index).map_or(0, |offset| i64::from(*offset));
                    adjusted >= i64::from(on)
                })
                .count()
        })
    }
}

/// Turn-flow flags set by a reaction card and consumed by the turn driver at the next
/// boundary. A `u8` bitfield rather than bool fields: `GameState` is already at the three-
/// bool limit the lints allow, and these four flags live and die together inside
/// `advance_turn`. In-flight resolution data — excluded from `GameState`'s manual
/// `PartialEq`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TransientFlags(u8);

impl TransientFlags {
    /// Deadly Plot: the agenda being resolved is discarded with no effect.
    pub const AGENDA_DISCARDED: u8 = 1 << 0;
    /// Coup d'Etat: the strategic action that just began is not resolved; the turn ends.
    pub const STRATEGIC_CANCELLED: u8 = 1 << 1;
    /// Crisis: the seat the turn is moving to has its turn skipped.
    pub const SKIP_NEXT_TURN: u8 = 1 << 2;
    /// Master Plan: the player whose turn is ending may act again on the same turn.
    pub const ADDITIONAL_ACTION: u8 = 1 << 3;
    /// Puppets on a String: the player whose turn is ending, and who has passed, takes one
    /// fresh action turn — new `turn_seq`, start-of-turn hooks — without un-passing.
    pub const PUPPET_ACTION: u8 = 1 << 4;
    /// Black Market Dealings: the transaction in flight may include relics (as fragments),
    /// action cards, and unscored secret objectives. Set while the negotiation whose opening
    /// triggered it is still on the table, cleared when it closes or the turn ends.
    pub const BLACK_MARKET: u8 = 1 << 5;

    #[must_use]
    pub const fn has(self, flag: u8) -> bool {
        self.0 & flag == flag
    }

    pub fn set(&mut self, flag: u8) {
        self.0 |= flag;
    }

    pub fn clear(&mut self, flag: u8) {
        self.0 &= !flag;
    }
}

/// The whole game, as a value.
///
/// `initiative_order` is derived from held strategy cards rather than stored, so it cannot
/// drift out of step with them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameState {
    pub players: Vec<Player>,
    pub seating_order: Vec<PlayerId>,
    pub speaker: PlayerId,
    pub phase: Phase,
    pub round: u32,
    pub active: Option<PlayerId>,

    // -- strategy cards ---------------------------------------------------------
    pub unclaimed_strategy_cards: Vec<StrategyCardId>,
    /// Trade goods sitting on a strategy card nobody took (LRR 83.4).
    ///
    /// A card left unchosen gains one at the end of the strategy phase, and whoever picks
    /// it up later takes the pile with it. Without this the low-initiative cards — the ones
    /// players skip — are worth exactly as much in round nine as in round one, and the
    /// compensation the rules pay for going late does not exist. **Compared**, unlike the
    /// other maps here.
    pub strategy_card_goods: BTreeMap<StrategyCardId, i32>,
    /// Initiative number per strategy card, from the content corpus. Not compared.
    pub card_initiative: BTreeMap<StrategyCardId, i32>,
    /// How many strategy cards each player drafts (LRR 83.2).
    ///
    /// Two in a three or four player game, one otherwise. Stored rather than derived from
    /// the player count deliberately: the engine is driven with one and two player states
    /// as test harnesses and partial imports, where the count is an artefact of the fixture
    /// and not a statement about the variant being played.
    pub strategy_cards_per_player: usize,

    // -- board ------------------------------------------------------------------
    /// Space areas by system. Absent entries are empty systems. Not compared.
    pub board: BTreeMap<SystemId, SystemState>,
    /// The system activated by the tactical action in progress (LRR 89.1a).
    pub active_system: Option<SystemId>,
    /// Which step of a multi-step action is awaiting a decision, if any.
    pub pending: Option<String>,
    /// Planets whose cards are exhausted, so their resources and influence are unavailable
    /// until the status phase readies them (LRR 34.2, 34.3).
    pub exhausted_planets: BTreeSet<PlanetId>,

    // -- objectives -------------------------------------------------------------
    /// Public objectives turned faceup, scoreable by anyone who qualifies (LRR 61.11).
    pub revealed_objectives: Vec<ObjectiveId>,
    /// Still facedown, stage I first then stage II (LRR 61.13, 61.14a).
    pub objective_deck: Vec<ObjectiveId>,
    /// What each player has already scored (LRR 61.8). Not compared.
    pub scored_objectives: BTreeMap<PlayerId, BTreeSet<ObjectiveId>>,
    /// Set when the game has been decided, so the loop stops.
    pub finished: bool,

    // -- decks ------------------------------------------------------------------
    /// Exploration decks by trait, drawn from the front (LRR 35.2a). Not compared.
    pub exploration_decks: BTreeMap<String, Vec<String>>,
    /// Attachments stuck to each planet (LRR 35.8). Not compared.
    pub planet_attachments: BTreeMap<PlanetId, Vec<String>>,
    pub relic_deck: Vec<RelicId>,
    pub agenda_deck: Vec<String>,
    pub action_card_deck: Vec<ActionCardId>,
    /// Action cards that have left play: every played card, and every card discarded by a
    /// hand-limit, agenda, or punishment effect. Reverse Engineer takes cards from here; a
    /// card handed to another player (a transaction, a steal) never enters the pile. It is
    /// history the rest of the state cannot recover — who may take which card from the pile
    /// is play-relevant — so it is compared.
    #[serde(default)]
    pub discarded_action_cards: Vec<ActionCardId>,
    pub secret_deck: Vec<SecretObjectiveId>,
    /// Laws in play: alias to the outcome that passed (LRR 8.20). Not compared.
    pub laws: BTreeMap<String, String>,
    /// LRR 27.4: once the custodians token is lifted, every round has an agenda phase.
    pub custodians_removed: bool,

    // -- tokens on the board ------------------------------------------------------
    /// Systems still holding a frontier token (`PoK` 35.5). Placed at setup on every system
    /// without a planet, and removed when a ship ends its movement there.
    pub frontier_tokens: BTreeSet<SystemId>,
    /// Thunder's Edge expedition: slice to the player who claimed it. Not compared.
    pub expedition_slices: BTreeMap<String, PlayerId>,
    /// Set when a breakthrough roll brings the Fracture into play.
    pub fracture_in_play: bool,
    /// Systems holding an ingress token, the only way a Fracture system is adjacent to
    /// anything in the main galaxy.
    pub ingress_tokens: BTreeSet<SystemId>,
    /// Creuss wormhole token kind to system. Dynamic wormholes are board state, not printed
    /// tile data, and Wormhole Generator can move either token. Not compared.
    pub wormhole_tokens: BTreeMap<String, SystemId>,
    /// The ion storm token: where it sits, and which face is up (`ALPHA` or `BETA`).
    ///
    /// Its own field rather than an entry in `wormhole_tokens`, which is keyed by kind and so holds
    /// one system per kind. The ion storm shows alpha or beta and can coexist with a Creuss token
    /// of the same letter somewhere else, which that map cannot express.
    #[serde(default)]
    pub ion_storm: Option<(SystemId, String)>,
    /// Planets placed onto a tile during play, mapped to the system they were placed in.
    ///
    /// Twelve planets in the corpus have no printed `tileId` (Mirage, Custodia Vigilia, the ocean
    /// planets) because they arrive from a deck rather than from the map. The corpus therefore
    /// cannot answer "which planets are in this system" for them, and this overlay is what does --
    /// see `ti4_engine::planets::in_system`, which unions the two.
    #[serde(default)]
    pub placed_planets: BTreeMap<PlanetId, SystemId>,
    /// Players whose influence currently pays a unit's cost as if it were resources (Freelancers).
    ///
    /// Held as a set rather than a flag because more than one seat can be mid-production in a
    /// nested resolution, and cleared by the effect that set it -- its life is one production, not
    /// one window, which is why it needs no sequence number the way `combat_bonus_round` does.
    #[serde(default)]
    pub influence_pays_for_units: BTreeSet<PlayerId>,
    /// Systems holding a Crimson breach placed by Exile II.
    pub breach_tokens: BTreeSet<SystemId>,
    /// The system the Thunder's Edge planet token was placed in, once the expedition
    /// completed and it flipped to its planet side.
    pub thunders_edge_system: Option<SystemId>,
    /// The galactic event in play, if one was put into play.
    pub galactic_event: Option<String>,
    /// System tiles purged from the board (currently The Silver Flame's failed roll).
    pub purged_systems: BTreeSet<SystemId>,

    // -- sequence counters --------------------------------------------------------
    /// Increments at the start of every combat round, anywhere on the board.
    pub combat_round_seq: u32,
    /// Increments at the start of every use of PRODUCTION.
    pub production_seq: u32,
    /// Increments on every system activation.
    pub activation_seq: u32,
    /// Increments when an agenda is revealed, so a card scoped to "this agenda" can say which.
    #[serde(default)]
    pub agenda_seq: u32,
    /// Pairs who have resolved a transaction this round, in the order they resolved.
    ///
    /// Lie in Wait fires "after 2 of your neighbors resolve a transaction", which is a fact about
    /// the round rather than about the deal in hand, so it cannot be answered from one transaction.
    /// Cleared when a round begins, because the card counts *this* round's deals.
    #[serde(default)]
    pub transactions_this_round: Vec<(PlayerId, PlayerId)>,
    /// Increments whenever an action-phase turn actually passes to a player. It does *not*
    /// increment between Fleet Logistics' first and second actions, because those are
    /// explicitly the same turn.
    pub turn_seq: u32,
    /// Allocator for typed secret-objective trigger occurrences.
    ///
    /// This sequence affects replay-visible event identity and is included in structural equality.
    #[serde(default)]
    pub feat_occurrence_seq: u64,
    /// Combat occurrences a player has already used to score a secret objective.
    #[serde(default)]
    pub scored_feat_occurrences: BTreeSet<(PlayerId, FeatOccurrence)>,
    /// Skilled Retreat: the combat round at which a card declared the space combat a draw.
    /// Without it the player who stayed reads as the winner, since the retreating fleet
    /// simply is not there any more — true of an ordinary retreat, and exactly what this
    /// card overrides.
    pub combat_draw_round: Option<u32>,

    // -- reroll windows (Fire Team, Scramble Frequency, Aglnlan Oln) -------------
    /// Rolls made by each player at the moment currently open to a reroll window, keyed by
    /// the roller. Set at the roll site, cleared when the window behind it closes. Not
    /// compared: in-flight resolution data, like the production bookkeeping above.
    #[serde(default)]
    pub reroll_staging: BTreeMap<PlayerId, RerollSet>,
    /// The roller named by the most recent roll-window emission — the "that player" of
    /// Scramble Frequency. Cleared with the staging. Not compared.
    #[serde(default)]
    pub last_reroll_player: Option<PlayerId>,
    /// The unit most recently committed to a planet: its owner, the system, the planet and
    /// the unit itself. The commit step records this before emitting `UNITS_COMMITTED`, and
    /// Parley reads it back to return the unit to the space area. Every later landing
    /// overwrites it before its window opens, and Parley clears it when it acts, so a stale
    /// value is never read. In-flight bookkeeping — not compared.
    #[serde(default)]
    pub last_committed_unit: Option<(PlayerId, SystemId, PlanetId, Unit)>,
    /// The most recent SUSTAIN DAMAGE use: the system, the player whose ship sustained,
    /// the unit type that did it, and the player whose unit or ability produced the hit it
    /// cancelled. Both emission sites (the combat window's sustain stage and the
    /// absorption path) record it right before emitting `SUSTAIN_DAMAGE_USED`, and Direct
    /// Hit reads it back in the window that follows — the event itself is consumed by the
    /// timing machinery, which the effect cannot see. In-flight bookkeeping — not compared.
    #[serde(default)]
    pub last_sustain: Option<(SystemId, PlayerId, UnitTypeId, PlayerId)>,
    /// Destructions staged by a card effect to be announced once the card's own resolution is
    /// complete. A Direct Hit destroys a ship from inside a timing-window effect, which holds no
    /// resolver of its own, so it records the removal here and the card-announce step emits the
    /// `SHIP_DESTROYED` event through the game's resolver on its behalf. The tuple carries the
    /// system, the owner, and the destroyed unit's type; the `last` fact is recomputed at
    /// emission time from the board, which nothing else has touched in between. In-flight — not compared.
    #[serde(default)]
    pub pending_destructions: Vec<(SystemId, PlayerId, UnitTypeId)>,
    /// The most recent destroyed ship: the system, the owner, and the unit type. Both
    /// emission sites (the combat window's casualty step and the card-announce drain of
    /// staged destructions) record it right before emitting `SHIP_DESTROYED`, and cards that
    /// react to a destruction and need to know *which* ship it was — Courageous to the End
    /// rolls against that ship's combat value, Crash Landing acts on the system — read it
    /// back: the event itself is consumed by the timing machinery, which the effect cannot
    /// see. In-flight bookkeeping — not compared.
    #[serde(default)]
    pub last_ship_destroyed: Option<(SystemId, PlayerId, UnitTypeId)>,
    /// The capture a `PLANET_CONTROL_GAINED` window is reacting to: the system, the planet,
    /// the player who gained control, and the former controller when there was one. The frame
    /// records it right before the emission, because a card played into the window — Infiltrate
    /// replaces structures on that planet, Reparations asks who the planet was taken from —
    /// cannot read the payload of the event that summoned it. In-flight bookkeeping — not
    /// compared.
    #[serde(default)]
    pub last_control_gained: Option<(SystemId, PlanetId, PlayerId, Option<PlayerId>)>,
    /// The space combat a `SPACE_COMBAT_WON` window is reacting to: the system and the
    /// opponents the winner fought — the sides when the fight opened, minus the winner. By
    /// the time the window runs the losers' ships are off the board, so the board itself
    /// answers with the winner alone, and Salvage, which takes its opponents' commodities,
    /// reads the handoff instead. In-flight bookkeeping — not compared.
    #[serde(default)]
    pub last_combat_sides: Option<(SystemId, Vec<PlayerId>)>,
    /// The action card a `ACTION_CARD_DISCARDED` window is reacting to: the player who
    /// discarded it and the card's alias. The frame records it in the discard announcement
    /// itself, which is the one place that knows both, because the effect — Reverse Engineer
    /// takes that card from the pile — cannot read the event that summoned it. In-flight
    /// bookkeeping — not compared.
    #[serde(default)]
    pub last_action_discarded: Option<(PlayerId, ActionCardId)>,
    /// Hits staged by Reflective Shielding to be absorbed once the sustain window that played
    /// the card has closed: the system, the victim (the sustained hit's producer — "your
    /// opponent" in the card text) and the count. The sustain step drains it straight after
    /// the emission, so the victim's own sustain answers and loss choices still happen.
    /// In-flight bookkeeping — not compared.
    #[serde(default)]
    pub pending_reflective_hits: Option<(SystemId, PlayerId, usize)>,
    /// The strategy card choice just made: the picker and the card chosen. The driver
    /// records it right before emitting `STRATEGY_CARD_CHOSEN`, and Public Disgrace reads
    /// it back in the window that follows to put the picker's choice back on the mat and
    /// make a different one — the event itself is consumed by the timing machinery,
    /// which the effect cannot see. In-flight bookkeeping — not compared.
    #[serde(default)]
    pub last_strategy_choice: Option<(PlayerId, StrategyCardId)>,

    // -- agenda-phase bookkeeping (in-flight, not compared) -----------------------
    /// Veto: when played into the `AGENDA_REVEALED` window, the alias of the agenda drawn
    /// from the top of the agenda deck that replaces the one just revealed. The revealed
    /// agenda is already out of the deck (drawn at the start of the agenda phase), so this
    /// is the next one behind it. Consumed and cleared by the vote driver before it builds
    /// the vote window. In-flight resolution data — not compared.
    #[serde(default)]
    pub agenda_veto_replacement: Option<String>,
    /// Confusing / Confounding Legal Text: the player who is the elected player after the
    /// card redirects the outcome. Consumed and cleared by the vote driver before it applies
    /// the agenda's own effect. In-flight resolution data — not compared.
    #[serde(default)]
    pub agenda_elected_override: Option<PlayerId>,
    /// The outcome each player actually voted for on the agenda currently being resolved,
    /// mirrored from the vote's ballot before the `AGENDA_RESOLVED` window opens and cleared
    /// when it closes. A guard played into that window (Deadly Plot's "if you voted for or
    /// predicted another outcome") can only read `GameState`, and the ballot itself lives in
    /// the vote window the driver holds. In-flight resolution data — not compared.
    #[serde(default)]
    pub agenda_votes: BTreeMap<PlayerId, String>,

    // -- turn-flow bookkeeping (in-flight, not compared) ---------------------------
    /// Turn-flow flags set by reaction cards and consumed at the next turn boundary:
    /// Deadly Plot discards the agenda being resolved (no effect, no payouts, no law, no
    /// elected feat), Coup d'Etat cancels the strategic action that just began without
    /// exhausting its card, Crisis makes the turn driver skip the seat the turn moves to,
    /// and Master Plan keeps the same player's turn going for an additional action (same
    /// `turn_seq`, no transaction reset — the Fleet Logistics reading in `phase.rs`).
    /// Each flag is cleared at the point it is consumed, and an explicit pass declines the
    /// Master Plan grant, so a stale value cannot reach the next turn or agenda.
    #[serde(default)]
    pub transient_flags: TransientFlags,

    // -- production bookkeeping ----------------------------------------------------
    /// Fighters placed by the PRODUCTION use currently resolving. Prophecy of Ixth asks how
    /// many fighters were *produced*, which the board cannot answer — fighters already
    /// sitting there were not produced by this use.
    pub fighters_produced_this_use: i32,
    /// Hacan's Auto-Factories counts non-fighter ships produced by one production effect.
    /// Choices resolve one unit type at a time, so the completed board cannot recover which
    /// ships belonged to the same effect.
    pub nonfighter_ships_produced_this_use: i32,
    /// All units produced during the current use, for effects requiring at least one actual
    /// unit rather than merely opening a PRODUCTION window.
    pub units_produced_this_use: i32,
    /// Sarween Tools and AI Development Algorithm reduce one combined PRODUCTION bill.
    /// Production is selected incrementally here, so the unused discount carries across
    /// selections within the current `production_seq`.
    pub production_discount_remaining: i32,
    /// Hegemonic Trade Policy's one planet whose printed values are swapped for the current
    /// production use.
    pub production_value_swapped_planet: Option<PlanetId>,
    /// Letnev's Gravleash Maneuvers: origin system to the highest move value among ships
    /// already selected to move from it this tactical action. Movement is chosen one ship
    /// at a time, so retaining the anchor lets later slower hulls legally inherit it
    /// without treating every stationary ship as "moving". Not compared.
    pub gravleash_move_values: BTreeMap<SystemId, i32>,

    // -- transactions (LRR 94) -----------------------------------------------------
    /// The outcomes on offer for the agenda currently being voted on.
    ///
    /// Set when the agenda is revealed and read by anything played into that window, which is
    /// what lets Imperial Rider predict one of them without reaching into the vote.
    pub agenda_choices: Vec<String>,
    /// Imperial Rider: the outcome each player predicted for the agenda being voted on.
    ///
    /// Held on the game rather than the seat because it belongs to one agenda, not to a player,
    /// and is cleared when that agenda resolves. A player with an entry here has given up their
    /// vote on it — the card's own cost.
    pub agenda_predictions: BTreeMap<PlayerId, String>,
    /// Who each player has already transacted with this turn. LRR 94.1 caps it at one per
    /// neighbour, so this clears when the turn passes. Not compared.
    pub transactions_this_turn: BTreeMap<PlayerId, BTreeSet<PlayerId>>,
    /// Trade goods each player has received *through a transaction*, cumulatively.
    ///
    /// Separate from `Player::trade_goods` because provenance is not recoverable from a
    /// wallet: a trade good taken off a strategy card and one negotiated out of a neighbour
    /// look identical once banked, and only the second says anything about whether the seat
    /// is using the table. Not compared.
    pub traded_goods: BTreeMap<PlayerId, i32>,
    /// The subset paid for with a promissory note — the deepest form of the deal, trading a
    /// future obligation for present goods. Not compared.
    pub traded_goods_for_promissory: BTreeMap<PlayerId, i32>,
    /// Promise outcomes. Absent, or present with `None`, means still outstanding.
    /// Not compared.
    #[serde(with = "promise_map")]
    pub promises: BTreeMap<PromiseKey, Option<bool>>,
    /// Support for the Throne: owner to the player holding it faceup. An absent owner still
    /// has their own note in hand.
    ///
    /// Its own field rather than folded into `promissory_notes` because Support is the one
    /// note whose position is worth a victory point, and a great deal of scoring reads it
    /// directly. Not compared.
    pub support_holders: BTreeMap<PlayerId, PlayerId>,
    /// Every other promissory note: note id to the player holding it.
    ///
    /// A note id is `alias:owner` — `"cf:letnev"` is the Letnev player's Ceasefire. Notes
    /// start in their owner's hand, move by transaction, and return to the owner when their
    /// effect resolves. Not compared.
    pub promissory_notes: BTreeMap<String, PlayerId>,
    /// Notes faceup in a play area rather than held in hand (LRR 69.3). Alliance and Trade
    /// Convoys work from the play area; the rest resolve from hand.
    pub promissory_faceup: BTreeSet<String>,
}

/// Equality over the declared, compared fields only — the oracle marks 20 of these maps
/// `compare=False`, and the trace-equality tooling depends on that.
///
/// Note what this means: **`board` is excluded**, so two states differing only in unit
/// positions compare equal. That is surprising, and it is the oracle's behaviour, so it is
/// reproduced rather than quietly improved. Use [`GameState::identical`] where a full
/// structural comparison is wanted.
impl PartialEq for GameState {
    fn eq(&self, other: &Self) -> bool {
        self.players == other.players
            && self.seating_order == other.seating_order
            && self.speaker == other.speaker
            && self.phase == other.phase
            && self.round == other.round
            && self.active == other.active
            && self.unclaimed_strategy_cards == other.unclaimed_strategy_cards
            && self.strategy_card_goods == other.strategy_card_goods
            && self.strategy_cards_per_player == other.strategy_cards_per_player
            && self.active_system == other.active_system
            && self.pending == other.pending
            && self.exhausted_planets == other.exhausted_planets
            && self.revealed_objectives == other.revealed_objectives
            && self.objective_deck == other.objective_deck
            && self.finished == other.finished
            && self.relic_deck == other.relic_deck
            && self.agenda_deck == other.agenda_deck
            && self.action_card_deck == other.action_card_deck
            && self.discarded_action_cards == other.discarded_action_cards
            && self.secret_deck == other.secret_deck
            && self.custodians_removed == other.custodians_removed
            && self.frontier_tokens == other.frontier_tokens
            && self.fracture_in_play == other.fracture_in_play
            && self.ingress_tokens == other.ingress_tokens
            && self.breach_tokens == other.breach_tokens
            && self.thunders_edge_system == other.thunders_edge_system
            && self.galactic_event == other.galactic_event
            && self.purged_systems == other.purged_systems
            && self.combat_round_seq == other.combat_round_seq
            && self.production_seq == other.production_seq
            && self.activation_seq == other.activation_seq
            && self.turn_seq == other.turn_seq
            && self.feat_occurrence_seq == other.feat_occurrence_seq
            && self.scored_feat_occurrences == other.scored_feat_occurrences
            && self.combat_draw_round == other.combat_draw_round
            && self.fighters_produced_this_use == other.fighters_produced_this_use
            && self.nonfighter_ships_produced_this_use == other.nonfighter_ships_produced_this_use
            && self.units_produced_this_use == other.units_produced_this_use
            && self.production_discount_remaining == other.production_discount_remaining
            && self.production_value_swapped_planet == other.production_value_swapped_planet
            && self.promissory_faceup == other.promissory_faceup
    }
}

impl GameState {
    /// A game at the start of its first strategy phase.
    #[must_use]
    pub fn new(
        player_ids: &[PlayerId],
        strategy_card_ids: &[StrategyCardId],
        card_initiative: BTreeMap<StrategyCardId, i32>,
        speaker: Option<PlayerId>,
        cards_per_player: usize,
    ) -> Self {
        let speaker = speaker
            .or_else(|| player_ids.first().cloned())
            .unwrap_or_else(|| PlayerId::new(""));
        Self {
            players: player_ids.iter().cloned().map(Player::new).collect(),
            seating_order: player_ids.to_vec(),
            speaker,
            phase: Phase::Strategy,
            round: 1,
            active: None,
            unclaimed_strategy_cards: strategy_card_ids.to_vec(),
            strategy_card_goods: BTreeMap::new(),
            card_initiative,
            strategy_cards_per_player: cards_per_player,
            board: BTreeMap::new(),
            active_system: None,
            pending: None,
            exhausted_planets: BTreeSet::new(),
            revealed_objectives: Vec::new(),
            objective_deck: Vec::new(),
            scored_objectives: BTreeMap::new(),
            finished: false,
            exploration_decks: BTreeMap::new(),
            planet_attachments: BTreeMap::new(),
            relic_deck: Vec::new(),
            agenda_deck: Vec::new(),
            action_card_deck: Vec::new(),
            discarded_action_cards: Vec::new(),
            secret_deck: Vec::new(),
            laws: BTreeMap::new(),
            custodians_removed: false,
            frontier_tokens: BTreeSet::new(),
            expedition_slices: BTreeMap::new(),
            fracture_in_play: false,
            ingress_tokens: BTreeSet::new(),
            wormhole_tokens: BTreeMap::new(),
            ion_storm: None,
            placed_planets: BTreeMap::new(),
            influence_pays_for_units: BTreeSet::new(),
            breach_tokens: BTreeSet::new(),
            thunders_edge_system: None,
            galactic_event: None,
            purged_systems: BTreeSet::new(),
            combat_round_seq: 0,
            production_seq: 0,
            activation_seq: 0,
            agenda_seq: 0,
            transactions_this_round: Vec::new(),
            turn_seq: 0,
            feat_occurrence_seq: 0,
            scored_feat_occurrences: BTreeSet::new(),
            combat_draw_round: None,
            reroll_staging: BTreeMap::new(),
            last_reroll_player: None,
            last_committed_unit: None,
            last_sustain: None,
            pending_destructions: Vec::new(),
            last_ship_destroyed: None,
            last_control_gained: None,
            last_combat_sides: None,
            last_action_discarded: None,
            pending_reflective_hits: None,
            last_strategy_choice: None,
            agenda_veto_replacement: None,
            agenda_elected_override: None,
            agenda_votes: BTreeMap::new(),
            transient_flags: TransientFlags::default(),
            fighters_produced_this_use: 0,
            nonfighter_ships_produced_this_use: 0,
            units_produced_this_use: 0,
            production_discount_remaining: 0,
            production_value_swapped_planet: None,
            gravleash_move_values: BTreeMap::new(),
            agenda_choices: Vec::new(),
            agenda_predictions: BTreeMap::new(),
            transactions_this_turn: BTreeMap::new(),
            traded_goods: BTreeMap::new(),
            traded_goods_for_promissory: BTreeMap::new(),
            promises: BTreeMap::new(),
            support_holders: BTreeMap::new(),
            promissory_notes: BTreeMap::new(),
            promissory_faceup: BTreeSet::new(),
        }
    }

    /// Full structural comparison, including the fields [`PartialEq`] skips.
    #[must_use]
    pub fn identical(&self, other: &Self) -> bool {
        self == other
            && self.board == other.board
            && self.card_initiative == other.card_initiative
            && self.scored_objectives == other.scored_objectives
            && self.exploration_decks == other.exploration_decks
            && self.planet_attachments == other.planet_attachments
            && self.laws == other.laws
            && self.expedition_slices == other.expedition_slices
            && self.wormhole_tokens == other.wormhole_tokens
            && self.ion_storm == other.ion_storm
            && self.placed_planets == other.placed_planets
            && self.gravleash_move_values == other.gravleash_move_values
            && self.agenda_choices == other.agenda_choices
            && self.agenda_predictions == other.agenda_predictions
            && self.transactions_this_turn == other.transactions_this_turn
            && self.traded_goods == other.traded_goods
            && self.traded_goods_for_promissory == other.traded_goods_for_promissory
            && self.promises == other.promises
            && self.support_holders == other.support_holders
            && self.promissory_notes == other.promissory_notes
            && self.players.iter().zip(&other.players).all(|(a, b)| {
                a.relic_fragments == b.relic_fragments
                    && a.leaders == b.leaders
                    && a.assimilated_technologies == b.assimilated_technologies
            })
    }

    // -- lookups ----------------------------------------------------------------

    #[must_use]
    pub fn player(&self, id: &PlayerId) -> Option<&Player> {
        self.players.iter().find(|p| &p.id == id)
    }

    pub fn player_mut(&mut self, id: &PlayerId) -> Option<&mut Player> {
        self.players.iter_mut().find(|p| &p.id == id)
    }

    /// Players ordered by their strategy card's initiative number.
    ///
    /// A player holding two cards — the three-player deal — takes their initiative from the
    /// lower-numbered one, so a seat holding Leadership and Imperial goes first, not last.
    /// Players holding no card sort last, stably by seating.
    #[must_use]
    pub fn initiative_order(&self) -> Vec<PlayerId> {
        let mut ordered: Vec<&Player> = self.players.iter().collect();
        ordered.sort_by_key(|p| {
            let initiative = p
                .strategy_cards
                .iter()
                .map(|c| self.card_initiative.get(c).copied().unwrap_or(99))
                .min()
                .unwrap_or(99);
            let seat = self
                .seating_order
                .iter()
                .position(|id| id == &p.id)
                .unwrap_or(usize::MAX);
            (initiative, seat)
        });
        ordered.iter().map(|p| p.id.clone()).collect()
    }

    /// Seating order rotated to begin at a player.
    #[must_use]
    pub fn clockwise_from(&self, id: &PlayerId) -> Vec<PlayerId> {
        let Some(start) = self.seating_order.iter().position(|p| p == id) else {
            return self.seating_order.clone();
        };
        self.seating_order[start..]
            .iter()
            .chain(&self.seating_order[..start])
            .cloned()
            .collect()
    }

    #[must_use]
    pub fn all_passed(&self) -> bool {
        self.players.iter().all(|p| p.passed)
    }

    // -- board lookups -----------------------------------------------------------

    /// The state of a system. An absent system is an empty one.
    #[must_use]
    pub fn system_state(&self, system: &SystemId) -> SystemState {
        self.board.get(system).cloned().unwrap_or_default()
    }

    /// A system's state for mutation, created empty if absent.
    pub fn system_mut(&mut self, system: &SystemId) -> &mut SystemState {
        self.board.entry(system.clone()).or_default()
    }

    #[must_use]
    pub fn units_in(&self, system: &SystemId) -> &[Unit] {
        self.board.get(system).map_or(&[], |s| s.units.as_slice())
    }

    #[must_use]
    pub fn ships_of(&self, player: &PlayerId, system: &SystemId) -> Vec<&Unit> {
        self.board
            .get(system)
            .map(|s| s.units_of(player))
            .unwrap_or_default()
    }

    #[must_use]
    pub fn systems_with_token(&self, player: &PlayerId) -> BTreeSet<&SystemId> {
        self.board
            .iter()
            .filter(|(_, s)| s.command_tokens.contains(player))
            .map(|(id, _)| id)
            .collect()
    }

    /// Systems held by somebody else, which block passage (LRR 58.4b).
    #[must_use]
    pub fn systems_with_ships_other_than(&self, player: &PlayerId) -> BTreeSet<&SystemId> {
        self.board
            .iter()
            .filter(|(_, s)| s.units.iter().any(|u| &u.owner != player))
            .map(|(id, _)| id)
            .collect()
    }

    #[must_use]
    pub fn systems_with_units_of(&self, player: &PlayerId) -> BTreeSet<&SystemId> {
        self.board
            .iter()
            .filter(|(_, s)| s.has_units_of(player))
            .map(|(id, _)| id)
            .collect()
    }

    /// `(system, planet)` for every planet the player controls, in sorted order.
    ///
    /// The oracle memoises this per state; scorers ask for it constantly, 70,885 inner
    /// iterations in a round-4 six-player game. No cache here: the scan is a `BTreeMap`
    /// walk, and a cache on a mutable value is the kind of thing that goes stale.
    #[must_use]
    pub fn controlled_planets(&self, player: &PlayerId) -> Vec<(&SystemId, &PlanetId)> {
        self.board
            .iter()
            .flat_map(|(system, state)| {
                state
                    .planet_control
                    .iter()
                    .filter(move |(_, owner)| *owner == player)
                    .map(move |(planet, _)| (system, planet))
            })
            .collect()
    }

    // -- transactions (LRR 94) ----------------------------------------------------

    #[must_use]
    pub fn transacted_with(&self, player: &PlayerId) -> BTreeSet<PlayerId> {
        self.transactions_this_turn
            .get(player)
            .cloned()
            .unwrap_or_default()
    }

    /// LRR 94.1 limits each *pair*, so both sides remember it.
    pub fn record_transaction(&mut self, a: &PlayerId, b: &PlayerId) {
        self.transactions_this_turn
            .entry(a.clone())
            .or_default()
            .insert(b.clone());
        self.transactions_this_turn
            .entry(b.clone())
            .or_default()
            .insert(a.clone());
    }

    /// A new turn restores everyone's one transaction per neighbour.
    /// The command tokens a faction still has in reinforcements (LRR 20.4).
    ///
    /// Sixteen per player, counting every token on the command sheet and every one placed on the
    /// board. Running out is a real constraint -- it is what stops a late-game player buying an
    /// unbounded number of tactical actions with influence -- and without it the three pools grew
    /// without limit.
    #[must_use]
    pub fn tokens_in_reinforcements(&self, player: &PlayerId) -> i32 {
        let on_sheet = self.player(player).map_or(0, |seat| {
            seat.tactic_tokens + seat.fleet_tokens + seat.strategic_tokens
        });
        let on_board = i32::try_from(
            self.board
                .values()
                .filter(|here| here.command_tokens.contains(player))
                .count(),
        )
        .unwrap_or(i32::MAX);
        (TOKENS_PER_FACTION - on_sheet - on_board).max(0)
    }

    /// Gain command tokens into a pool, capped by what is left in reinforcements (LRR 20.4/20.4a).
    ///
    /// Returns how many were actually gained, which is not always what was asked for: "if a player
    /// would gain a command token but has none available in their reinforcements, that player
    /// cannot gain that command token."
    pub fn gain_token(&mut self, player: &PlayerId, pool: TokenPool, count: i32) -> i32 {
        if count <= 0 {
            // Returning a token to reinforcements is never capped.
            if let Some(seat) = self.player_mut(player) {
                seat.gain_token_uncapped(pool, count);
            }
            return count;
        }
        let granted = count.min(self.tokens_in_reinforcements(player));
        if granted > 0 && let Some(seat) = self.player_mut(player) {
            seat.gain_token_uncapped(pool, granted);
        }
        granted
    }

    pub fn clear_transactions(&mut self) {
        self.transactions_this_turn.clear();
    }

    pub fn record_promise(&mut self, promiser: &PlayerId, partner: &PlayerId, promise: &str) {
        self.promises
            .entry((promiser.clone(), partner.clone(), promise.to_owned()))
            .or_default();
    }

    pub fn settle_promise(
        &mut self,
        promiser: &PlayerId,
        partner: &PlayerId,
        promise: &str,
        kept: bool,
    ) {
        self.promises.insert(
            (promiser.clone(), partner.clone(), promise.to_owned()),
            Some(kept),
        );
    }

    /// How many promises this player has kept and broken.
    #[must_use]
    pub fn promise_record(&self, player: &PlayerId) -> (usize, usize) {
        let mut kept = 0;
        let mut broken = 0;
        for ((who, _, _), outcome) in &self.promises {
            if who != player {
                continue;
            }
            match outcome {
                Some(true) => kept += 1,
                Some(false) => broken += 1,
                None => {}
            }
        }
        (kept, broken)
    }

    // -- agendas (LRR 8) -----------------------------------------------------------

    #[must_use]
    pub fn laws_in_play(&self) -> Vec<&String> {
        self.laws.keys().collect()
    }

    /// LRR 8.20: a law that passed stays in play permanently.
    pub fn enact_law(&mut self, alias: &str, outcome: &str) {
        self.laws.insert(alias.to_owned(), outcome.to_owned());
    }

    pub fn repeal_law(&mut self, alias: &str) {
        self.laws.remove(alias);
    }

    /// Claim an expedition slice. Each may be claimed once, by one player.
    pub fn claim_slice(&mut self, slice: &str, player: &PlayerId) {
        self.expedition_slices
            .insert(slice.to_owned(), player.clone());
    }

    // -- objectives -----------------------------------------------------------------

    #[must_use]
    pub fn scored_by(&self, player: &PlayerId) -> BTreeSet<ObjectiveId> {
        self.scored_objectives
            .get(player)
            .cloned()
            .unwrap_or_default()
    }

    pub fn record_score(&mut self, player: &PlayerId, objective: ObjectiveId) {
        self.scored_objectives
            .entry(player.clone())
            .or_default()
            .insert(objective);
    }

    // -- feats ----------------------------------------------------------------------

    /// Begin a concrete secret-objective trigger occurrence.
    ///
    /// # Panics
    ///
    /// Panics if the game allocates more than `u64::MAX` occurrences.
    #[must_use]
    pub fn begin_feat_occurrence(&mut self) -> FeatOccurrence {
        self.feat_occurrence_seq = self
            .feat_occurrence_seq
            .checked_add(1)
            .expect("feat occurrence sequence exhausted");
        FeatOccurrence(self.feat_occurrence_seq)
    }

    /// Record a feat against one concrete trigger occurrence.
    pub fn record_event_feat(&mut self, player: &PlayerId, feat: Feat, occurrence: FeatOccurrence) {
        if let Some(seat) = self.player_mut(player)
            && !seat.event_feats.contains(&(feat, occurrence))
        {
            seat.event_feats.push((feat, occurrence));
            seat.event_feats.sort_unstable();
        }
    }

    /// Whether this player did a feat in the exact trigger occurrence.
    #[must_use]
    pub fn did_at_occurrence(
        &self,
        player: &PlayerId,
        feat: Feat,
        occurrence: FeatOccurrence,
    ) -> bool {
        self.player(player)
            .is_some_and(|seat| seat.event_feats.contains(&(feat, occurrence)))
    }

    #[must_use]
    pub fn scored_at_occurrence(&self, player: &PlayerId, occurrence: FeatOccurrence) -> bool {
        self.scored_feat_occurrences
            .contains(&(player.clone(), occurrence))
    }

    pub fn record_occurrence_score(&mut self, player: &PlayerId, occurrence: FeatOccurrence) {
        self.scored_feat_occurrences
            .insert((player.clone(), occurrence));
    }

    /// Turn the top facedown public objective faceup (LRR 81.2).
    pub fn reveal_objective(&mut self) -> Option<ObjectiveId> {
        if self.objective_deck.is_empty() {
            return None;
        }
        let top = self.objective_deck.remove(0);
        self.revealed_objectives.push(top.clone());
        Some(top)
    }

    pub fn exhaust_planet(&mut self, planet: PlanetId) {
        self.exhausted_planets.insert(planet);
    }

    /// Ready one exhausted planet card (LRR 71).
    pub fn ready_planet(&mut self, planet: &PlanetId) {
        self.exhausted_planets.remove(planet);
    }

    /// Status phase: ready every exhausted card (LRR 34.2).
    pub fn ready_all_planets(&mut self) {
        self.exhausted_planets.clear();
    }

    // -- board updates ---------------------------------------------------------------

    pub fn move_units(&mut self, origin: &SystemId, destination: &SystemId, units: &[Unit]) {
        self.system_mut(origin).remove(units);
        self.system_mut(destination).add(units);
    }

    pub fn destroy_units(&mut self, system: &SystemId, units: &[Unit]) {
        self.system_mut(system).remove(units);
    }

    // -- strategy cards ----------------------------------------------------------------

    fn initiative_sorted(&self, cards: &mut [StrategyCardId]) {
        cards.sort_by_key(|c| self.card_initiative.get(c).copied().unwrap_or(99));
    }

    /// Add a card to a player's holding, keeping it in initiative order.
    pub fn deal_strategy_card(&mut self, player: &PlayerId, card: StrategyCardId) -> bool {
        let Some(existing) = self.player(player) else {
            return false;
        };
        let mut held = existing.strategy_cards.clone();
        held.push(card);
        self.initiative_sorted(&mut held);
        if let Some(p) = self.player_mut(player) {
            p.strategy_cards = held;
        }
        true
    }

    /// Spend one held card, leaving any others untouched.
    pub fn exhaust_strategy_card(&mut self, player: &PlayerId, card: StrategyCardId) -> bool {
        self.player_mut(player)
            .is_some_and(|p| p.exhausted_strategy_cards.insert(card))
    }

    /// Exchange one held card for another, preserving the rest of the holding.
    ///
    /// Imperial Arbiter and Quantum Datahub Node both swap a single card. Assigning the
    /// whole holding for those would discard a second held card in a three-player game —
    /// the swap would silently destroy a card.
    ///
    /// An exhausted card handed over arrives exhausted: the strategic action was already
    /// taken and the swap does not give it back.
    pub fn swap_strategy_card(
        &mut self,
        player: &PlayerId,
        out: &StrategyCardId,
        into: StrategyCardId,
    ) -> bool {
        let Some(existing) = self.player(player) else {
            return false;
        };
        let mut held: Vec<StrategyCardId> = existing
            .strategy_cards
            .iter()
            .filter(|c| *c != out)
            .cloned()
            .collect();
        held.push(into.clone());
        self.initiative_sorted(&mut held);

        let mut spent = existing.exhausted_strategy_cards.clone();
        let was_spent = spent.remove(out);
        if was_spent {
            spent.insert(into);
        }

        if let Some(p) = self.player_mut(player) {
            p.strategy_cards = held;
            p.exhausted_strategy_cards = spent;
        }
        true
    }

    /// Drop a player's whole holding, e.g. at the end of a round.
    ///
    /// A dropped card must not stay behind in the exhausted set, or a later deal of the
    /// same card would arrive already spent.
    pub fn clear_strategy_cards(&mut self, player: &PlayerId) {
        if let Some(p) = self.player_mut(player) {
            p.strategy_cards.clear();
            p.exhausted_strategy_cards.clear();
        }
    }
}

#[cfg(test)]
mod tests {

    /// 20.4/20.4a: a faction has sixteen command tokens and cannot gain a seventeenth.
    ///
    /// Eight start on the sheet. A token is either on the sheet or on the board, never both, so
    /// placing one on the board does not free a slot -- it moves the count from one side of the
    /// sum to the other. Without this the three pools grew without bound, and a late-game player
    /// could buy an unlimited number of tactical actions with influence.
    #[test]
    fn a_faction_has_sixteen_command_tokens_and_no_more() {
        let player = PlayerId::new("a");
        let mut state = GameState::new(
            std::slice::from_ref(&player),
            &[],
            BTreeMap::new(),
            None,
            0,
        );

        // 3 + 3 + 2 on the sheet at setup leaves eight in reinforcements.
        assert_eq!(state.tokens_in_reinforcements(&player), 8);

        assert_eq!(state.gain_token(&player, TokenPool::Tactic, 5), 5);
        assert_eq!(state.tokens_in_reinforcements(&player), 3);

        // Asking for more than remains gains only what remains (20.4a).
        assert_eq!(state.gain_token(&player, TokenPool::Fleet, 10), 3);
        assert_eq!(state.tokens_in_reinforcements(&player), 0);
        assert_eq!(state.gain_token(&player, TokenPool::Strategic, 1), 0);

        // A token placed on the board still counts against the sixteen.
        let system = SystemId::new("18");
        state.board.entry(system.clone()).or_default();
        state.system_mut(&system).place_token(player.clone());
        assert_eq!(
            state.tokens_in_reinforcements(&player),
            0,
            "placing does not free a slot"
        );
        if let Some(seat) = state.player_mut(&player) {
            seat.gain_token_uncapped(TokenPool::Tactic, -1);
        }
        assert_eq!(
            state.tokens_in_reinforcements(&player),
            0,
            "and spending it into that system leaves the count where it was"
        );
    }
    use super::*;

    fn cards() -> BTreeMap<StrategyCardId, i32> {
        [
            ("leadership", 1),
            ("diplomacy", 2),
            ("politics", 3),
            ("construction", 4),
            ("trade", 5),
            ("warfare", 6),
            ("technology", 7),
            ("imperial", 8),
        ]
        .into_iter()
        .map(|(c, i)| (StrategyCardId::new(c), i))
        .collect()
    }

    fn game(players: &[&str]) -> GameState {
        let ids: Vec<PlayerId> = players.iter().map(|p| PlayerId::new(*p)).collect();
        let deck: Vec<StrategyCardId> = cards().keys().cloned().collect();
        GameState::new(&ids, &deck, cards(), None, 1)
    }

    fn pid(id: &str) -> PlayerId {
        PlayerId::new(id)
    }

    fn card(id: &str) -> StrategyCardId {
        StrategyCardId::new(id)
    }

    fn unit(type_id: &str, owner: &str) -> Unit {
        Unit::new(UnitTypeId::new(type_id), pid(owner))
    }

    // -- setup ----------------------------------------------------------------

    #[test]
    fn a_new_game_starts_in_the_strategy_phase_of_round_one() {
        let g = game(&["a", "b", "c"]);
        assert_eq!(g.phase, Phase::Strategy);
        assert_eq!(g.round, 1);
        assert_eq!(
            g.speaker,
            pid("a"),
            "the first seat speaks unless told otherwise"
        );
        assert_eq!(g.players.len(), 3);
        assert!(!g.finished);
    }

    #[test]
    fn every_player_opens_with_the_standard_command_tokens() {
        for player in &game(&["a", "b"]).players {
            assert_eq!(player.tactic_tokens, 3);
            assert_eq!(player.fleet_tokens, 3);
            assert_eq!(player.strategic_tokens, 2);
            assert_eq!(player.total_tokens(), 8);
        }
    }

    #[test]
    fn the_strategy_and_agenda_phases_order_by_speaker() {
        assert!(Phase::Strategy.uses_speaker_order());
        assert!(Phase::Agenda.uses_speaker_order());
        assert!(!Phase::Action.uses_speaker_order());
        assert!(!Phase::Status.uses_speaker_order());
    }

    #[test]
    fn event_feats_are_scoped_to_one_monotonic_occurrence() {
        let mut state = game(&["a", "b"]);
        let first = state.begin_feat_occurrence();
        let second = state.begin_feat_occurrence();
        assert_eq!(first, FeatOccurrence(1));
        assert_eq!(second, FeatOccurrence(2));

        state.record_event_feat(&pid("a"), Feat::WonInAnAnomaly, first);
        state.record_event_feat(&pid("a"), Feat::WonInAnAnomaly, first);

        assert!(state.did_at_occurrence(&pid("a"), Feat::WonInAnAnomaly, first));
        assert!(!state.did_at_occurrence(&pid("a"), Feat::WonInAnAnomaly, second));
        assert!(!state.did_at_occurrence(&pid("b"), Feat::WonInAnAnomaly, first));
        assert_eq!(
            state.player(&pid("a")).unwrap().event_feats,
            vec![(Feat::WonInAnAnomaly, first)],
            "recording the same event evidence twice is idempotent"
        );
    }

    #[test]
    fn event_feats_participate_in_state_equality() {
        // The direct-vs-stepped equivalence invariant compares whole states; feat evidence gates
        // secret-scoring eligibility, so it must be part of the projection (M07-021).
        let mut base = game(&["a", "b"]);
        let occurrence = base.begin_feat_occurrence();
        let mut diverged = base.clone();
        diverged.record_event_feat(&pid("a"), Feat::WonInAnAnomaly, occurrence);

        assert_ne!(
            base, diverged,
            "feat evidence is part of the canonical projection"
        );
    }

    // -- command tokens --------------------------------------------------------

    #[test]
    fn a_token_pool_cannot_be_spent_below_zero() {
        let mut player = Player::new(pid("a"));
        player.strategic_tokens = 1;
        assert!(player.spend_token(TokenPool::Strategic));
        assert_eq!(player.strategic_tokens, 0);
        assert!(
            !player.spend_token(TokenPool::Strategic),
            "an empty pool refuses"
        );
        assert_eq!(player.strategic_tokens, 0, "and does not go negative");
    }

    #[test]
    fn a_gained_token_goes_into_the_pool_of_choice() {
        let mut player = Player::new(pid("a"));
        player.gain_token_uncapped(TokenPool::Fleet, 2);
        assert_eq!(player.fleet_tokens, 5);
        assert_eq!(player.tactic_tokens, 3, "other pools are untouched");
        assert_eq!(player.tokens(TokenPool::Fleet), 5);
    }

    // -- strategy cards --------------------------------------------------------

    #[test]
    fn initiative_comes_from_the_lowest_numbered_card_held() {
        let mut g = game(&["a", "b", "c"]);
        g.deal_strategy_card(&pid("a"), card("imperial"));
        g.deal_strategy_card(&pid("b"), card("diplomacy"));
        g.deal_strategy_card(&pid("c"), card("leadership"));
        assert_eq!(g.initiative_order(), vec![pid("c"), pid("b"), pid("a")]);
    }

    #[test]
    fn a_seat_holding_two_cards_acts_on_the_lower_one() {
        // The three-player deal. Leadership and Imperial goes first, not last.
        let mut g = game(&["a", "b", "c"]);
        g.deal_strategy_card(&pid("a"), card("imperial"));
        g.deal_strategy_card(&pid("a"), card("leadership"));
        g.deal_strategy_card(&pid("b"), card("diplomacy"));
        assert_eq!(
            g.player(&pid("a")).unwrap().strategy_card(),
            Some(&card("leadership")),
            "the holding must be sorted by initiative"
        );
        assert_eq!(g.initiative_order()[0], pid("a"));
    }

    #[test]
    fn players_holding_no_card_sort_last_stably_by_seating() {
        let mut g = game(&["a", "b", "c"]);
        g.deal_strategy_card(&pid("c"), card("leadership"));
        assert_eq!(g.initiative_order(), vec![pid("c"), pid("a"), pid("b")]);
    }

    #[test]
    fn a_cardless_player_has_not_exhausted_their_strategy_card() {
        // A vacuous `all()` would read true here and let a cardless player pass.
        let player = Player::new(pid("a"));
        assert!(!player.strategy_card_exhausted());
        assert!(!player.has_unused_strategy_card());
    }

    #[test]
    fn each_held_card_exhausts_separately() {
        let mut g = game(&["a", "b", "c"]);
        g.deal_strategy_card(&pid("a"), card("leadership"));
        g.deal_strategy_card(&pid("a"), card("imperial"));
        g.exhaust_strategy_card(&pid("a"), card("leadership"));

        let player = g.player(&pid("a")).unwrap();
        assert!(
            !player.strategy_card_exhausted(),
            "one card is still unspent"
        );
        assert_eq!(player.unused_strategy_cards(), vec![&card("imperial")]);

        g.exhaust_strategy_card(&pid("a"), card("imperial"));
        assert!(g.player(&pid("a")).unwrap().strategy_card_exhausted());
    }

    #[test]
    fn swapping_a_card_preserves_the_rest_of_the_holding() {
        // Assigning the whole holding here would silently destroy the second card.
        let mut g = game(&["a", "b", "c"]);
        g.deal_strategy_card(&pid("a"), card("leadership"));
        g.deal_strategy_card(&pid("a"), card("imperial"));
        g.swap_strategy_card(&pid("a"), &card("imperial"), card("trade"));

        let player = g.player(&pid("a")).unwrap();
        assert_eq!(
            player.strategy_cards,
            vec![card("leadership"), card("trade")],
            "still two cards, still in initiative order"
        );
    }

    #[test]
    fn an_exhausted_card_handed_over_arrives_exhausted() {
        let mut g = game(&["a", "b"]);
        g.deal_strategy_card(&pid("a"), card("imperial"));
        g.exhaust_strategy_card(&pid("a"), card("imperial"));
        g.swap_strategy_card(&pid("a"), &card("imperial"), card("trade"));

        let player = g.player(&pid("a")).unwrap();
        assert!(
            player.exhausted_strategy_cards.contains(&card("trade")),
            "the strategic action was already taken; the swap does not give it back"
        );
        assert!(!player.exhausted_strategy_cards.contains(&card("imperial")));
    }

    #[test]
    fn an_unexhausted_card_handed_over_arrives_ready() {
        let mut g = game(&["a", "b"]);
        g.deal_strategy_card(&pid("a"), card("imperial"));
        g.swap_strategy_card(&pid("a"), &card("imperial"), card("trade"));
        assert!(
            g.player(&pid("a"))
                .unwrap()
                .exhausted_strategy_cards
                .is_empty()
        );
    }

    #[test]
    fn dropping_a_holding_does_not_leave_a_card_exhausted_behind() {
        // Otherwise a later deal of the same card would arrive already spent.
        let mut g = game(&["a", "b"]);
        g.deal_strategy_card(&pid("a"), card("imperial"));
        g.exhaust_strategy_card(&pid("a"), card("imperial"));
        g.clear_strategy_cards(&pid("a"));
        g.deal_strategy_card(&pid("a"), card("imperial"));
        assert!(!g.player(&pid("a")).unwrap().strategy_card_exhausted());
    }

    // -- seating ---------------------------------------------------------------

    #[test]
    fn clockwise_from_rotates_the_seating_order() {
        let g = game(&["a", "b", "c", "d"]);
        assert_eq!(
            g.clockwise_from(&pid("c")),
            vec![pid("c"), pid("d"), pid("a"), pid("b")]
        );
        assert_eq!(g.clockwise_from(&pid("a")), g.seating_order);
    }

    #[test]
    fn everyone_passing_ends_the_action_phase() {
        let mut g = game(&["a", "b"]);
        assert!(!g.all_passed());
        for player in &mut g.players {
            player.passed = true;
        }
        assert!(g.all_passed());
    }

    // -- board -----------------------------------------------------------------

    #[test]
    fn an_absent_system_is_an_empty_one() {
        let g = game(&["a"]);
        let empty = g.system_state(&SystemId::new("18"));
        assert!(empty.units.is_empty());
        assert!(g.units_in(&SystemId::new("18")).is_empty());
    }

    #[test]
    fn moving_units_takes_them_from_one_system_and_puts_them_in_another() {
        let mut g = game(&["a"]);
        let (from, to) = (SystemId::new("18"), SystemId::new("19"));
        let carrier = unit("carrier", "a");
        g.system_mut(&from).add(std::slice::from_ref(&carrier));

        g.move_units(&from, &to, std::slice::from_ref(&carrier));
        assert!(g.units_in(&from).is_empty());
        assert_eq!(g.units_in(&to), &[carrier]);
    }

    #[test]
    fn removing_a_unit_removes_one_of_it_not_every_copy() {
        // Units are interchangeable values with no identity; removing every match would
        // destroy a whole stack.
        let mut g = game(&["a"]);
        let system = SystemId::new("18");
        let fighter = unit("fighter", "a");
        g.system_mut(&system)
            .add(&[fighter.clone(), fighter.clone(), fighter.clone()]);

        g.destroy_units(&system, std::slice::from_ref(&fighter));
        assert_eq!(g.units_in(&system).len(), 2);
    }

    #[test]
    fn landing_moves_units_from_the_space_area_onto_a_planet() {
        let mut g = game(&["a"]);
        let system = SystemId::new("01");
        let planet = PlanetId::new("jord");
        let infantry = unit("infantry", "a");
        g.system_mut(&system)
            .add(&[infantry.clone(), unit("carrier", "a")]);

        g.system_mut(&system)
            .land(&planet, std::slice::from_ref(&infantry));
        assert_eq!(g.system_state(&system).on_planet(&planet), &[infantry]);
        assert_eq!(g.units_in(&system).len(), 1, "the carrier stays in space");
    }

    #[test]
    fn control_of_a_planet_is_recorded_per_system() {
        let mut g = game(&["a", "b"]);
        let system = SystemId::new("01");
        let planet = PlanetId::new("jord");
        g.system_mut(&system).set_control(planet.clone(), pid("a"));

        assert!(g.system_state(&system).controls_a_planet(&pid("a")));
        assert!(!g.system_state(&system).controls_a_planet(&pid("b")));
        assert_eq!(g.controlled_planets(&pid("a")), vec![(&system, &planet)]);
        assert!(g.controlled_planets(&pid("b")).is_empty());
    }

    #[test]
    fn a_damaged_unit_replaces_the_undamaged_one_in_place() {
        let mut g = game(&["a"]);
        let system = SystemId::new("18");
        let dread = unit("dreadnought", "a");
        let damaged = Unit {
            sustained_damage: true,
            ..dread.clone()
        };
        g.system_mut(&system)
            .add(&[dread.clone(), unit("fighter", "a")]);

        assert!(g.system_mut(&system).replace_unit(&dread, damaged.clone()));
        assert_eq!(g.units_in(&system)[0], damaged);
        assert_eq!(g.units_in(&system).len(), 2);
    }

    #[test]
    fn systems_held_by_someone_else_are_found_for_blocking_passage() {
        let mut g = game(&["a", "b"]);
        let (mine, theirs) = (SystemId::new("18"), SystemId::new("19"));
        g.system_mut(&mine).add(&[unit("carrier", "a")]);
        g.system_mut(&theirs).add(&[unit("carrier", "b")]);

        assert_eq!(
            g.systems_with_ships_other_than(&pid("a")),
            BTreeSet::from([&theirs])
        );
        assert_eq!(g.systems_with_units_of(&pid("a")), BTreeSet::from([&mine]));
    }

    // -- transactions and promises ----------------------------------------------

    #[test]
    fn a_transaction_is_remembered_by_both_sides() {
        // LRR 94.1 limits each pair, not each player.
        let mut g = game(&["a", "b"]);
        g.record_transaction(&pid("a"), &pid("b"));
        assert!(g.transacted_with(&pid("a")).contains(&pid("b")));
        assert!(g.transacted_with(&pid("b")).contains(&pid("a")));

        g.clear_transactions();
        assert!(g.transacted_with(&pid("a")).is_empty());
    }

    #[test]
    fn an_outstanding_promise_counts_as_neither_kept_nor_broken() {
        let mut g = game(&["a", "b"]);
        g.record_promise(&pid("a"), &pid("b"), "vote with me");
        assert_eq!(g.promise_record(&pid("a")), (0, 0));

        g.settle_promise(&pid("a"), &pid("b"), "vote with me", true);
        assert_eq!(g.promise_record(&pid("a")), (1, 0));
    }

    #[test]
    fn recording_a_promise_twice_does_not_overwrite_its_outcome() {
        let mut g = game(&["a", "b"]);
        g.record_promise(&pid("a"), &pid("b"), "p");
        g.settle_promise(&pid("a"), &pid("b"), "p", false);
        g.record_promise(&pid("a"), &pid("b"), "p");
        assert_eq!(g.promise_record(&pid("a")), (0, 1));
    }

    // -- objectives and laws -----------------------------------------------------

    #[test]
    fn revealing_an_objective_moves_it_from_the_deck_to_the_table() {
        let mut g = game(&["a"]);
        g.objective_deck = vec![ObjectiveId::new("one"), ObjectiveId::new("two")];
        assert_eq!(g.reveal_objective(), Some(ObjectiveId::new("one")));
        assert_eq!(g.revealed_objectives, vec![ObjectiveId::new("one")]);
        assert_eq!(g.objective_deck, vec![ObjectiveId::new("two")]);
    }

    #[test]
    fn revealing_from_an_empty_deck_changes_nothing() {
        let mut g = game(&["a"]);
        assert_eq!(g.reveal_objective(), None);
        assert!(g.revealed_objectives.is_empty());
    }

    #[test]
    fn a_law_stays_in_play_until_repealed() {
        let mut g = game(&["a"]);
        g.enact_law("fleet_regulations", "for");
        assert_eq!(g.laws_in_play(), vec![&"fleet_regulations".to_owned()]);
        g.repeal_law("fleet_regulations");
        assert!(g.laws_in_play().is_empty());
    }

    #[test]
    fn readying_planets_clears_every_exhausted_card() {
        let mut g = game(&["a"]);
        g.exhaust_planet(PlanetId::new("jord"));
        g.exhaust_planet(PlanetId::new("mr"));
        g.ready_planet(&PlanetId::new("jord"));
        assert_eq!(g.exhausted_planets.len(), 1);
        g.ready_all_planets();
        assert!(g.exhausted_planets.is_empty());
    }

    // -- equality ------------------------------------------------------------------

    #[test]
    fn equality_ignores_the_fields_the_oracle_marks_uncompared() {
        // Surprising, and deliberate: the oracle marks `board` compare=False and the
        // trace-equality tooling depends on it.
        let a = game(&["a", "b"]);
        let mut b = a.clone();
        b.system_mut(&SystemId::new("18"))
            .add(&[unit("carrier", "a")]);

        assert_eq!(a, b, "board differences do not affect declared equality");
        assert!(!a.identical(&b), "but a full comparison sees them");
    }

    #[test]
    fn equality_does_see_the_compared_fields() {
        let a = game(&["a", "b"]);
        let mut b = a.clone();
        b.round = 2;
        assert_ne!(a, b);

        let mut c = a.clone();
        c.player_mut(&pid("a")).unwrap().victory_points = 1;
        assert_ne!(a, c);
    }

    #[test]
    fn strategy_card_goods_are_compared_unlike_the_other_maps() {
        let a = game(&["a", "b"]);
        let mut b = a.clone();
        b.strategy_card_goods.insert(card("leadership"), 1);
        assert_ne!(a, b, "an unclaimed card's trade goods are real state");
    }

    #[test]
    fn a_clone_is_equal_and_identical_to_its_source() {
        let mut g = game(&["a", "b"]);
        g.system_mut(&SystemId::new("18"))
            .add(&[unit("carrier", "a")]);
        g.enact_law("x", "for");
        let copy = g.clone();
        assert_eq!(g, copy);
        assert!(g.identical(&copy));
    }

    // -- serialisation -------------------------------------------------------------

    #[test]
    fn a_game_state_round_trips_through_json() {
        let mut g = game(&["a", "b"]);
        g.system_mut(&SystemId::new("18"))
            .add(&[unit("carrier", "a")]);
        g.deal_strategy_card(&pid("a"), card("leadership"));
        g.record_promise(&pid("a"), &pid("b"), "p");

        let json = serde_json::to_string(&g).unwrap();
        let back: GameState = serde_json::from_str(&json).unwrap();
        assert!(g.identical(&back));
    }

    #[test]
    fn serialisation_is_stable_across_repeated_encodings() {
        let g = game(&["a", "b", "c"]);
        assert_eq!(
            serde_json::to_string(&g).unwrap(),
            serde_json::to_string(&g).unwrap()
        );
    }
}
