//! The MLP input projection (M09-024b1, Tier-C architecture ruling of 2026-08-25).
//!
//! # What this is
//!
//! The MLP does not consume the schema-4 explicit vector as emitted. It consumes a **projection**
//! of it, applied *before* vocabulary lookup: unbounded memorisation crosses are suppressed, and
//! the eight acting-seat facts are restored under a bounded bare family.
//!
//! # Why a projection rather than a change to the extractor
//!
//! The schema-4 vector is what six trained champions score with, and what M09-019b's inventory pin
//! and the legacy-subvector pin protect. Changing it to suit the MLP would move every one of those
//! at once. So the extractor is left exactly as it is and the MLP takes a different **view** of its
//! output. `explicit_choice_features` is byte-for-byte what it was; everything here is downstream
//! of it.
//!
//! # What is suppressed, and why it is a predicate rather than a list
//!
//! An **unbounded memorisation cross** is a family whose identity is the Cartesian product of two
//! free lexical identities, or of a full option identity and a state fact. Measured over the §6.1
//! schedule, three such families were 91.3% of a 203,843-name vocabulary — a dense column each, at
//! `width` weights apiece, for names seen in a vanishing fraction of games by construction.
//!
//! The ruling states this as a predicate on family *shape*, not as a list to maintain: any new
//! family with either shape is excluded by default and needs an architecture review to enter the
//! dense input. [`EXCLUDED_FAMILIES`] is the current grammar's answer to the predicate, not the
//! predicate itself.
//!
//! `state-kind` survives the predicate deliberately: its crossing axis is the bounded canonical
//! decision kind, so its columns transfer between games rather than naming one option in one board.
//!
//! # Suppressed, not routed
//!
//! An excluded name is dropped before lookup. It does **not** fall into its family's OOV column.
//! Those three reserved rows stay in the registry so that no v1 index moves, and are permanently
//! dead: never a routing destination, zeroed and masked from optimization by M09-026/M09-028.
//! Collapsing 186,088 distinct names into three columns would inject a dense signal that means
//! nothing, which is worse than dropping them.

use std::collections::BTreeSet;

use ti4_engine::choice::{Choice, ChoiceOption, Observed};
use ti4_model::id::PlayerId;

use ti4_policy::features::{FeatureVector, seat_facts};

/// The bounded bare family carrying the acting-seat facts into the MLP input.
///
/// The eight facts — round, the three token pools, goods, commodities, controlled planets and
/// technologies — are emitted by the schema-4 extractor **only** crossed, under `state-kind:` or
/// `state-option:`. Suppressing `state-option` without this would erase all eight from every
/// uniform-kind fixed-vocabulary decision, which is the `StateCross::ByOption` branch.
pub const SEAT_STATE_FAMILY: &str = "seat-state";

/// What a registered feature family is to the MLP input.
///
/// A **total** classification: every family in the frozen registry has exactly one role, and a
/// family with no role is not admitted. That direction matters. An earlier draft was a three-name
/// deny-list, which admitted anything unlisted — the opposite of the architecture ruling's
/// requirement that an unclassified family stays out until reviewed, and it silently admitted two
/// legacy-only families that the schema-4 extractor never emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FamilyRole {
    /// Gets a dense column. Its identity transfers between games.
    Transferable,
    /// An unbounded memorisation cross: the Cartesian product of two free lexical identities, or
    /// of a full option identity and a state fact. Suppressed before lookup.
    UnboundedCross,
    /// Emitted only by the legacy schema-2 hashed extractor. The MLP consumes one schema-4
    /// explicit path, so these names never occur on it — but they *do* occur in the r6 checkpoint,
    /// which is discovery source (a), so they must be rejected there rather than assumed absent.
    LegacyOnly,
}

