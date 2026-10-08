//! Movement legality for the tactical action.
//!
//! Ported from the oracle's `engine/movement.py`, whose rule list is quoted from the Living
//! Rules Reference rather than recalled:
//!
//! - **58.4a** a ship must end its movement in the active system
//! - **58.4b** it cannot move *through* a system containing another player's ships
//! - **58.4c** it cannot move at all if it started in another system containing one of its own
//!   faction's command tokens
//! - **58.4d** it may move through systems containing its own command tokens
//! - **58.4e** it may leave the active system and return, given move value
//! - **58.4f** it moves along adjacent systems, and the number of systems *entered* cannot
//!   exceed its move value
//! - **11.1** a ship cannot move through or into an asteroid field
//! - **86.1** a ship cannot move through or into a supernova
//! - **59.1** a ship can only move into a nebula if it is the active system, and (59.1a) cannot
//!   move through one
//! - **59.2** a ship that begins the Movement step in a nebula treats its move value as 1
//! - **41.1** a ship moving out of or through a gravity rift applies +1 to its move value, and
//!   (41.3) a rift may affect the same ship several times in one movement
//!
//! Gravity rifts make the budget path-dependent — a route through two rifts is worth two extra
//! movement — so reachability is a search rather than a distance comparison. The 41.2
//! destruction roll is a *consequence* of moving, not a legality question, and belongs to the
//! tactical action rather than here.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use ti4_content::ContentStore;
use ti4_content::galaxy::{Galaxy, System, all_systems};
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlayerId, SystemId};
use ti4_model::state::GameState;

/// The occupancy facts movement legality depends on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Board {
    /// Systems containing ships belonging to somebody other than the moving player.
    pub enemy_ships: BTreeSet<String>,
    /// Systems containing the moving player's own command tokens.
    pub own_command_tokens: BTreeSet<String>,
    /// The moving player, when the board was read for one. Faction modules' per-player movement
    /// effects (extra adjacency, supernova passage, ships that pass blockades) apply only when
    /// this is set, which `for_player` does.
    pub mover: Option<PlayerId>,
}

impl Board {
    /// Read the occupancy facts for one player out of a game state.
    ///
    /// Only *ships* block passage: ground forces sit on planets and 58.4b speaks of ships.
    /// Counting a lone infantry as a blockade would close routes the rules leave open.
    #[must_use]
    pub fn for_player(
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
        player: &PlayerId,
    ) -> Self {
        let catalogue = ti4_content::units::catalogue(content, sources);
        let mut enemy_ships = BTreeSet::new();
        let mut own_command_tokens = BTreeSet::new();
        for (system_id, system) in &state.board {
            if system.units.iter().any(|unit| {
                &unit.owner != player
                    && catalogue
                        .get(unit.type_id.as_str())
                        .is_some_and(ti4_content::units::UnitType::is_ship)
            }) {
                enemy_ships.insert(system_id.to_string());
            }
            if system.command_tokens.contains(player) {
                own_command_tokens.insert(system_id.to_string());
            }
        }
        Self {
            enemy_ships,
            own_command_tokens,
            mover: Some(player.clone()),
        }
    }

    #[must_use]
    pub fn has_enemy_ships(&self, system_id: &str) -> bool {
        self.enemy_ships.contains(system_id)
    }
}

/// Reachability for one player's ships towards one active system.
///
/// The flag count is the oracle's, not an accident of design: each is a distinct printed
/// ability with its own interaction, and collapsing them into an enum or bitset would lose the
/// documented reason each exists separately (notably that Antimass Deflectors must *not* imply
/// Nav Suite). Kept one-to-one with `MovementRules` in `engine/movement.py`.
///
/// The ability modifiers are ported in full even though nothing sets most of them yet: they are
/// what the rules *are*, and a caller that gains Nav Suite later should find the rule already
/// written rather than have to reopen this search.
#[derive(Debug, Clone)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "one field per printed ability, as the oracle has"
)]
pub struct MovementRules<'a> {
    /// Borrowed, unless a faction module put wormholes on the map (Creuss flagship), in which case
    /// an owned copy carrying them.
    galaxy: Cow<'a, Galaxy>,
    /// Resolved once. Looking a system up per search step rebuilt an index over the whole
    /// system corpus, which is how the objective predicates first went quadratic.
    systems: BTreeMap<&'a str, System<'a>>,
    active_system: String,
    board: Board,

    /// A law in play changes what movement may do (Shared Research).
    pub nebulae_open: bool,
    /// Antimass Deflectors permits asteroid fields while leaving every other anomaly rule
    /// intact. This cannot use `anomalies_ignored`, which would also turn off gravity-rift
    /// bonuses and nebula restrictions.
    pub asteroid_fields_open: bool,
    /// Magmus Reactor permits supernovas without switching off other anomalies.
    pub supernovae_open: bool,
    /// Gashlai Physiology: ships may move *through* supernovas (as intermediate systems) but
    /// still may not end a move in one. Set from `MovementHooks::may_pass_through_supernova`.
    pub supernovae_pass_through: bool,
    /// The Dominus Orb, for the activation it was purged into: ships may leave systems holding
    /// this player's command tokens (58.4c suspended).
    pub command_tokens_ignored: bool,
    /// Nav Suite: "ignore the effect of anomalies" for this tactical action. Every anomaly rule
    /// below is an effect of an anomaly, so this turns off all of them together — the supernova
    /// and asteroid bars, both nebula restrictions, the nebula move cap, and the gravity rift
    /// bonus. A rift's +1 is as much an effect as a supernova's bar, so ignoring anomalies
    /// gives it up along with the rest.
    pub anomalies_ignored: bool,
    /// In The Silence Of Space: ships starting in this system may move through systems
    /// containing other players' ships. One named system, not a blanket permission — the card
    /// says "your ships in the chosen system".
    pub ignore_enemy_ships_from: Option<String>,
    /// Light/Wave Deflector is the blanket version.
    pub ignore_enemy_ships: bool,
    /// Spatial Conduit Cylinders: systems treated as adjacent to the active system for this
    /// activation only. Adjacency is otherwise a property of the map, and this is the one thing
    /// that reaches past it — so it is a parameter rather than something the galaxy is asked to
    /// pretend about.
    pub also_adjacent: BTreeSet<String>,
    /// Non-map edges that apply throughout a route. The Fracture's seven-system chain and every
    /// ingress-to-egress connection live here because either may be an intermediate step.
    extra_adjacency: BTreeMap<String, BTreeSet<String>>,
    /// Dynamic Creuss tokens create an extra wormhole edge.
    pub token_wormhole_systems: BTreeSet<String>,
    /// Aerie Hololattice systems may be entered but not moved through by opponents.
    pub barred_transit: BTreeSet<String>,
    /// Systems made into gravity rifts by Dimensional Tears.
    pub gravity_rift_systems: BTreeSet<String>,
    /// The subset of [`Self::gravity_rift_systems`] this mover's ships do not roll for
    /// (`MovementHooks::rift_roll_exempt`). They are still gravity rifts for every other rule.
    pub rift_roll_exempt: BTreeSet<String>,
    /// The Circlet of the Void: this player's units never roll for a rift.
    ///
    /// Kept beside the other modifiers rather than checked at the card, so the immunity is
    /// honoured wherever the roll happens — the mistake Nav Suite nearly made.
    pub rifts_ignored: bool,
    /// Extra steps granted once per route, at the first gravity rift it leaves or passes, on top of
    /// the printed +1 (Crucible: "apply an additional +1 to the move values of your ships that
    /// would move out of or through a gravity rift"; the note: "only ever add +1 movement total,
    /// regardless of how many gravity rifts you pass through").
    pub rift_extra_steps: i32,
    /// Ship types of the moving player that may pass other players' ships, from
    /// `MovementHooks::may_move_through_ships`. Consulted only by [`Self::path_from_ship`].
    passing_ship_types: BTreeSet<String>,
    /// Ship types of the moving player that may pass through systems holding the player's own units
    /// despite other players' ships, earning a step per such system (`MovementHooks::own_unit_passage`).
    own_passage_types: BTreeSet<String>,
    /// The systems that hold at least one of the moving player's units, read when the rules are built.
    own_unit_systems: BTreeSet<String>,
    /// Per ship type of the moving player: systems that type is treated as adjacent to from
    /// wherever it stands (`MovementHooks::unit_adjacent_systems`). Consulted only by
    /// [`Self::path_from_ship`].
    ship_adjacent: BTreeMap<String, BTreeSet<String>>,
    /// Ship types of the moving player that may leave systems holding the player's command
    /// token (`MovementHooks::ignores_command_tokens`). Consulted only by [`Self::path_from_ship`].
    token_free_types: BTreeSet<String>,
    /// Empyrean Voidborn: nebulae do not affect this mover's ships' movement
    /// (`MovementHooks::ignores_nebulae`).
    nebulae_ignored: bool,
    /// Borders this mover does not treat as adjacent, normalised `(low, high)`
    /// (`MovementHooks::blocked_borders`; Void Tether).
    blocked_edges: BTreeSet<(String, String)>,
    /// Firmament agent Myru Vos: ships that are not transporting units may also move through other
    /// players' ships (`MovementHooks::unladen_pass`). Used by [`Self::path_from_ship`] only when no
    /// route exists without it, so a ship that can sail clear keeps its freedom to carry cargo.
    unladen_pass: bool,
}

impl<'a> MovementRules<'a> {
    /// Rules for moving towards `active_system`.
    #[must_use]
    pub fn new(
        galaxy: &'a Galaxy,
        content: &'a ContentStore,
        sources: SourceSet,
        active_system: &str,
        board: Board,
    ) -> Self {
        Self::with_laws(galaxy, content, sources, active_system, board, None)
    }

