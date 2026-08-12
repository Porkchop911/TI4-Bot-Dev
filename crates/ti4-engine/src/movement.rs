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

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use ti4_content::ContentStore;
use ti4_content::galaxy::{Galaxy, System, all_systems};
use ti4_model::content_types::SourceSet;
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;

/// The occupancy facts movement legality depends on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Board {
    /// Systems containing ships belonging to somebody other than the moving player.
    pub enemy_ships: BTreeSet<String>,
    /// Systems containing the moving player's own command tokens.
    pub own_command_tokens: BTreeSet<String>,
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
    galaxy: &'a Galaxy,
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
    /// Dynamic Creuss tokens create an extra wormhole edge.
    pub token_wormhole_systems: BTreeSet<String>,
    /// Aerie Hololattice systems may be entered but not moved through by opponents.
    pub barred_transit: BTreeSet<String>,
    /// Systems made into gravity rifts by Dimensional Tears.
    pub gravity_rift_systems: BTreeSet<String>,
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
        Self {
            galaxy,
            systems: all_systems(content, sources),
            active_system: active_system.to_owned(),
            board,
            nebulae_open: false,
            asteroid_fields_open: false,
            supernovae_open: false,
            anomalies_ignored: false,
            ignore_enemy_ships_from: None,
            ignore_enemy_ships: false,
            also_adjacent: BTreeSet::new(),
            token_wormhole_systems: BTreeSet::new(),
            barred_transit: BTreeSet::new(),
            gravity_rift_systems: BTreeSet::new(),
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
        self.nebulae_open || self.anomalies_ignored
    }

    /// Whether a ship may end or pass a step in this system at all.
    #[must_use]
    pub fn can_enter(&self, system_id: &str) -> bool {
        if self.anomalies_ignored {
            return true;
        }
        let Some(system) = self.system(system_id) else {
            // A system the corpus does not describe is not a licence to move anywhere.
            return false;
        };
        if (system.is_supernova() && !self.supernovae_open)
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
        if !self.can_enter(system_id) {
            return false;
        }
        if self.system(system_id).is_some_and(System::is_nebula) && !self.nebulae_open() {
            return false; // 59.1a — never an intermediate, unless a law says otherwise
        }
        if self.barred_transit.contains(system_id) {
            return false;
        }
        if self.ignore_enemy_ships {
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
        if origin == self.active_system {
            return true;
        }
        !self.board.own_command_tokens.contains(origin)
    }

    #[must_use]
    pub fn can_reach(&self, origin: &str, move_value: i32) -> bool {
        self.path_from(origin, move_value).is_some()
    }

    /// A legal route from `origin` to the active system, or `None`.
    ///
    /// Breadth-first, so the route returned enters the fewest systems. Search state carries the
    /// remaining budget because gravity rifts extend it en route.
    #[must_use]
    pub fn path_from(&self, origin: &str, move_value: i32) -> Option<Vec<String>> {
        if !self.may_depart(origin) {
            return None;
        }

        // 59.2: starting inside a nebula caps the move value at 1 — unless anomalies are being
        // ignored, in which case the nebula is not there to cap it.
        let in_nebula =
            self.system(origin).is_some_and(System::is_nebula) && !self.anomalies_ignored;
        let budget = if in_nebula { 1 } else { move_value };
        if budget <= 0 {
            return None;
        }

        let mut queue: VecDeque<(String, i32, i32, Vec<String>)> =
            VecDeque::from([(origin.to_owned(), 0, budget, vec![origin.to_owned()])]);
        // Revisiting is only worthwhile with a larger budget left over.
        let mut best: BTreeMap<String, i32> = BTreeMap::new();

        while let Some((current, entered, mut allowance, route)) = queue.pop_front() {
            // 41.1: leaving a rift is worth an extra step, and 41.3 allows that to happen more
            // than once in one movement. The bonus must land *before* the budget is judged,
            // because it is what pays for the departure — a ship arriving at a rift with
            // nothing left can still leave it.
            if self.is_gravity_rift(&current) && !self.anomalies_ignored {
                allowance += 1;
            }

            let remaining = allowance - entered;
            if best.get(&current).is_some_and(|seen| *seen >= remaining) {
                continue;
            }
            best.insert(current.clone(), remaining);
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
            // The conduit joins the active system to the listed ones in both directions: a
            // route out of one of them is what the card buys.
            if self.also_adjacent.contains(&current) {
                neighbours.insert(self.active_system.clone());
            } else if current == self.active_system {
                neighbours.extend(self.also_adjacent.iter().cloned());
            }

            for neighbour in neighbours {
                if !self.can_enter(&neighbour) {
                    continue;
                }
                let mut arrived = route.clone();
                arrived.push(neighbour.clone());
                if neighbour == self.active_system {
                    return Some(arrived); // 58.4a — movement ends here
                }
                if !self.can_pass_through(&neighbour, Some(origin)) {
                    continue;
                }
                queue.push_back((neighbour, entered + 1, allowance, arrived));
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

#[cfg(test)]
mod tests {
    use ti4_content::galaxy::Galaxy;
    use ti4_model::content_types::POK;

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
}