/// The frozen role of every registered family.
///
/// Written out rather than derived from `EXPLICIT_FIXED_FAMILIES`, for the same reason the OOV
/// registry is written out: deriving it would mean a family added to a grammar is admitted to the
/// dense input as a side effect of an ordinary edit. Admission is an architecture decision, and
/// `the_classification_covers_exactly_the_registry` fails when this table and the registry drift
/// so the decision cannot be skipped.
const FAMILY_ROLES: [(&str, FamilyRole); 47] = [
    // M10-035. Transferable: every action-feasibility fact is a bounded count or flag about the
    // option under consideration -- "planets this activation could take" means the same thing in
    // every game, on every map, for every faction.
    (ACTION_FAMILY, FamilyRole::Transferable),
    ("*-unit", FamilyRole::Transferable),
    ("ability", FamilyRole::Transferable),
    // OBS-004a. Transferable: every actor-inventory fact is a closed readiness/count bucket about
    // the acting seat's own faceup holdings -- "how many relics you hold" means the same thing in
    // every game, the same way `faction-commodities` and `ability` already do.
    ("actor-inventory", FamilyRole::Transferable),
    ("card", FamilyRole::Transferable),
    // OBS-008b2. Transferable: the combat decision surface is a stable subtype, an option count,
    // and a bounded hit-count fact or expectation -- each means the same thing in any game.
    ("combat", FamilyRole::Transferable),
    // OBS-008e/f/g/h/i. Transferable: the content decision surface (trade, agenda, reactions,
    // action cards, faction abilities, exploration, relics, leaders, and remaining audit rows) is
    // a stable subtype and an option count -- the same shape as every other closed-grammar family.
    ("content", FamilyRole::Transferable),
    // M09-027b. Transferable: every critic identity is bounded and means the same thing next game
    // — the round, the acting seat's economy and score, opponent counts, and the `objective_
    // progress:<family>` / `ability:<x>` / `faction_tech:<t>` tokens, all of which are corpus
    // identities rather than board ones. Admitting it is what gives `critic-state:*` names real
    // columns; without the entry the closed default routes the whole critic vector to one column
    // and `V` is a rank-1 sum (F-M09-027-3).
    ("critic-state", FamilyRole::Transferable),
    ("destination", FamilyRole::Transferable),
    ("faction-commodities", FamilyRole::Transferable),
    ("faction-home", FamilyRole::Transferable),
    ("faction-start-tech", FamilyRole::Transferable),
    ("faction-start-unit", FamilyRole::Transferable),
    ("faction-tech", FamilyRole::Transferable),
    ("invasion", FamilyRole::Transferable),
    ("kind", FamilyRole::Transferable),
    // Faction crosses are a legacy-only channel: the explicit path asserts it never emits them.
    // `option-faction` would also fail the unbounded-cross shape; `LegacyOnly` is recorded because
    // it is the reason the MLP never sees them.
    ("kind-faction", FamilyRole::LegacyOnly),
    ("landing", FamilyRole::Transferable),
    ("objective-count", FamilyRole::Transferable),
    ("objective-met", FamilyRole::Transferable),
    ("objective-need", FamilyRole::Transferable),
    ("objective-progress", FamilyRole::Transferable),
    ("objective-stage", FamilyRole::Transferable),
    ("opponent-secrets-held", FamilyRole::Transferable),
    // OBS-005. Transferable: every opponent-slot fact is a bounded count/flag about a
    // relationship-ranked slot position, not a player identity -- "the combat counterpart's trade
    // goods" means the same thing next game no matter which seat that turns out to be.
    ("opponent-slot", FamilyRole::Transferable),
    ("option", FamilyRole::Transferable),
    ("option-faction", FamilyRole::LegacyOnly),
    ("option-system", FamilyRole::Transferable),
    ("origin", FamilyRole::Transferable),
    ("pay", FamilyRole::Transferable),
    ("payload", FamilyRole::Transferable),
    ("payload-bool", FamilyRole::Transferable),
    ("payload-count", FamilyRole::Transferable),
    ("payload-number", FamilyRole::Transferable),
    ("payload-number-kind", FamilyRole::Transferable),
    ("placement", FamilyRole::Transferable),
    ("production", FamilyRole::Transferable),
    ("prompt-bigram", FamilyRole::UnboundedCross),
    // The schema-4 extractor retains this for compatibility, but the MLP contract never derives
    // a feature from prompt wording. It is suppressed before vocabulary lookup like every other
    // retired presentation channel.
    ("prompt-kind", FamilyRole::LegacyOnly),
    ("prompt-option", FamilyRole::UnboundedCross),
    ("route", FamilyRole::Transferable),
    ("seat-state", FamilyRole::Transferable),
    ("state-kind", FamilyRole::Transferable),
    ("state-option", FamilyRole::UnboundedCross),
    // OBS-008d1. Transferable: a stable subtype, an option count, and an optional-ness flag —
    // each means the same thing in any game, the way `tactical`/`combat` already do.
    ("strategy", FamilyRole::Transferable),
    // OBS-008a1. Transferable: the tactical decision surface is a stable subtype, an option count,
    // and a bounded command-token pool delta — each means the same thing in any game.
    ("tactical", FamilyRole::Transferable),
    ("target", FamilyRole::Transferable),
];

/// The role of a family, or `None` if it has none.
///
/// `None` is the fail-closed answer, not an error to paper over: a family nobody classified is a
/// family nobody decided to put in the model.
#[must_use]
pub fn role_of(family: &str) -> Option<FamilyRole> {
    // The `<canonical-kind>-unit` families share one registry entry, but admission is **not** a
    // suffix test. An earlier draft mapped every family ending in `-unit` to the shared role, so
    // `never-reviewed-unit:x` was admitted — reopening by suffix exactly the closed default this
    // classification exists to hold (F-M09-024b1-3). Checkpoint names are a discovery source, so an
    // arbitrary historical `*-unit` family would have entered the dense vocabulary without the
    // architecture review a new family requires.
    //
    // The approved list is pinned instead, and an unrecognised suffix family falls through to
    // `None` like any other unclassified name.
    let key = if APPROVED_UNIT_FAMILIES.contains(&family) {
        ti4_policy::vocabulary::UNIT_SUFFIX_FAMILY
    } else {
        family
    };
    FAMILY_ROLES
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, role)| *role)
}

/// The `<canonical-kind>-unit` families approved for the dense input.
///
/// Frozen, for the same reason `OOV_FAMILIES_V1` is: the left half is a canonical decision kind and
/// `canonical_feature_kind` passes unknown kinds through unchanged, so the set of families the
/// extractor *could* emit is open. The set the architecture approved is not. Adding one is a
/// review, and `every_approved_unit_family_is_emitted_by_the_grammar` fails when this list and the
/// observed families disagree.
pub const APPROVED_UNIT_FAMILIES: [&str; 6] = [
    "casualty-unit",
    "commit-unit",
    "load-unit",
    "move-unit",
    "produce-unit",
    "transit-unit",
];

/// Whether a family is an unbounded memorisation cross.
#[must_use]
pub fn is_unbounded_cross(family: &str) -> bool {
    role_of(family) == Some(FamilyRole::UnboundedCross)
}

/// Every family that can never be a routing destination on the MLP path.
///
/// Both non-transferable roles: the unbounded crosses, suppressed by shape, and the legacy-only
/// channels, which the schema-4 extractor does not emit at all. Their reserved rows are retained so
/// no v1 index moves, and are dead — five of them, not the three the crosses alone would give.
#[must_use]
pub fn inactive_families() -> Vec<&'static str> {
    FAMILY_ROLES
        .iter()
        .filter(|(_, role)| *role != FamilyRole::Transferable)
        .map(|(name, _)| *name)
        .collect()
}

/// Whether a feature name survives the projection into the dense input.
///
/// Closed by default: admitted only if its family is classified `Transferable`.
#[must_use]
pub fn admits(name: &str) -> bool {
    role_of(ti4_policy::vocabulary::family_of(name)) == Some(FamilyRole::Transferable)
}