    /// Rules that also honour the laws in play.
    #[must_use]
    pub fn with_laws(
        galaxy: &'a Galaxy,
        content: &'a ContentStore,
        sources: SourceSet,
        active_system: &str,
        board: Board,
        state: Option<&GameState>,
    ) -> Self {
        let mut rules = Self {
            galaxy: Cow::Borrowed(galaxy),
            systems: all_systems(content, sources),
            active_system: active_system.to_owned(),
            board,
            // Shared Research is the law that opens them; without it this stays false.
            nebulae_open: state.is_some_and(crate::laws::nebulae_passable),
            asteroid_fields_open: false,
            supernovae_open: false,
            supernovae_pass_through: false,
            command_tokens_ignored: false,
            anomalies_ignored: false,
            ignore_enemy_ships_from: None,
            ignore_enemy_ships: false,
            also_adjacent: BTreeSet::new(),
            extra_adjacency: BTreeMap::new(),
            token_wormhole_systems: BTreeSet::new(),
            barred_transit: BTreeSet::new(),
            gravity_rift_systems: BTreeSet::new(),
            rift_roll_exempt: BTreeSet::new(),
            rifts_ignored: false,
            rift_extra_steps: 0,
            passing_ship_types: BTreeSet::new(),
            own_passage_types: BTreeSet::new(),
            own_unit_systems: BTreeSet::new(),
            ship_adjacent: BTreeMap::new(),
            token_free_types: BTreeSet::new(),
            nebulae_ignored: false,
            blocked_edges: BTreeSet::new(),
            unladen_pass: false,
        };
        if let Some(state) = state {
            rules.apply_faction_modules(state, content, sources);
        }
        if let Some(state) = state
            && state.fracture_in_play
        {
            let fracture = crate::fracture::systems(content, sources);
            for pair in fracture.windows(2) {
                let left = pair[0].to_string();
                let right = pair[1].to_string();
                rules
                    .extra_adjacency
                    .entry(left.clone())
                    .or_default()
                    .insert(right.clone());
                rules.extra_adjacency.entry(right).or_default().insert(left);
            }
            for ingress in &state.ingress_tokens {
                for egress in crate::fracture::egress_systems(content, sources) {
                    rules
                        .extra_adjacency
                        .entry(ingress.to_string())
                        .or_default()
                        .insert(egress.to_string());
                    rules
                        .extra_adjacency
                        .entry(egress.to_string())
                        .or_default()
                        .insert(ingress.to_string());
                }
            }
        }
        rules
    }

    /// Faction-module movement effects: wormholes carried by pieces, this player's extra
    /// adjacency, supernova passage, ships that pass blockades. Every hook is empty today, so
    /// this changes nothing for the in-scope factions.
    fn apply_faction_modules(
        &mut self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
    ) {
        use crate::factions::hooks_movement as hooks;
        if hooks::any(|table| table.extra_wormholes.is_some()) {
            let mut owned = (*self.galaxy).clone();
            if hooks::apply_extra_wormholes(state, &mut owned) {
                self.galaxy = Cow::Owned(owned);
            }
        }
        // Wormholes a move may not use: a severed system's (Sever) and, for a restricted mover, every
        // kind it may not use (Sundered). On a copy of the map, so adjacency queries outside movement
        // still see them.
        if hooks::any(|table| {
            table.severed_systems.is_some() || table.usable_wormhole_kinds.is_some()
        }) {
            let limits = hooks::wormhole_limits(state, self.board.mover.as_ref());
            if !limits.is_empty() {
                let mut owned = (*self.galaxy).clone();
                limits.apply(&mut owned);
                self.galaxy = Cow::Owned(owned);
            }
        }
        let Some(mover) = self.board.mover.clone() else {
            return;
        };
        for (a, b) in hooks::linked_systems(state, content, sources, &self.galaxy, &mover) {
            self.extra_adjacency
                .entry(a.clone())
                .or_default()
                .insert(b.clone());
            self.extra_adjacency.entry(b).or_default().insert(a);
        }
        if hooks::may_enter_supernova(state, content, sources, &mover) {
            self.supernovae_open = true;
        }
        if hooks::may_pass_through_supernova(state, content, sources, &mover) {
            self.supernovae_pass_through = true;
        }
        if hooks::any(|table| table.ignores_nebulae.is_some())
            && hooks::ignores_nebulae(state, &mover)
        {
            self.nebulae_ignored = true;
        }
        if hooks::any(|table| table.blocked_borders.is_some()) {
            self.blocked_edges = hooks::blocked_borders(state, &mover);
        }
        if hooks::any(|table| table.unladen_pass.is_some()) {
            self.unladen_pass = hooks::unladen_pass(state, &mover);
        }
        if hooks::any(|table| table.passable_owners.is_some()) {
            // Aetherpassage: a system whose only foreign ships belong to players who allow the
            // passage no longer blocks (58.4b); one that also holds anybody else's still does.
            let allowed = hooks::passable_owners(state, &mover);
            if !allowed.is_empty() {
                let types = ti4_content::units::catalogue(content, sources);
                let cleared: Vec<String> = self
                    .board
                    .enemy_ships
                    .iter()
                    .filter(|system_id| {
                        state
                            .board
                            .get(&SystemId::new(system_id.as_str()))
                            .is_some_and(|system| {
                                // At least one foreign ship, all of them allowing: an entry with
                                // none was put there by another hook and is not ours to clear.
                                let mut foreign = system
                                    .units
                                    .iter()
                                    .filter(|unit| {
                                        unit.owner != mover
                                            && types
                                                .get(unit.type_id.as_str())
                                                .is_some_and(ti4_content::units::UnitType::is_ship)
                                    })
                                    .peekable();
                                foreign.peek().is_some()
                                    && foreign.all(|unit| allowed.contains(&unit.owner))
                            })
                    })
                    .cloned()
                    .collect();
                for system_id in cleared {
                    self.board.enemy_ships.remove(&system_id);
                }
            }
        }
        if hooks::any(|table| table.blocks_passage.is_some()) {
            // Aerie Hololattice: the system may still be entered (it is the active system), but
            // not crossed; `barred_transit` is exactly that rule.
            for system in state.board.keys() {
                if hooks::blocks_passage(state, content, sources, &mover, system) {
                    self.barred_transit.insert(system.to_string());
                }
            }
        }
        if hooks::any(|table| {
            table.may_move_through_ships.is_some()
                || table.own_unit_passage.is_some()
                || table.unit_adjacent_systems.is_some()
                || table.ignores_command_tokens.is_some()
        }) {
            let active = SystemId::new(self.active_system.as_str());
            let types: BTreeSet<&str> = state
                .board
                .values()
                .flat_map(|system| system.units.iter())
                .filter(|unit| unit.owner == mover)
                .map(|unit| unit.type_id.as_str())
                .collect();
            for ship_type in types {
                let site = hooks::PassSite {
                    player: &mover,
                    active: &active,
                    ship_type,
                };
                if hooks::may_move_through_ships(state, content, sources, &site) {
                    self.passing_ship_types.insert(ship_type.to_owned());
                }
                if hooks::own_unit_passage(state, content, sources, &site) {
                    self.own_passage_types.insert(ship_type.to_owned());
                }
                let near = hooks::unit_adjacent_systems(state, content, sources, &mover, ship_type);
                if !near.is_empty() {
                    self.ship_adjacent.insert(ship_type.to_owned(), near);
                }
                if hooks::ignores_command_tokens(state, content, sources, &mover, ship_type) {
                    self.token_free_types.insert(ship_type.to_owned());
                }
            }
            if !self.own_passage_types.is_empty() {
                self.own_unit_systems = state
                    .board
                    .iter()
                    .filter(|(_, board)| {
                        board
                            .units
                            .iter()
                            .chain(board.planet_units.values().flatten())
                            .any(|unit| unit.owner == mover)
                    })
                    .map(|(system, _)| system.to_string())
                    .collect();
            }
        }
    }