thread_local! {
    /// Memoised admission, keyed by the interned key.
    ///
    /// Admission is a pure function of the name, and [`ti4_policy::intern::FeatureKey`] is a pure
    /// function of the name, so the answer for a key never changes and caching it is sound.
    ///
    /// Thread-local rather than a shared map: a decider runs on one thread for the life of a game,
    /// so this needs no lock at all — and a lock is most of what was being paid.
    static ADMITTED: std::cell::RefCell<std::collections::HashMap<ti4_policy::intern::FeatureKey, bool>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// [`admits`] for a caller that already holds the key, memoised.
///
/// # Why this exists
///
/// `project_vector` ran `admits(&name_of(key))` for **every feature of every option of every
/// decision**: a global `RwLock` read, a `String` allocation, a `split_once`, and then a linear
/// scan of the forty `FAMILY_ROLES` entries comparing strings. At roughly forty features across
/// twenty options that is eight hundred locks and allocations and up to thirty-two thousand string
/// comparisons for one decision.
///
/// M09-029 measured the consequence: feature extraction alone cost more than the entire linear
/// game it was being compared against, so the throughput gate was about to charge the architecture
/// for a memoisable string lookup.
#[must_use]
pub fn admits_key(key: ti4_policy::intern::FeatureKey) -> bool {
    ADMITTED.with(|cache| {
        if let Some(known) = cache.borrow().get(&key) {
            return *known;
        }
        let verdict = admits(&ti4_policy::intern::name_of(key));
        cache.borrow_mut().insert(key, verdict);
        verdict
    })
}

/// The acting-seat facts under the bare family, for one position.
///
/// Option-invariant by construction — the same eight values on every option of a choice — which is
/// the point: MLP plan §4.1's nonlinear per-option trunk can let them interact with option facts,
/// where a linear head would see a constant and ignore them.
/// The family carrying what an option would achieve, as distinct from what the seat holds.
///
/// Separate from `seat-state` on purpose. A seat fact is the same for every option of a choice and
/// says where the seat stands; an action fact differs *between* options and says what each one
/// would do about it. Sharing a family would put them in one out-of-vocabulary column and make an
/// unknown seat fact indistinguishable from an unknown action fact.
pub const ACTION_FAMILY: &str = "action-plan";

/// What one option would do about the opening deficit.
///
/// # Why this exists
///
/// `features::explicit_option_features_with` describes an option by tokenising its id and label and
/// then dropping every all-digit token. Its own comment records the reason -- planet identities must
/// not become features, "so that the policy learns about planets rather than about Archon Ren" --
/// and the cost: system ids are numbers, so an activation option carries the *kind* of move and
/// nothing about its destination.
///
/// The seat facts added in M10-034 tell a seat it holds two systems and needs a third. Nothing told
/// it which offered move supplies one. A reachability search then measured the consequence: raising
/// the search budget 3.75x moved recovery of failed openings from 64% to 65%, so for about a third
/// of failures the clearing line has effectively zero probability under the policy at several
/// decisions in a row. That is what being unable to see the option produces.
///
/// # What is emitted, and for which options
///
/// Only where the option's target actually resolves. Each head names its target differently and a
/// guessed target would produce a confident feature describing the wrong square:
///
/// - **activation** (`ACTIVATE_KIND`) -- the option id *is* the destination system.
/// - **movement** (`MOVE_KIND`) -- the destination is the already-activated system; the option's
///   payload names the origin, the unit and its capacity.
/// - **commit** (`COMMIT_KIND`) -- the payload names the planet being landed on.
///
/// Everything else gets nothing rather than a default, because a zero here would be a claim.
///
/// These are properties, never identities: "planets this would gain" rather than "system 72". That
/// is the same rule the planet-id exclusion enforces, applied to the facts that replace it.
#[expect(
    clippy::too_many_lines,
    reason = "one arm per option kind; splitting them would separate each arm from the closures               (`count`, `destination`, `name`) that every arm shares"
)]
fn action_facts(
    seen: &Observed<'_>,
    option: &ChoiceOption,
    player: &PlayerId,
) -> Vec<(String, f64)> {
    let controlled = seen.controlled_planets(player);
    let held_systems: std::collections::BTreeSet<&ti4_model::id::SystemId> =
        controlled.iter().map(|(system, _)| *system).collect();

    let name = |suffix: &str| format!("{ACTION_FAMILY}:{suffix}");
    #[expect(
        clippy::cast_precision_loss,
        reason = "planet, system and unit counts are single digits"
    )]
    let count = |value: usize| -> f64 { value as f64 };

    // How many planets in `system` this seat does not already control, and whether reaching it
    // would add a system it holds nothing in.
    // Planets come from the **content**, not from `planet_units`.
    //
    // `SystemState::planet_units` holds an entry only where units were placed -- `seating` creates
    // them for starting forces and nothing creates them for an empty planet. Counting its keys
    // therefore counts planets somebody is already standing on, and reports **zero** for the
    // untouched systems a seat most wants to take. Mecatol Rex reads as a system with no planets.
    //
    // That is not an unhelpful feature, it is an inverted one, and it is why two training runs on
    // this family measured flat: `activate-free-planets` was largest exactly where the seat should
    // not go.
    let destination = |system: &ti4_model::id::SystemId| -> (Vec<ti4_model::id::PlanetId>, bool) {
        let mine: std::collections::BTreeSet<&ti4_model::id::PlanetId> = controlled
            .iter()
            .filter(|(held, _)| *held == system)
            .map(|(_, planet)| *planet)
            .collect();
        let free = ti4_content::galaxy::system(seen.content(), system.as_str(), seen.sources())
            .map_or_else(Vec::new, |tile| {
                tile.planets()
                    .into_iter()
                    .map(|planet| ti4_model::id::PlanetId::new(planet.to_owned()))
                    .filter(|planet| !mine.contains(planet))
                    .collect()
            });
        (free, !held_systems.contains(system))
    };

    match option.kind.as_str() {
        kind if kind == ti4_engine::tactical::ACTIVATE_KIND => {
            let system = ti4_model::id::SystemId::new(option.id.clone());
            let (free, adds) = destination(&system);

            // What the system is worth taking for, beyond a bare planet count. A two-planet tile
            // of one resource each and a two-planet tile of four are the same number and very
            // different moves.
            let planets = ti4_content::galaxy::all_planets(seen.content(), seen.sources());
            let (resources, influence) = free.iter().fold((0i64, 0i64), |(r, i), planet| {
                planets.get(planet.as_str()).map_or((r, i), |record| {
                    (r + record.resources(), i + record.influence())
                })
            });

            // What could actually get there.
            //
            // This is the fact whose absence was measurable: with only "how many planets are
            // free" and "is it a new system" on an activation option, nothing told the policy
            // whether a single one of its ships could reach the tile, and a quarter of every
            // activation went to a system where the movement step then offered nothing at all.
            //
            // `movable_into` is the engine reachability search itself, so gravity drive,
            // action-card effects and laws count exactly as the real movement step counts them.
            let reachable = seen.movable_into(player, &system);
            let hulls = reachable.len();
            let capacity: i64 = reachable.iter().map(|hull| hull.capacity).sum();
            // Ground this fleet would actually deliver, capacity matched against what waits at
            // each origin. Summing capacity alone counts lift that has nothing to lift, and
            // summing garrisons counts infantry with no ride.
            let mut by_origin: std::collections::BTreeMap<&ti4_model::id::SystemId, i64> =
                std::collections::BTreeMap::new();
            for hull in &reachable {
                *by_origin.entry(&hull.origin).or_default() += hull.capacity;
            }
            let load: i64 = by_origin
                .into_iter()
                .map(|(origin, lift)| {
                    let waiting = seen
                        .system(origin)
                        .planet_units
                        .values()
                        .flatten()
                        .filter(|unit| &unit.owner == player)
                        .count();
                    lift.min(i64::try_from(waiting).unwrap_or(i64::MAX))
                })
                .sum();

            // Who is already there. An empty tile and a defended one read identically to a policy
            // that cannot see the garrison, and the second is a fight rather than an expansion.
            let occupants = seen.system(&system);
            let enemy_ships = occupants
                .units
                .iter()
                .filter(|unit| &unit.owner != player)
                .count();
            let enemy_ground = occupants
                .planet_units
                .values()
                .flatten()
                .filter(|unit| &unit.owner != player)
                .count();

            // Whether taking this tile would move a revealed objective, asked of the engine
            // requirement functions against the position this action would produce -- not a
            // reimplementation of what each card counts.
            //
            // Skipped when there is nothing to gain: the two positions are then identical by
            // construction, so the answer is zero without computing it.
            let (advances, completes) = if free.is_empty() {
                (0usize, 0usize)
            } else {
                let before = seen.revealed_objective_progress(player);
                let after = seen.revealed_objective_progress_gaining(player, &free);
                let mut advances = 0usize;
                let mut completes = 0usize;
                for card in &after {
                    let Some(was) = before.iter().find(|prior| prior.alias == card.alias) else {
                        continue;
                    };
                    if was.satisfied {
                        continue;
                    }
                    if card.have > was.have {
                        advances += 1;
                    }
                    if card.satisfied {
                        completes += 1;
                    }
                }
                (advances, completes)
            };

            #[expect(
                clippy::cast_precision_loss,
                reason = "resources, influence and capacities are single digits"
            )]
            let amount = |value: i64| -> f64 { value as f64 };
            vec![
                (name("activate-free-planets"), count(free.len())),
                (name("activate-adds-system"), f64::from(u8::from(adds))),
                (name("activate-gain-resources"), amount(resources)),
                (name("activate-gain-influence"), amount(influence)),
                (name("activate-reachable-hulls"), count(hulls)),
                (name("activate-reachable-capacity"), amount(capacity)),
                (name("activate-reachable-load"), amount(load)),
                (name("activate-can-reach"), f64::from(u8::from(hulls > 0))),
                (name("activate-enemy-ships"), count(enemy_ships)),
                (name("activate-enemy-ground"), count(enemy_ground)),
                (name("activate-objective-advances"), count(advances)),
                (name("activate-objective-completes"), count(completes)),
            ]
        }
        kind if kind == ti4_engine::tactical::MOVE_KIND => {
            // No destination facts here, deliberately.
            //
            // Movement in a tactical action moves *into* the already-activated system, so every
            // movement option in a choice shares one destination. An earlier version emitted the
            // destination's free planets and whether it would add a system, and both took the same
            // value for every option — features that cannot separate the things they are asked to
            // choose between. They occupied columns, trained weights and added noise to the head
            // where the diagnosed failure lives, and run-011 measured the cost.
            //
            // What varies per option is the *carrier*: how much this hull can lift, how much is
            // waiting where it sits, and how much it would therefore actually deliver.
            let capacity = option
                .payload
                .get("capacity")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0)
                .max(0);
            let ground = option
                .payload
                .get("origin")
                .and_then(serde_json::Value::as_str)
                .map_or(0, |origin| {
                    seen.system(&ti4_model::id::SystemId::new(origin.to_owned()))
                        .planet_units
                        .values()
                        .flatten()
                        .filter(|unit| &unit.owner == player)
                        .count()
                });
            #[expect(
                clippy::cast_precision_loss,
                reason = "unit capacities are single digits"
            )]
            let capacity_value = capacity as f64;
            // What this move would actually deliver: a hull with capacity four sitting where there
            // is one infantry lifts one, and an empty hull over three lifts none. Neither the
            // capacity nor the garrison says that on its own.
            let load = usize::try_from(capacity).unwrap_or(0).min(ground);
            vec![
                (name("move-capacity"), capacity_value),
                (name("move-ground-at-origin"), count(ground)),
                (name("move-effective-load"), count(load)),
                (name("move-carries-ground"), f64::from(u8::from(load > 0))),
            ]
        }
        kind if kind == ti4_engine::invasion::COMMIT_KIND => {
            let Some(planet) = option
                .payload
                .get("planet")
                .and_then(serde_json::Value::as_str)
            else {
                return Vec::new();
            };
            let planet = ti4_model::id::PlanetId::new(planet.to_owned());
            let already = controlled.iter().any(|(_, held)| **held == planet);
            let adds_system = seen
                .active_system()
                .is_some_and(|system| !held_systems.contains(system));
            vec![
                (name("commit-new-planet"), f64::from(u8::from(!already))),
                (name("commit-adds-system"), f64::from(u8::from(adds_system))),
            ]
        }
        _ => Vec::new(),
    }
}

pub fn seat_state_facts(
    seen: &Observed<'_>,
    player: &PlayerId,
    baseline: ti4_policy::progress::Baseline,
) -> Vec<(String, f64)> {
    let mut facts: Vec<(String, f64)> = seat_facts(seen, player)
        .into_iter()
        .map(|(name, value)| (format!("{SEAT_STATE_FAMILY}:{name}"), value))
        .collect();
    facts.extend(public_game_state_facts(seen, player));
    facts.extend(opening_facts(seen, player, baseline));
    facts
}

/// The MLP-only public information contract shared by every option in a decision.
///
/// The legacy schema-4 extractor deliberately remains frozen; this projection is where the
/// nonlinear actor receives the state needed to distinguish positions whose legal option labels
/// happen to be identical. Names encode transferable roles and card identities, never a physical
/// player, system or planet identity.
#[expect(
    clippy::too_many_lines,
    reason = "the feature inventory is kept together so its public-information boundary is auditable"
)]
fn public_game_state_facts(seen: &Observed<'_>, player: &PlayerId) -> Vec<(String, f64)> {
    let mut facts = Vec::new();
    let mut push = |name: String, value: f64| {
        if value != 0.0 {
            facts.push((format!("{SEAT_STATE_FAMILY}:{name}"), value));
        }
    };
    #[expect(clippy::cast_precision_loss, reason = "public table counts are small")]
    let count = |value: usize| value as f64;
    #[expect(
        clippy::cast_precision_loss,
        reason = "public economy totals are small"
    )]
    let amount = |value: i64| value as f64;

    let phase = match seen.phase() {
        ti4_model::state::Phase::Strategy => "strategy",
        ti4_model::state::Phase::Action => "action",
        ti4_model::state::Phase::Status => "status",
        ti4_model::state::Phase::Agenda => "agenda",
    };
    push(format!("phase:{phase}"), 1.0);
    if let Some(step) = seen.pending_step() {
        push(format!("pending-step:{step}"), 1.0);
    }
    push(
        "active-player-present".to_owned(),
        f64::from(u8::from(seen.active_player().is_some())),
    );
    push(
        "actor-is-active".to_owned(),
        f64::from(u8::from(seen.active_player() == Some(player))),
    );
    push(
        "actor-is-speaker".to_owned(),
        f64::from(u8::from(seen.speaker() == player)),
    );
    push(
        "active-system-present".to_owned(),
        f64::from(u8::from(seen.active_system().is_some())),
    );
    push(
        "custodians-removed".to_owned(),
        f64::from(u8::from(seen.custodians_removed())),
    );

    let players = seen.players();
    let initiative = seen.initiative_order();
    if let Some(position) = initiative.iter().position(|candidate| candidate == player) {
        push("initiative-rank".to_owned(), count(position + 1));
    }
    push("players".to_owned(), count(players.len()));

    let own = seen.seat(player);
    if let Some(seat) = own.as_ref() {
        push("victory-points".to_owned(), f64::from(seat.victory_points));
        push(
            "action-cards-held".to_owned(),
            count(seat.action_cards_held),
        );
        push(
            "secret-objectives-held".to_owned(),
            count(seat.secret_objectives_held),
        );
        push("passed".to_owned(), f64::from(u8::from(seat.passed)));
        push(
            "exhausted-technologies".to_owned(),
            count(seat.exhausted_technologies.len()),
        );
        for technology in seat.technologies {
            let readiness = if seat.exhausted_technologies.contains(technology) {
                "used"
            } else {
                "ready"
            };
            push(
                format!("own-technology:{}:{readiness}", technology.as_str()),
                1.0,
            );
        }
        for card in seat.strategy_cards {
            let readiness = if seat.exhausted_strategy_cards.contains(card) {
                "used"
            } else {
                "ready"
            };
            push(
                format!("own-strategy-card:{}:{readiness}", card.as_str()),
                1.0,
            );
        }
    }

    let controlled = seen.controlled_planets(player);
    let ready_planets = controlled
        .iter()
        .filter(|(_, planet)| seen.planet_is_ready(planet))
        .count();
    push("ready-planets".to_owned(), count(ready_planets));
    push(
        "exhausted-planets".to_owned(),
        count(controlled.len().saturating_sub(ready_planets)),
    );
    push(
        "spendable-resources".to_owned(),
        amount(seen.available_spend(player, ti4_engine::production::Spend::Resources)),
    );
    push(
        "spendable-influence".to_owned(),
        amount(seen.available_spend(player, ti4_engine::production::Spend::Influence)),
    );
    push(
        "systems-with-units".to_owned(),
        count(seen.systems_with_units_of(player).len()),
    );
    push(
        "systems-with-token".to_owned(),
        count(seen.systems_with_token(player).len()),
    );
    push("units-held".to_owned(), count(seen.units_held(player)));
    push(
        "scored-objectives".to_owned(),
        count(seen.scored_by(player).len()),
    );
    push(
        "scoreable-public".to_owned(),
        count(seen.scoreable_public(player)),
    );
    push(
        "scoreable-secret".to_owned(),
        count(seen.scoreable_secret(player)),
    );

    push(
        "production-opportunities".to_owned(),
        count(seen.production_system_count(player)),
    );

    let mut fullest_fleet = 0usize;
    let mut occupied_systems = 0usize;
    let mut contested_systems = 0usize;
    let mut own_units_on_board = 0usize;
    let mut enemy_units_on_board = 0usize;
    let mut active_own_units = 0usize;
    let mut active_enemy_units = 0usize;
    for (system_id, system) in seen.board() {
        let mut owners = BTreeSet::new();
        let mut fleet = 0usize;
        for unit in system
            .units
            .iter()
            .chain(system.planet_units.values().flatten())
        {
            owners.insert(&unit.owner);
            if &unit.owner == player {
                own_units_on_board += 1;
                if ti4_content::units::unit_type(
                    seen.content(),
                    unit.type_id.as_str(),
                    seen.sources(),
                )
                .as_ref()
                .is_some_and(ti4_engine::fleet::counts_against_supply)
                {
                    fleet += 1;
                }
            } else {
                enemy_units_on_board += 1;
            }
            if seen.active_system() == Some(system_id) {
                if &unit.owner == player {
                    active_own_units += 1;
                } else {
                    active_enemy_units += 1;
                }
            }
        }
        if !owners.is_empty() {
            occupied_systems += 1;
        }
        if owners.len() > 1 {
            contested_systems += 1;
        }
        fullest_fleet = fullest_fleet.max(fleet);
    }
    let fleet_limit = seen.fleet_supply_limit(player).max(0);
    let fullest_fleet = i32::try_from(fullest_fleet).unwrap_or(i32::MAX);
    push("fleet-supply-limit".to_owned(), f64::from(fleet_limit));
    push(
        "fleet-supply-headroom".to_owned(),
        f64::from((fleet_limit - fullest_fleet).max(0)),
    );
    push("occupied-systems".to_owned(), count(occupied_systems));
    push("contested-systems".to_owned(), count(contested_systems));
    push("own-units-on-board".to_owned(), count(own_units_on_board));
    push(
        "enemy-units-on-board".to_owned(),
        count(enemy_units_on_board),
    );
    push(
        "active-system-own-units".to_owned(),
        count(active_own_units),
    );
    push(
        "active-system-enemy-units".to_owned(),
        count(active_enemy_units),
    );

    let mut passed_players = 0usize;
    let mut opponent_vp = 0i32;
    let mut leader_vp = own.as_ref().map_or(0, |seat| seat.victory_points);
    let mut opponent_trade_goods = 0i32;
    let mut opponent_commodities = 0i32;
    let mut opponent_tactic_tokens = 0i32;
    let mut opponent_fleet_tokens = 0i32;
    let mut opponent_strategic_tokens = 0i32;
    let mut opponent_spendable_resources = 0i64;
    let mut opponent_spendable_influence = 0i64;
    let mut opponent_action_cards = 0usize;
    let mut opponent_planets = 0usize;
    let mut opponent_controlled_systems = 0usize;
    let mut opponent_systems_with_units = 0usize;
    let mut opponent_units = 0usize;
    for other in &players {
        let Some(seat) = seen.seat(other) else {
            continue;
        };
        passed_players += usize::from(seat.passed);
        for card in seat.strategy_cards {
            let readiness = if seat.exhausted_strategy_cards.contains(card) {
                "used"
            } else {
                "ready"
            };
            push(
                format!("table-strategy-card:{}:{readiness}", card.as_str()),
                1.0,
            );
        }
        if *other == player {
            continue;
        }
        leader_vp = leader_vp.max(seat.victory_points);
        opponent_vp += seat.victory_points;
        opponent_trade_goods += seat.trade_goods;
        opponent_commodities += seat.commodities;
        opponent_tactic_tokens += seat.tactic_tokens;
        opponent_fleet_tokens += seat.fleet_tokens;
        opponent_strategic_tokens += seat.strategic_tokens;
        opponent_spendable_resources +=
            seen.available_spend(other, ti4_engine::production::Spend::Resources);
        opponent_spendable_influence +=
            seen.available_spend(other, ti4_engine::production::Spend::Influence);
        opponent_action_cards += seat.action_cards_held;
        let held = seen.controlled_planets(other);
        opponent_planets += held.len();
        opponent_controlled_systems += held
            .iter()
            .map(|(system, _)| *system)
            .collect::<BTreeSet<_>>()
            .len();
        opponent_systems_with_units += seen.systems_with_units_of(other).len();
        opponent_units += seen.units_held(other);
        for technology in seat.technologies {
            push(format!("opponent-technology:{}", technology.as_str()), 1.0);
        }
    }
    push("passed-players".to_owned(), count(passed_players));
    push(
        "active-players".to_owned(),
        count(players.len().saturating_sub(passed_players)),
    );
    push(
        "opponent-victory-points-total".to_owned(),
        f64::from(opponent_vp),
    );
    push("leader-victory-points".to_owned(), f64::from(leader_vp));
    push(
        "victory-points-behind-leader".to_owned(),
        f64::from(leader_vp - own.as_ref().map_or(0, |seat| seat.victory_points)),
    );
    push(
        "opponent-trade-goods-total".to_owned(),
        f64::from(opponent_trade_goods),
    );
    push(
        "opponent-commodities-total".to_owned(),
        f64::from(opponent_commodities),
    );
    push(
        "opponent-tactic-tokens-total".to_owned(),
        f64::from(opponent_tactic_tokens),
    );
    push(
        "opponent-strategic-tokens-total".to_owned(),
        f64::from(opponent_strategic_tokens),
    );
    push(
        "opponent-fleet-tokens-total".to_owned(),
        f64::from(opponent_fleet_tokens),
    );
    push(
        "opponent-action-cards-total".to_owned(),
        count(opponent_action_cards),
    );
    push(
        "opponent-spendable-resources-total".to_owned(),
        amount(opponent_spendable_resources),
    );
    push(
        "opponent-spendable-influence-total".to_owned(),
        amount(opponent_spendable_influence),
    );
    push("opponent-planets-total".to_owned(), count(opponent_planets));
    push(
        "opponent-controlled-systems-total".to_owned(),
        count(opponent_controlled_systems),
    );
    push(
        "opponent-systems-with-units-total".to_owned(),
        count(opponent_systems_with_units),
    );
    push("opponent-units-total".to_owned(), count(opponent_units));

    let faceup_notes = seen.promissory_notes();
    push(
        "faceup-promissory-notes".to_owned(),
        count(faceup_notes.len()),
    );
    for (note, holder) in faceup_notes {
        let alias = note
            .split_once(':')
            .map_or(note.as_str(), |(alias, _)| alias);
        push(format!("faceup-promissory:{alias}"), 1.0);
        if &holder == player {
            push(format!("own-faceup-promissory:{alias}"), 1.0);
        }
    }
    let supports = seen.support_holders();
    push(
        "supports-held".to_owned(),
        count(supports.values().filter(|holder| *holder == player).count()),
    );
    push(
        "own-support-held-by-opponent".to_owned(),
        f64::from(u8::from(supports.contains_key(player))),
    );

    let mecatol = ti4_model::id::PlanetId::new("mecatol_rex");
    let mecatol_controller = players.iter().find(|candidate| {
        seen.controlled_planets(candidate)
            .iter()
            .any(|(_, planet)| **planet == mecatol)
    });
    push(
        "actor-controls-mecatol".to_owned(),
        f64::from(u8::from(
            mecatol_controller.is_some_and(|owner| *owner == player),
        )),
    );
    push(
        "opponent-controls-mecatol".to_owned(),
        f64::from(u8::from(
            mecatol_controller.is_some_and(|owner| *owner != player),
        )),
    );

    facts
}