    fn system(&self, system_id: &str) -> Option<&System<'a>> {
        self.systems.get(system_id)
    }

    /// Whether a system is a gravity rift, printed or created.
    ///
    /// Public because 41.2's destruction roll needs the same answer, and it lives in
    /// [`crate::transit`] — a consequence of moving rather than a legality question.
    #[must_use]
    pub fn is_rift(&self, system_id: &str) -> bool {
        self.is_gravity_rift(system_id)
    }

    fn is_gravity_rift(&self, system_id: &str) -> bool {
        self.system(system_id).is_some_and(System::is_gravity_rift)
            || self.gravity_rift_systems.contains(system_id)
    }

    const fn nebulae_open(&self) -> bool {
        self.nebulae_open || self.anomalies_ignored || self.nebulae_ignored
    }

    /// Whether a ship may end or pass a step in this system at all.
    #[must_use]
    pub fn can_enter(&self, system_id: &str) -> bool {
        self.enterable(system_id, false)
    }

    /// [`Self::can_enter`] for a step that is only passed through when `passing`: Gashlai
    /// Physiology opens supernovas to those and to nothing else.
    fn enterable(&self, system_id: &str, passing: bool) -> bool {
        if self.anomalies_ignored {
            return true;
        }
        let Some(system) = self.system(system_id) else {
            // A system the corpus does not describe is not a licence to move anywhere.
            return false;
        };
        if (system.is_supernova()
            && !self.supernovae_open
            && !(passing && self.supernovae_pass_through))
            || (system.is_asteroid_field() && !self.asteroid_fields_open)
        {
            return false; // 86.1, 11.1
        }
        if system.is_nebula() && system_id != self.active_system {
            return self.nebulae_open(); // 59.1
        }
        true
    }

    /// Whether a ship may continue *beyond* this system.
    #[must_use]
    pub fn can_pass_through(&self, system_id: &str, origin: Option<&str>) -> bool {
        self.can_pass_through_ship(system_id, origin, None)
    }

    /// [`Self::can_pass_through`] for one ship type, which may be allowed to pass other players'
    /// ships where the rules otherwise bar it (58.4b): see
    /// `MovementHooks::may_move_through_ships`. Every other bar still applies.
    #[must_use]
    pub fn can_pass_through_ship(
        &self,
        system_id: &str,
        origin: Option<&str>,
        ship_type: Option<&str>,
    ) -> bool {
        self.passes(system_id, origin, ship_type, false)
    }

    /// [`Self::can_pass_through_ship`], optionally lifting 58.4b altogether (`past_ships`): the
    /// second search of an unladen ship under Myru Vos. Every other bar still applies.
    fn passes(
        &self,
        system_id: &str,
        origin: Option<&str>,
        ship_type: Option<&str>,
        past_ships: bool,
    ) -> bool {
        if !self.enterable(system_id, true) {
            return false;
        }
        if self.system(system_id).is_some_and(System::is_nebula) && !self.nebulae_open() {
            return false; // 59.1a — never an intermediate, unless a law says otherwise
        }
        if self.barred_transit.contains(system_id) {
            return false;
        }
        if self.ignore_enemy_ships || past_ships {
            return true;
        }
        if ship_type.is_some_and(|kind| self.passing_ship_types.contains(kind)) {
            return true; // a faction's ship that passes blockades
        }
        // A ship that passes through systems holding its owner's own units, other players' ships
        // notwithstanding (Deepwrought Luminous); every other system still blocks.
        if ship_type.is_some_and(|kind| self.own_passage_types.contains(kind))
            && self.own_unit_systems.contains(system_id)
        {
            return true;
        }
        if origin.is_some() && origin.map(str::to_owned) == self.ignore_enemy_ships_from {
            return true;
        }
        !self.board.has_enemy_ships(system_id) // 58.4b
    }

    /// 58.4c: a command token pins ships, except in the active system (58.4e).
    #[must_use]
    pub fn may_depart(&self, origin: &str) -> bool {
        if origin == self.active_system || self.command_tokens_ignored {
            return true;
        }
        !self.board.own_command_tokens.contains(origin)
    }

    /// [`Self::may_depart`] for one ship type, which a module may free of 58.4c.
    fn may_depart_ship(&self, origin: &str, ship_type: Option<&str>) -> bool {
        self.may_depart(origin)
            || ship_type.is_some_and(|kind| self.token_free_types.contains(kind))
    }

    #[must_use]
    pub fn can_reach(&self, origin: &str, move_value: i32) -> bool {
        self.path_from(origin, move_value).is_some()
    }

    /// [`Self::can_reach`] for one ship type; see [`Self::path_from_ship`].
    #[must_use]
    pub fn can_reach_ship(&self, origin: &str, move_value: i32, ship_type: Option<&str>) -> bool {
        self.path_from_ship(origin, move_value, ship_type).is_some()
    }

    /// A legal route from `origin` to the active system, or `None`.
    ///
    /// Breadth-first, so the route returned enters the fewest systems. Search state carries the
    /// remaining budget because gravity rifts extend it en route.
    #[must_use]
    pub fn path_from(&self, origin: &str, move_value: i32) -> Option<Vec<String>> {
        self.path_from_ship(origin, move_value, None)
    }

    /// [`Self::path_from`] for one ship type: the route a ship of that type could take, which is
    /// longer than the type-blind one when the type may pass other players' ships (Mentak
    /// Corsair, Yssaril flagship). A caller that offers a move found with a type and then looks
    /// the route up without it would refuse its own offer, so both must pass the same type.
    #[must_use]
    pub fn path_from_ship(
        &self,
        origin: &str,
        move_value: i32,
        ship_type: Option<&str>,
    ) -> Option<Vec<String>> {
        // Myru Vos: an unladen ship may also go through other players' ships, but only when no
        // route avoids them, so a ship that can sail clear is never denied its cargo for it.
        self.search_path(origin, move_value, ship_type, false).or_else(|| {
            self.unladen_pass
                .then(|| self.search_path(origin, move_value, ship_type, true))
                .flatten()
        })
    }

    fn search_path(
        &self,
        origin: &str,
        move_value: i32,
        ship_type: Option<&str>,
        past_ships: bool,
    ) -> Option<Vec<String>> {
        if !self.may_depart_ship(origin, ship_type) {
            return None;
        }

        // 59.2: starting inside a nebula caps the move value at 1 — unless anomalies are being
        // ignored, in which case the nebula is not there to cap it.
        let in_nebula = self.system(origin).is_some_and(System::is_nebula)
            && !self.anomalies_ignored
            && !self.nebulae_ignored;
        let budget = if in_nebula { 1 } else { move_value };
        if budget <= 0 {
            return None;
        }

        // The last field is whether this route has already taken the Crucible bonus: it is worth
        // +1 in total however many rifts the route passes, so it is carried per route.
        let mut queue: VecDeque<(String, i32, i32, Vec<String>, bool)> =
            VecDeque::from([(origin.to_owned(), 0, budget, vec![origin.to_owned()], false)]);
        // Revisiting is only worthwhile with a larger budget left over (and the same bonus state).
        let mut best: BTreeMap<(String, bool), i32> = BTreeMap::new();

        while let Some((current, entered, mut allowance, route, mut bonus_used)) = queue.pop_front()
        {
            // 41.1: leaving a rift is worth an extra step, and 41.3 allows that to happen more
            // than once in one movement. The bonus must land *before* the budget is judged,
            // because it is what pays for the departure — a ship arriving at a rift with
            // nothing left can still leave it.
            if self.is_gravity_rift(&current) && !self.anomalies_ignored {
                allowance += 1;
                if !bonus_used && self.rift_extra_steps > 0 {
                    allowance += self.rift_extra_steps;
                    bonus_used = true;
                }
            }

            // Moving through a system that holds the mover's own units earns a step for ships that
            // may (Deepwrought Luminous). Only an intermediate system is "moved through": the
            // origin is entered at 0 and the active system ends the route before it is queued.
            if entered > 0
                && ship_type.is_some_and(|kind| self.own_passage_types.contains(kind))
                && self.own_unit_systems.contains(&current)
            {
                allowance += 1;
            }

            let remaining = allowance - entered;
            let key = (current.clone(), bonus_used);
            if best.get(&key).is_some_and(|seen| *seen >= remaining) {
                continue;
            }
            best.insert(key, remaining);
            if remaining <= 0 {
                continue;
            }

            let mut neighbours: BTreeSet<String> = self
                .galaxy
                .adjacent(&current)
                .into_iter()
                .map(ToOwned::to_owned)
                .collect();
            if self.token_wormhole_systems.contains(&current) {
                neighbours.extend(
                    self.token_wormhole_systems
                        .iter()
                        .filter(|id| *id != &current)
                        .cloned(),
                );
            }
            if let Some(extra) = self.extra_adjacency.get(&current) {
                neighbours.extend(extra.iter().cloned());
            }
            // A ship treated as adjacent to some systems (Nomad Memoria) is, wherever it stands.
            if let Some(near) = ship_type.and_then(|kind| self.ship_adjacent.get(kind)) {
                neighbours.extend(near.iter().filter(|id| **id != current).cloned());
            }
            // The conduit joins the active system to the listed ones in both directions: a
            // route out of one of them is what the card buys.
            if self.also_adjacent.contains(&current) {
                neighbours.insert(self.active_system.clone());
            } else if current == self.active_system {
                neighbours.extend(self.also_adjacent.iter().cloned());
            }
            // Void Tether: a tethered border is not an adjacency for this mover.
            if !self.blocked_edges.is_empty() {
                neighbours.retain(|other| {
                    let edge = if current.as_str() < other.as_str() {
                        (current.clone(), other.clone())
                    } else {
                        (other.clone(), current.clone())
                    };
                    !self.blocked_edges.contains(&edge)
                });
            }

            for neighbour in neighbours {
                let ends_here = neighbour == self.active_system;
                // The step that ends the move is judged by `can_enter`; every other step is
                // judged by `can_pass_through_ship`, whose first test is the passing form of it.
                if ends_here && !self.can_enter(&neighbour) {
                    continue;
                }
                let mut arrived = route.clone();
                arrived.push(neighbour.clone());
                if ends_here {
                    return Some(arrived); // 58.4a — movement ends here
                }
                if !self.passes(&neighbour, Some(origin), ship_type, past_ships) {
                    continue;
                }
                queue.push_back((neighbour, entered + 1, allowance, arrived, bonus_used));
            }
        }

        None
    }

    /// Which systems ships of this move value could reach the active system from.
    #[must_use]
    pub fn origins_within_range(
        &self,
        move_value: i32,
        candidates: Option<&BTreeSet<String>>,
    ) -> BTreeSet<String> {
        let pool: Vec<String> = candidates.map_or_else(
            || {
                self.galaxy
                    .system_ids()
                    .into_iter()
                    .map(ToOwned::to_owned)
                    .collect()
            },
            |given| given.iter().cloned().collect(),
        );
        pool.into_iter()
            .filter(|origin| self.can_reach(origin, move_value))
            .collect()
    }
}

// -- player-aware adjacency ----------------------------------------------------------------------