/// What the opening bar asks for, and where this seat stands against it.
///
/// The reward has always known these; the policy did not. `seat_facts` offers *absolute* controlled
/// planets and nothing about gains since setup, distinct systems, units built, or how far short each
/// component is — so a decision could not be conditioned on "I hold two systems and this activation
/// must reach a third", or "I have the planets and still owe a unit". The two measured failure
/// classes are exactly those two sentences.
///
/// Both the level and the deficit are emitted. They are redundant given the requirement, but the
/// requirement is a constant the model would have to learn to subtract, and a deficit that reaches
/// zero is a far easier thing to condition on than a level that reaches three.
///
/// # Concentration
///
/// Ship and ground spread are here for the same reason. Failed openings finish with forces in fewer
/// systems than cleared ones, and a seat cannot see that about itself from any existing fact. What
/// is offered is the count, not a reward for raising it: paying directly for spread would teach
/// ships to disperse to no purpose, while a fact lets the policy learn when spread is worth having.
///
/// `outside-active` is the resource a spreading decision actually spends — forces not already
/// committed to the system being resolved. It is the difference between "I have three carriers" and
/// "I have three carriers *left to send somewhere else*".
fn opening_facts(
    seen: &Observed<'_>,
    player: &PlayerId,
    baseline: ti4_policy::progress::Baseline,
) -> Vec<(String, f64)> {
    let requirement = ti4_engine::opening::DEFAULT_REQUIREMENT;
    let progress = ti4_policy::progress::measure(seen, player, baseline);

    let controlled = seen.controlled_planets(player);
    let ship_systems = seen.systems_with_units_of(player);
    let active = seen.active_system();

    // Ground forces per system, over the systems this seat is present in at all.
    //
    // `progress.systems` already counts distinct systems holding a controlled planet, and a seat's
    // ground forces are very nearly the same set -- an earlier version emitted both and they were
    // duplicates. What the failure analysis actually separated cleared seats from failed ones on
    // was *concentration*: how much is piled in one place, not how many places. 18.9% of Jol-Nar's
    // failures ended with every ground force in a single system against 0.6% of its successes.
    let mut ground: std::collections::BTreeMap<&ti4_model::id::SystemId, usize> =
        std::collections::BTreeMap::new();
    for system in controlled
        .iter()
        .map(|(system, _)| *system)
        .chain(ship_systems.iter().copied())
    {
        if ground.contains_key(system) {
            continue;
        }
        let held = seen
            .system(system)
            .planet_units
            .values()
            .flatten()
            .filter(|unit| &unit.owner == player)
            .count();
        ground.insert(system, held);
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "planet, system and unit counts are single digits"
    )]
    let deficit = |held: i64, bar: usize| -> f64 {
        let bar = i64::try_from(bar).unwrap_or(i64::MAX);
        (bar - held).max(0) as f64
    };
    #[expect(
        clippy::cast_precision_loss,
        reason = "planet, system and unit counts are single digits"
    )]
    let level = |value: i64| -> f64 { value as f64 };
    #[expect(
        clippy::cast_precision_loss,
        reason = "system counts are single digits"
    )]
    let count = |value: usize| -> f64 { value as f64 };

    vec![
        // Where this seat stands on each part of the bar.
        (
            format!("{SEAT_STATE_FAMILY}:opening-planets-gained"),
            level(progress.planets_gained),
        ),
        (
            format!("{SEAT_STATE_FAMILY}:opening-systems"),
            level(progress.systems),
        ),
        (
            format!("{SEAT_STATE_FAMILY}:opening-units-gained"),
            level(progress.units_gained),
        ),
        // And how much of each is still owed. Zero means that part is done.
        (
            format!("{SEAT_STATE_FAMILY}:opening-planets-needed"),
            deficit(progress.planets_gained, requirement.planets_gained),
        ),
        (
            format!("{SEAT_STATE_FAMILY}:opening-systems-needed"),
            deficit(progress.systems, requirement.systems),
        ),
        (
            format!("{SEAT_STATE_FAMILY}:opening-units-needed"),
            // The reward's shaping proxy, not the gate. The gate is now a fleet composition --
            // two capacity ships and three ground forces -- and a `Progress` carries neither, so
            // the policy cannot be shown the real deficit without carrying the composition on
            // every decision. Recorded rather than papered over: this feature under-describes the
            // bar it is named for.
            deficit(progress.units_gained, 1),
        ),
        // Where the forces are, which is what decides whether the deficits above can still close.
        (
            format!("{SEAT_STATE_FAMILY}:opening-ship-systems"),
            count(ship_systems.len()),
        ),
        (
            format!("{SEAT_STATE_FAMILY}:opening-ground-forces"),
            count(ground.values().sum::<usize>()),
        ),
        (
            format!("{SEAT_STATE_FAMILY}:opening-ship-systems-outside-active"),
            count(
                ship_systems
                    .iter()
                    .filter(|system| active.is_none_or(|current| **system != current))
                    .count(),
            ),
        ),
        (
            format!("{SEAT_STATE_FAMILY}:opening-largest-ground-stack"),
            count(ground.values().copied().max().unwrap_or(0)),
        ),
    ]
}

/// Project one already-extracted vector into the MLP input.
///
/// Suppression happens here, before any vocabulary lookup, so an excluded name never reaches a
/// column at all.
///
/// Three sources meet: the extractor's own features after suppression, the seat facts shared by
/// every option of the choice, and this option's action facts. The seat facts are interned by the
/// caller, once per choice -- registering them per option meant a lock and a hash for each of them
/// on every legal option (M09-029).
fn project_vector(
    vector: &FeatureVector,
    seat_state: &[(ti4_policy::intern::FeatureKey, f64)],
    action: &[(ti4_policy::intern::FeatureKey, f64)],
) -> FeatureVector {
    let kept = vector
        .iter()
        .filter(|(key, _)| admits_key(**key))
        .map(|(key, value)| (*key, *value));
    FeatureVector::from_pairs(
        kept.chain(seat_state.iter().copied())
            .chain(action.iter().copied()),
    )
}

/// Every option of a choice, as the MLP sees it.
///
/// Takes the same inputs as [`ti4_policy::features::explicit_choice_features`], including the explicit
/// held-secret records, so the hidden-information boundary is exactly the one M09-021 established:
/// live play passes the acting seat's own cards, bound at ask time; offline contexts compute them
/// on state they already hold.
#[must_use]
pub fn mlp_choice_features(
    profile: &std::rc::Rc<std::cell::RefCell<super::Profile>>,
    seen: &Observed<'_>,
    choice: &Choice,
    player: &PlayerId,
    held_secrets: &[ti4_engine::objectives::CardProgress],
    baseline: ti4_policy::progress::Baseline,
) -> Vec<FeatureVector> {
    let started = std::time::Instant::now();
    let seat_state = interned_seat_state(seen, player, baseline);
    // Seat facts are shared by every option of the choice; action facts are not, and that is the
    // point of them. Zipping by position is safe because `explicit_choice_features` returns one
    // vector per option in the choice's own order -- a contract the length check below pins rather
    // than trusts.
    profile.borrow_mut().add("feature_seat", started.elapsed());
    let started = std::time::Instant::now();
    let vectors =
        ti4_policy::features::prompt_free_choice_features(seen, choice, player, held_secrets);
    profile
        .borrow_mut()
        .add("feature_explicit", started.elapsed());
    debug_assert_eq!(
        vectors.len(),
        choice.options.len(),
        "one feature vector per option"
    );
    vectors
        .iter()
        .zip(&choice.options)
        .map(|(vector, option)| {
            let started = std::time::Instant::now();
            let action: Vec<(ti4_policy::intern::FeatureKey, f64)> =
                action_facts(seen, option, player)
                    .into_iter()
                    .map(|(name, value)| (ti4_policy::intern::register(&name), value))
                    .collect();
            profile
                .borrow_mut()
                .add("feature_action", started.elapsed());
            let started = std::time::Instant::now();
            let result = project_vector(vector, &seat_state, &action);
            profile
                .borrow_mut()
                .add("feature_project", started.elapsed());
            result
        })
        .collect()
}