/// Adjacency as one player sees it: the map's, plus wormholes that modules' pieces carry (visible to
/// everyone) and the links only this player has (Creuss Quantum Entanglement, Winnu Lazax Gate
/// Folding).
///
/// This is the one place to ask a player-aware adjacency question outside movement itself
/// (transaction neighbours, space cannon range, retreat, agents...). `MovementRules` builds the same
/// thing for movement. With every faction module empty it answers exactly what `Galaxy::adjacent`
/// does. Build one per decision rather than per query: the hooks run when it is built.
#[derive(Debug, Clone)]
pub struct PlayerAdjacency<'a> {
    galaxy: Cow<'a, Galaxy>,
    links: BTreeMap<String, BTreeSet<String>>,
    /// Borders this player does not treat as adjacent (`MovementHooks::blocked_borders`).
    blocked: BTreeSet<(String, String)>,
}

impl<'a> PlayerAdjacency<'a> {
    /// Adjacency for `player` in this state.
    #[must_use]
    pub fn new(
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
        galaxy: &'a Galaxy,
        player: &PlayerId,
    ) -> Self {
        use crate::factions::hooks_movement as hooks;
        let mut galaxy = Cow::Borrowed(galaxy);
        if hooks::any(|table| table.extra_wormholes.is_some()) {
            let mut owned = galaxy.as_ref().clone();
            if hooks::apply_extra_wormholes(state, &mut owned) {
                galaxy = Cow::Owned(owned);
            }
        }
        let mut links: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (a, b) in hooks::linked_systems(state, content, sources, &galaxy, player) {
            links.entry(a.clone()).or_default().insert(b.clone());
            links.entry(b).or_default().insert(a);
        }
        let blocked = if hooks::any(|table| table.blocked_borders.is_some()) {
            hooks::blocked_borders(state, player)
        } else {
            BTreeSet::new()
        };
        Self {
            galaxy,
            links,
            blocked,
        }
    }

    /// Systems adjacent to `system` for this player.
    #[must_use]
    pub fn neighbours(&self, system: &str) -> BTreeSet<String> {
        let mut found: BTreeSet<String> = self
            .galaxy
            .adjacent(system)
            .into_iter()
            .map(ToOwned::to_owned)
            .collect();
        if let Some(extra) = self.links.get(system) {
            found.extend(extra.iter().cloned());
        }
        found.remove(system);
        if !self.blocked.is_empty() {
            found.retain(|other| {
                let edge = if system < other.as_str() {
                    (system.to_owned(), other.clone())
                } else {
                    (other.clone(), system.to_owned())
                };
                !self.blocked.contains(&edge)
            });
        }
        found
    }

    /// Whether two systems are adjacent for this player.
    #[must_use]
    pub fn are_adjacent(&self, a: &str, b: &str) -> bool {
        self.neighbours(a).contains(b)
    }
}

// -- map edits -----------------------------------------------------------------------------------

/// The Nova Seed tile, the Muaat supernova.
pub const NOVA_SEED: &str = "81";

/// A change to which tile sits where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapEdit {
    /// Creuss hero: "Swap the positions of any 2 non-Fracture systems that contain wormholes or
    /// your units." The two systems exchange hexes; each keeps its contents (the game state is
    /// keyed by system, not by position).
    Swap { a: String, b: String },
    /// Muaat hero: "replace that system tile with the Muaat supernova tile". The tile on the hex
    /// becomes `new`.
    Replace { old: String, new: String },
}

impl MapEdit {
    fn encode(&self) -> String {
        match self {
            Self::Swap { a, b } => format!("swap|{a}|{b}"),
            Self::Replace { old, new } => format!("replace|{old}|{new}"),
        }
    }

    fn decode(text: &str) -> Option<Self> {
        let mut parts = text.split('|');
        match (parts.next()?, parts.next()?, parts.next()?, parts.next()) {
            ("swap", a, b, None) => Some(Self::Swap {
                a: a.to_owned(),
                b: b.to_owned(),
            }),
            ("replace", old, new, None) => Some(Self::Replace {
                old: old.to_owned(),
                new: new.to_owned(),
            }),
            _ => None,
        }
    }
}

const MAP_EDIT_PREFIX: &str = "map_edit|";

/// The map edits this state records, oldest first: what a viewer applies to the setup board to
/// show the tiles where they stand in this frame.
#[must_use]
pub fn recorded_map_edits(state: &GameState) -> Vec<MapEdit> {
    recorded_edits(state)
}

/// The recorded edits, oldest first.
fn recorded_edits(state: &GameState) -> Vec<MapEdit> {
    state
        .faction_marks
        .range(MAP_EDIT_PREFIX.to_owned()..)
        .take_while(|(key, _)| key.starts_with(MAP_EDIT_PREFIX))
        .filter_map(|(_, value)| MapEdit::decode(value))
        .collect()
}

/// Why a map edit was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MapEditError {
    #[error(transparent)]
    Galaxy(#[from] ti4_content::galaxy::GalaxyError),
}

fn apply_to_galaxy_only(
    galaxy: &mut Galaxy,
    content: &ContentStore,
    sources: SourceSet,
    edit: &MapEdit,
) -> Result<(), ti4_content::galaxy::GalaxyError> {
    match edit {
        MapEdit::Swap { a, b } => galaxy.swap_systems(a, b),
        MapEdit::Replace { old, new } => galaxy.replace_system(content, old, new, sources),
    }
}

/// Bring `galaxy` up to the edits recorded in `state`. Idempotent: the galaxy counts the edits it
/// carries. A game that re-derives its map from the state (restore, branch) calls this after
/// rebuilding it; `laws::apply_to_galaxy` should call it every step (see the BF-00e evidence).
///
/// # Errors
/// The first edit that no longer fits the map; edits before it stay applied.
pub fn replay_map_edits(
    state: &GameState,
    galaxy: &mut Galaxy,
    content: &ContentStore,
    sources: SourceSet,
) -> Result<(), MapEditError> {
    for edit in recorded_edits(state).iter().skip(galaxy.edits_applied()) {
        apply_to_galaxy_only(galaxy, content, sources, edit)?;
    }
    Ok(())
}

/// Make a map edit: change the galaxy, record it in the state so a re-derived map follows, and
/// carry the system's contents with a replaced tile.
///
/// Atomic: on error neither the galaxy nor the state has changed. For [`MapEdit::Replace`] the
/// contents of the old tile move to the new one the way the Muaat hero's notes describe: units in
/// space and command tokens move; the frontier token stays; Creuss wormhole tokens on the old tile
/// are returned (removed from `wormhole_tokens`) and the ion storm token is purged. Planet control
/// and units on the old tile's planets are discarded with it: the **caller** destroys other
/// players' units and handles the purged planet cards *before* calling.
///
/// # Errors
/// [`MapEditError::Galaxy`] for a system not on the grid, a replacement already on the map or
/// not in the corpus, or a swap of a system with itself.
pub fn apply_map_edit(
    state: &mut GameState,
    galaxy: &mut Galaxy,
    content: &ContentStore,
    sources: SourceSet,
    edit: &MapEdit,
) -> Result<(), MapEditError> {
    // Catch the galaxy up first, so the index recorded below is the galaxy's own.
    replay_map_edits(state, galaxy, content, sources)?;
    let mut trial = galaxy.clone();
    apply_to_galaxy_only(&mut trial, content, sources, edit)?;
    *galaxy = trial;

    if let MapEdit::Replace { old, new } = edit {
        let (old, new) = (SystemId::new(old.as_str()), SystemId::new(new.as_str()));
        if let Some(gone) = state.board.remove(&old) {
            let moved = state.system_mut(&new);
            moved.units = gone.units;
            moved.command_tokens = gone.command_tokens;
        }
        if state.frontier_tokens.remove(&old) {
            state.frontier_tokens.insert(new);
        }
        state.wormhole_tokens.retain(|_, system| system != &old);
        if state
            .ion_storm
            .as_ref()
            .is_some_and(|(system, _)| system == &old)
        {
            state.ion_storm = None;
        }
    }
    let index = state
        .faction_marks
        .keys()
        .filter(|key| key.starts_with(MAP_EDIT_PREFIX))
        .count();
    state
        .faction_marks
        .insert(format!("{MAP_EDIT_PREFIX}{index:04}"), edit.encode());
    Ok(())
}

#[cfg(test)]
mod tests {
    use ti4_content::galaxy::Galaxy;
    use ti4_model::content_types::{FULL, POK};
    use ti4_model::id::SystemId;

    use super::*;

    /// A one-ring map: `centre` surrounded by six `outer` systems.
    ///
    /// Derived from the galaxy's real adjacency rather than asserted about hard-coded tiles,
    /// so the fixture cannot drift from what the map actually does.
    ///
    /// Note the ring itself is a route: two opposite outer systems are two apart *through the
    /// centre* but also three apart *around the ring*. Tests that block the centre therefore
    /// use a move value of 2, which the detour does not fit into. Using a larger value would
    /// have made them pass for the wrong reason, or fail for one.
    struct Hub {
        galaxy: Galaxy,
        centre: String,
        outer: Vec<String>,
    }

    impl Hub {
        /// The outer system directly across the centre from `from`.
        ///
        /// Two apart is not enough to identify it: ring positions two seats round are also two
        /// apart, and their route avoids the centre entirely. The opposite tile is the one
        /// whose *only* shared neighbour is the centre, which is what makes the centre a
        /// genuine bottleneck for a move value of 2.
        fn across(&self, from: &str) -> String {
            let neighbours_of = |id: &str| -> BTreeSet<String> {
                self.galaxy
                    .adjacent(id)
                    .into_iter()
                    .map(ToOwned::to_owned)
                    .collect()
            };
            let from_neighbours = neighbours_of(from);
            self.outer
                .iter()
                .find(|other| {
                    other.as_str() != from
                        && self.galaxy.distance(from, other) == Some(2)
                        && &from_neighbours & &neighbours_of(other)
                            == BTreeSet::from([self.centre.clone()])
                })
                .cloned()
                .expect("every outer system has one opposite")
        }
    }

    fn plain_systems(count: usize) -> Vec<String> {
        ti4_content::galaxy::all_systems(ContentStore::embedded(), POK)
            .iter()
            .filter(|(_, system)| !system.is_anomaly() && !system.is_hyperlane())
            .map(|(id, _)| (*id).to_owned())
            .take(count)
            .collect()
    }

    /// A system of one anomaly kind, chosen from the corpus by property rather than by id so
    /// the fixture says what it needs instead of naming a tile whose meaning must be looked up.
    fn a_system_where(kind: &str) -> String {
        ti4_content::galaxy::all_systems(ContentStore::embedded(), POK)
            .iter()
            .find(|(_, system)| match kind {
                "nebula" => system.is_nebula(),
                "supernova" => system.is_supernova(),
                "asteroid field" => system.is_asteroid_field(),
                "gravity rift" => system.is_gravity_rift(),
                other => unreachable!("unknown anomaly kind {other}"),
            })
            .map(|(id, _)| (*id).to_owned())
            .expect("the corpus has one")
    }

    /// Build a hub whose tiles are `ids`, the first at the centre and the rest around it.
    fn hub_from(ids: &[String]) -> Hub {
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let galaxy = Galaxy::build(ContentStore::embedded(), &refs, POK, 1).unwrap();
        Hub {
            galaxy,
            centre: ids[0].clone(),
            outer: ids[1..].to_vec(),
        }
    }

    /// A hub whose centre is `centre_id` and whose ring is ordinary systems.
    fn hub_with_centre(centre_id: &str) -> Hub {
        let mut ids = vec![centre_id.to_owned()];
        ids.extend(
            plain_systems(8)
                .into_iter()
                .filter(|id| id != centre_id)
                .take(6),
        );
        hub_from(&ids)
    }

    /// A hub whose centre is ordinary and whose first ring seat is `outer_id`.
    fn hub_with_outer(outer_id: &str) -> Hub {
        let plain: Vec<String> = plain_systems(9)
            .into_iter()
            .filter(|id| id != outer_id)
            .collect();
        let mut ids = vec![plain[0].clone(), outer_id.to_owned()];
        ids.extend(plain[1..6].iter().cloned());
        hub_from(&ids)
    }

    fn plain_hub() -> Hub {
        hub_with_centre(&plain_systems(1)[0])
    }