/// The bare seat facts, interned once for the whole choice.
///
/// `register` rather than `FeatureKey::of`: the key alone puts a value in the vector but leaves it
/// nameless, so `names_of` resolves it to an empty string and the discovery pass that builds the
/// vocabulary from names would never see it. The agreement test between projected names and
/// projected vectors is what catches that.
fn interned_seat_state(
    seen: &Observed<'_>,
    player: &PlayerId,
    baseline: ti4_policy::progress::Baseline,
) -> Vec<(ti4_policy::intern::FeatureKey, f64)> {
    seat_state_facts(seen, player, baseline)
        .into_iter()
        .map(|(name, value)| (ti4_policy::intern::register(&name), value))
        .collect()
}

/// One option, as the MLP sees it.
#[must_use]
pub fn mlp_option_features(
    seen: &Observed<'_>,
    choice: &Choice,
    option: &ChoiceOption,
    player: &PlayerId,
    held_secrets: &[ti4_engine::objectives::CardProgress],
    baseline: ti4_policy::progress::Baseline,
) -> FeatureVector {
    let seat_state = interned_seat_state(seen, player, baseline);
    let vector = ti4_policy::features::prompt_free_option_features(
        seen,
        choice,
        option,
        player,
        held_secrets,
    );
    let action: Vec<(ti4_policy::intern::FeatureKey, f64)> = action_facts(seen, option, player)
        .into_iter()
        .map(|(name, value)| (ti4_policy::intern::register(&name), value))
        .collect();
    project_vector(&vector, &seat_state, &action)
}

/// The projection applied to a set of discovered names.
///
/// M09-024b2's discovery pass runs through this so the vocabulary it builds and the vectors the
/// model is fed agree by construction rather than by two lists being kept in step.
#[must_use]
pub fn project_names<I, S>(names: I) -> BTreeSet<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    names
        .into_iter()
        .filter(|name| admits(name.as_ref()))
        .map(|name| name.as_ref().to_owned())
        .collect()
}