    fn movement_rules<'a>(hub: &'a Hub, active: &str, board: Board) -> MovementRules<'a> {
        MovementRules::new(&hub.galaxy, ContentStore::embedded(), POK, active, board)
    }

    #[test]
    fn a_ship_reaches_a_system_within_its_move_value() {
        // 58.4f: the number of systems entered cannot exceed the move value.
        let hub = plain_hub();
        let (near_a, near_b) = (hub.outer[0].clone(), hub.across(&hub.outer[0]));
        let rules = movement_rules(&hub, &near_b, Board::default());

        assert!(rules.can_reach(&near_a, 2), "two systems entered");
        assert!(!rules.can_reach(&near_a, 1), "one is not enough");
    }

    #[test]
    fn an_ingress_and_each_fracture_egress_are_adjacent_for_movement() {
        let hub = plain_hub();
        let ingress = SystemId::new(&hub.centre);
        let egress = crate::fracture::egress_systems(ContentStore::embedded(), FULL)
            .into_iter()
            .next()
            .expect("the Fracture has printed egresses");
        let mut state = crate::fixtures::game(&["a"]);
        state.fracture_in_play = true;
        state.ingress_tokens.insert(ingress.clone());

        let into_fracture = MovementRules::with_laws(
            &hub.galaxy,
            ContentStore::embedded(),
            FULL,
            egress.as_str(),
            Board::default(),
            Some(&state),
        );
        assert_eq!(
            into_fracture.path_from(ingress.as_str(), 1),
            Some(vec![ingress.to_string(), egress.to_string()])
        );

        let out_of_fracture = MovementRules::with_laws(
            &hub.galaxy,
            ContentStore::embedded(),
            FULL,
            ingress.as_str(),
            Board::default(),
            Some(&state),
        );
        assert_eq!(
            out_of_fracture.path_from(egress.as_str(), 1),
            Some(vec![egress.to_string(), ingress.to_string()])
        );
    }

    #[test]
    fn a_route_can_cross_the_fracture_interior_from_an_ingress() {
        let hub = plain_hub();
        let ingress = SystemId::new(&hub.centre);
        let mut state = crate::fixtures::game(&["a"]);
        state.fracture_in_play = true;
        state.ingress_tokens.insert(ingress.clone());

        let rules = MovementRules::with_laws(
            &hub.galaxy,
            ContentStore::embedded(),
            FULL,
            "fracture4",
            Board::default(),
            Some(&state),
        );
        assert_eq!(
            rules.path_from(ingress.as_str(), 3),
            Some(vec![
                ingress.to_string(),
                "fracture2".to_owned(),
                "fracture3".to_owned(),
                "fracture4".to_owned(),
            ])
        );
    }

    #[test]
    fn the_route_returned_enters_the_fewest_systems() {
        let hub = plain_hub();
        let (near_a, near_b) = (hub.outer[0].clone(), hub.across(&hub.outer[0]));
        let rules = movement_rules(&hub, &near_b, Board::default());

        let path = rules.path_from(&near_a, 5).unwrap();
        assert_eq!(
            path,
            vec![near_a, hub.centre.clone(), near_b],
            "straight through the centre, not wandering the ring"
        );
    }

    #[test]
    fn neutral_ships_on_the_board_block_passage_but_not_arrival() {
        // Neutral rule 9: they are "another player's ships" for every game effect, so 58.4b bars
        // moving through them exactly as it bars moving through a seated opponent. Built from a
        // real state through `Board::for_player`, not from a hand-written board.
        let hub = plain_hub();
        let (near_a, near_b) = (hub.outer[0].clone(), hub.across(&hub.outer[0]));
        let mover = ti4_model::id::PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        crate::fixtures::put(
            &mut state,
            &SystemId::new(&hub.centre),
            "neutral_cruiser",
            &crate::neutral_units::owner(),
            1,
        );
        let content = ContentStore::embedded();

        let board = Board::for_player(&state, content, FULL, &mover);
        assert!(
            board.has_enemy_ships(&hub.centre),
            "a neutral cruiser is another player's ship"
        );
        let through = MovementRules::new(&hub.galaxy, content, FULL, &near_b, board.clone());
        assert!(
            !through.can_reach(&near_a, 2),
            "the neutral cruiser in the centre blocks the straight route"
        );
        let into = MovementRules::new(&hub.galaxy, content, FULL, &hub.centre, board);
        assert!(
            into.can_reach(&near_a, 1),
            "moving into the neutral garrison is the tactical action"
        );
    }

    #[test]
    fn enemy_ships_block_passage_but_not_arrival() {
        // 58.4b bars moving *through* an occupied system; the active system is where the
        // movement ends, so occupancy there is the whole point of going.
        let hub = plain_hub();
        let (near_a, near_b) = (hub.outer[0].clone(), hub.across(&hub.outer[0]));
        let board = Board {
            enemy_ships: BTreeSet::from([hub.centre.clone()]),
            ..Board::default()
        };
        let blocked = movement_rules(&hub, &near_b, board.clone());
        assert!(!blocked.can_reach(&near_a, 2), "the centre is occupied");

        let arriving = movement_rules(&hub, &hub.centre, board);
        assert!(
            arriving.can_reach(&near_a, 1),
            "moving into the enemy is the tactical action"
        );
    }

    #[test]
    fn a_command_token_pins_ships_except_in_the_active_system() {
        // 58.4c, and 58.4e's exception.
        let hub = plain_hub();
        let (near_a, near_b) = (hub.outer[0].clone(), hub.across(&hub.outer[0]));
        let board = Board {
            own_command_tokens: BTreeSet::from([near_a.clone(), near_b.clone()]),
            ..Board::default()
        };
        let rules = movement_rules(&hub, &near_b, board);

        assert!(!rules.may_depart(&near_a), "pinned by its own token");
        assert!(!rules.can_reach(&near_a, 2));
        assert!(
            rules.may_depart(&near_b),
            "58.4e: it may leave the active system and return"
        );
    }

    #[test]
    fn own_command_tokens_do_not_block_passage() {
        // 58.4d: through its own tokens freely. Only *departure* is pinned.
        let hub = plain_hub();
        let (near_a, near_b) = (hub.outer[0].clone(), hub.across(&hub.outer[0]));
        let board = Board {
            own_command_tokens: BTreeSet::from([hub.centre.clone()]),
            ..Board::default()
        };
        let rules = movement_rules(&hub, &near_b, board);

        assert!(rules.can_reach(&near_a, 2));
    }

    #[test]
    fn supernovae_and_asteroid_fields_are_impassable() {
        // 86.1 and 11.1.
        for name in ["supernova", "asteroid field"] {
            let centre = a_system_where(name);
            let hub = hub_with_centre(&centre);
            let (near_a, near_b) = (hub.outer[0].clone(), hub.across(&hub.outer[0]));
            let rules = movement_rules(&hub, &near_b, Board::default());

            assert!(!rules.can_enter(&centre), "a {name} may not be entered");
            assert!(!rules.can_reach(&near_a, 2), "a {name} blocks the route");
        }
    }

    #[test]
    fn a_nebula_can_be_entered_only_as_the_active_system() {
        // 59.1, and 59.1a: never an intermediate.
        let nebula = a_system_where("nebula");
        let hub = hub_with_centre(&nebula);
        let (near_a, near_b) = (hub.outer[0].clone(), hub.across(&hub.outer[0]));

        let through = movement_rules(&hub, &near_b, Board::default());
        assert!(!through.can_enter(&nebula), "not the active system");
        assert!(!through.can_reach(&near_a, 2));

        let into = movement_rules(&hub, &nebula, Board::default());
        assert!(into.can_enter(&nebula), "it is the destination");
        assert!(into.can_reach(&near_a, 1));
    }

    #[test]
    fn shared_research_opens_the_nebulae_to_movement() {
        // The nebulae_open flag existed unused until a law could set it. 59.1 bars a nebula as
        // an intermediate; the law lifts that.
        let nebula = a_system_where("nebula");
        let hub = hub_with_centre(&nebula);
        let (near_a, near_b) = (hub.outer[0].clone(), hub.across(&hub.outer[0]));
        let mut state = crate::fixtures::game(&["a"]);

        let closed = MovementRules::with_laws(
            &hub.galaxy,
            ContentStore::embedded(),
            POK,
            &near_b,
            Board::default(),
            Some(&state),
        );
        assert!(!closed.can_reach(&near_a, 2), "the nebula blocks the route");

        state.enact_law("shared_research", "for");
        let opened = MovementRules::with_laws(
            &hub.galaxy,
            ContentStore::embedded(),
            POK,
            &near_b,
            Board::default(),
            Some(&state),
        );
        assert!(opened.can_reach(&near_a, 2), "the law opens it");
    }

    #[test]
    fn leaving_a_nebula_caps_the_move_value_at_one() {
        // 59.2. A move-2 ship starting in a nebula still only moves one system.
        let nebula = a_system_where("nebula");
        let hub = hub_with_outer(&nebula);
        let beyond = hub.across(&nebula);

        let near = movement_rules(&hub, &hub.centre, Board::default());
        assert!(near.can_reach(&nebula, 2), "one system away is fine");

        // Two systems away: reachable with move 2 from anywhere else, but the nebula caps it.
        let far = movement_rules(&hub, &beyond, Board::default());
        assert!(
            !far.can_reach(&nebula, 2),
            "move value 2 is capped to 1 by the nebula"
        );
    }

    #[test]
    fn a_gravity_rift_pays_for_an_extra_system() {
        // 41.1: a move-1 ship starting in a rift reaches two systems away.
        let rift = a_system_where("gravity rift");
        let hub = hub_with_outer(&rift);
        let beyond = hub.across(&rift);
        let rules = movement_rules(&hub, &beyond, Board::default());

        assert!(
            rules.can_reach(&rift, 1),
            "the rift's +1 pays for the second system"
        );
    }

    #[test]
    fn the_rift_bonus_lands_before_the_budget_is_judged() {
        // A ship arriving at a rift with nothing left can still leave it: the bonus is what
        // pays for the departure. Ordering this the other way silently strands ships.
        let rift = a_system_where("gravity rift");
        let hub = hub_with_outer(&rift);
        // centre -> rift -> across. Two systems entered, on a move value of 1: the rift's
        // bonus is granted on arrival and pays for the step out.
        let beyond = hub.across(&rift);
        let rules = movement_rules(&hub, &beyond, Board::default());

        assert!(
            rules.can_reach(&hub.centre, 1),
            "move 1 enters the rift, which then pays for the next step"
        );
    }

    #[test]
    fn ignoring_anomalies_gives_up_the_rift_bonus_too() {
        // Nav Suite turns off every anomaly effect, and a rift's +1 is as much an effect as a
        // supernova's bar. Keeping the bonus while dropping the bars would be a better card
        // than the one printed.
        let rift = a_system_where("gravity rift");
        let hub = hub_with_outer(&rift);
        let beyond = hub.across(&rift);
        let mut rules = movement_rules(&hub, &beyond, Board::default());
        rules.anomalies_ignored = true;

        assert!(
            !rules.can_reach(&rift, 1),
            "no bar, but no bonus either - move 1 reaches one system"
        );
        assert!(rules.can_reach(&rift, 2));
    }

    /// A three-ring galaxy of ordinary systems, with the two opposite corners of the outer ring and
    /// the systems strictly between them (the one straight line, so the only shortest route).
    fn long_line() -> (Galaxy, String, String, Vec<String>) {
        let ids: Vec<String> = plain_systems(200)
            .into_iter()
            .filter(|id| {
                ti4_content::galaxy::system(ContentStore::embedded(), id, POK)
                    .is_some_and(|system| system.wormholes().is_empty())
            })
            .take(37)
            .collect();
        assert_eq!(ids.len(), 37);
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let galaxy = Galaxy::build(ContentStore::embedded(), &refs, POK, 3).unwrap();
        let line_between = |a: &String, b: &String| -> Vec<String> {
            let mut between: Vec<String> = ids
                .iter()
                .filter(|x| {
                    galaxy.distance(a, x).unwrap() + galaxy.distance(x, b).unwrap() == 6
                        && *x != a
                        && *x != b
                })
                .cloned()
                .collect();
            between.sort_by_key(|x| galaxy.distance(a, x));
            between
        };
        // Opposite corners lie on a straight line, so the route between them is unique.
        let (a, b) = ids
            .iter()
            .flat_map(|a| ids.iter().map(move |b| (a, b)))
            .find(|(a, b)| galaxy.distance(a, b) == Some(6) && line_between(a, b).len() == 5)
            .expect("opposite corners");
        let between = line_between(a, b);
        (galaxy, a.clone(), b.clone(), between)
    }

    #[test]
    fn the_crucible_bonus_is_one_extra_step_however_many_rifts_the_route_passes() {
        // Route a, s1..s5, b: six systems entered. Rifts at s1, s2, s3: printed +1 each.
        let (galaxy, a, b, between) = long_line();
        assert_eq!(between.len(), 5);
        let make = |extra: i32, rifts: &[usize]| {
            let mut rules =
                MovementRules::new(&galaxy, ContentStore::embedded(), POK, &b, Board::default());
            rules.gravity_rift_systems = rifts.iter().map(|i| between[*i].clone()).collect();
            rules.rift_extra_steps = extra;
            rules
        };
        // Three rifts: move 1 + 3 = 4 systems, short of six. One Crucible step makes five: still
        // short. Per-rift it would make seven and arrive, so this fails if the bonus repeats.
        assert!(!make(0, &[0, 1, 2]).can_reach(&a, 1));
        assert!(
            !make(1, &[0, 1, 2]).can_reach(&a, 1),
            "the Crucible adds +1 in total, not per rift"
        );
        // Two rifts: move 1 + 2 = 3, plus the one step = 4; still six away. Needs the step: with
        // move 3 the printed bonuses reach five, short by one that only the Crucible step pays.
        assert!(!make(0, &[0, 1]).can_reach(&a, 3));
        assert!(
            make(1, &[0, 1]).can_reach(&a, 3),
            "two rifts plus the single extra step reach a six-system route on move 3"
        );
    }

    #[test]
    fn ignoring_anomalies_opens_supernovae() {
        let supernova = a_system_where("supernova");
        let hub = hub_with_centre(&supernova);
        let (near_a, near_b) = (hub.outer[0].clone(), hub.across(&hub.outer[0]));
        let mut rules = movement_rules(&hub, &near_b, Board::default());

        assert!(!rules.can_reach(&near_a, 2));
        rules.anomalies_ignored = true;
        assert!(rules.can_enter(&supernova));
        assert!(rules.can_reach(&near_a, 2));
    }

    #[test]
    fn antimass_deflectors_open_asteroids_without_opening_anything_else() {
        // The reason this is a separate flag: it must not become a general anomaly licence.
        let asteroid = a_system_where("asteroid field");
        let supernova = a_system_where("supernova");
        let hub = hub_with_centre(&asteroid);
        let (near_a, near_b) = (hub.outer[0].clone(), hub.across(&hub.outer[0]));
        let mut rules = movement_rules(&hub, &near_b, Board::default());
        rules.asteroid_fields_open = true;

        assert!(rules.can_enter(&asteroid));
        assert!(rules.can_reach(&near_a, 2));
        assert!(!rules.can_enter(&supernova), "supernovae are still barred");
    }

    #[test]
    fn a_blanket_permission_lets_ships_pass_enemies() {
        let hub = plain_hub();
        let (near_a, near_b) = (hub.outer[0].clone(), hub.across(&hub.outer[0]));
        let board = Board {
            enemy_ships: BTreeSet::from([hub.centre.clone()]),
            ..Board::default()
        };
        let mut rules = movement_rules(&hub, &near_b, board);
        assert!(!rules.can_reach(&near_a, 2));

        rules.ignore_enemy_ships = true;
        assert!(rules.can_reach(&near_a, 2));
    }

    #[test]
    fn in_the_silence_of_space_frees_only_the_named_origin() {
        // "your ships in the chosen system" - one origin, not a blanket permission.
        let hub = plain_hub();
        let (near_a, near_b) = (hub.outer[0].clone(), hub.across(&hub.outer[0]));
        let board = Board {
            enemy_ships: BTreeSet::from([hub.centre.clone()]),
            ..Board::default()
        };
        let mut rules = movement_rules(&hub, &near_b, board);
        rules.ignore_enemy_ships_from = Some(near_a.clone());

        assert!(rules.can_reach(&near_a, 2), "the named origin passes");

        // The same rule from the other side: name a different origin and this one is barred
        // again. The ring has exactly one system opposite each, so the permission is moved
        // rather than a second equivalent origin being found.
        let other = hub.outer.iter().find(|id| **id != near_a).unwrap().clone();
        rules.ignore_enemy_ships_from = Some(other);
        assert!(
            !rules.can_reach(&near_a, 2),
            "an origin that was not named is still blocked"
        );
    }

    #[test]
    fn barred_transit_can_be_entered_but_not_crossed() {
        let hub = plain_hub();
        let (near_a, near_b) = (hub.outer[0].clone(), hub.across(&hub.outer[0]));
        let mut blocked = movement_rules(&hub, &near_b, Board::default());
        blocked.barred_transit = BTreeSet::from([hub.centre.clone()]);
        assert!(!blocked.can_reach(&near_a, 2), "cannot be crossed");
        assert!(blocked.can_enter(&hub.centre), "but may be entered");

        let mut arriving = movement_rules(&hub, &hub.centre, Board::default());
        arriving.barred_transit = BTreeSet::from([hub.centre.clone()]);
        assert!(arriving.can_reach(&near_a, 1), "arriving there is legal");
    }

    #[test]
    fn origins_within_range_reports_every_legal_start() {
        let hub = plain_hub();
        let (near_a, near_b) = (hub.outer[0].clone(), hub.across(&hub.outer[0]));
        let rules = movement_rules(&hub, &hub.centre, Board::default());

        let origins = rules.origins_within_range(1, None);
        assert!(origins.contains(&near_a), "one away");
        assert!(
            !origins.contains(&hub.centre),
            "58.4e is leave *and return*, which enters two systems - move 1 cannot"
        );
        assert!(
            rules.origins_within_range(2, None).contains(&hub.centre),
            "with move 2 it can leave the active system and come back"
        );

        let far = movement_rules(&hub, &near_b, Board::default());
        assert!(
            !far.origins_within_range(1, None).contains(&near_a),
            "two away, move value 1"
        );
    }

    #[test]
    fn a_move_value_of_zero_reaches_nothing() {
        let hub = plain_hub();
        let near_a = hub.outer[0].clone();
        let rules = movement_rules(&hub, &hub.centre, Board::default());
        assert!(!rules.can_reach(&near_a, 0));
    }

    #[test]
    fn enemy_ships_are_read_from_ships_not_ground_forces() {
        // 58.4b speaks of ships. Counting a lone infantry as a blockade would close routes
        // the rules leave open.
        use ti4_model::id::{SystemId, UnitTypeId};
        use ti4_model::units::Unit;

        let id = plain_systems(1)[0].clone();
        let players = [PlayerId::new("a"), PlayerId::new("b")];
        let mut state =
            crate::setup::start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        let system = SystemId::new(id.clone());
        state
            .system_mut(&system)
            .units
            .push(Unit::new(UnitTypeId::new("infantry"), players[1].clone()));

        let board = Board::for_player(&state, ContentStore::embedded(), POK, &players[0]);
        assert!(!board.has_enemy_ships(&id), "infantry is not a blockade");

        state
            .system_mut(&system)
            .units
            .push(Unit::new(UnitTypeId::new("destroyer"), players[1].clone()));
        let board = Board::for_player(&state, ContentStore::embedded(), POK, &players[0]);
        assert!(board.has_enemy_ships(&id), "a destroyer is");
    }

    // -- faction-module movement hooks (BF-00e) -----------------------------------------------

    use crate::factions::hooks_movement::{MovementHooks, with_test_hooks};

    /// A state whose marks name two systems, for hooks that cannot capture.
    fn marked(state: &mut GameState, pairs: &[(&str, &str)]) {
        for (key, value) in pairs {
            state
                .faction_marks
                .insert((*key).to_owned(), (*value).to_owned());
        }
    }

    fn mover_board(player: &str) -> Board {
        Board {
            mover: Some(PlayerId::new(player)),
            ..Board::default()
        }
    }

    fn with_state<'a>(
        hub: &'a Hub,
        state: &GameState,
        active: &str,
        player: &str,
    ) -> MovementRules<'a> {
        MovementRules::with_laws(
            &hub.galaxy,
            ContentStore::embedded(),
            POK,
            active,
            mover_board(player),
            Some(state),
        )
    }

    #[test]
    fn empty_modules_change_no_route_for_any_player() {
        let hub = plain_hub();
        let state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], FULL);
        let (near_a, near_b) = (hub.outer[0].clone(), hub.across(&hub.outer[0]));
        let plain = movement_rules(&hub, &near_b, Board::default());
        for player in ["a", "b"] {
            let rules = with_state(&hub, &state, &near_b, player);
            for move_value in 0..4 {
                assert_eq!(
                    rules.path_from(&near_a, move_value),
                    plain.path_from(&near_a, move_value),
                    "{player} at move {move_value}"
                );
            }
            let adjacency = PlayerAdjacency::new(
                &state,
                ContentStore::embedded(),
                POK,
                &hub.galaxy,
                &PlayerId::new(player),
            );
            for id in std::iter::once(&hub.centre).chain(&hub.outer) {
                let want: BTreeSet<String> = hub
                    .galaxy
                    .adjacent(id)
                    .into_iter()
                    .map(ToOwned::to_owned)
                    .collect();
                assert_eq!(adjacency.neighbours(id), want, "{id} for {player}");
            }
        }
    }

    #[test]
    fn a_players_extra_link_shortens_only_their_routes() {
        let hub = plain_hub();
        let (near_a, near_b) = (hub.outer[0].clone(), hub.across(&hub.outer[0]));
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], FULL);
        marked(&mut state, &[("link_x", &near_a), ("link_y", &near_b)]);
        let hooks = MovementHooks {
            linked_systems: Some(|state, _, _, _, who| {
                match (
                    who.as_str(),
                    state.faction_marks.get("link_x"),
                    state.faction_marks.get("link_y"),
                ) {
                    ("a", Some(x), Some(y)) => vec![(x.clone(), y.clone())],
                    _ => Vec::new(),
                }
            }),
            ..MovementHooks::NONE
        };
        with_test_hooks(hooks, || {
            let mine = with_state(&hub, &state, &near_b, "a");
            assert_eq!(
                mine.path_from(&near_a, 1),
                Some(vec![near_a.clone(), near_b.clone()]),
                "one step along the link, in the direction away from the hook's first system too"
            );
            let back = with_state(&hub, &state, &near_a, "a");
            assert!(back.can_reach(&near_b, 1), "links run both ways");
            let theirs = with_state(&hub, &state, &near_b, "b");
            assert!(!theirs.can_reach(&near_a, 1), "another player has no link");
            let adjacency = PlayerAdjacency::new(
                &state,
                ContentStore::embedded(),
                POK,
                &hub.galaxy,
                &PlayerId::new("a"),
            );
            assert!(adjacency.are_adjacent(&near_a, &near_b));
            assert!(adjacency.are_adjacent(&near_b, &near_a));
        });
        let after = with_state(&hub, &state, &near_b, "a");
        assert!(!after.can_reach(&near_a, 1), "the test hook is gone");
    }

    #[test]
    fn a_wormhole_a_piece_carries_links_for_everyone() {
        let hub = plain_hub();
        let (near_a, near_b) = (hub.outer[0].clone(), hub.across(&hub.outer[0]));
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], FULL);
        marked(&mut state, &[("wh_x", &near_a), ("wh_y", &near_b)]);
        let hooks = MovementHooks {
            extra_wormholes: Some(|state| {
                ["wh_x", "wh_y"]
                    .into_iter()
                    .filter_map(|key| state.faction_marks.get(key))
                    .map(|system| (system.clone(), "DELTA".to_owned()))
                    .collect()
            }),
            ..MovementHooks::NONE
        };
        with_test_hooks(hooks, || {
            for player in ["a", "b"] {
                let rules = with_state(&hub, &state, &near_b, player);
                assert!(rules.can_reach(&near_a, 1), "{player} uses the delta pair");
            }
            // Law switches still apply to it: Enforced Travel Ban silences wormholes during
            // movement (the gate keeps hex adjacency only).
            let mut banned = hub.galaxy.clone();
            banned.wormholes_off = true;
            let off = MovementRules::with_laws(
                &banned,
                ContentStore::embedded(),
                POK,
                &near_b,
                mover_board("a"),
                Some(&state),
            );
            assert!(!off.can_reach(&near_a, 1));
        });
        let rules = with_state(&hub, &state, &near_b, "a");
        assert!(!rules.can_reach(&near_a, 1), "hook removed, no wormhole");
    }

    #[test]
    fn supernova_passage_is_per_player_and_needs_the_hook() {
        let supernova = a_system_where("supernova");
        let hub = hub_with_centre(&supernova);
        let (near_a, near_b) = (hub.outer[0].clone(), hub.across(&hub.outer[0]));
        let state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], FULL);
        assert!(!with_state(&hub, &state, &near_b, "a").can_reach(&near_a, 2));
        let hooks = MovementHooks {
            may_enter_supernova: Some(|_, _, _, who| who.as_str() == "a"),
            ..MovementHooks::NONE
        };
        with_test_hooks(hooks, || {
            assert!(with_state(&hub, &state, &near_b, "a").can_reach(&near_a, 2));
            assert!(
                !with_state(&hub, &state, &near_b, "b").can_reach(&near_a, 2),
                "only the hook's player"
            );
            let blind = movement_rules(&hub, &near_b, Board::default());
            assert!(!blind.can_reach(&near_a, 2), "no mover, no module effect");
        });
    }

    #[test]
    fn passing_through_a_supernova_is_not_ending_a_move_in_one() {
        let supernova = a_system_where("supernova");
        let hub = hub_with_centre(&supernova);
        let (near_a, near_b) = (hub.outer[0].clone(), hub.across(&hub.outer[0]));
        let state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], FULL);
        let pass_only = MovementHooks {
            may_pass_through_supernova: Some(|_, _, _, who| who.as_str() == "a"),
            ..MovementHooks::NONE
        };
        with_test_hooks(pass_only, || {
            assert!(
                with_state(&hub, &state, &near_b, "a").can_reach(&near_a, 2),
                "through is allowed"
            );
            assert!(
                !with_state(&hub, &state, &hub.centre, "a").can_reach(&near_a, 1),
                "ending in the supernova is not"
            );
            assert!(!with_state(&hub, &state, &hub.centre, "a").can_enter(&hub.centre));
            assert!(
                !with_state(&hub, &state, &near_b, "b").can_reach(&near_a, 2),
                "only the hook's player"
            );
        });
        let enter_only = MovementHooks {
            may_enter_supernova: Some(|_, _, _, who| who.as_str() == "a"),
            ..MovementHooks::NONE
        };
        with_test_hooks(enter_only, || {
            assert!(with_state(&hub, &state, &hub.centre, "a").can_reach(&near_a, 1));
        });
        assert!(
            !with_state(&hub, &state, &near_b, "a").can_reach(&near_a, 2),
            "neutral"
        );
    }

    #[test]
    fn a_module_can_bar_passage_through_a_system_for_other_players_only() {
        let hub = plain_hub();
        let (near_a, near_b) = (hub.outer[0].clone(), hub.across(&hub.outer[0]));
        let state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], FULL);
        assert!(
            with_state(&hub, &state, &near_b, "a").can_reach(&near_a, 2),
            "neutral"
        );
        let hololattice = MovementHooks {
            // b owns the structures: every other player is barred from the centre.
            blocks_passage: Some(|_, _, _, mover, _| mover.as_str() != "b"),
            ..MovementHooks::NONE
        };
        with_test_hooks(hololattice, || {
            assert!(!with_state(&hub, &state, &near_b, "a").can_reach(&near_a, 2));
            assert!(
                with_state(&hub, &state, &hub.centre, "a").can_reach(&near_a, 1),
                "the system itself may still be the destination"
            );
            assert!(with_state(&hub, &state, &near_b, "b").can_reach(&near_a, 2));
        });
    }

    #[test]
    fn a_ship_type_that_passes_blockades_routes_through_them() {
        let hub = plain_hub();
        let (near_a, near_b) = (hub.outer[0].clone(), hub.across(&hub.outer[0]));
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], FULL);
        // a owns a corsair-like cruiser and a plain carrier somewhere; b's ship blocks the centre.
        let hub_system = SystemId::new(hub.centre.as_str());
        let foreign = ti4_model::units::Unit::new(
            ti4_model::id::UnitTypeId::new("destroyer"),
            PlayerId::new("b"),
        );
        state.system_mut(&hub_system).add(&[foreign]);
        let board = Board::for_player(&state, ContentStore::embedded(), FULL, &PlayerId::new("a"));
        assert!(board.has_enemy_ships(&hub.centre));
        let make = |active: &str, state: &GameState| {
            MovementRules::with_laws(
                &hub.galaxy,
                ContentStore::embedded(),
                FULL,
                active,
                board.clone(),
                Some(state),
            )
        };
        assert!(!make(&near_b, &state).can_reach_ship(&near_a, 2, Some("sol_carrier")));
        marked(&mut state, &[("allow_pass", "yes")]);
        // The hook is asked about the types the player owns on the board: Sol's carrier.
        let carrier_type = state
            .board
            .values()
            .flat_map(|system| system.units.iter())
            .find(|unit| unit.owner.as_str() == "a" && unit.type_id.as_str().contains("carrier"))
            .map(|unit| unit.type_id.to_string())
            .expect("sol starts with a carrier");
        let hooks = MovementHooks {
            may_move_through_ships: Some(|state, _, _, site| {
                site.player.as_str() == "a"
                    && site.ship_type.contains("carrier")
                    && state.faction_marks.contains_key("allow_pass")
            }),
            ..MovementHooks::NONE
        };
        with_test_hooks(hooks, || {
            let rules = make(&near_b, &state);
            assert!(
                rules.can_reach_ship(&near_a, 2, Some(&carrier_type)),
                "the allowed type passes the blockade"
            );
            assert!(
                !rules.can_reach_ship(&near_a, 2, Some("destroyer")),
                "another type does not"
            );
            assert!(
                !rules.can_reach(&near_a, 2),
                "the type-blind route keeps the blockade"
            );
            // The active system itself may hold the blockade: entering is not passing.
            assert!(make(&hub.centre, &state).can_reach_ship(&near_a, 1, Some("destroyer")));
        });
    }

    #[test]
    fn map_edits_swap_and_replace_record_and_replay() {
        let content = ContentStore::embedded();
        let hub = plain_hub();
        let mut state = crate::fixtures::seated_game(&[("a", "sol")], FULL);
        let mut galaxy = hub.galaxy.clone();
        let (x, y) = (hub.outer[0].clone(), hub.across(&hub.outer[0]));
        assert!(galaxy.coord_of(&x) != galaxy.coord_of(&y));
        let (hx, hy) = (galaxy.coord_of(&x).unwrap(), galaxy.coord_of(&y).unwrap());

        apply_map_edit(
            &mut state,
            &mut galaxy,
            content,
            FULL,
            &MapEdit::Swap {
                a: x.clone(),
                b: y.clone(),
            },
        )
        .unwrap();
        assert_eq!(galaxy.coord_of(&x), Some(hy));
        assert_eq!(galaxy.coord_of(&y), Some(hx));
        assert_eq!(
            state.faction_marks.get("map_edit|0000").unwrap(),
            &format!("swap|{x}|{y}")
        );

        // A map re-derived from the base replays to the same place, and replaying is idempotent.
        let mut rebuilt = hub.galaxy.clone();
        replay_map_edits(&state, &mut rebuilt, content, FULL).unwrap();
        assert_eq!(rebuilt, galaxy);
        replay_map_edits(&state, &mut rebuilt, content, FULL).unwrap();
        assert_eq!(rebuilt, galaxy, "idempotent");

        // Replace: the Nova Seed takes the hex; space units and command tokens go with it.
        let target = SystemId::new(hub.outer[1].as_str());
        let ship = ti4_model::units::Unit::new(
            ti4_model::id::UnitTypeId::new("war_sun"),
            PlayerId::new("a"),
        );
        state.system_mut(&target).add(std::slice::from_ref(&ship));
        state.system_mut(&target).place_token(PlayerId::new("a"));
        state
            .wormhole_tokens
            .insert("ALPHA".to_owned(), target.clone());
        let hex = galaxy.coord_of(target.as_str()).unwrap();
        apply_map_edit(
            &mut state,
            &mut galaxy,
            content,
            FULL,
            &MapEdit::Replace {
                old: target.to_string(),
                new: NOVA_SEED.to_owned(),
            },
        )
        .unwrap();
        assert_eq!(galaxy.coord_of(NOVA_SEED), Some(hex));
        assert!(galaxy.coord_of(target.as_str()).is_none());
        let nova = state.system_state(&SystemId::new(NOVA_SEED));
        assert_eq!(nova.units, vec![ship], "the war sun stands on the new tile");
        assert!(nova.command_tokens.contains(&PlayerId::new("a")));
        assert!(
            !state.board.contains_key(&target),
            "the old tile's entry is gone"
        );
        assert!(
            state.wormhole_tokens.is_empty(),
            "Creuss tokens are returned"
        );
        let nova_tile = ti4_content::galaxy::system(content, NOVA_SEED, FULL).unwrap();
        assert!(nova_tile.is_supernova());
        assert_eq!(
            state
                .faction_marks
                .keys()
                .filter(|k| k.starts_with("map_edit|"))
                .count(),
            2
        );
    }

    #[test]
    fn a_refused_map_edit_changes_nothing() {
        let content = ContentStore::embedded();
        let hub = plain_hub();
        let mut state = crate::fixtures::seated_game(&[("a", "sol")], FULL);
        let mut galaxy = hub.galaxy.clone();
        let (state_before, galaxy_before) = (state.clone(), galaxy.clone());
        let marks_before = state.faction_marks.clone();
        for edit in [
            MapEdit::Swap {
                a: hub.centre.clone(),
                b: hub.centre.clone(),
            },
            MapEdit::Swap {
                a: hub.centre.clone(),
                b: "nowhere".to_owned(),
            },
            MapEdit::Replace {
                old: "nowhere".to_owned(),
                new: NOVA_SEED.to_owned(),
            },
            MapEdit::Replace {
                old: hub.centre.clone(),
                new: hub.outer[0].clone(),
            },
            MapEdit::Replace {
                old: hub.centre.clone(),
                new: "not a tile".to_owned(),
            },
        ] {
            assert!(
                apply_map_edit(&mut state, &mut galaxy, content, FULL, &edit).is_err(),
                "{edit:?}"
            );
            assert_eq!(galaxy, galaxy_before, "{edit:?}");
            assert!(state == state_before, "{edit:?}");
            assert_eq!(state.faction_marks, marks_before, "{edit:?}");
        }
    }

    #[test]
    fn the_creuss_gate_and_home_are_adjacent_for_everyone_by_their_delta_wormholes() {
        let content = ContentStore::embedded();
        let hub = plain_hub();
        let mut galaxy = Galaxy::placed(
            content,
            &[
                (crate::seating::CREUSS_GATE, ti4_model::hex::Hex::new(0, 0)),
                (hub.centre.as_str(), ti4_model::hex::Hex::new(1, 0)),
            ],
            FULL,
        )
        .unwrap();
        crate::seating::place_creuss_home(&mut galaxy, content, FULL).unwrap();
        let rules = MovementRules::new(
            &galaxy,
            content,
            FULL,
            crate::seating::CREUSS_HOME,
            Board::default(),
        );
        assert!(
            rules.can_reach(crate::seating::CREUSS_GATE, 1),
            "gate to home, one step"
        );
        let out = MovementRules::new(&galaxy, content, FULL, &hub.centre, Board::default());
        assert!(
            out.can_reach(crate::seating::CREUSS_HOME, 2),
            "home out through the gate to its neighbour: two systems entered"
        );
    }
}
