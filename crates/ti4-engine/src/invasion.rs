//! Invasion (LRR 49), with ground combat under LRR 42.
//!
//! Ported from the oracle's `engine/invasion.py`: `_bombardment`, `_bombardable`,
//! `_commit_ground_forces`, `_ground_combat`, `_roll_ground` and `_establish_control`.
//!
//! Choices are asked inline through a [`Table`], matching `combat.rs`.

use ti4_content::ContentStore;
use ti4_content::units::{UnitType, catalogue};
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlanetId, PlayerId, SystemId};
use ti4_model::state::{Feat, FeatOccurrence, GameState};
use ti4_model::units::Unit;

use crate::choice::{
    Choice, ChoiceOption, DECLINE_KIND, IllegalChoice, Observed, Resolving, Table, Window,
};
use crate::combat::MAX_ROUNDS;
use crate::dice::Dice;
use crate::rng::GameRng;

/// The choice kind for committing a ground force to a planet (the oracle's `commit`).
pub const COMMIT_KIND: &str = "commit";
/// The choice kind for choosing which of your own ground forces dies.
pub const GROUND_CASUALTY_KIND: &str = "ground_casualty";

/// What an invasion did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InvasionReport {
    /// Planets explored on being taken from nobody (35.1), with what the card did.
    pub explored: Vec<(PlanetId, crate::exploration::Explored)>,
    /// Planets ground forces were committed to.
    pub committed: Vec<PlanetId>,
    /// Planets whose control changed, with who held them before.
    pub captured: Vec<(PlanetId, Option<PlayerId>)>,
    /// Ground forces destroyed by bombardment.
    pub bombardment_kills: usize,
    /// Whether this invasion lifted the custodians token from Mecatol Rex (27.3).
    pub custodians_removed: bool,
}

/// 27.2: six influence, paid before ground forces are committed.
pub const CUSTODIANS_COST: i64 = 6;

/// Whether this invader may lift the custodians token now (27.2).
///
/// Mecatol only, once, and only by a player who can actually pay. Until this existed there was no
/// production path in the engine that removed the token: every assignment was in a test, so the
/// agenda phase -- which 8.1 gates on the token being lifted -- never ran in a simulated game, and
/// with it went every law and every agenda victory point. In 5,881 recorded human games the
/// custodians point is the single most-scored entry.
#[must_use]
pub fn custodians_removable(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> bool {
    if state.custodians_removed || system.as_str() != crate::seating::MECATOL {
        return false;
    }
    // 27.2a: "If a player cannot commit ground forces to land on Mecatol Rex, they cannot remove
    // the custodians token." The rule reads as a restriction and it is also a gate on a victory
    // point -- without it a seat with six influence and no army could buy the point outright and
    // then commit nothing, which is the one thing 27.2 is written to prevent.
    if landable(state, content, sources, player, system).is_empty() {
        return false;
    }
    crate::production::available(
        state,
        content,
        sources,
        player,
        crate::production::Spend::Influence,
    ) >= CUSTODIANS_COST
}

/// 15.1f: Planetary Shield makes a planet immune to bombardment entirely.
///
/// A war sun ignores it — which is most of what a war sun is for, so leaving it out would make
/// the unit strictly worse than the rules give.
#[must_use]
pub fn bombardable(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
    planet: &PlanetId,
    invader: &PlayerId,
) -> bool {
    // Conventions of War outranks every reason bombardment would otherwise be allowed, war sun
    // included: the law says it cannot be used against units on a cultural planet at all.
    if crate::laws::bombardment_forbidden(state, content, sources, planet) {
        return false;
    }
    let types = catalogue(content, sources);
    let board = state.system_state(system);
    let has_warsun = board.units_of(invader).into_iter().any(|unit| {
        types
            .get(unit.type_id.as_str())
            .is_some_and(|kind| kind.base_type() == "warsun")
    });
    if has_warsun {
        return true;
    }
    // L1Z1X's commander ignores a planetary shield outright, which is the whole card.
    if crate::leaders::ignores_planetary_shield(state, invader) {
        return true;
    }
    // Disable: "your opponents' PDS units lose PLANETARY SHIELD ... during this invasion."
    // The markers are keyed to the activation that owns the invasion, so only Disable cards
    // played for *this* invasion matter. A shield survives only while every Disable in play
    // belongs to the shield's own owner — an opponent's copy strips it.
    let disabled_holders: Vec<PlayerId> = state
        .players
        .iter()
        .filter(|seat| seat.disable_invasion.contains(&state.activation_seq))
        .map(|seat| seat.id.clone())
        .collect();
    !board.on_planet(planet).iter().any(|unit| {
        if !types
            .get(unit.type_id.as_str())
            .is_some_and(UnitType::planetary_shield)
        {
            return false;
        }
        !disabled_holders.iter().any(|holder| holder != &unit.owner)
    })
}

/// 49.1: the invader's bombarding ships fire at ground forces on the planets below.
///
/// Returns how many ground forces were destroyed. On a coexisting planet the invader must
/// choose, per bombarding unit, whose units take the hits (coexistence 7, 7.1), so a decider
/// is threaded in.
///
/// # Errors
///
/// [`IllegalChoice`] when the decider answers with something not offered.
#[allow(
    clippy::too_many_arguments,
    reason = "one parameter per distinct input"
)]
pub fn bombardment(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    dice: &mut Dice,
    rng: &mut GameRng,
    table: &mut Table,
    system: &SystemId,
    invader: &PlayerId,
) -> Result<usize, IllegalChoice> {
    let occurrence = state.begin_feat_occurrence();
    let plan = roll_bombard_plan(state, content, sources, dice, rng, system, invader);
    apply_bombard_plan(state, table, system, invader, &plan, occurrence).map(|(killed, _)| killed)
}

/// What was rolled on one planet: each bombarding unit's hit total (roll order) and the
/// ground forces that stood there before the bombardment, so "the last ground force on a
/// planet" (Make an Example of Their World) can be judged after the hits are assigned.
#[derive(Debug, Clone)]
struct BombardPlan {
    planet: PlanetId,
    groups: Vec<usize>,
    held: usize,
    victims: std::collections::BTreeSet<PlayerId>,
}

/// A unit's ground-combat threshold, with the faction shift applied.
///
/// Jol-Nar's Fragile is "-1 to all combat rolls" and Sardakk's Unrelenting is "+1" -- neither says
/// "in space". `combat_modifier` has carried a `context` parameter since it was written, and its
/// only caller passed "space", so ground combat rolled the printed value and Jol-Nar infantry
/// fought at full strength everywhere.
///
/// A shift applies to the *roll*, so it moves the threshold the other way, exactly as the space
/// path does.
#[must_use]
pub fn ground_combat_value(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
    unit_type: &str,
) -> Option<i64> {
    let printed = catalogue(content, sources)
        .get(unit_type)
        .and_then(ti4_content::units::UnitType::combat_hits_on)?;
    let mut faction = crate::faction_abilities::combat_modifier(state, content, player, "ground");

    // Jol-Nar's Shield Paling mech: "Your infantry on this planet are not affected by your Fragile
    // faction ability." Only infantry, and only on the planet the mech is standing on -- which is
    // why this is decided per planet rather than per seat.
    if faction < 0 && unit_type.contains("infantry") && shield_paling(state, player, system, planet)
    {
        faction = 0;
    }
    Some(printed - faction)
}

/// Whether a Jol-Nar Shield Paling mech is on this planet, shielding its owner's infantry.
fn shield_paling(
    state: &GameState,
    player: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) -> bool {
    state
        .system_state(system)
        .on_planet_of(planet, player)
        .into_iter()
        .any(|unit| unit.type_id.as_str() == "jolnar_mech")
}

/// Letnev's Dunlain Reaper mech, offered at the start of each ground-combat round.
///
/// > DEPLOY: At the start of a round of ground combat, you may spend 2 resources to replace 1 of
/// > your infantry in that combat with 1 mech from your reinforcements.
///
/// 20.3 -- a DEPLOY ability places a unit from reinforcements -- so the swap is gated on supply,
/// and 20.5 makes it once per timing window, which one round is. Offered only to a player who
/// actually has an infantry in this combat, since that is what the mech replaces.
fn dunlain_reaper(
    state: &mut GameState,
    ctx: &mut Resolving<'_>,
    player: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) {
    const MECH: &str = "letnev_mech";
    const COST: i64 = 2;
    let holds_mech_unit = state
        .player(player)
        .is_some_and(|seat| seat.faction.as_str() == "letnev");
    if !holds_mech_unit {
        return;
    }
    let Some(infantry) = state
        .system_state(system)
        .on_planet_of(planet, player)
        .into_iter()
        .find(|unit| unit.type_id.as_str().contains("infantry"))
        .cloned()
    else {
        return; // nothing to replace
    };
    // 31.4 and 20.4: no mech in reinforcements, no deploy.
    if crate::supply::allowed(state, ctx.content, ctx.sources, player, &ti4_model::id::UnitTypeId::new(MECH), 1) == 0 {
        return;
    }
    if crate::production::available(
        state,
        ctx.content,
        ctx.sources,
        player,
        crate::production::Spend::Resources,
    ) < COST
    {
        return; // 22.3: not offered when it cannot be paid for
    }

    let choice = crate::choice::Choice::new(
        player.clone(),
        format!("Dunlain Reaper: spend 2 resources to replace an infantry on {planet}"),
        vec![
            crate::choice::ChoiceOption::labelled(
                "deploy".to_owned(),
                "unit",
                "deploy the mech".to_owned(),
            ),
            crate::choice::ChoiceOption::decline(),
        ],
    );
    let Ok(answer) = ctx.table.ask(&choice) else {
        return;
    };
    if answer.is_decline() {
        return;
    }
    if !crate::production::pay(
        state,
        ctx.content,
        ctx.sources,
        ctx.table,
        player,
        COST,
        crate::production::Spend::Resources,
    )
    .unwrap_or(false)
    {
        return;
    }
    if let Some(here) = state.board.get_mut(system)
        && let Some(stack) = here.planet_units.get_mut(planet)
        && let Some(at) = stack.iter().position(|unit| *unit == infantry)
    {
        stack[at] = Unit::new(ti4_model::id::UnitTypeId::new(MECH), player.clone());
    }
}

/// The invader's units that may bombard `planet` this invasion.
///
/// Ordinarily the ships in the space area (49.1). L1Z1X's Anihilator mech is the exception:
/// "While not participating in ground combat, this unit can use its BOMBARDMENT ability on planets
/// in its system as if it were a ship." So a mech standing on some *other* planet joins the
/// bombardment, and one standing on the planet it would be shooting at does not -- that is what
/// participating in the ground combat means here.
fn bombarders(
    state: &GameState,
    invader: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) -> Vec<Unit> {
    let board = state.system_state(system);
    let mut found: Vec<Unit> = board.units_of(invader).into_iter().cloned().collect();
    for (where_it_stands, standing) in &board.planet_units {
        if where_it_stands == planet {
            continue; // it is in the ground combat for this planet, so it does not bombard it
        }
        found.extend(
            standing
                .iter()
                .filter(|unit| &unit.owner == invader && unit.type_id.as_str() == "l1z1x_mech")
                .cloned(),
        );
    }
    found
}

/// Roll the invader's bombarding ships on every planet they can reach (49.1).
///
/// The dice-consuming half of the bombardment: both the invasion window and this synchronous
/// wrapper call it, so a given seed makes the same bombardment rolls no matter how the hits
/// are assigned afterwards.
fn roll_bombard_plan(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    dice: &mut Dice,
    rng: &mut GameRng,
    system: &SystemId,
    invader: &PlayerId,
) -> Vec<BombardPlan> {
    // Entropic scars rules 2 and 4: bombardment is a unit ability, so it cannot be used by
    // ships in a scar, nor against ground forces in one. Both directions collapse to the same
    // system here, since bombardment fires from the active system onto planets in it.
    if !crate::entropic_scars::abilities_usable(content, sources, system, Some(system)) {
        return Vec::new();
    }
    let types = catalogue(content, sources);
    let planets: Vec<PlanetId> = state
        .system_state(system)
        .planet_units
        .keys()
        .cloned()
        .collect();

    let mut plan = Vec::new();
    // Every unit's roll, kept for the reroll windows the bombardment opens (Agnlan Oln,
    // Scramble Frequency): one entry per bombarding unit, tagged with its planet.
    let mut staged = Vec::new();
    // Blitz: "each of your non-fighter ships in the active system that do not have
    // BOMBARDMENT gain BOMBARDMENT 6 until the end of the invasion." The card's window
    // opened before this plan was built, so the marker, if present, applies to every roll
    // below; it is keyed to the activation that owns the invasion.
    let blitzed = state
        .player(invader)
        .is_some_and(|seat| seat.blitz_invasion.contains(&state.activation_seq));
    for planet in planets {
        if !bombardable(state, content, sources, system, &planet, invader) {
            continue;
        }
        let defenders: Vec<Unit> = state
            .system_state(system)
            .on_planet(&planet)
            .iter()
            .filter(|unit| &unit.owner != invader)
            .cloned()
            .collect();
        if defenders.is_empty() {
            continue;
        }

        let mut groups: Vec<usize> = Vec::new();
        for unit in bombarders(state, invader, system, &planet) {
            let Some(kind) = types.get(unit.type_id.as_str()) else {
                continue;
            };
            let (value, count) = match kind.bombard_hits_on() {
                Some(value) => (value, usize::try_from(kind.bombard_dice()).unwrap_or(0)),
                // Blitz grants BOMBARDMENT 6 — one die — to a non-fighter ship with no
                // bombard value of its own.
                None if blitzed && kind.is_ship() && !kind.is_fighter() => (6, 1),
                None => continue,
            };
            if count == 0 {
                continue;
            }
            // Bunker: "during this invasion, apply -4 to the result of each BOMBARDMENT roll
            // against planets you control." The window that hosts these rolls is opened after
            // the driver's invasion events, so the marker is in place by the time the rolls
            // are made. One entry per copy, so two Bunkers on the same planet give -8.
            let bunker_penalty = state
                .system_state(system)
                .planet_control
                .get(&planet)
                .and_then(|controller| state.player(controller))
                .map_or(0, |seat| {
                    4 * i64::try_from(
                        seat.bunker_invasion
                            .iter()
                            .filter(|seq| **seq == state.activation_seq)
                            .count(),
                    )
                    .unwrap_or(i64::MAX)
                });
            let value = value + bunker_penalty;
            let roll = dice.roll(
                rng,
                count,
                "bombardment",
                Some(u32::try_from(value).unwrap_or(u32::MAX)),
            );
            let produced = roll.hits();
            if produced > 0 {
                groups.push(produced);
            }
            staged.push(ti4_model::state::RerollEntry {
                unit: unit.type_id.to_string(),
                planet: Some(planet.clone()),
                hits_on: Some(u32::try_from(value).unwrap_or(u32::MAX)),
                faces: roll.faces,
                rerolled: std::collections::BTreeSet::new(),
                deltas: std::collections::BTreeMap::new(),
                // The bombarding unit sits in the system's space area; `planet` names the
                // target it hits, not where the unit stands.
                unit_types: std::iter::once((unit.type_id.to_string(), 1)).collect(),
            });
        }
        let victims: std::collections::BTreeSet<PlayerId> =
            defenders.iter().map(|unit| unit.owner.clone()).collect();
        plan.push(BombardPlan {
            planet,
            groups,
            held: defenders.len(),
            victims,
        });
    }
    if staged.iter().any(|roll| !roll.faces.is_empty()) {
        state.reroll_staging.insert(
            invader.clone(),
            ti4_model::state::RerollSet {
                kind: "bombardment".into(),
                system: system.clone(),
                rolls: staged,
            },
        );
        state.last_reroll_player = Some(invader.clone());
    }
    plan
}

/// Destroy up to `produced` of `owner`'s ground forces on `planet` (their deterministic
/// on-planet order) and return how many fell. Coexistence 7.2: a unit's hits stop at the
/// chosen player's own units and do not spill to anyone else's.
fn take_bombard_hits(
    state: &mut GameState,
    system: &SystemId,
    planet: &PlanetId,
    owner: &PlayerId,
    produced: usize,
) -> usize {
    if produced == 0 {
        return 0;
    }
    let doomed: Vec<Unit> = state
        .system_state(system)
        .on_planet(planet)
        .iter()
        .filter(|unit| unit.owner == *owner)
        .take(produced)
        .cloned()
        .collect();
    if doomed.is_empty() {
        return 0;
    }
    state.system_mut(system).remove_from_planet(planet, &doomed);
    doomed.len()
}

/// The coexistence 7/7.1 question: whose units on the planet take this bombarding unit's
/// hits. Only players still holding ground forces there are offered.
fn bombardment_target_question(
    invader: &PlayerId,
    planet: &PlanetId,
    hits: usize,
    present: &std::collections::BTreeSet<PlayerId>,
) -> Option<Choice> {
    if present.is_empty() {
        return None;
    }
    Some(Choice::new(
        invader.clone(),
        format!("whose units on {planet} take the bombardment's next hits ({hits} hits)"),
        present
            .iter()
            .map(|player| {
                ChoiceOption::labelled(
                    player.as_str(),
                    "bombardment_target",
                    format!("{player}'s units"),
                )
            })
            .collect(),
    ))
}

/// Assign an already-rolled bombardment synchronously, asking the decider on coexisting
/// planets (7, 7.1) and capping each unit's hits at the chosen player's forces (7.2).
///
/// Returns how many ground forces were destroyed and whether the "last ground force on a
/// planet" feat fired. The invasion window applies the same rolls through its pause-and-
/// answer state machine instead, because there the choice crosses a step boundary.
fn apply_bombard_plan(
    state: &mut GameState,
    table: &mut Table,
    system: &SystemId,
    invader: &PlayerId,
    plan: &[BombardPlan],
    occurrence: FeatOccurrence,
) -> Result<(usize, bool), IllegalChoice> {
    let mut killed = 0;
    let mut noted = false;
    for entry in plan {
        let mut taken = 0;
        for produced in &entry.groups {
            let target = if entry.victims.len() == 1 {
                entry.victims.iter().next().expect("a single owner").clone()
            } else {
                let present: std::collections::BTreeSet<PlayerId> = state
                    .system_state(system)
                    .on_planet(&entry.planet)
                    .iter()
                    .filter(|unit| &unit.owner != invader)
                    .map(|unit| unit.owner.clone())
                    .collect();
                let Some(question) =
                    bombardment_target_question(invader, &entry.planet, *produced, &present)
                else {
                    // 7.2: nobody left with units takes the remaining hits.
                    break;
                };
                PlayerId::new(table.ask(&question)?.id)
            };
            taken += take_bombard_hits(state, system, &entry.planet, &target, *produced);
        }
        killed += taken;
        // Make an Example of Their World asks for the last ground force on a planet and asks
        // for it during this step: counted here rather than after the invasion, because
        // ground combat clears planets too and the empty planet afterwards does not say
        // which step emptied it.
        if taken == entry.held
            && entry.victims.len() == 1
            && state
                .system_state(system)
                .on_planet(&entry.planet)
                .iter()
                .all(|unit| &unit.owner == invader)
        {
            state.record_event_feat(invader, Feat::BombardedOutTheLastGroundForces, occurrence);
            noted = true;
        }
    }
    Ok((killed, noted))
}

/// Ground forces this player has in the system's space area, available to land.
#[must_use]
pub fn landable(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> Vec<Unit> {
    let types = catalogue(content, sources);
    state
        .system_state(system)
        .units_of(player)
        .into_iter()
        .filter(|unit| {
            types
                .get(unit.type_id.as_str())
                .is_some_and(UnitType::is_ground_force)
        })
        .cloned()
        .collect()
}

/// The planets of `system` that a ground force may land on right now.
///
/// 27.1 keeps Mecatol Rex off the table while the custodians token sits there; everything else
/// in the system is landable.
fn landable_planets(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
) -> Vec<PlanetId> {
    // F-M08-019-1 (C1): iterate the active system record's own `planets` array — canonical
    // order. Membership is identical to `galaxy::planets_in` for every system in this corpus;
    // only the file layout of planets.json differs, and choice-option order must not follow it.
    let Some(record) = content.get(
        ti4_model::content_types::ContentType::Systems,
        system.as_str(),
    ) else {
        return Vec::new();
    };
    record
        .strings("planets")
        .into_iter()
        .map(ToOwned::to_owned)
        // Planets placed onto this tile during play (Mirage). They are not in the system record --
        // that is what "placed during play" means -- so without this an invasion cannot reach one.
        // Appended rather than merged, which keeps the record's canonical order for the printed
        // planets and puts the arrival last, where it arrived.
        .chain(
            state
                .placed_planets
                .iter()
                .filter(|(_, went)| *went == system)
                .map(|(planet, _)| planet.to_string()),
        )
        .filter_map(|name| {
            let name = name.as_str();
            // Scope filter mirrors planets_in: a planet outside the active source set is not on
            // this board. (Every system's planets come from one source, so this either keeps or
            // drops the whole array — never a subset.)
            content
                .get(ti4_model::content_types::ContentType::Planets, name)
                .filter(|planet| planet.in_sources(sources))
                .map(|_| PlanetId::new(name))
        })
        .filter(|planet| planet.as_str() != "mr" || state.custodians_removed)
        // Space stations rule 5: "Structures and ground forces cannot be committed to or placed on
        // a space station." They are listed in the system's `planets` array because they carry a
        // planet card, so without this they were offered to every invasion like any other planet --
        // and taking one was worth a planet *and* a system, since three of the four sit on tiles
        // whose only other planet is real and the fourth has none. 6.2% of measured opening
        // clearances depended on it. See `plans/evidence/SPACE_STATIONS_AUDIT.md`.
        .filter(|planet| !ti4_content::galaxy::is_space_station(content, planet.as_str(), sources))
        // Demilitarized Zone: units cannot land on the elected planet.
        .filter(|planet| !crate::laws::planet_is_demilitarized(state, planet))
        .collect()
}

/// One option per *distinguishable* landing — unit type, sustained damage and planet — plus the
/// terminator. Two identical undamaged infantry are one move written twice, not a choice; a
/// damaged copy of the same type is its own options.
fn commit_options(troops: &[Unit], planets: &[PlanetId]) -> Vec<ChoiceOption> {
    let mut seen = std::collections::BTreeSet::new();
    let mut options = Vec::new();
    for (index, unit) in troops.iter().enumerate() {
        for planet in planets {
            if !seen.insert((
                unit.type_id.to_string(),
                unit.sustained_damage,
                planet.to_string(),
            )) {
                continue;
            }
            let mut label = format!("land {}", unit.type_id);
            if unit.sustained_damage {
                label.push_str(" (damaged)");
            }
            options.push(
                ChoiceOption::labelled(
                    format!("commit|{index}|{planet}"),
                    COMMIT_KIND,
                    format!("{label} on {planet}"),
                )
                .with("planet", planet.to_string())
                .with("unit", unit.type_id.to_string()),
            );
        }
    }
    options.push(ChoiceOption::labelled(
        "done_committing",
        DECLINE_KIND,
        "commit no more ground forces",
    ));
    options
}

/// 49.2: commit ground forces from space onto planets, one at a time.
///
/// # Errors
/// [`IllegalChoice`] when a decider answers with something not offered.
#[allow(
    clippy::too_many_arguments,
    reason = "one parameter per distinct input"
)]
pub fn commit_ground_forces(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut Table,
    invader: &PlayerId,
    system: &SystemId,
) -> Result<Vec<PlanetId>, IllegalChoice> {
    if crate::planets::in_system(state, content, sources, system).is_empty() {
        return Ok(Vec::new());
    }

    let mut committed: std::collections::BTreeSet<PlanetId> = std::collections::BTreeSet::new();
    loop {
        let troops = landable(state, content, sources, invader, system);
        if troops.is_empty() {
            break;
        }

        // Re-read each iteration: the custodians token can come down mid-sequence and open
        // Mecatol Rex, exactly as in the oracle.
        let planets = landable_planets(state, content, sources, system);
        let options = commit_options(&troops, &planets);

        let choice = Choice::new(
            invader.clone(),
            format!("commit ground forces in {system}"),
            options,
        );
        let answer = table.ask_seeing(&choice, &Observed::new(state, content, sources, None))?;
        if answer.is_decline() {
            break;
        }
        let mut parts = answer.id.splitn(3, '|');
        let (_, index, planet) = (parts.next(), parts.next(), parts.next());
        let (Some(index), Some(planet)) = (
            index.and_then(|i| i.parse::<usize>().ok()),
            planet.map(PlanetId::new),
        ) else {
            break;
        };
        let Some(unit) = troops.get(index).cloned() else {
            break;
        };
        state.system_mut(system).remove(std::slice::from_ref(&unit));
        state
            .system_mut(system)
            .planet_units
            .entry(planet.clone())
            .or_default()
            .push(unit);
        committed.insert(planet);
    }
    Ok(committed.into_iter().collect())
}

/// Roll one side's ground forces on a planet (42.1).
#[allow(
    clippy::too_many_arguments,
    reason = "one parameter per distinct input"
)]
fn roll_ground(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    dice: &mut Dice,
    rng: &mut GameRng,
    player: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) -> usize {
    let types = catalogue(content, sources);
    // Ground rolls group dice by combat value, so each staged entry is one value's pool.
    // The pool's unit types ride along too: destruction rules that name "the units that
    // rerolled" need to know what sat behind each value's dice.
    let mut fighting: std::collections::BTreeMap<
        i64,
        (i64, std::collections::BTreeMap<String, u32>),
    > = std::collections::BTreeMap::new();
    for unit in state.system_state(system).on_planet_of(planet, player) {
        let Some(kind) = types.get(unit.type_id.as_str()) else {
            continue;
        };
        let Some(value) =
            ground_combat_value(state, content, sources, player, system, planet, unit.type_id.as_str())
        else {
            continue;
        };
        let _ = kind;
        let slot = fighting.entry(value).or_insert((0, std::collections::BTreeMap::new()));
        slot.0 += kind.combat_dice();
        *slot.1.entry(unit.type_id.to_string()).or_insert(0) += 1;
    }
    let mut set = ti4_model::state::RerollSet {
        kind: "ground".into(),
        system: system.clone(),
        rolls: Vec::new(),
    };
    let mut hits = 0;
    for (value, (count, unit_types)) in fighting {
        let dice_count = usize::try_from(count).unwrap_or(0);
        if dice_count == 0 {
            continue;
        }
        let roll = dice.roll(
            rng,
            dice_count,
            "ground combat",
            Some(u32::try_from(value).unwrap_or(u32::MAX)),
        );
        hits += roll.hits();
        set.rolls.push(ti4_model::state::RerollEntry {
            unit: format!("combat value {value}"),
            planet: Some(planet.clone()),
            hits_on: Some(u32::try_from(value).unwrap_or(u32::MAX)),
            faces: roll.faces,
            rerolled: std::collections::BTreeSet::new(),
            deltas: std::collections::BTreeMap::new(),
            unit_types,
        });
    }
    if set.rolls.iter().any(|roll| !roll.faces.is_empty()) {
        state.reroll_staging.insert(player.clone(), set);
        state.last_reroll_player = Some(player.clone());
    }
    hits
}

/// Remove `hits` of one player's ground forces from a planet, the owner choosing.
fn absorb_ground(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut Table,
    player: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
    hits: usize,
) -> Result<(), IllegalChoice> {
    let types = catalogue(content, sources);
    for _ in 0..hits {
        // LRR 42: only ground forces take hits in a ground combat; structures survive the
        // fight and die when control changes hands instead (KD-2).
        let present: Vec<Unit> = state
            .system_state(system)
            .on_planet_of(planet, player)
            .into_iter()
            .filter(|unit| {
                types
                    .get(unit.type_id.as_str())
                    .is_some_and(UnitType::is_ground_force)
            })
            .cloned()
            .collect();
        if present.is_empty() {
            return Ok(()); // 15.2a
        }
        let doomed = if let [only] = present.as_slice() {
            only.clone()
        } else {
            let mut seen = std::collections::BTreeSet::new();
            let mut options = Vec::new();
            for (index, unit) in present.iter().enumerate() {
                if !seen.insert((unit.type_id.to_string(), unit.sustained_damage)) {
                    continue;
                }
                options.push(ChoiceOption::labelled(
                    format!("destroy|{index}"),
                    GROUND_CASUALTY_KIND,
                    format!("destroy {}", unit.type_id),
                ));
            }
            let choice = Choice::new(player.clone(), format!("assign a hit on {planet}"), options);
            let answer =
                table.ask_seeing(&choice, &Observed::new(state, content, sources, None))?;
            let index = answer
                .id
                .strip_prefix("destroy|")
                .and_then(|rest| rest.parse::<usize>().ok())
                .unwrap_or(0);
            present.get(index).unwrap_or(&present[0]).clone()
        };
        state
            .system_mut(system)
            .remove_from_planet(planet, std::slice::from_ref(&doomed));
    }
    Ok(())
}

/// Fight a ground combat on one planet (42).
///
/// # Errors
/// [`IllegalChoice`] when a decider answers with something not offered.
#[allow(
    clippy::too_many_arguments,
    reason = "one parameter per distinct input"
)]
pub fn ground_combat(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut Table,
    dice: &mut Dice,
    rng: &mut GameRng,
    system: &SystemId,
    planet: &PlanetId,
    invader: &PlayerId,
) -> Result<Option<PlayerId>, IllegalChoice> {
    // LRR 42: only rival ground forces make this a fight; structures do not (KD-2).
    let defender = ground_force_owners(state, content, sources, system, planet)
        .into_iter()
        .find(|owner| owner != invader);
    let Some(defender) = defender else {
        return Ok(Some(invader.clone()));
    };

    for _ in 1..=MAX_ROUNDS {
        state.combat_round_seq = state.combat_round_seq.saturating_add(1);
        // 42.3: the fight ends when one side has no ground forces left — structures never
        // fight (KD-2).
        let owners = ground_force_owners(state, content, sources, system, planet);
        if !owners.contains(invader) || !owners.contains(&defender) {
            break;
        }

        let attacker_hits =
            roll_ground(state, content, sources, dice, rng, invader, system, planet);
        let defender_hits = roll_ground(
            state, content, sources, dice, rng, &defender, system, planet,
        );
        // 42.2: simultaneous, as in space.
        absorb_ground(
            state,
            content,
            sources,
            table,
            &defender,
            system,
            planet,
            attacker_hits,
        )?;
        absorb_ground(
            state,
            content,
            sources,
            table,
            invader,
            system,
            planet,
            defender_hits,
        )?;

        // L1Z1X's Harrow bombards again at the end of each round. The hits are assigned here
        // rather than by the faction layer, because who loses a unit is the invasion's decision.
        //
        // 63.2: "The Planetary Shield ability prevents an L1Z1X player from using their Harrow
        // faction ability." Harrow *is* a bombardment, so it answers to the same gate the printed
        // BOMBARDMENT does -- including the war sun exemption and Disable, which is the reason to
        // ask `can_bombard` rather than to re-test the shield here.
        let harrow = if bombardable(state, content, sources, system, planet, invader) {
            crate::faction_abilities::ground_combat_round_ended(
                state, content, sources, dice, rng, invader, system,
            )
        } else {
            0
        };
        if harrow > 0 {
            absorb_ground(
                state, content, sources, table, &defender, system, planet, harrow,
            )?;
        }
    }

    let invader_left = !state
        .system_state(system)
        .on_planet_of(planet, invader)
        .is_empty();
    Ok(invader_left.then(|| invader.clone()))
}

/// 49.5: whoever has ground forces left takes the planet.
///
/// Two details that are easy to lose and both change play:
///
/// * **49.5d** — if every committed force died, the previous holder keeps the planet. Control
///   does not fall to the invader by default.
/// * A captured planet is taken **exhausted**. Its resources and influence belong to the round
///   after the one you spent conquering it; without this a planet could be spent the same turn
///   it was invaded.
pub fn establish_control(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
    invader: &PlayerId,
    committed: &[PlanetId],
) -> Vec<(PlanetId, Option<PlayerId>)> {
    let types = catalogue(content, sources);
    let mut captured = Vec::new();
    for planet in committed {
        let holds = state
            .system_state(system)
            .on_planet_of(planet, invader)
            .into_iter()
            .any(|unit| {
                types
                    .get(unit.type_id.as_str())
                    .is_some_and(UnitType::is_ground_force)
            });
        if !holds {
            continue; // 49.5d
        }
        let previous = state
            .system_state(system)
            .planet_control
            .get(planet)
            .cloned();
        if previous.as_ref() == Some(invader) {
            continue; // 49.5c
        }
        state
            .system_mut(system)
            .set_control(planet.clone(), invader.clone());
        state.exhaust_planet(planet.clone());
        // Everything that reads "when you gain control of a planet" fires here, where control
        // actually changes hands: the L1Z1X breakthrough and the Minister of Exploration.
        crate::breakthroughs::on_gain_control(state, content, sources, invader, system, planet);
        crate::laws::on_gain_control(state, invader);
        // The Crown of Emphidia changes hands when a planet in its owner's home system is taken.
        if crate::laws::owner_home_system(state, content, "crown_of_emphidia").as_ref()
            == Some(system)
        {
            crate::laws::steal_throne_card(state, "crown_of_emphidia", invader);
        }
        captured.push((planet.clone(), previous));
    }
    captured
}

/// Where an open invasion has reached.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Stage {
    /// The invader's bombarding ships have rolled (49.1); their hits are being assigned, in
    /// roll order, before any other invasion decision. Pauses on coexisting planets for the
    /// per-unit target choice (coexistence 7, 7.1).
    Bombarding,
    /// Mid-bombardment on the planet at `planet_index` of the window's bombardment plan:
    /// `groups` holds each bombarding unit's hit total, `next_group` the next unit to be
    /// assigned, `taken` the units destroyed on the planet so far.
    ChoosingBombardment {
        planet_index: usize,
        planet: PlanetId,
        groups: Vec<usize>,
        next_group: usize,
        taken: usize,
        held: usize,
        victims: std::collections::BTreeSet<PlayerId>,
        occurrence: FeatOccurrence,
    },
    /// Offering to lift the custodians token from Mecatol Rex (27.2).
    Custodians,
    /// Choosing which ground forces to land, and where (49.2).
    Committing,
    /// Fighting on `planets[index]`, having already resolved the earlier ones.
    Fighting {
        planets: Vec<PlanetId>,
        index: usize,
        defender: PlayerId,
    },
    /// A combat ended and any scoring window must close before the next planet/control step.
    Advancing {
        planets: Vec<PlanetId>,
        index: usize,
    },
    /// A coexistence combat ended and the winner may start another against the next coexister
    /// (coexistence 12). Declining leaves the rest coexisting.
    ChoosingNextCombat {
        planets: Vec<PlanetId>,
        index: usize,
        planet: PlanetId,
        remaining: Vec<PlayerId>,
    },
    /// A planet changed hands and its former controller's scoring window must close before
    /// gain-control effects and exploration continue.
    FinalizingControl {
        planets: Vec<PlanetId>,
        index: usize,
        planet: PlanetId,
        previous: Option<PlayerId>,
    },
    Done,
}

/// [`ground_force_owners`], reachable from a sibling module's tests.
///
/// The coexistence tests need the same answer the fight uses; exposing it is cheaper than
/// duplicating the unit-type filter and letting the copy drift from the original.
#[doc(hidden)]
#[must_use]
pub fn ground_force_owners_for_test(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
    planet: &PlanetId,
) -> std::collections::BTreeSet<PlayerId> {
    ground_force_owners(state, content, sources, system, planet)
}

/// LRR 49/42: who has ground forces on `planet`.
///
/// Only ground forces make a planet contested and can be casualties of a ground combat.
/// Structures roll no dice; a planet holding only rival structures falls without resistance,
/// and its structures are destroyed when control changes hands (KD-2). The Titans' PDS is the
/// one structure that is also a ground force, so it fights like infantry here.
fn ground_force_owners(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
    planet: &PlanetId,
) -> std::collections::BTreeSet<PlayerId> {
    let types = catalogue(content, sources);
    state
        .system_state(system)
        .on_planet(planet)
        .iter()
        .filter(|unit| {
            types
                .get(unit.type_id.as_str())
                .is_some_and(UnitType::is_ground_force)
        })
        .map(|unit| unit.owner.clone())
        .collect()
}

/// An invasion, resolvable one decision at a time (LRR 49).
///
/// Bombardment is rolled when the window opens: 49.1 puts it before ground forces are
/// committed, so deferring even its rolls would let a player commit knowing what a
/// bombardment they had not yet suffered was going to do. The rolled hits are then assigned
/// inside the settle loop, pausing on coexisting planets for the invader's per-unit target
/// choices (coexistence 7, 7.1).
#[derive(Debug, Clone)]
pub struct InvasionWindow {
    invader: PlayerId,
    system: SystemId,
    stage: Stage,
    report: InvasionReport,
    pending_scoring_occurrences: std::collections::VecDeque<(FeatOccurrence, bool)>,
    current_ground_occurrence: Option<FeatOccurrence>,
    notes_at_tactical_start: crate::combat::NoteHoldings,
    /// The bombardment rolled when the window opened (49.1), awaiting assignment.
    bombard_plan: Vec<BombardPlan>,
    /// Next entry of `bombard_plan` to assign or finish.
    bombard_index: usize,
    bombard_occurrence: FeatOccurrence,
    /// The bombardment's reroll windows (Agnlan Oln, Scramble Frequency) have been opened,
    /// so the hits were already recomputed from the possibly rerolled dice.
    bombard_announced: bool,
}

impl InvasionWindow {
    /// Open an invasion, rolling its bombardment immediately (49.1).
    #[must_use]
    pub fn new(
        state: &mut GameState,
        content: &ContentStore,
        sources: SourceSet,
        dice: &mut Dice,
        rng: &mut GameRng,
        invader: &PlayerId,
        system: &SystemId,
    ) -> Self {
        let notes = crate::combat::note_holdings(state);
        Self::new_with_notes(state, content, sources, dice, rng, invader, system, notes)
    }

    /// Open an invasion with note holdings captured at the start of its tactical action.
    #[must_use]
    pub fn new_with_notes(
        state: &mut GameState,
        content: &ContentStore,
        sources: SourceSet,
        dice: &mut Dice,
        rng: &mut GameRng,
        invader: &PlayerId,
        system: &SystemId,
        notes_at_tactical_start: crate::combat::NoteHoldings,
    ) -> Self {
        let occurrence = state.begin_feat_occurrence();
        // A stale set left by the synchronous APIs can only name rolls that are not this
        // bombardment's, so the window would react to the wrong dice; the roll below
        // restages the invader's own rolls.
        state.reroll_staging.clear();
        state.last_reroll_player = None;
        let bombard_plan = roll_bombard_plan(state, content, sources, dice, rng, system, invader);
        Self {
            invader: invader.clone(),
            system: system.clone(),
            stage: Stage::Bombarding,
            report: InvasionReport::default(),
            pending_scoring_occurrences: std::collections::VecDeque::new(),
            current_ground_occurrence: None,
            notes_at_tactical_start,
            bombard_plan,
            bombard_index: 0,
            bombard_occurrence: occurrence,
            bombard_announced: false,
        }
    }

    #[must_use]
    pub fn take_scoring_occurrence(&mut self) -> Option<(FeatOccurrence, bool)> {
        self.pending_scoring_occurrences.pop_front()
    }

    /// Advance the bombardment as far as it can without asking anyone: assign the hits on
    /// single-owner planets inline, and pause (stopping the loop) on the first planet that
    /// needs a per-unit target choice.
    fn step_bombardment(&mut self, state: &mut GameState) {
        loop {
            let Some(entry) = self.bombard_plan.get(self.bombard_index) else {
                self.stage = Stage::Custodians;
                return;
            };
            if entry.groups.is_empty() {
                self.bombard_index += 1;
                continue;
            }
            // Lift the entry's data out before touching self mutably again.
            let (planet, groups, held, victims) = (
                entry.planet.clone(),
                entry.groups.clone(),
                entry.held,
                entry.victims.clone(),
            );
            if victims.len() == 1 {
                let target = victims.iter().next().expect("a single owner");
                let mut taken = 0;
                for produced in &groups {
                    taken += take_bombard_hits(state, &self.system, &planet, target, *produced);
                }
                self.report.bombardment_kills += taken;
                self.complete_bombard_plan(
                    state,
                    &planet,
                    taken,
                    held,
                    &victims,
                    self.bombard_occurrence,
                );
                self.bombard_index += 1;
                continue;
            }
            // Coexistence 7, 7.1: the invader picks, per bombarding unit, whose units take the
            // hits. The window pauses here until the driver's decider answers.
            self.stage = Stage::ChoosingBombardment {
                planet_index: self.bombard_index,
                planet,
                groups,
                next_group: 0,
                taken: 0,
                held,
                victims,
                occurrence: self.bombard_occurrence,
            };
            return;
        }
    }

    /// Finish a planet's bombardment: if the step destroyed its last ground force, fire the
    /// feat (counted during the step -- later ground combat clears planets too, and the empty
    /// planet afterwards does not say which step emptied it).
    fn complete_bombard_plan(
        &mut self,
        state: &mut GameState,
        planet: &PlanetId,
        taken: usize,
        held: usize,
        victims: &std::collections::BTreeSet<PlayerId>,
        occurrence: FeatOccurrence,
    ) {
        if taken == held
            && victims.len() == 1
            && state
                .system_state(&self.system)
                .on_planet(planet)
                .iter()
                .all(|unit| unit.owner == self.invader)
        {
            state.record_event_feat(
                &self.invader,
                Feat::BombardedOutTheLastGroundForces,
                occurrence,
            );
            self.pending_scoring_occurrences
                .push_back((occurrence, false));
        }
    }

    /// The one-shot windows after the bombardment rolls: Aglnlan Oln rerolls first, then the
    /// other players' Scramble Frequency. Every planet's groups are then recomputed from the
    /// possibly rerolled dice — same order, same zero-skip, as the plan was built — and the
    /// staging is spent.
    fn announce_bombard_rerolls(&mut self, state: &mut GameState, ctx: &mut Resolving<'_>) {
        if !state.reroll_staging.contains_key(&self.invader) {
            return;
        }
        crate::combat::open_reroll_windows(state, ctx, &self.invader);
        if let Some(set) = state.reroll_staging.get(&self.invader).cloned() {
            for entry in &mut self.bombard_plan {
                entry.groups = set
                    .rolls
                    .iter()
                    .filter(|roll| roll.planet.as_ref() == Some(&entry.planet))
                    .map(ti4_model::state::RerollEntry::hits)
                    .filter(|hits| *hits > 0)
                    .collect();
            }
        }
        state.reroll_staging.remove(&self.invader);
        state.last_reroll_player = None;
    }

    pub fn settle(&mut self, state: &mut GameState, ctx: &mut Resolving<'_>) {
        loop {
            match self.stage.clone() {
                Stage::Bombarding => {
                    if !self.bombard_announced {
                        self.bombard_announced = true;
                        self.announce_bombard_rerolls(state, ctx);
                    }
                    self.step_bombardment(state);
                    // The stage moved on (a later bombardment step, the custodians stage, or
                    // the coexistence pause); re-match it.
                }
                Stage::ChoosingBombardment {
                    planet_index,
                    planet,
                    taken,
                    held,
                    victims,
                    occurrence,
                    ..
                } => {
                    // If the planet's ground forces are already gone, the remaining hits have
                    // no target (coexistence 7.2) and the planet finishes without asking.
                    let present: std::collections::BTreeSet<PlayerId> = state
                        .system_state(&self.system)
                        .on_planet(&planet)
                        .iter()
                        .filter(|unit| unit.owner != self.invader)
                        .map(|unit| unit.owner.clone())
                        .collect();
                    if present.is_empty() {
                        self.complete_bombard_plan(
                            state, &planet, taken, held, &victims, occurrence,
                        );
                        self.bombard_index = planet_index + 1;
                        self.stage = Stage::Bombarding;
                    } else {
                        // Wait for the invader's choice (the driver presents pending_choice).
                        return;
                    }
                }
                Stage::Advancing { planets, index } => {
                    self.advance_fighting(state, ctx, &planets, index);
                    return;
                }
                Stage::ChoosingNextCombat {
                    planets,
                    index,
                    remaining,
                    ..
                } if remaining.is_empty() => {
                    // Nobody left to fight: coexistence 12's "until there are no more coexisting
                    // players" arm, reached without asking.
                    self.stage = Stage::Advancing {
                        planets,
                        index: index + 1,
                    };
                }
                Stage::FinalizingControl {
                    planets,
                    index,
                    planet,
                    previous,
                } => {
                    self.finish_control_gain(state, ctx, &planet, previous.as_ref());
                    self.advance_control(state, ctx, &planets, index);
                    return;
                }
                Stage::Custodians
                    if !custodians_removable(
                        state,
                        ctx.content,
                        ctx.sources,
                        &self.invader,
                        &self.system,
                    ) =>
                {
                    self.stage = Stage::Committing;
                }
                Stage::Committing
                    if self
                        .landing_options(state, ctx.content, ctx.sources)
                        .is_empty() =>
                {
                    let planets = self.report.committed.clone();
                    if planets.is_empty() {
                        self.stage = Stage::Done;
                    } else {
                        self.advance_fighting(state, ctx, &planets, 0);
                    }
                    return;
                }
                _ => return,
            }
        }
    }

    #[must_use]
    pub const fn is_done(&self) -> bool {
        matches!(self.stage, Stage::Done)
    }

    /// What the invasion did.
    #[must_use]
    pub fn into_report(self) -> InvasionReport {
        self.report
    }

    /// Ground forces still in the space area, and the planets they could land on.
    fn landing_options(
        &self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
    ) -> Vec<ChoiceOption> {
        let troops = landable(state, content, sources, &self.invader, &self.system);
        if troops.is_empty() {
            return Vec::new();
        }
        let planets = landable_planets(state, content, sources, &self.system);
        commit_options(&troops, &planets)
    }

    /// The commit-ground-forces ask, or `None` when there is nothing left to land.
    fn committing_choice(
        &self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
    ) -> Option<Choice> {
        let options = self.landing_options(state, content, sources);
        if options.is_empty() {
            return None;
        }
        Some(Choice::new(
            self.invader.clone(),
            format!("commit ground forces in {}", self.system),
            options,
        ))
    }

    fn finish_committing(&mut self, state: &mut GameState, ctx: &mut Resolving<'_>) {
        let planets = self.report.committed.clone();
        if planets.is_empty() {
            self.stage = Stage::Done;
        } else {
            self.advance_fighting(state, ctx, &planets, 0);
        }
    }

    /// The event a Fire Team hooks: named by the roller, so the window is `actor_is`.
    fn emit_ground_rolls_made(
        &self,
        state: &mut GameState,
        ctx: &mut Resolving<'_>,
        planet: &PlanetId,
        player: &PlayerId,
        hits: usize,
    ) {
        let mut payload = std::collections::BTreeMap::new();
        payload.insert("system".to_owned(), self.system.to_string().into());
        payload.insert("planet".to_owned(), planet.to_string().into());
        payload.insert("player".to_owned(), player.to_string().into());
        payload.insert("hits".to_owned(), i64::try_from(hits).unwrap_or(0).into());
        let _ = ctx.emit(state, "GROUND_ROLLS_MADE", payload);
    }

    /// The hits the staged dice now show after the reroll windows at the roll site — else the
    /// originals. The caller clears the staging after using these.
    fn rerolled_ground_hits(
        state: &GameState,
        invader: &PlayerId,
        invader_hits: usize,
        defender: &PlayerId,
        defender_hits: usize,
    ) -> (usize, usize) {
        (
            state
                .reroll_staging
                .get(invader)
                .map_or(invader_hits, crate::combat::staged_hits),
            state
                .reroll_staging
                .get(defender)
                .map_or(defender_hits, crate::combat::staged_hits),
        )
    }

    fn resolve_ground_round(
        &mut self,
        state: &mut GameState,
        ctx: &mut Resolving<'_>,
        planets: Vec<PlanetId>,
        index: usize,
        defender: PlayerId,
    ) {
        let (content, sources) = (ctx.content, ctx.sources);
        let planet = planets[index].clone();
        // Letnev's Dunlain Reaper: "DEPLOY: At the start of a round of ground combat, you may spend
        // 2 resources to replace 1 of your infantry in that combat with 1 mech from your
        // reinforcements." Before the dice, so the mech fights the round it arrives for.
        for who in [self.invader.clone(), defender.clone()] {
            dunlain_reaper(state, ctx, &who, &self.system, &planet);
        }
        // 42.2: hits are simultaneous, so both sides roll before either loses anything.
        let attacker_hits = roll_ground(
            state,
            content,
            sources,
            ctx.dice,
            ctx.rng,
            &self.invader,
            &self.system,
            &planet,
        );
        let defender_hits = roll_ground(
            state,
            content,
            sources,
            ctx.dice,
            ctx.rng,
            &defender,
            &self.system,
            &planet,
        );
        state.combat_round_seq = state.combat_round_seq.saturating_add(1);
        // "After your ground forces make combat rolls during a round of ground combat." Emitted
        // between the rolls and the removals, which is what the window means: a card played here
        // acts on the hits before anyone dies of them.
        for (who, hits) in [(&self.invader, attacker_hits), (&defender, defender_hits)] {
            self.emit_ground_rolls_made(state, ctx, &planet, who, hits);
        }
        // Fire Team's window ("reroll any number of your dice") opened at those emits; the
        // hits that remove units are what the possibly rerolled dice now show.
        let (attacker_hits, defender_hits) = Self::rerolled_ground_hits(
            state,
            &self.invader,
            attacker_hits,
            &defender,
            defender_hits,
        );
        state.reroll_staging.clear();
        state.last_reroll_player = None;
        remove_ground(
            state,
            content,
            sources,
            &self.system,
            &planet,
            &defender,
            attacker_hits,
        );
        remove_ground(
            state,
            content,
            sources,
            &self.system,
            &planet,
            &self.invader,
            defender_hits,
        );
        self.finish_ground_round(state, ctx, planets, index, defender, &planet);
    }

    /// 42.3: the fight ends when one side has no ground forces left on the planet — not when
    /// its last structure falls, because structures never fight (KD-2). Both sides surviving
    /// simply starts the next round on the same planet.
    fn finish_ground_round(
        &mut self,
        state: &mut GameState,
        ctx: &mut Resolving<'_>,
        planets: Vec<PlanetId>,
        index: usize,
        defender: PlayerId,
        planet: &PlanetId,
    ) {
        let (content, sources) = (ctx.content, ctx.sources);
        let owners = ground_force_owners(state, content, sources, &self.system, planet);
        let invader_survives = owners.contains(&self.invader);
        let defender_survives = owners.contains(&defender);
        if invader_survives && defender_survives {
            self.stage = Stage::Fighting {
                planets,
                index,
                defender,
            };
            return;
        }

        let winner = if invader_survives {
            self.invader.clone()
        } else {
            defender.clone()
        };
        let loser = if winner == self.invader {
            defender
        } else {
            self.invader.clone()
        };
        let noted = self
            .current_ground_occurrence
            .take()
            .is_some_and(|occurrence| {
                let noted = note_ground_combat_win_feats(
                    state,
                    content,
                    sources,
                    &self.system,
                    &winner,
                    &loser,
                    &self.notes_at_tactical_start,
                    occurrence,
                );
                if noted {
                    self.pending_scoring_occurrences
                        .push_back((occurrence, true));
                }
                noted
            });
        if noted {
            self.stage = Stage::Advancing {
                planets,
                index: index + 1,
            };
        } else {
            self.advance_fighting(state, ctx, &planets, index + 1);
        }
    }

    /// Move to the next planet that still needs a fight, or finish and take control.
    fn advance_fighting(
        &mut self,
        state: &mut GameState,
        ctx: &mut Resolving<'_>,
        planets: &[PlanetId],
        mut index: usize,
    ) {
        while index < planets.len() {
            let planet = &planets[index];
            // LRR 49/42: a fight happens only where the invader landed ground forces and rival
            // ground forces stand. Structures roll no dice and are not ground forces (KD-2): a
            // structure-only planet falls without resistance, and its structures die when
            // control changes hands instead of in a combat that never should have happened.
            let owners = ground_force_owners(state, ctx.content, ctx.sources, &self.system, planet);
            if !owners.contains(&self.invader) {
                index += 1;
                continue;
            }
            // Coexistence 11: a player who had no units here and commits ground forces "must
            // start a ground combat against the player that controls that planet" -- not against
            // whichever rival happens to sort first. Off a coexisting planet there is only one
            // rival anyway, so this ordering is invisible there and load-bearing here.
            let controller = state
                .system_state(&self.system)
                .planet_control
                .get(planet)
                .cloned()
                .filter(|holder| *holder != self.invader && owners.contains(holder));
            let next =
                controller.or_else(|| owners.iter().find(|owner| **owner != self.invader).cloned());
            if let Some(defender) = next {
                self.current_ground_occurrence = Some(state.begin_feat_occurrence());
                self.stage = Stage::Fighting {
                    planets: planets.to_vec(),
                    index,
                    defender,
                };
                return;
            }

            // Coexistence 9 and 12: the invader holds the ground here and coexisters remain. The
            // rule offers *another* combat rather than forcing one, so this is a choice.
            //
            // Reached only after the loop above found no ordinary defender, which is what makes it
            // the "won the combat" moment: a coexister still standing is not a defender, because
            // coexistence is precisely the state where their presence starts no fight.
            let coexisting: Vec<PlayerId> =
                crate::coexistence::coexisters(state, &self.system, planet)
                    .into_iter()
                    .filter(|player| *player != self.invader)
                    .collect();
            if !coexisting.is_empty() {
                self.stage = Stage::ChoosingNextCombat {
                    planets: planets.to_vec(),
                    index,
                    planet: planet.clone(),
                    remaining: coexisting,
                };
                return;
            }
            index += 1;
        }
        let committed = self.report.committed.clone();
        self.advance_control(state, ctx, &committed, 0);
    }

    /// Establish and finish control one planet at a time so a control-loss occurrence is not
    /// delayed past later captures, gain-control effects, or exploration.
    fn advance_control(
        &mut self,
        state: &mut GameState,
        ctx: &mut Resolving<'_>,
        planets: &[PlanetId],
        mut index: usize,
    ) {
        while index < planets.len() {
            let planet = planets[index].clone();
            let Some((planet, previous)) = establish_control(
                state,
                ctx.content,
                ctx.sources,
                &self.system,
                &self.invader,
                std::slice::from_ref(&planet),
            )
            .into_iter()
            .next() else {
                index += 1;
                continue;
            };
            self.report
                .captured
                .push((planet.clone(), previous.clone()));

            let lost_home = previous.as_ref().is_some_and(|holder| {
                state
                    .player(holder)
                    .and_then(|seat| seat.home_system.as_ref())
                    == Some(&self.system)
            });
            if lost_home {
                let holder = previous.as_ref().expect("lost home has a former holder");
                let occurrence = state.begin_feat_occurrence();
                state.record_event_feat(holder, Feat::LostAHomePlanet, occurrence);
                self.pending_scoring_occurrences
                    .push_back((occurrence, false));
                self.stage = Stage::FinalizingControl {
                    planets: planets.to_vec(),
                    index: index + 1,
                    planet,
                    previous,
                };
                return;
            }

            self.finish_control_gain(state, ctx, &planet, previous.as_ref());
            index += 1;
        }
        self.stage = Stage::Done;
    }

    fn finish_control_gain(
        &mut self,
        state: &mut GameState,
        ctx: &mut Resolving<'_>,
        planet: &PlanetId,
        previous: Option<&PlayerId>,
    ) {
        let (content, sources) = (ctx.content, ctx.sources);
        // L1Z1X's Assimilate converts the structures on a planet as it changes hands, before
        // anything else looks at what is standing on it.
        crate::faction_abilities::control_gained(
            state,
            content,
            sources,
            &self.invader,
            &self.system,
            planet,
        );

        // LRR 49: the rival structures left on a captured planet are destroyed as control
        // changes hands — whatever Assimilate did not convert above. No rival ground force can
        // stand here: the invader holds the planet only because every one of them is dead.
        let types = catalogue(content, sources);
        let standing = state
            .system_state(&self.system)
            .planet_units
            .get(planet)
            .cloned()
            .unwrap_or_default();
        let doomed: Vec<Unit> = standing
            .into_iter()
            .filter(|unit| {
                unit.owner != self.invader
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(UnitType::is_structure)
            })
            .collect();
        if !doomed.is_empty() {
            state
                .system_mut(&self.system)
                .remove_from_planet(planet, &doomed);
        }

        // Two printed windows read "when you gain control of a planet", so a capture is
        // announced before the exploration that follows it. The former holder is named for
        // the window that reads "a planet you control": control has already changed by the
        // time the event is read, so only this frame knows who the planet was taken from.
        let mut payload = std::collections::BTreeMap::new();
        payload.insert("player".to_owned(), self.invader.to_string().into());
        payload.insert("planet".to_owned(), planet.to_string().into());
        payload.insert("system".to_owned(), self.system.to_string().into());
        // The window that follows cannot read this event's payload, so the frame names the
        // capture for it: Infiltrate acts on the planet, Reparations on its former holder.
        state.last_control_gained = Some((
            self.system.clone(),
            planet.clone(),
            self.invader.clone(),
            previous.cloned(),
        ));
        if let Some(previous) = previous {
            payload.insert("previous_owner".to_owned(), previous.to_string().into());
        }
        let _ = ctx.emit(state, "PLANET_CONTROL_GAINED", payload);

        // Technology AFTER windows resolve before exploration. Integrated Economy is the first.
        let _ = crate::technology::control_gained(
            state,
            ctx.content,
            ctx.sources,
            None,
            ctx.table,
            &self.invader,
            &self.system,
            planet,
        );

        // 35.1: a planet nobody controlled is explored; one taken off another player is not.
        // Only this frame knows which, which is why `captured` carries the previous holder — a
        // caller told merely that control changed would explore every conquest and draw cards
        // the rules do not give.
        if previous.is_none()
            && let Some(deck) = crate::exploration::trait_of(content, sources, planet)
            && let Some(outcome) =
                crate::exploration::explore_with(state, ctx, &self.invader, &deck, Some(planet))
        {
            self.report.explored.push((planet.clone(), outcome));
        }
    }
}

impl Window for InvasionWindow {
    /// A fresh window starts on its bombardment, and bombardment advances by
    /// [`Self::settle`] rather than by any player choice, so driving one settles before the
    /// first question and after every answer that leaves nothing to ask.
    ///
    /// Drive stops once a scoring occurrence is queued: that pause belongs to the caller (the
    /// driver opens the occurrence's scoring window there), exactly as between its own steps.
    fn drive(
        &mut self,
        state: &mut GameState,
        ctx: &mut Resolving<'_>,
    ) -> Result<(), IllegalChoice> {
        self.settle(state, ctx);
        while self.pending_scoring_occurrences.is_empty() && !self.is_done() {
            if let Some(choice) = self.pending_choice(state, ctx.content, ctx.sources) {
                let answer = ctx.ask_seeing(state, &choice)?;
                self.resolve(state, ctx, answer)?;
            } else {
                self.settle(state, ctx);
            }
        }
        Ok(())
    }

    fn pending_choice(
        &self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
    ) -> Option<Choice> {
        match &self.stage {
            Stage::Done
            | Stage::Bombarding
            | Stage::Advancing { .. }
            | Stage::FinalizingControl { .. } => None,
            Stage::ChoosingBombardment {
                planet,
                groups,
                next_group,
                ..
            } => {
                // Coexistence 7, 7.1, asked once per bombarding unit (7.1), with the choices
                // narrowed to the players still holding units on the planet (7.2 makes the
                // rest of a unit's hits waste if its chosen target runs out).
                let Some(next) = (*next_group..groups.len()).find(|&i| groups[i] > 0) else {
                    // settle() finishes the planet; present nothing so the driver settles it.
                    return None;
                };
                let present: std::collections::BTreeSet<PlayerId> = state
                    .system_state(&self.system)
                    .on_planet(planet)
                    .iter()
                    .filter(|unit| unit.owner != self.invader)
                    .map(|unit| unit.owner.clone())
                    .collect();
                bombardment_target_question(&self.invader, planet, groups[next], &present)
            }
            Stage::ChoosingNextCombat {
                planet, remaining, ..
            } => {
                let next = remaining.first()?;
                Some(Choice::new(
                    self.invader.clone(),
                    format!("start another ground combat on {planet}"),
                    vec![
                        ChoiceOption::labelled(
                            format!("fight|{next}"),
                            GROUND_CASUALTY_KIND,
                            format!("fight {next} on {planet}"),
                        ),
                        ChoiceOption::labelled(
                            "decline",
                            crate::choice::DECLINE_KIND,
                            format!("leave {next} coexisting on {planet}"),
                        ),
                    ],
                ))
            }
            Stage::Custodians => {
                // Falls through rather than returning None: the driver stops the moment a window
                // has no choice, so a stage that is merely inapplicable would end the invasion
                // before any ground force was committed.
                if !custodians_removable(state, content, sources, &self.invader, &self.system) {
                    return self.committing_choice(state, content, sources);
                }
                Some(Choice::new(
                    self.invader.clone(),
                    format!("spend {CUSTODIANS_COST} influence to remove the custodians token"),
                    vec![
                        ChoiceOption::labelled("no", "decline", "leave it"),
                        ChoiceOption::labelled(
                            "yes",
                            "custodians",
                            "remove it for a victory point",
                        ),
                    ],
                ))
            }
            Stage::Committing => self.committing_choice(state, content, sources),
            Stage::Fighting {
                planets,
                index,
                defender,
            } => {
                // One casualty decision at a time; the roll itself happens on resolve.
                let planet = planets.get(*index)?;
                let _ = defender;
                // Unreachable in practice — resolve_ground_round leaves the stage the moment a
                // side has no ground forces left — but guard on the same predicate as the fight.
                if !ground_force_owners(state, content, sources, &self.system, planet)
                    .contains(&self.invader)
                {
                    return None;
                }
                Some(Choice::new(
                    self.invader.clone(),
                    format!("fight a round on {planet}"),
                    vec![ChoiceOption::labelled(
                        "fight",
                        GROUND_CASUALTY_KIND,
                        format!("fight a round on {planet}"),
                    )],
                ))
            }
        }
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one arm per invasion stage, read as a table; splitting them hides that each                   arm's job is to fall through to the next stage"
    )]
    fn resolve(
        &mut self,
        state: &mut GameState,
        ctx: &mut Resolving<'_>,
        answer: ChoiceOption,
    ) -> Result<(), IllegalChoice> {
        let (content, sources) = (ctx.content, ctx.sources);
        let Some(choice) = self.pending_choice(state, content, sources) else {
            return Ok(());
        };
        let option = crate::choice::validate(&choice, answer)?;

        match self.stage.clone() {
            Stage::Done
            | Stage::Bombarding
            | Stage::Advancing { .. }
            | Stage::FinalizingControl { .. } => {}
            Stage::ChoosingBombardment {
                planet_index,
                planet,
                groups,
                next_group,
                taken,
                held,
                victims,
                occurrence,
            } => {
                // Coexistence 7, 7.1: apply the chosen target to this bombarding unit's hits.
                // 7.2 caps the destruction at the chosen player's remaining units.
                let next = (next_group..groups.len())
                    .find(|&i| groups[i] > 0)
                    .expect("paused on a bombardment that had hits to assign");
                let target = PlayerId::new(option.id);
                let applied =
                    take_bombard_hits(state, &self.system, &planet, &target, groups[next]);
                self.report.bombardment_kills += applied;
                let taken = taken + applied;
                let exhausted = next + 1 >= groups.len()
                    || state
                        .system_state(&self.system)
                        .on_planet(&planet)
                        .iter()
                        .find(|unit| unit.owner != self.invader)
                        .is_none();
                if exhausted {
                    self.complete_bombard_plan(state, &planet, taken, held, &victims, occurrence);
                    self.bombard_index = planet_index + 1;
                    self.stage = Stage::Bombarding;
                } else {
                    self.stage = Stage::ChoosingBombardment {
                        planet_index,
                        planet,
                        groups,
                        next_group: next + 1,
                        taken,
                        held,
                        victims,
                        occurrence,
                    };
                }
            }
            Stage::ChoosingNextCombat {
                planets,
                index,
                planet,
                mut remaining,
            } => {
                // Coexistence 12: the winner "may start an additional ground combat against
                // another coexisting player … until they decline to start another ground combat or
                // there are no more coexisting players. Any coexisting players that they do not
                // resolve a ground combat against remain coexisting."
                //
                // Declining therefore ends the chain outright rather than skipping one opponent:
                // the rule offers *another* combat, not a queue to work through.
                // Declining and having nobody left are the same outcome: the chain ends and the
                // planet is done. Rule 12 phrases them as two ways to stop, not two results.
                if option.is_decline() || remaining.is_empty() {
                    self.stage = Stage::Advancing {
                        planets,
                        index: index + 1,
                    };
                } else {
                    let defender = remaining.remove(0);
                    // The chosen coexister stops coexisting for the duration: they are now a
                    // side in a combat, and `ground_force_owners` already sees their units.
                    state
                        .system_mut(&self.system)
                        .coexisting
                        .entry(planet.clone())
                        .or_default()
                        .remove(&defender);
                    self.current_ground_occurrence = Some(state.begin_feat_occurrence());
                    self.stage = Stage::Fighting {
                        planets,
                        index,
                        defender,
                    };
                }
            }
            Stage::Custodians
                if !custodians_removable(state, content, sources, &self.invader, &self.system) =>
            {
                // The ask that was actually answered was the commit one, reached by fall-through.
                self.stage = Stage::Committing;
                return self.resolve(state, ctx, option);
            }
            Stage::Custodians => {
                if !option.is_decline() {
                    // 27.3: pay six influence, take the token, gain a victory point.
                    if crate::production::pay(
                        state,
                        content,
                        sources,
                        ctx.table,
                        &self.invader,
                        CUSTODIANS_COST,
                        crate::production::Spend::Influence,
                    )? {
                        state.custodians_removed = true;
                        if let Some(seat) = state.player_mut(&self.invader) {
                            seat.victory_points =
                                (seat.victory_points + 1).min(crate::objectives::VICTORY_TARGET);
                        }
                        self.report.custodians_removed = true;
                    }
                }
                self.stage = Stage::Committing;
            }
            Stage::Committing => {
                if option.is_decline() {
                    self.finish_committing(state, ctx);
                } else if let Some(rest) = option.id.strip_prefix("commit|") {
                    let mut parts = rest.splitn(2, '|');
                    let (Some(index), Some(planet)) = (
                        parts.next().and_then(|i| i.parse::<usize>().ok()),
                        parts.next().map(PlanetId::new),
                    ) else {
                        return Ok(());
                    };
                    let troops = landable(state, content, sources, &self.invader, &self.system);
                    if let Some(unit) = troops.get(index).cloned() {
                        // Parley reads the landing back through this marker: the emission's
                        // AFTER window resolves before the commit step continues, and the
                        // marker is the one fact the effect can trust.
                        state.last_committed_unit = Some((
                            self.invader.clone(),
                            self.system.clone(),
                            planet.clone(),
                            unit.clone(),
                        ));
                        state
                            .system_mut(&self.system)
                            .remove(std::slice::from_ref(&unit));
                        state
                            .system_mut(&self.system)
                            .planet_units
                            .entry(planet.clone())
                            .or_default()
                            .push(unit);
                        // "After another player commits units to land on a planet you control."
                        // Emitted per landing, carrying the controller so `actor_is_not` can pick
                        // out the player whose planet it is.
                        let controller = state
                            .system_state(&self.system)
                            .planet_control
                            .get(&planet)
                            .cloned();
                        let mut payload = std::collections::BTreeMap::new();
                        payload.insert("system".to_owned(), self.system.to_string().into());
                        payload.insert("planet".to_owned(), planet.to_string().into());
                        payload.insert("player".to_owned(), self.invader.to_string().into());
                        if let Some(holder) = controller {
                            payload.insert("controller".to_owned(), holder.to_string().into());
                        }
                        let _ = ctx.emit(state, "UNITS_COMMITTED", payload);

                        if !self.report.committed.contains(&planet) {
                            self.report.committed.push(planet);
                        }
                    }
                }
            }
            Stage::Fighting {
                planets,
                index,
                defender,
            } => {
                self.resolve_ground_round(state, ctx, planets, index, defender);
            }
        }

        // Committing settles when nothing is left to land.
        if matches!(self.stage, Stage::Committing)
            && self.landing_options(state, content, sources).is_empty()
        {
            self.finish_committing(state, ctx);
        }
        Ok(())
    }
}

fn note_ground_combat_win_feats(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
    winner: &PlayerId,
    loser: &PlayerId,
    notes_at_tactical_start: &crate::combat::NoteHoldings,
    occurrence: FeatOccurrence,
) -> bool {
    let mut noted = false;
    if ti4_content::galaxy::all_systems(content, sources)
        .get(system.as_str())
        .is_some_and(ti4_content::galaxy::System::is_anomaly)
    {
        state.record_event_feat(winner, Feat::WonInAnAnomaly, occurrence);
        noted = true;
    }
    if crate::combat::is_rival_home_system(state, winner, system) {
        state.record_event_feat(winner, Feat::WonInARivalHome, occurrence);
        noted = true;
    }
    let most = state
        .players
        .iter()
        .map(|seat| seat.victory_points)
        .max()
        .unwrap_or(0);
    if state
        .player(loser)
        .is_some_and(|seat| seat.victory_points == most)
    {
        state.record_event_feat(winner, Feat::WonAgainstThePointsLeader, occurrence);
        noted = true;
    }
    let holds_note = notes_at_tactical_start
        .get(winner)
        .is_some_and(|issuers| issuers.contains(loser));
    if holds_note {
        state.record_event_feat(winner, Feat::WonAgainstANoteHolder, occurrence);
        noted = true;
    }
    noted
}

/// Remove `hits` of a player's ground forces from a planet, weakest-first.
///
/// No choice is offered: every ground force on a planet is interchangeable in this model, so
/// asking would be a decision between identical options.
fn remove_ground(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
    planet: &PlanetId,
    player: &PlayerId,
    hits: usize,
) {
    let types = catalogue(content, sources);
    for _ in 0..hits {
        // LRR 42: only ground forces take hits in a ground combat; structures survive the
        // fight and die when control changes hands instead (KD-2).
        let present: Vec<Unit> = state
            .system_state(system)
            .on_planet_of(planet, player)
            .into_iter()
            .cloned()
            .collect();
        let doomed = present
            .iter()
            .find(|unit| {
                types
                    .get(unit.type_id.as_str())
                    .is_some_and(UnitType::is_ground_force)
            })
            .cloned();
        let Some(doomed) = doomed else {
            return; // 15.2a
        };
        state
            .system_mut(system)
            .remove_from_planet(planet, std::slice::from_ref(&doomed));
    }
}

/// Run a whole invasion for the active player (LRR 49).
///
/// # Errors
/// [`IllegalChoice`] when a decider answers with something not offered.
#[allow(
    clippy::too_many_arguments,
    reason = "one parameter per distinct input"
)]
pub fn resolve(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut Table,
    dice: &mut Dice,
    rng: &mut GameRng,
    system: &SystemId,
    invader: &PlayerId,
) -> Result<InvasionReport, IllegalChoice> {
    let mut window = InvasionWindow::new(state, content, sources, dice, rng, invader, system);
    let mut ctx = Resolving {
        content,
        sources,
        dice,
        rng,
        table,
        timing: None,
    };
    while !window.is_done() {
        window.drive(state, &mut ctx)?;
        // The synchronous API cannot present secret-objective choices. It still records each
        // exact occurrence, then consumes the pause and completes the invasion.
        while window.take_scoring_occurrence().is_some() {}
        window.settle(state, &mut ctx);
    }
    Ok(window.into_report())
}

#[cfg(test)]
mod tests {

    /// 27.2a: no ground forces, no custodians token — and so no victory point.
    ///
    /// The token is worth a point, so a seat with six influence and an empty fleet could otherwise
    /// buy that point and commit nothing. The rule says they cannot remove the token at all.
    #[test]
    fn the_custodians_token_needs_troops_not_just_influence() {
        let content = ContentStore::embedded();
        let sources = POK;
        let player = invader();
        let mecatol = SystemId::new(crate::seating::MECATOL);
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.board.entry(mecatol.clone()).or_default();

        // Influence enough and no army: the token stays.
        if let Some(seat) = state.player_mut(&player) {
            seat.trade_goods = 0;
        }
        crate::fixtures::put(&mut state, &mecatol, "cruiser", &player, 1);
        assert!(
            !custodians_removable(&state, content, sources, &player, &mecatol),
            "a fleet with nothing to land cannot lift the token"
        );

        crate::fixtures::put(&mut state, &mecatol, "infantry", &player, 1);
        // Still needs the influence, which this seat does not have — so the guard under test is
        // proven by giving it the troops and watching the *other* condition decide.
        let with_troops = custodians_removable(&state, content, sources, &player, &mecatol);
        if let Some(seat) = state.player_mut(&player) {
            seat.trade_goods = 0;
        }
        assert_eq!(
            with_troops,
            crate::production::available(
                &state,
                content,
                sources,
                &player,
                crate::production::Spend::Influence,
            ) >= 6,
            "with troops present, only the influence decides"
        );
    }

    /// Jol-Nar's Fragile applies on the ground, not only in space.
    ///
    /// "-1 to all combat rolls" is not a space rule, and `combat_modifier` has carried a `context`
    /// parameter for exactly this. Its only caller passed "space", so ground combat rolled the
    /// printed value: Jol-Nar infantry fought at full strength on every planet in the game.
    #[test]
    fn jol_nar_infantry_are_fragile_on_the_ground_too() {
        let content = ContentStore::embedded();
        let (mut state, system, planet) = arena();
        let player = invader();
        if let Some(seat) = state.player_mut(&player) {
            seat.faction = ti4_model::id::FactionId::new("jolnar");
        }
        on_planet(&mut state, &system, &planet, "infantry", &player, 1);

        let printed = catalogue(content, POK)
            .get("infantry")
            .and_then(ti4_content::units::UnitType::combat_hits_on)
            .expect("infantry have a combat value");
        assert_eq!(
            ground_combat_value(&state, content, POK, &player, &system, &planet, "infantry"),
            Some(printed + 1),
            "Fragile is -1 to the roll, so the threshold rises by one"
        );

        if let Some(seat) = state.player_mut(&player) {
            seat.faction = ti4_model::id::FactionId::new("sol");
        }
        assert_eq!(
            ground_combat_value(&state, content, POK, &player, &system, &planet, "infantry"),
            Some(printed),
            "and another faction rolls the printed value"
        );
    }

    /// L1Z1X's Anihilator mech bombards from the ground, but not the planet it is standing on.
    ///
    /// "While not participating in ground combat, this unit can use its BOMBARDMENT ability on
    /// planets in its system as if it were a ship." Standing on the planet it would shoot at *is*
    /// participating, so the mech joins the bombardment of every other planet and not that one.
    #[test]
    fn an_anihilator_mech_bombards_other_planets_in_its_system() {
        let (mut state, system, planet) = arena();
        let player = invader();
        let elsewhere = PlanetId::new("a_different_planet");
        on_planet(&mut state, &system, &elsewhere, "l1z1x_mech", &player, 1);

        let from_elsewhere = bombarders(&state, &player, &system, &planet);
        assert!(
            from_elsewhere
                .iter()
                .any(|unit| unit.type_id.as_str() == "l1z1x_mech"),
            "a mech on another planet joins the bombardment"
        );

        let at_home = bombarders(&state, &player, &system, &elsewhere);
        assert!(
            !at_home
                .iter()
                .any(|unit| unit.type_id.as_str() == "l1z1x_mech"),
            "but it does not bombard the planet it is standing on"
        );
    }

    /// Jol-Nar's Shield Paling mech lifts Fragile from the infantry standing with it.
    ///
    /// One of four in-scope mech abilities that no coverage helper counted, so none of them showed
    /// up as a gap. Only infantry, only that planet: the mech itself still rolls Fragile, and
    /// infantry on the next planet over are unhelped.
    #[test]
    fn a_shield_paling_lifts_fragile_from_the_infantry_beside_it() {
        let content = ContentStore::embedded();
        let (mut state, system, planet) = arena();
        let player = invader();
        if let Some(seat) = state.player_mut(&player) {
            seat.faction = ti4_model::id::FactionId::new("jolnar");
        }
        on_planet(&mut state, &system, &planet, "infantry", &player, 1);
        let printed = catalogue(content, POK)
            .get("infantry")
            .and_then(ti4_content::units::UnitType::combat_hits_on)
            .expect("infantry have a combat value");

        assert_eq!(
            ground_combat_value(&state, content, POK, &player, &system, &planet, "infantry"),
            Some(printed + 1),
            "Fragile applies without the mech"
        );

        on_planet(&mut state, &system, &planet, "jolnar_mech", &player, 1);
        assert_eq!(
            ground_combat_value(&state, content, POK, &player, &system, &planet, "infantry"),
            Some(printed),
            "and the mech lifts it"
        );
        let mech_printed = catalogue(content, POK)
            .get("jolnar_mech")
            .and_then(ti4_content::units::UnitType::combat_hits_on)
            .expect("the mech has a combat value");
        assert_eq!(
            ground_combat_value(&state, content, POK, &player, &system, &planet, "jolnar_mech"),
            Some(mech_printed + 1),
            "but only for infantry: the mech itself is still Fragile"
        );
    }

    /// 63.2: a planetary shield stops Harrow, as it stops any other bombardment.
    ///
    /// Harrow is a bombardment that fires again at the end of each ground-combat round, and the
    /// shield names it explicitly. It was firing through the shield because the faction layer asks
    /// only "does this seat have Harrow" -- the gate lives in the invasion, where the planet is
    /// known, so Harrow now goes through the same `bombardable` every other bombardment does.
    #[test]
    fn a_planetary_shield_stops_harrow() {
        let (mut state, system, planet) = arena();
        if let Some(seat) = state.player_mut(&invader()) {
            seat.faction = ti4_model::id::FactionId::new("l1z1x");
        }
        in_space(&mut state, &system, "dreadnought", &invader(), 1);

        assert!(
            bombardable(
                &state,
                ContentStore::embedded(),
                POK,
                &system,
                &planet,
                &invader()
            ),
            "with no shield the bombardment is allowed"
        );

        on_planet(&mut state, &system, &planet, "pds", &holder(), 1);
        assert!(
            !bombardable(
                &state,
                ContentStore::embedded(),
                POK,
                &system,
                &planet,
                &invader()
            ),
            "and the shield stops it -- which is the gate Harrow now asks"
        );
    }
    use ti4_model::content_types::DEFAULT as ALL_SOURCES;
    use ti4_model::content_types::POK;

    /// Space stations rule 5: ground forces cannot be committed to a space station.
    ///
    /// Checked on real tiles rather than an invented one. 117 carries only The Watchtower, so the
    /// whole tile must offer nothing; 109 carries Bellatrix *and* Tsion Station, so it must offer
    /// exactly the real planet -- which is the case that distinguishes "filtered" from "skipped
    /// the tile".
    #[test]
    fn ground_forces_cannot_land_on_a_space_station() {
        let content = ti4_content::ContentStore::embedded();
        let state = crate::fixtures::game(&["a"]);

        let station_only = landable_planets(&state, content, ALL_SOURCES, &SystemId::new("117"));
        assert!(
            station_only.is_empty(),
            "The Watchtower is the only planet on 117 and is a station, so nothing may land: {station_only:?}"
        );

        let mixed = landable_planets(&state, content, ALL_SOURCES, &SystemId::new("109"));
        assert_eq!(
            mixed,
            vec![PlanetId::new("bellatrix")],
            "109 must offer its real planet and not Tsion Station"
        );
    }

    use ti4_model::id::UnitTypeId;

    use super::*;
    use crate::setup::start_game;

    fn invader() -> PlayerId {
        PlayerId::new("a")
    }
    fn holder() -> PlayerId {
        PlayerId::new("b")
    }

    /// A system the corpus gives planets, so landing has somewhere to go.
    fn arena() -> (GameState, SystemId, PlanetId) {
        let state =
            start_game(ContentStore::embedded(), &[invader(), holder()], POK, None).unwrap();
        let (system, planet) = ti4_content::galaxy::all_planets(ContentStore::embedded(), POK)
            .iter()
            .find(|(_, p)| p.system_id().is_some() && !p.is_placed_during_play())
            .map(|(id, p)| (SystemId::new(p.system_id().unwrap()), PlanetId::new(*id)))
            .expect("the corpus has a placed planet");
        (state, system, planet)
    }

    fn on_planet(
        state: &mut GameState,
        system: &SystemId,
        planet: &PlanetId,
        kind: &str,
        owner: &PlayerId,
        count: usize,
    ) {
        for _ in 0..count {
            state
                .system_mut(system)
                .planet_units
                .entry(planet.clone())
                .or_default()
                .push(Unit::new(UnitTypeId::new(kind), owner.clone()));
        }
    }

    fn in_space(
        state: &mut GameState,
        system: &SystemId,
        kind: &str,
        owner: &PlayerId,
        count: usize,
    ) {
        for _ in 0..count {
            state
                .system_mut(system)
                .units
                .push(Unit::new(UnitTypeId::new(kind), owner.clone()));
        }
    }

    fn kit() -> (Table, Dice, GameRng) {
        (Table::new(), Dice::new(), GameRng::new(5))
    }

    #[test]
    fn a_planetary_shield_blocks_bombardment_and_a_war_sun_ignores_it() {
        // 15.1f, and the exception that is most of what a war sun is for.
        let (mut state, system, planet) = arena();
        on_planet(&mut state, &system, &planet, "pds", &holder(), 1);
        in_space(&mut state, &system, "dreadnought", &invader(), 1);

        assert!(
            !bombardable(
                &state,
                ContentStore::embedded(),
                POK,
                &system,
                &planet,
                &invader()
            ),
            "a PDS shields the planet"
        );

        in_space(&mut state, &system, "warsun", &invader(), 1);
        assert!(
            bombardable(
                &state,
                ContentStore::embedded(),
                POK,
                &system,
                &planet,
                &invader()
            ),
            "a war sun ignores the shield"
        );
    }

    #[test]
    fn bombardment_kills_defenders_and_spares_your_own() {
        let (mut state, system, planet) = arena();
        on_planet(&mut state, &system, &planet, "infantry", &holder(), 4);
        on_planet(&mut state, &system, &planet, "infantry", &invader(), 2);
        in_space(&mut state, &system, "dreadnought", &invader(), 6);
        let (mut table, mut dice, mut rng) = kit();

        bombardment(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut dice,
            &mut rng,
            &mut table,
            &system,
            &invader(),
        )
        .expect("single owner: no choice to refuse");

        assert_eq!(
            state
                .system_state(&system)
                .on_planet_of(&planet, &invader())
                .len(),
            2,
            "your own troops are never bombarded"
        );
    }

    #[test]
    fn separate_bombardments_record_separate_noncombat_occurrences() {
        let (mut state, system, planet) = arena();
        in_space(&mut state, &system, "dreadnought", &invader(), 1);
        let mut dice = Dice::from_faces([10, 10]);
        let mut rng = GameRng::new(1);
        let mut table = Table::new();

        for _ in 0..2 {
            on_planet(&mut state, &system, &planet, "infantry", &holder(), 1);
            assert_eq!(
                bombardment(
                    &mut state,
                    ContentStore::embedded(),
                    POK,
                    &mut dice,
                    &mut rng,
                    &mut table,
                    &system,
                    &invader(),
                )
                .expect("single owner: no choice to refuse"),
                1
            );
        }

        let occurrences: Vec<_> = state
            .player(&invader())
            .unwrap()
            .event_feats
            .iter()
            .filter_map(|(feat, occurrence)| {
                (*feat == Feat::BombardedOutTheLastGroundForces).then_some(*occurrence)
            })
            .collect();
        assert_eq!(occurrences.len(), 2);
        assert_ne!(occurrences[0], occurrences[1]);
    }

    #[test]
    fn ground_combat_in_any_rivals_home_records_darkening_the_skies() {
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let c = PlayerId::new("c");
        let mut state = start_game(
            ContentStore::embedded(),
            &[a.clone(), b.clone(), c.clone()],
            POK,
            None,
        )
        .unwrap();
        let (_, system, _) = arena();
        state.player_mut(&b).unwrap().home_system = Some(system.clone());
        let occurrence = state.begin_feat_occurrence();
        let notes = crate::combat::note_holdings(&state);

        assert!(note_ground_combat_win_feats(
            &mut state,
            ContentStore::embedded(),
            POK,
            &system,
            &a,
            &c,
            &notes,
            occurrence,
        ));
        assert!(state.did_at_occurrence(&a, Feat::WonInARivalHome, occurrence));
    }

    #[test]
    fn ground_combat_uses_note_holdings_from_tactical_action_start() {
        let (mut state, system, _) = arena();
        // Production-format key: the suffix is the owner's faction name, resolved to that
        // faction's seat. holder b plays Hacan and owns the Trade Convoys note; receipt puts it
        // faceup in the invader's play area (the corpus marks convoys playArea).
        state.player_mut(&holder()).unwrap().faction = ti4_model::id::FactionId::new("hacan");
        crate::promissory::take(
            &mut state,
            ContentStore::embedded(),
            &invader(),
            "convoys:hacan",
        );
        let notes = crate::combat::note_holdings(&state);
        state.promissory_notes.clear();
        let occurrence = state.begin_feat_occurrence();

        assert!(note_ground_combat_win_feats(
            &mut state,
            ContentStore::embedded(),
            POK,
            &system,
            &invader(),
            &holder(),
            &notes,
            occurrence,
        ));
        assert!(state.did_at_occurrence(&invader(), Feat::WonAgainstANoteHolder, occurrence));
    }

    #[test]
    fn ground_combat_resolves_production_note_keys_to_seated_issuers() {
        // Betray a Friend on the ground path: the winner holds the loser's note faceup in its
        // play area, and the production key "terraform:titans" must resolve to the Titans' seat —
        // not to a PlayerId built from the faction name.
        let (mut state, system, _) = arena();
        let content = ContentStore::embedded();
        let a = invader(); // titans: the issuer, and the loser
        let b = holder(); // hacan: holds a's note, and wins
        state.player_mut(&a).unwrap().faction = ti4_model::id::FactionId::new("titans");
        state.player_mut(&b).unwrap().faction = ti4_model::id::FactionId::new("hacan");
        crate::promissory::take(&mut state, content, &b, "terraform:titans");
        let notes = crate::combat::note_holdings(&state);
        let occurrence = state.begin_feat_occurrence();

        assert!(note_ground_combat_win_feats(
            &mut state,
            ContentStore::embedded(),
            POK,
            &system,
            &b,
            &a,
            &notes,
            occurrence,
        ));
        assert!(state.did_at_occurrence(&b, Feat::WonAgainstANoteHolder, occurrence));
    }

    #[test]
    fn an_undefended_planet_is_not_bombarded() {
        let (mut state, system, _) = arena();
        in_space(&mut state, &system, "dreadnought", &invader(), 4);
        let (mut table, mut dice, mut rng) = kit();

        let killed = bombardment(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut dice,
            &mut rng,
            &mut table,
            &system,
            &invader(),
        )
        .expect("no defenders: no choice to refuse");

        assert_eq!(killed, 0);
        assert_eq!(dice.count(), 0, "nothing to shoot at, so no dice");
    }

    /// Answers like [`Scripted`] but records what every question offered, so a test can
    /// assert not only the outcome but what the engine was allowed to say.
    struct RecordingScripted {
        inner: crate::choice::Scripted,
        seen: std::rc::Rc<std::cell::RefCell<Vec<Vec<String>>>>,
    }

    impl crate::choice::Decider for RecordingScripted {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            self.seen
                .borrow_mut()
                .push(choice.ids().into_iter().map(String::from).collect());
            self.inner.choose(choice)
        }
    }

    #[test]
    fn a_coexisting_planets_bombardment_is_chosen_per_unit_and_capped_at_each_target() {
        // Coexistence 7, 7.1, 7.2: the invader chooses, per bombarding unit, whose units on
        // the planet take the hits, and a unit's hits stop at the chosen player's own units
        // rather than spilling to a different player's forces.
        //
        // Invader's fleet: a dreadnought (one hit), a Sardakkian dreadnought (two hits),
        // another dreadnought (one hit). B holds one infantry on the planet, C holds two.
        // The first unit names C, the second names B (destroying B's only infantry and
        // wasting its surplus hit, which must not spill to C), the third names C again,
        // which empties the planet. The answers deliberately differ from the first offered
        // option, so a decider that ignored them would visibly change the result.
        let (system, planet) = crate::fixtures::a_placed_planet();
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let c = PlayerId::new("c");
        let content = ContentStore::embedded();

        let mut state = crate::fixtures::game(&["a", "b", "c"]);
        state
            .system_mut(&system)
            .set_control(planet.clone(), b.clone());
        on_planet(&mut state, &system, &planet, "infantry", &b, 1);
        on_planet(&mut state, &system, &planet, "infantry", &c, 2);
        in_space(&mut state, &system, "dreadnought", &a, 1);
        in_space(&mut state, &system, "sardakk_dreadnought", &a, 1);
        in_space(&mut state, &system, "dreadnought", &a, 1);

        let mut dice = Dice::from_faces([8, 6, 6, 8]);
        let mut rng = GameRng::new(7);
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let decider = RecordingScripted {
            inner: crate::choice::Scripted::new(["c", "b", "c"]),
            seen: seen.clone(),
        };
        let mut table = Table::with_default(Box::new(decider));

        let mut window =
            InvasionWindow::new(&mut state, content, POK, &mut dice, &mut rng, &a, &system);
        let mut ctx = Resolving {
            content,
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };

        // The window's own decisions run against the table, exactly as the public `resolve`
        // wrapper does, and the driver does through its step boundary.
        while !window.is_done() {
            window
                .drive(&mut state, &mut ctx)
                .expect("the script only picks what is offered");
            while window.take_scoring_occurrence().is_some() {}
            window.settle(&mut state, &mut ctx);
        }

        let report = window.into_report();
        assert_eq!(
            report.bombardment_kills, 3,
            "1 + 1 (the second unit's other hit wasted on B) + 1"
        );
        let expected: Vec<Vec<String>> = vec![
            vec!["b".to_owned(), "c".to_owned()],
            vec!["b".to_owned(), "c".to_owned()],
            vec!["c".to_owned()],
        ];
        assert_eq!(
            seen.borrow().as_slice(),
            expected.as_slice(),
            "each unit is asked separately, and B stops being offered once B holds nothing there"
        );

        let left: Vec<Unit> = state.system_state(&system).on_planet(&planet).to_vec();
        assert!(
            left.is_empty(),
            "every infantry was named: C twice, B once, B's surplus hit wasted"
        );
        assert_eq!(dice.count(), 3, "one roll per bombarding ship");
    }

    #[test]
    fn ground_forces_land_from_space_onto_a_planet() {
        let (mut state, system, planet) = arena();
        in_space(&mut state, &system, "infantry", &invader(), 2);
        let (mut table, _, _) = kit();

        let committed = commit_ground_forces(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &invader(),
            &system,
        )
        .unwrap();

        assert!(!committed.is_empty());
        assert!(
            landable(&state, ContentStore::embedded(), POK, &invader(), &system).is_empty(),
            "they left the space area"
        );
        let _ = planet;
    }

    #[test]
    fn an_uncontested_landing_takes_the_planet_exhausted() {
        // A captured planet is taken exhausted: its resources belong to the round after the
        // one you spent conquering it.
        let (mut state, system, planet) = arena();
        on_planet(&mut state, &system, &planet, "infantry", &invader(), 1);

        let captured = establish_control(
            &mut state,
            ContentStore::embedded(),
            POK,
            &system,
            &invader(),
            std::slice::from_ref(&planet),
        );

        assert_eq!(captured, vec![(planet.clone(), None)]);
        assert_eq!(
            state.system_state(&system).planet_control.get(&planet),
            Some(&invader())
        );
        assert!(
            state.exhausted_planets.contains(&planet),
            "taken exhausted, not ready to spend"
        );
    }

    #[test]
    fn losing_a_home_planet_records_a_separate_occurrence_for_its_previous_holder() {
        let (mut state, system, planet) = arena();
        state.player_mut(&holder()).unwrap().home_system = Some(system.clone());
        state
            .system_mut(&system)
            .set_control(planet.clone(), holder());
        on_planet(&mut state, &system, &planet, "infantry", &invader(), 1);
        let mut window = InvasionWindow {
            invader: invader(),
            system: system.clone(),
            stage: Stage::Done,
            report: InvasionReport {
                committed: vec![planet.clone()],
                ..InvasionReport::default()
            },
            pending_scoring_occurrences: std::collections::VecDeque::new(),
            current_ground_occurrence: None,
            notes_at_tactical_start: crate::combat::note_holdings(&state),
            bombard_plan: Vec::new(),
            bombard_index: 0,
            bombard_occurrence: state.begin_feat_occurrence(),
            bombard_announced: true,
        };
        let mut dice = Dice::new();
        let mut rng = GameRng::new(1);
        let mut table = Table::new();
        let mut ctx = Resolving {
            content: ContentStore::embedded(),
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };

        window.advance_fighting(&mut state, &mut ctx, std::slice::from_ref(&planet), 0);

        let (occurrence, combat) = window
            .take_scoring_occurrence()
            .expect("the control loss creates a scoring occurrence");
        assert!(!combat, "control loss is not a combat occurrence");
        assert!(state.did_at_occurrence(&holder(), Feat::LostAHomePlanet, occurrence));
        assert!(!state.did_at_occurrence(&invader(), Feat::LostAHomePlanet, occurrence));
        assert!(
            matches!(window.stage, Stage::FinalizingControl { .. }),
            "gain-control effects wait until the loss-scoring pause closes"
        );

        window.settle(&mut state, &mut ctx);
        assert!(
            window.is_done(),
            "the retained invasion resumes after scoring"
        );
    }

    #[test]
    fn taking_an_unowned_planet_explores_it_and_taking_one_off_a_rival_does_not() {
        // 35.1. A caller told merely that control changed would explore every conquest and
        // draw cards the rules do not give.
        let explore_once = |previous_holder: Option<PlayerId>| {
            let (mut state, system, planet) = arena();
            // A planet with a trait, so it has a deck to explore into.
            let deck = crate::exploration::trait_of(ContentStore::embedded(), POK, &planet)?;
            state
                .exploration_decks
                .insert(deck, vec!["minent".to_owned()]);
            if let Some(holder) = previous_holder {
                state
                    .system_mut(&system)
                    .set_control(planet.clone(), holder);
            }
            on_planet(&mut state, &system, &planet, "infantry", &invader(), 1);

            let mut window = InvasionWindow {
                invader: invader(),
                system: system.clone(),
                stage: Stage::Done,
                report: InvasionReport {
                    committed: vec![planet.clone()],
                    ..InvasionReport::default()
                },
                pending_scoring_occurrences: std::collections::VecDeque::new(),
                current_ground_occurrence: None,
                notes_at_tactical_start: crate::combat::note_holdings(&state),
                bombard_plan: Vec::new(),
                bombard_index: 0,
                bombard_occurrence: state.begin_feat_occurrence(),
                bombard_announced: true,
            };
            let mut dice = Dice::new();
            let mut rng = GameRng::new(1);
            let mut inner = Table::new();
            let mut ctx = crate::choice::Resolving {
                content: ContentStore::embedded(),
                sources: POK,
                dice: &mut dice,
                rng: &mut rng,
                table: &mut inner,
                timing: None,
            };
            window.advance_fighting(&mut state, &mut ctx, &[planet], 0);
            Some(window.into_report().explored.len())
        };

        if let Some(unowned) = explore_once(None) {
            assert_eq!(unowned, 1, "a planet nobody held is explored");
        }
        if let Some(conquered) = explore_once(Some(holder())) {
            assert_eq!(conquered, 0, "a planet taken off a rival is not");
        }
    }

    #[test]
    fn a_wiped_out_invasion_leaves_the_planet_with_its_holder() {
        // 49.5d: everything died, so the defender keeps what they had.
        let (mut state, system, planet) = arena();
        state
            .system_mut(&system)
            .set_control(planet.clone(), holder());

        let captured = establish_control(
            &mut state,
            ContentStore::embedded(),
            POK,
            &system,
            &invader(),
            std::slice::from_ref(&planet),
        );

        assert!(captured.is_empty());
        assert_eq!(
            state.system_state(&system).planet_control.get(&planet),
            Some(&holder()),
            "control did not fall to the invader by default"
        );
    }

    #[test]
    fn recapturing_your_own_planet_changes_nothing() {
        // 49.5c.
        let (mut state, system, planet) = arena();
        state
            .system_mut(&system)
            .set_control(planet.clone(), invader());
        on_planet(&mut state, &system, &planet, "infantry", &invader(), 1);

        let captured = establish_control(
            &mut state,
            ContentStore::embedded(),
            POK,
            &system,
            &invader(),
            std::slice::from_ref(&planet),
        );

        assert!(captured.is_empty());
        assert!(
            !state.exhausted_planets.contains(&planet),
            "it was not re-taken, so it was not exhausted"
        );
    }

    #[test]
    fn ground_combat_ends_with_one_side_holding_the_planet() {
        let (mut state, system, planet) = arena();
        on_planet(&mut state, &system, &planet, "infantry", &invader(), 4);
        on_planet(&mut state, &system, &planet, "infantry", &holder(), 1);
        let (mut table, mut dice, mut rng) = kit();

        ground_combat(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &mut dice,
            &mut rng,
            &system,
            &planet,
            &invader(),
        )
        .unwrap();

        let board = state.system_state(&system);
        let both = !board.on_planet_of(&planet, &invader()).is_empty()
            && !board.on_planet_of(&planet, &holder()).is_empty();
        assert!(
            !both,
            "a ground combat does not end with both sides standing"
        );
    }

    #[test]
    fn each_defended_planet_gets_a_distinct_ground_combat_occurrence() {
        let mut state =
            start_game(ContentStore::embedded(), &[invader(), holder()], POK, None).unwrap();
        state.player_mut(&holder()).unwrap().victory_points = 1;
        let planets = ti4_content::galaxy::all_planets(ContentStore::embedded(), POK);
        let (system, pair) = planets
            .iter()
            .filter_map(|(id, record)| {
                record
                    .system_id()
                    .map(|system| (SystemId::new(system), PlanetId::new(*id)))
            })
            .fold(
                std::collections::BTreeMap::<SystemId, Vec<PlanetId>>::new(),
                |mut grouped, (system, planet)| {
                    grouped.entry(system).or_default().push(planet);
                    grouped
                },
            )
            .into_iter()
            .find_map(|(system, planets)| {
                (planets.len() >= 2).then(|| (system, planets[..2].to_vec()))
            })
            .expect("the corpus has a two-planet system");
        for planet in &pair {
            on_planet(&mut state, &system, planet, "infantry", &invader(), 1);
            on_planet(&mut state, &system, planet, "infantry", &holder(), 1);
        }
        let mut window = InvasionWindow {
            invader: invader(),
            system,
            stage: Stage::Done,
            report: InvasionReport {
                committed: pair.clone(),
                ..InvasionReport::default()
            },
            pending_scoring_occurrences: std::collections::VecDeque::new(),
            current_ground_occurrence: None,
            notes_at_tactical_start: crate::combat::note_holdings(&state),
            bombard_plan: Vec::new(),
            bombard_index: 0,
            bombard_occurrence: state.begin_feat_occurrence(),
            bombard_announced: true,
        };
        let mut dice = Dice::from_faces([10, 1, 10, 1]);
        let mut rng = GameRng::new(1);
        let mut table = Table::new();
        let mut ctx = Resolving {
            content: ContentStore::embedded(),
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };

        window.advance_fighting(&mut state, &mut ctx, &pair, 0);
        let first_answer = window
            .pending_choice(&state, ContentStore::embedded(), POK)
            .unwrap()
            .options[0]
            .clone();
        window.resolve(&mut state, &mut ctx, first_answer).unwrap();
        let (first, first_is_combat) = window.take_scoring_occurrence().unwrap();
        assert!(first_is_combat);

        window.settle(&mut state, &mut ctx);
        let second_answer = window
            .pending_choice(&state, ContentStore::embedded(), POK)
            .unwrap()
            .options[0]
            .clone();
        window.resolve(&mut state, &mut ctx, second_answer).unwrap();
        let (second, second_is_combat) = window.take_scoring_occurrence().unwrap();
        assert!(second_is_combat);
        assert_ne!(
            first, second,
            "rule 61.7 caps each ground combat separately"
        );
    }

    #[test]
    fn an_empty_planet_needs_no_ground_combat() {
        let (mut state, system, planet) = arena();
        on_planet(&mut state, &system, &planet, "infantry", &invader(), 2);
        let (mut table, mut dice, mut rng) = kit();

        let winner = ground_combat(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &mut dice,
            &mut rng,
            &system,
            &planet,
            &invader(),
        )
        .unwrap();

        assert_eq!(winner, Some(invader()));
        assert_eq!(dice.count(), 0, "nobody to fight");
    }

    /// Like [`arena`], but never on Mecatol Rex, where the custodians token would change the
    /// commit flow.
    fn arena_off_mecatol() -> (GameState, SystemId, PlanetId) {
        let state =
            start_game(ContentStore::embedded(), &[invader(), holder()], POK, None).unwrap();
        let planets = ti4_content::galaxy::all_planets(ContentStore::embedded(), POK);
        let (id, p) = planets
            .iter()
            .find(|(_, p)| {
                p.system_id().is_some()
                    && !p.is_placed_during_play()
                    && p.system_id() != Some(crate::seating::MECATOL)
            })
            .expect("the corpus has a placed planet outside Mecatol Rex");
        (
            state,
            SystemId::new(p.system_id().unwrap()),
            PlanetId::new(*id),
        )
    }

    /// Commits every ground force to one planet, records every prompt it sees, and answers any
    /// other prompt with its first option — enough to run a pre-fix engine through the spurious
    /// fight without panicking.
    struct CommitAndRecord {
        planet: PlanetId,
        seen: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    }

    impl crate::choice::Decider for CommitAndRecord {
        fn choose(
            &mut self,
            choice: &crate::choice::Choice,
        ) -> Result<crate::choice::ChoiceOption, crate::choice::IllegalChoice> {
            self.seen.borrow_mut().push(choice.prompt.clone());
            let wanted = format!("commit|0|{}", self.planet.as_str());
            if let Some(option) = choice.options.iter().find(|o| o.id == wanted) {
                return Ok(option.clone());
            }
            if let Some(done) = choice.options.iter().find(|o| o.id == "done_committing") {
                return Ok(done.clone());
            }
            choice
                .options
                .first()
                .cloned()
                .ok_or(crate::choice::IllegalChoice::NoOptions {
                    player: choice.player.clone(),
                    prompt: choice.prompt.clone(),
                })
        }
    }

    #[test]
    fn a_structure_only_planet_falls_without_resistance() {
        // LRR 49 (KD-2): structures are not ground forces, so a planet holding only rival
        // structures is uncontested — it falls without resistance and its structures are
        // destroyed when control changes hands. No fight prompt may ever be offered.
        let content = ContentStore::embedded();
        let (mut state, system, planet) = arena_off_mecatol();
        state.player_mut(&invader()).unwrap().faction = ti4_model::id::FactionId::new("hacan");
        state
            .system_mut(&system)
            .set_control(planet.clone(), holder());
        on_planet(&mut state, &system, &planet, "pds", &holder(), 1);
        on_planet(&mut state, &system, &planet, "spacedock", &holder(), 1);
        in_space(&mut state, &system, "infantry", &invader(), 2);

        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut table = Table::with_default(Box::new(CommitAndRecord {
            planet: planet.clone(),
            seen: seen.clone(),
        }));
        // Pre-fix, the spurious fight consumes exactly these two faces in round one.
        let mut dice = Dice::from_faces([10u32, 8]);
        let mut rng = GameRng::new(7);
        let mut window = InvasionWindow::new(
            &mut state,
            content,
            POK,
            &mut dice,
            &mut rng,
            &invader(),
            &system,
        );
        let mut ctx = crate::choice::Resolving {
            content,
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };

        while !window.is_done() {
            crate::choice::Window::drive(&mut window, &mut state, &mut ctx).unwrap();
            while window.take_scoring_occurrence().is_some() {}
            window.settle(&mut state, &mut ctx);
        }
        let report = window.into_report();

        assert!(
            seen.borrow().iter().all(|prompt| !prompt.contains("fight")),
            "no ground combat may be offered on a structure-only planet: {:?}",
            seen.borrow()
        );
        assert!(
            dice.rolled("ground combat").is_empty(),
            "the spurious fight consumes no dice once it stops happening"
        );
        assert_eq!(report.captured, vec![(planet.clone(), Some(holder()))]);
        let units = state
            .system_state(&system)
            .planet_units
            .get(&planet)
            .cloned()
            .unwrap_or_default();
        assert_eq!(
            units.len(),
            2,
            "only the invader's ground forces remain: {units:?}"
        );
        assert!(
            units
                .iter()
                .all(|unit| unit.owner == invader() && unit.type_id.as_str() == "infantry"),
            "the rival structures are destroyed when control changes hands: {units:?}"
        );
    }

    #[test]
    fn structures_survive_a_legitimate_ground_fight_and_die_when_control_changes() {
        // LRR 42 (KD-2): in a real ground combat only ground forces fight and take hits. The
        // rival PDS must still stand when the last rival infantry dies; it is destroyed later,
        // when control changes hands — not by a combat hit that could never target it.
        let content = ContentStore::embedded();
        let (mut state, system, planet) = arena_off_mecatol();
        state.player_mut(&invader()).unwrap().faction = ti4_model::id::FactionId::new("hacan");
        // The combat win must be a rival-home win so its occurrence pauses the invasion at a
        // point where the fight is over but control has not yet transferred.
        state.player_mut(&holder()).unwrap().home_system = Some(system.clone());
        state
            .system_mut(&system)
            .set_control(planet.clone(), holder());
        // The PDS is stored first, so a pre-fix casualty pool (any unit) kills it on the
        // first hit.
        on_planet(&mut state, &system, &planet, "pds", &holder(), 1);
        on_planet(&mut state, &system, &planet, "infantry", &holder(), 1);
        in_space(&mut state, &system, "infantry", &invader(), 2);

        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut table = Table::with_default(Box::new(CommitAndRecord {
            planet: planet.clone(),
            seen: seen.clone(),
        }));
        // Round one only: the invader's two infantry hit (10, 8), the rival infantry misses
        // (1). Both pre-fix and post-fix the fight ends after this round.
        let mut dice = Dice::from_faces([10u32, 8, 1]);
        let mut rng = GameRng::new(7);
        let mut window = InvasionWindow::new(
            &mut state,
            content,
            POK,
            &mut dice,
            &mut rng,
            &invader(),
            &system,
        );
        let mut ctx = crate::choice::Resolving {
            content,
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };

        // Commit and fight to the combat-win pause: the win occurrence is queued before
        // control transfers, so this is the moment to inspect what the fight itself destroyed.
        crate::choice::Window::drive(&mut window, &mut state, &mut ctx).unwrap();
        let (occurrence, is_combat) = window
            .take_scoring_occurrence()
            .expect("the combat win queues a scoring occurrence");
        assert!(is_combat, "the pause holds the ground-combat win");
        assert!(state.did_at_occurrence(&invader(), Feat::WonInARivalHome, occurrence));

        let mid: Vec<Unit> = state
            .system_state(&system)
            .planet_units
            .get(&planet)
            .cloned()
            .unwrap_or_default();
        assert!(
            mid.iter()
                .any(|unit| unit.type_id.as_str() == "pds" && unit.owner == holder()),
            "the PDS survived the ground combat itself: {mid:?}"
        );

        while !window.is_done() {
            while window.take_scoring_occurrence().is_some() {}
            window.settle(&mut state, &mut ctx);
        }
        let report = window.into_report();

        assert_eq!(report.captured, vec![(planet.clone(), Some(holder()))]);
        let units = state
            .system_state(&system)
            .planet_units
            .get(&planet)
            .cloned()
            .unwrap_or_default();
        assert!(
            units
                .iter()
                .all(|unit| unit.owner == invader() && unit.type_id.as_str() == "infantry"),
            "the PDS dies when control changes hands, not in the fight: {units:?}"
        );
    }

    #[test]
    fn an_invasion_with_no_troops_commits_nothing() {
        // 49.2c: straight on to Production.

        let (mut state, system, _) = arena();
        let (mut table, mut dice, mut rng) = kit();

        let report = resolve(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &mut dice,
            &mut rng,
            &system,
            &invader(),
        )
        .unwrap();

        assert!(report.committed.is_empty());
        assert!(report.captured.is_empty());
    }

    #[test]
    fn a_whole_invasion_takes_an_undefended_planet() {
        let (mut state, system, _) = arena();
        in_space(&mut state, &system, "infantry", &invader(), 2);
        let (mut table, mut dice, mut rng) = kit();

        let report = resolve(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &mut dice,
            &mut rng,
            &system,
            &invader(),
        )
        .unwrap();

        assert!(!report.captured.is_empty(), "the planet changed hands");
        for (planet, _) in &report.captured {
            assert!(state.exhausted_planets.contains(planet));
        }
    }

    type RecordedAsk = (String, Vec<(String, String, String)>);

    /// A decider that records every choice it is asked to answer, answering from a queue of ids.
    struct CommitRecording {
        wanted: std::collections::VecDeque<String>,
        seen: std::rc::Rc<std::cell::RefCell<Vec<RecordedAsk>>>,
    }

    impl CommitRecording {
        fn new(wanted: &[String]) -> (Self, std::rc::Rc<std::cell::RefCell<Vec<RecordedAsk>>>) {
            let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            (
                Self {
                    wanted: wanted.iter().cloned().collect(),
                    seen: seen.clone(),
                },
                seen,
            )
        }

        fn record(&self, choice: &crate::choice::Choice) {
            self.seen.borrow_mut().push((
                choice.prompt.clone(),
                choice
                    .options
                    .iter()
                    .map(|option| (option.id.clone(), option.kind.clone(), option.label.clone()))
                    .collect(),
            ));
        }
    }

    impl crate::choice::Decider for CommitRecording {
        fn choose(
            &mut self,
            choice: &crate::choice::Choice,
        ) -> Result<crate::choice::ChoiceOption, crate::choice::IllegalChoice> {
            self.record(choice);
            let Some(wanted) = self.wanted.pop_front() else {
                return Err(crate::choice::IllegalChoice::ScriptDiverged {
                    player: choice.player.clone(),
                    wanted: "<script exhausted>".to_owned(),
                    offered: choice.ids().into_iter().map(str::to_owned).collect(),
                });
            };
            choice.option(&wanted).cloned().ok_or_else(|| {
                crate::choice::IllegalChoice::ScriptDiverged {
                    player: choice.player.clone(),
                    wanted,
                    offered: choice.ids().into_iter().map(str::to_owned).collect(),
                }
            })
        }
    }

    /// A system the corpus places at least two planets in, so a landing is a real choice.
    fn two_planet_arena() -> (GameState, SystemId, PlanetId, PlanetId) {
        let state =
            start_game(ContentStore::embedded(), &[invader(), holder()], POK, None).unwrap();
        let content = ContentStore::embedded();
        let systems: std::collections::BTreeSet<&str> =
            ti4_content::galaxy::all_planets(content, POK)
                .iter()
                .filter_map(|(_, planet)| planet.system_id())
                .collect();
        for system in &systems {
            // F-M08-019-1 (C1): pa/pb follow the system record's own `planets` array
            // (canonical order), so option-order assertions track the oracle rather than the
            // planets.json layout. System 09 is selected: record [maaluuk, druua] vs file
            // order [druaa, maaluuk].
            if let Some(record) =
                content.get(ti4_model::content_types::ContentType::Systems, system)
            {
                let planets: Vec<PlanetId> = record
                    .strings("planets")
                    .into_iter()
                    .map(PlanetId::new)
                    .collect();
                if planets.len() >= 2 {
                    return (
                        state,
                        SystemId::new(*system),
                        planets[0].clone(),
                        planets[1].clone(),
                    );
                }
            }
        }
        panic!("the corpus has no two-planet system")
    }

    #[test]
    fn commit_ground_forces_offers_the_oracle_identity() {
        // engine/invasion.py:253–324 asks "commit ground forces in {system}" with ids
        // commit|{i}|{planet}, kind "commit", labels "land infantry on {p}", and the terminator
        // ("done_committing", "decline", "commit no more ground forces"). Two identical undamaged
        // infantry over two planets are one move each, so unit 1 contributes no options.
        let (mut state, system, pa, pb) = two_planet_arena();
        in_space(&mut state, &system, "infantry", &invader(), 2);
        let script = vec![format!("commit|0|{pa}"), "done_committing".to_owned()];
        let (recorder, seen) = CommitRecording::new(&script);
        let mut table = Table::with_default(Box::new(recorder));

        let committed = commit_ground_forces(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &invader(),
            &system,
        )
        .unwrap();

        assert_eq!(committed, vec![pa.clone()]);
        let asks = seen.borrow();
        assert_eq!(asks.len(), 2, "one landing, then the decline ask");
        assert_eq!(asks[0].0, format!("commit ground forces in {system}"));
        assert_eq!(
            asks[0].1,
            vec![
                (
                    format!("commit|0|{pa}"),
                    "commit".to_owned(),
                    format!("land infantry on {pa}")
                ),
                (
                    format!("commit|0|{pb}"),
                    "commit".to_owned(),
                    format!("land infantry on {pb}")
                ),
                (
                    "done_committing".to_owned(),
                    "decline".to_owned(),
                    "commit no more ground forces".to_owned()
                )
            ]
        );
        assert_eq!(asks[1].0, format!("commit ground forces in {system}"));
    }

    #[test]
    fn commit_options_follow_the_system_record_planet_order() {
        // F-M08-019-1 (C1): landing options must follow the system record's own `planets`
        // array, not the file layout of planets.json. System 09 prints [maaluuk, druua]; in
        // file order druaa precedes maaluuk — under the old code the option ids came out
        // swapped. Verified RED before the fix.
        let mut state =
            start_game(ContentStore::embedded(), &[invader(), holder()], POK, None).unwrap();
        let system = SystemId::new("09");
        in_space(&mut state, &system, "infantry", &invader(), 1);
        let (recorder, seen) = CommitRecording::new(&["done_committing".to_owned()]);
        let mut table = Table::with_default(Box::new(recorder));

        commit_ground_forces(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &invader(),
            &system,
        )
        .unwrap();

        let asks = seen.borrow();
        assert_eq!(asks.len(), 1);
        assert_eq!(
            asks[0].1,
            vec![
                (
                    "commit|0|maaluuk".to_owned(),
                    "commit".to_owned(),
                    "land infantry on maaluuk".to_owned()
                ),
                (
                    "commit|0|druaa".to_owned(),
                    "commit".to_owned(),
                    "land infantry on druaa".to_owned()
                ),
                (
                    "done_committing".to_owned(),
                    "decline".to_owned(),
                    "commit no more ground forces".to_owned()
                )
            ]
        );
    }

    #[test]
    fn commit_options_distinguish_sustained_damage() {
        // engine/choice.py:96 unit_label shows damage rather than folding it away; the dedup key
        // is (type, sustained damage, planet), so a damaged infantry is its own options.
        let (mut state, system, pa, pb) = two_planet_arena();
        in_space(&mut state, &system, "infantry", &invader(), 2);
        state
            .system_mut(&system)
            .units
            .last_mut()
            .expect("two troops are in space")
            .sustained_damage = true;
        let (recorder, seen) = CommitRecording::new(&["done_committing".to_owned()]);
        let mut table = Table::with_default(Box::new(recorder));

        commit_ground_forces(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &invader(),
            &system,
        )
        .unwrap();

        let asks = seen.borrow();
        assert_eq!(asks.len(), 1);
        assert_eq!(
            asks[0].1,
            vec![
                (
                    format!("commit|0|{pa}"),
                    "commit".to_owned(),
                    format!("land infantry on {pa}")
                ),
                (
                    format!("commit|0|{pb}"),
                    "commit".to_owned(),
                    format!("land infantry on {pb}")
                ),
                (
                    format!("commit|1|{pa}"),
                    "commit".to_owned(),
                    format!("land infantry (damaged) on {pa}")
                ),
                (
                    format!("commit|1|{pb}"),
                    "commit".to_owned(),
                    format!("land infantry (damaged) on {pb}")
                ),
                (
                    "done_committing".to_owned(),
                    "decline".to_owned(),
                    "commit no more ground forces".to_owned()
                )
            ]
        );
    }

    #[test]
    fn lifting_the_custodians_token_costs_six_influence_and_pays_a_point() {
        // 27.2/27.3. Until this existed, every assignment to `custodians_removed` in the whole
        // codebase was inside a test, so the agenda phase -- gated on the token by 8.1 -- never
        // ran in a simulated game, and every law and agenda victory point was unreachable.
        let content = ContentStore::embedded();
        let mut state = start_game(content, &[invader(), holder()], POK, None).unwrap();
        let mecatol = SystemId::new(crate::seating::MECATOL);
        assert!(!state.custodians_removed);

        // A freshly started game controls no planets, so the seat is funded with trade goods --
        // spendable as influence -- rather than by hand-placing planet control.
        if let Some(seat) = state.player_mut(&invader()) {
            seat.trade_goods = 6;
        }
        // 27.2a: the token cannot be lifted by a seat with nothing to commit, so the invasion that
        // lifts it has an army in the system. Funding influence alone used to be enough here,
        // which is the gap that rule closes.
        state.board.entry(mecatol.clone()).or_default();
        crate::fixtures::put(&mut state, &mecatol, "infantry", &invader(), 1);
        let influence = crate::production::available(
            &state,
            content,
            POK,
            &invader(),
            crate::production::Spend::Influence,
        );
        assert!(
            influence >= CUSTODIANS_COST,
            "a starting seat should be able to afford the token, had {influence}"
        );
        assert!(custodians_removable(
            &state,
            content,
            POK,
            &invader(),
            &mecatol
        ));

        let before = state
            .player(&invader())
            .map_or(0, |seat| seat.victory_points);
        let mut window = InvasionWindow {
            invader: invader(),
            system: mecatol.clone(),
            stage: Stage::Custodians,
            report: InvasionReport::default(),
            pending_scoring_occurrences: std::collections::VecDeque::new(),
            current_ground_occurrence: None,
            notes_at_tactical_start: crate::combat::note_holdings(&state),
            bombard_plan: Vec::new(),
            bombard_index: 0,
            bombard_occurrence: state.begin_feat_occurrence(),
            bombard_announced: true,
        };
        let choice = window
            .pending_choice(&state, content, POK)
            .expect("the custodians ask is offered on Mecatol");
        assert!(
            choice.prompt.contains("custodians"),
            "got {}",
            choice.prompt
        );

        let mut dice = Dice::new();
        let mut rng = GameRng::new(1);
        let mut table = Table::with_default(Box::new(crate::choice::FirstOption));
        let mut ctx = Resolving {
            content,
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };
        let yes = choice
            .options
            .iter()
            .find(|option| option.id == "yes")
            .cloned()
            .expect("the accepting option exists");
        window.resolve(&mut state, &mut ctx, yes).unwrap();

        assert!(state.custodians_removed, "the token comes off");
        assert_eq!(
            state
                .player(&invader())
                .map_or(0, |seat| seat.victory_points),
            before + 1,
            "27.3 pays a victory point"
        );
        let after = crate::production::available(
            &state,
            content,
            POK,
            &invader(),
            crate::production::Spend::Influence,
        );
        assert!(
            after <= influence - CUSTODIANS_COST,
            "six influence was spent"
        );
    }

    #[test]
    fn mecatol_is_not_offered_while_the_custodians_token_is_present() {
        // 27.1: nobody lands on Mecatol Rex while the custodians token sits there.
        // System 18 holds exactly one planet ("mr"), so with the token up the commit ask
        // offers nothing but the terminator; without it, the landing is offered.
        let content = ContentStore::embedded();
        let mut state = start_game(content, &[invader(), holder()], POK, None).unwrap();
        assert!(!state.custodians_removed);
        let mecatol_system = SystemId::new("18");
        in_space(&mut state, &mecatol_system, "infantry", &invader(), 1);

        let (recorder, seen) = CommitRecording::new(&["done_committing".to_owned()]);
        let mut table = Table::with_default(Box::new(recorder));
        commit_ground_forces(
            &mut state,
            content,
            POK,
            &mut table,
            &invader(),
            &mecatol_system,
        )
        .unwrap();
        let asks = seen.borrow();
        assert_eq!(asks.len(), 1);
        assert_eq!(
            asks[0].1, // the token keeps Mecatol Rex off the table
            vec![(
                "done_committing".to_owned(),
                "decline".to_owned(),
                "commit no more ground forces".to_owned()
            )]
        );

        state.custodians_removed = true;
        let (recorder, seen) = CommitRecording::new(&["done_committing".to_owned()]);
        let mut table = Table::with_default(Box::new(recorder));
        commit_ground_forces(
            &mut state,
            content,
            POK,
            &mut table,
            &invader(),
            &mecatol_system,
        )
        .unwrap();
        let asks = seen.borrow();
        assert_eq!(asks.len(), 1);
        assert_eq!(
            asks[0].1, // without the token, Mecatol Rex lands like any planet
            vec![
                (
                    "commit|0|mr".to_owned(),
                    "commit".to_owned(),
                    "land infantry on mr".to_owned()
                ),
                (
                    "done_committing".to_owned(),
                    "decline".to_owned(),
                    "commit no more ground forces".to_owned()
                )
            ]
        );
    }

    #[test]
    fn the_invasion_window_commit_ask_uses_the_oracle_identity() {
        // The staged window is the real-game path (armed from game.rs); it builds its own copy of
        // the option list, so the surface is asserted here rather than assumed shared.
        let (mut state, system, pa, pb) = two_planet_arena();
        in_space(&mut state, &system, "infantry", &invader(), 2);
        let (mut table, mut dice, mut rng) = kit();

        let mut window = InvasionWindow::new(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut dice,
            &mut rng,
            &invader(),
            &system,
        );
        let content = ContentStore::embedded();
        let mut ctx = Resolving {
            content,
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };
        // A fresh window sits on its (choice-free) bombardment until it settles.
        window.settle(&mut state, &mut ctx);
        let choice = window
            .pending_choice(&state, content, POK)
            .expect("troops in space mean a commit ask");

        assert_eq!(choice.prompt, format!("commit ground forces in {system}"));
        assert_eq!(
            choice
                .options
                .iter()
                .map(|o| o.id.as_str())
                .collect::<Vec<_>>(),
            vec![
                format!("commit|0|{pa}"),
                format!("commit|0|{pb}"),
                "done_committing".to_owned()
            ]
        );
        assert!(
            choice
                .options
                .iter()
                .all(|o| o.kind == "commit" || o.id == "done_committing")
        );
    }

    // -- reroll windows: Fire Team, Scramble Frequency, Aglnlan Oln ----------------
    //
    // These drive the real window machinery (armed reaction slots, staged dice, recompute
    // after the window) rather than calling the card effects directly, so a reroll that only
    // worked on the direct path cannot pass here.

    /// A resolver armed exactly as the driver arms one: a standing slot per player per
    /// window, the hand read at resolution time.
    fn armed_resolver(state: &GameState, players: Vec<PlayerId>) -> crate::timing::Resolver {
        let mut resolver = crate::timing::Resolver::new(
            players,
            Some(PlayerId::new("a")),
            crate::choice::Table::default(),
        );
        crate::reactions::arm(&mut resolver, state);
        resolver
    }

    /// Builds the manual invasion window the card tests drive, and runs it to completion.
    /// `script` is the exact answer to every question the run makes (window offers, commit
    /// offers, card offers); `dice` and `rng` pin the rolls. Returns the questions that were
    /// offered, in order.
    fn drive_invasion(
        state: &mut GameState,
        system: &SystemId,
        stage: Stage,
        report: InvasionReport,
        script: Vec<String>,
        dice: &mut Dice,
        rng: &mut GameRng,
    ) -> Vec<Vec<String>> {
        let a = invader();
        let b = holder();
        let content = ContentStore::embedded();
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let decider = RecordingScripted {
            inner: crate::choice::Scripted::new(script),
            seen: seen.clone(),
        };
        let mut table = Table::with_default(Box::new(decider));
        let mut resolver = armed_resolver(state, vec![a.clone(), b.clone()]);
        let mut event_sequence = crate::event::EventSequence::new();
        let mut window = InvasionWindow {
            invader: a,
            system: system.clone(),
            stage,
            report,
            pending_scoring_occurrences: std::collections::VecDeque::new(),
            current_ground_occurrence: None,
            notes_at_tactical_start: crate::combat::note_holdings(state),
            bombard_plan: Vec::new(),
            bombard_index: 0,
            bombard_occurrence: state.begin_feat_occurrence(),
            bombard_announced: true,
        };
        let mut ctx = Resolving {
            content,
            sources: POK,
            dice,
            rng,
            table: &mut table,
            timing: Some(crate::choice::TimingHandle {
                resolver: &mut resolver,
                sequence: &mut event_sequence,
                galaxy: None,
            }),
        };
        while !window.is_done() {
            window
                .drive(state, &mut ctx)
                .expect("the script only picks what is offered");
            while window.take_scoring_occurrence().is_some() {}
            window.settle(state, &mut ctx);
        }
        seen.borrow().clone()
    }

    #[test]
    fn fire_team_rerolls_your_own_ground_dice_before_anyone_is_removed() {
        // Fire Team: "After your ground forces make combat rolls during a round of ground
        // combat: Reroll any number of your dice." One infantry each (both hit on 8): the
        // invader's face 8 kills the defender's only infantry, and the defender's 3 kills
        // nothing. The reroll draws from the seeded stream (seed 4's first DICE draw is an 8),
        // so it kills the invader's only infantry too: both sides die in the same round and
        // the defender holds the planet.
        let a = invader();
        let b = holder();
        let (system, planet) = {
            let (_, s, p) = arena();
            (s, p)
        };

        let run = |with_card: bool| -> (Vec<Vec<String>>, GameState, Dice) {
            let (mut state, system, planet) = arena();
            state
                .system_mut(&system)
                .set_control(planet.clone(), b.clone());
            on_planet(&mut state, &system, &planet, "infantry", &a, 1);
            on_planet(&mut state, &system, &planet, "infantry", &b, 1);
            if with_card {
                state.player_mut(&b).unwrap().action_cards =
                    vec![ti4_model::id::ActionCardId::new("fire_team")];
            }
            // The preload feeds the two initial rolls only; a reroll re-draws from the seeded
            // stream (see Dice::reroll), and seed 4's first DICE-domain draw is an 8.
            let mut dice = Dice::from_faces([8u32, 3]);
            let mut rng = GameRng::new(4);
            let script: Vec<String> = if with_card {
                vec![
                    "fight".to_owned(),
                    "reaction:generic:GROUND_ROLLS_MADE:after".to_owned(),
                    "reroll|0:0".to_owned(),
                ]
            } else {
                vec!["fight".to_owned()]
            };
            let asks = drive_invasion(
                &mut state,
                &system,
                Stage::Fighting {
                    planets: vec![planet.clone()],
                    index: 0,
                    defender: b.clone(),
                },
                InvasionReport {
                    // The manual window models a committed landing, so a surviving invader can
                    // establish control the way the Committing stage would record it.
                    committed: vec![planet.clone()],
                    ..InvasionReport::default()
                },
                script,
                &mut dice,
                &mut rng,
            );
            (asks, state, dice)
        };

        // The card-less fight: the 8 kills the defender's only infantry, the 3 kills nothing,
        // the invader takes the planet, and nothing was ever rerolled.
        let (asks, state, dice) = run(false);
        assert_eq!(
            asks,
            vec![vec!["fight".to_owned()]],
            "no card in hand, so no reaction window opens; got {asks:?}"
        );
        let units: Vec<Unit> = state.system_state(&system).on_planet(&planet).to_vec();
        assert_eq!(units.len(), 1, "the invader's infantry is all that is left");
        assert_eq!(units[0].owner, a);
        assert_eq!(
            state.system_state(&system).planet_control.get(&planet),
            Some(&a),
            "the surviving invader holds the planet"
        );
        assert!(dice.rolled("fire team").is_empty(), "no card, no reroll");

        // With Fire Team: the defender rerolls the 3; the re-draw is an 8 (seed 4), so the
        // defender kills the invader's only infantry as well. Both sides die in the same
        // round, and the defender holds the planet.
        let (asks, state, dice) = run(true);
        assert_eq!(
            asks,
            vec![
                vec!["fight".to_owned()],
                vec![
                    "reaction:generic:GROUND_ROLLS_MADE:after".to_owned(),
                    "decline".to_owned()
                ],
                vec!["reroll|0:0".to_owned(), "decline".to_owned()],
            ],
            // The single playable card is auto-selected, so no inner card ask is recorded.
            "the round ask, the reaction offer, then one die question; got {asks:?}"
        );
        assert!(
            state.reroll_staging.is_empty(),
            "the staging is spent with the round"
        );
        let units: Vec<Unit> = state.system_state(&system).on_planet(&planet).to_vec();
        assert!(
            units.is_empty(),
            "the reroll let the defender kill the invader's last infantry: both sides died"
        );
        assert_eq!(
            state.system_state(&system).planet_control.get(&planet),
            Some(&b),
            "the invader died, so the defender keeps the planet"
        );
        assert!(
            state.player(&b).unwrap().action_cards.is_empty(),
            "the card was spent"
        );
        // The reroll ran through the game's own roller, recorded under the card's name.
        assert_eq!(
            dice.rolled("fire team").len(),
            1,
            "the die the decider named was the only one re-drawn"
        );
    }

    /// A system with two non-station planets, so a relocation has somewhere to go.
    fn dual_arena() -> (GameState, SystemId, PlanetId, PlanetId) {
        let content = ContentStore::embedded();
        let (system, planets) = ti4_content::galaxy::all_systems(content, POK)
            .iter()
            .find_map(|(name, _)| {
                let record = content.get(ti4_model::content_types::ContentType::Systems, name)?;
                let planets: Vec<PlanetId> = record
                    .strings("planets")
                    .into_iter()
                    .map(PlanetId::new)
                    .filter(|planet| {
                        !ti4_content::galaxy::is_space_station(content, planet.as_str(), POK)
                    })
                    .collect();
                (planets.len() >= 2).then(|| (SystemId::new(*name), planets))
            })
            .expect("the corpus has a two-planet system");
        let mut state = start_game(content, &[invader(), holder()], POK, None).unwrap();
        // The driver activates the system before opening the invasion window; card effects
        // read the state's active system, as in play.
        state.active_system = Some(system.clone());
        (state, system, planets[0].clone(), planets[1].clone())
    }
    /// The parley test arena: the invader holds one infantry in space, the holder two on the
    /// planet he controls. The cardless arm ends in a ground round (one die for the
    /// invader's infantry, two for the defender's, all hitting on 8); the parley arm rolls
    /// nothing and is asked to commit twice — once before the landing, then again after
    /// Parley puts the unit back in space, where it can be committed again.
    fn parley_run(with_card: bool) -> (Vec<Vec<String>>, GameState) {
        let a = invader();
        let b = holder();
        let (mut state, system, planet) = arena();
        state
            .system_mut(&system)
            .set_control(planet.clone(), b.clone());
        on_planet(&mut state, &system, &planet, "infantry", &b, 2);
        in_space(&mut state, &system, "infantry", &a, 1);
        if with_card {
            state.player_mut(&b).unwrap().action_cards =
                vec![ti4_model::id::ActionCardId::new("parley")];
        }
        let (mut dice, mut rng) = if with_card {
            (Dice::new(), GameRng::new(5))
        } else {
            (Dice::from_faces([8u32, 8, 8]), GameRng::new(5))
        };
        let script: Vec<String> = if with_card {
            vec![
                format!("commit|0|{planet}"),
                "reaction:generic:UNITS_COMMITTED:after".to_owned(),
                "done_committing".to_owned(),
            ]
        } else {
            vec![format!("commit|0|{planet}"), "fight".to_owned()]
        };
        let asks = drive_invasion(
            &mut state,
            &system,
            Stage::Committing,
            InvasionReport::default(),
            script,
            &mut dice,
            &mut rng,
        );
        (asks, state)
    }

    #[test]
    fn parley_returns_the_committed_unit_to_space() {
        // Parley: "Return the committed units to the space area." The invader lands one
        // infantry on the holder's planet, which stands over the holder's two. With Parley
        // the landing unit returns to space before any combat — both defender infantry stand
        // and the invader's infantry survives; without the card both sides roll 8 and die.
        let a = invader();
        let b = holder();
        let run = |with_card: bool| parley_run(with_card);

        // The cardless landing: both 8s kill and the invader's unit dies on the planet. Both
        // arms ran on the same deterministic arena, so its coordinates can be re-derived.
        let (asks, state) = run(false);
        let (_, system, planet) = arena();
        let commit_offer = vec![format!("commit|0|{planet}"), "done_committing".to_owned()];
        let on_planet = state.system_state(&system).on_planet(&planet).to_vec();
        assert_eq!(
            on_planet.len(),
            1,
            "the invader's unit died; one defender infantry stands"
        );
        assert_eq!(on_planet[0].owner, b);
        assert_eq!(
            asks,
            vec![commit_offer.clone(), vec!["fight".to_owned()]],
            "commit, then the fight"
        );

        // With Parley: the landing unit is back in space, nothing is fought, both defender
        // infantry stand, the planet is still the holder's, and the invader took nothing.
        let (asks, state) = run(true);
        let on_planet = state.system_state(&system).on_planet(&planet).to_vec();
        assert_eq!(
            on_planet.len(),
            2,
            "both defender infantry stand; the committed unit never landed"
        );
        assert!(on_planet.iter().all(|unit| unit.owner == b));
        let in_space = state
            .system_state(&system)
            .units_of(&a)
            .into_iter()
            .filter(|unit| unit.type_id.as_str() == "infantry")
            .count();
        assert_eq!(in_space, 1, "the committed unit is back in the space area");
        assert_eq!(
            state.system_state(&system).planet_control.get(&planet),
            Some(&b),
            "the invader took nothing"
        );
        assert!(
            state.player(&b).unwrap().action_cards.is_empty(),
            "the card was spent"
        );
        assert_eq!(
            state.last_committed_unit, None,
            "Parley cleared the marker it acted on"
        );
        let reaction_offer = vec![
            "reaction:generic:UNITS_COMMITTED:after".to_owned(),
            "decline".to_owned(),
        ];
        assert_eq!(
            asks,
            vec![commit_offer.clone(), reaction_offer, commit_offer],
            "commit, the reaction offer, then the commit offer again and the pass"
        );
    }

    /// The ghost squad test arena: the holder controls two planets with two of his infantry
    /// on the first; the invader holds one infantry in space. The cardless arm ends in a
    /// ground round (one die for the invader, two for the defender, all hitting on 8); the
    /// card arm commits, answers the reaction, makes the move, then declines the reverse.
    fn ghost_run(with_card: bool) -> (Vec<Vec<String>>, GameState, SystemId, PlanetId, PlanetId) {
        let a = invader();
        let b = holder();
        let (mut state, system, planet, other) = dual_arena();
        state
            .system_mut(&system)
            .set_control(planet.clone(), b.clone());
        state
            .system_mut(&system)
            .set_control(other.clone(), b.clone());
        on_planet(&mut state, &system, &planet, "infantry", &b, 2);
        in_space(&mut state, &system, "infantry", &a, 1);
        if with_card {
            state.player_mut(&b).unwrap().action_cards =
                vec![ti4_model::id::ActionCardId::new("ghost_squad")];
        }
        let (mut dice, mut rng) = if with_card {
            (Dice::new(), GameRng::new(5))
        } else {
            (Dice::from_faces([8u32, 8, 8]), GameRng::new(5))
        };
        let commit = format!("commit|0|{planet}");
        let script: Vec<String> = if with_card {
            vec![
                commit,
                "reaction:generic:UNITS_COMMITTED:after".to_owned(),
                format!("move|{planet}|{other}|infantry"),
                "decline".to_owned(),
            ]
        } else {
            vec![commit, "fight".to_owned()]
        };
        let asks = drive_invasion(
            &mut state,
            &system,
            Stage::Committing,
            InvasionReport::default(),
            script,
            &mut dice,
            &mut rng,
        );
        (asks, state, system, planet, other)
    }

    #[test]
    fn ghost_squad_relocates_the_holders_forces_before_the_fight() {
        // Ghost Squad: "Move any number of your ground forces from any planet you control
        // in the active system to any other planet you control in the active system." The
        // invader lands on the planet holding the holder's two infantry. With the card the
        // holder moves both infantry to his other planet before any combat: the invader's
        // lone infantry takes the first planet alone. Without it, both sides roll 8, the
        // invader's unit dies, and the holder keeps the first planet.
        let a = invader();
        let b = holder();
        let run = |with_card: bool| ghost_run(with_card);

        // The cardless landing: both 8s kill, the invader's unit dies on the first planet,
        // one defender infantry stands, and the holder keeps both planets.
        let (asks, state, system, planet, other) = run(false);
        let on_planet = state.system_state(&system).on_planet(&planet).to_vec();
        assert_eq!(
            on_planet.len(),
            1,
            "the invader's unit died; one defender infantry stands"
        );
        assert_eq!(on_planet[0].owner, b);
        assert_eq!(
            state.system_state(&system).planet_control.get(&planet),
            Some(&b),
            "the invader held nothing of its own on the planet, so it took nothing"
        );
        assert!(
            state.system_state(&system).on_planet(&other).is_empty(),
            "nothing was moved"
        );
        let commit_offer = vec![
            format!("commit|0|{planet}"),
            format!("commit|0|{other}"),
            "done_committing".to_owned(),
        ];
        assert_eq!(
            asks,
            vec![commit_offer.clone(), vec!["fight".to_owned()]],
            "commit, then the fight; got {asks:?}"
        );

        // With Ghost Squad: both infantry move to the second planet before any combat; the
        // invader's infantry stands on the first planet alone and takes it.
        let (asks, state, _, _, _) = run(true);
        let on_planet = state.system_state(&system).on_planet(&planet).to_vec();
        assert_eq!(
            on_planet.len(),
            1,
            "only the invader's infantry stands on the first planet"
        );
        assert_eq!(on_planet[0].owner, a);
        let moved: Vec<Unit> = state.system_state(&system).on_planet(&other).to_vec();
        assert_eq!(moved.len(), 2, "both of the holder's infantry relocated");
        assert!(moved.iter().all(|unit| unit.owner == b));
        assert_eq!(
            state.system_state(&system).planet_control.get(&planet),
            Some(&a),
            "the invader holds the first planet: no defender ground was there to fight"
        );
        assert_eq!(
            state.system_state(&system).planet_control.get(&other),
            Some(&b),
            "the holder still controls the planet the forces moved to"
        );
        assert!(
            state.player(&b).unwrap().action_cards.is_empty(),
            "the card was spent"
        );
        let move_offer = |from: &PlanetId, to: &PlanetId| {
            vec![format!("move|{from}|{to}|infantry"), "decline".to_owned()]
        };
        let reaction_offer = vec![
            "reaction:generic:UNITS_COMMITTED:after".to_owned(),
            "decline".to_owned(),
        ];
        assert_eq!(
            asks,
            vec![
                commit_offer,
                reaction_offer,
                move_offer(&planet, &other),
                move_offer(&other, &planet),
            ],
            "commit, the reaction offer, the move, then the reverse move the holder declines; got {asks:?}"
        );
    }
    #[test]
    fn scramble_frequency_rerolls_all_of_the_rollers_dice() {
        // Scramble Frequency: "After another player makes a BOMBARDMENT, SPACE CANNON, or
        // ANTI-FIGHTER BARRAGE roll: That player rerolls all of their dice." The invader's
        // dreadnought (hits on 5) rolls an 8 (one kill); the holder scrambles it, and the
        // re-draw from the seeded stream (seed 1's first DICE draw is a 4) kills nothing.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let c = PlayerId::new("c");
        let (system, planet) = crate::fixtures::a_placed_planet();

        let run = |play: bool| -> (Vec<Vec<String>>, GameState, Dice) {
            let mut state = crate::fixtures::game(&["a", "b", "c"]);
            state
                .system_mut(&system)
                .set_control(planet.clone(), b.clone());
            crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &b, 2);
            crate::fixtures::put(&mut state, &system, "dreadnought", &a, 1);
            state.player_mut(&c).unwrap().action_cards =
                vec![ti4_model::id::ActionCardId::new("scramble")];
            let content = ContentStore::embedded();
            let mut dice = Dice::from_faces([8u32, 2]);
            let mut rng = GameRng::new(1);
            let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            let script: Vec<String> = if play {
                vec!["reaction:generic:UNIT_ABILITY_ROLLED:after".to_owned()]
            } else {
                vec!["decline".to_owned()]
            };
            let decider = RecordingScripted {
                inner: crate::choice::Scripted::new(script),
                seen: seen.clone(),
            };
            let mut table = Table::with_default(Box::new(decider));
            let mut resolver = armed_resolver(&state, vec![a.clone(), b.clone(), c.clone()]);
            let mut event_sequence = crate::event::EventSequence::new();
            let mut window =
                InvasionWindow::new(&mut state, content, POK, &mut dice, &mut rng, &a, &system);
            let mut ctx = Resolving {
                content,
                sources: POK,
                dice: &mut dice,
                rng: &mut rng,
                table: &mut table,
                timing: Some(crate::choice::TimingHandle {
                    resolver: &mut resolver,
                    sequence: &mut event_sequence,
                    galaxy: None,
                }),
            };
            while !window.is_done() {
                window
                    .drive(&mut state, &mut ctx)
                    .expect("the script only picks what is offered");
                while window.take_scoring_occurrence().is_some() {}
                window.settle(&mut state, &mut ctx);
            }
            let asks = seen.borrow().clone();
            let _ = window;
            (asks, state, dice)
        };

        // Declined: the 8 lands its one kill.
        let (asks, state, dice) = run(false);
        assert_eq!(
            asks,
            vec![vec![
                "reaction:generic:UNIT_ABILITY_ROLLED:after".to_owned(),
                "decline".to_owned()
            ]],
            "the roller is not offered the window, only the other players; got {asks:?}"
        );
        let left: Vec<Unit> = state.system_state(&system).on_planet(&planet).to_vec();
        assert_eq!(left.len(), 1, "one of the two infantry was killed by the 8");
        assert_eq!(dice.count(), 1, "a declined scramble consumes no dice");
        assert!(dice.rolled("scramble frequency").is_empty());

        // Played: every die the invader made is rerolled, and the 2 kills nothing.
        let (asks, state, dice) = run(true);
        assert_eq!(
            asks,
            vec![vec![
                "reaction:generic:UNIT_ABILITY_ROLLED:after".to_owned(),
                "decline".to_owned()
            ]],
            "one reaction offer; the lone card auto-plays and asks no die question, all dice reroll; got {asks:?}"
        );
        assert!(state.reroll_staging.is_empty());
        let left: Vec<Unit> = state.system_state(&system).on_planet(&planet).to_vec();
        assert_eq!(left.len(), 2, "the scramble saved both infantry");
        assert_eq!(
            dice.rolled("scramble frequency").len(),
            1,
            "the forced reroll ran through the game's roller"
        );
        assert!(
            state.player(&c).unwrap().action_cards.is_empty(),
            "the card was spent even though it saved the day"
        );
    }

    #[test]
    fn the_jolnar_commander_may_reroll_any_ability_die() {
        // Aglnlan Oln's commander: "After you roll dice for a unit ability: You may reroll
        // any of those dice." The invader's dreadnought (hits on 5) bombs with a 3 (no hits);
        // rerolled, the re-draw from the seeded stream (seed 4's first DICE draw is an 8)
        // takes one of the defender's two infantry. Declining rerolls nothing.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let (system, planet) = crate::fixtures::a_placed_planet();

        let run = |reroll: bool| -> (Vec<Vec<String>>, GameState, Dice) {
            let mut state = crate::fixtures::game(&["a", "b"]);
            state
                .system_mut(&system)
                .set_control(planet.clone(), b.clone());
            crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &b, 2);
            crate::fixtures::put(&mut state, &system, "dreadnought", &a, 1);
            state.player_mut(&a).unwrap().leaders.insert(
                ti4_model::id::LeaderId::new("jolnarcommander"),
                ti4_model::state::LeaderStatus::Unlocked,
            );
            let content = ContentStore::embedded();
            let mut dice = Dice::from_faces([3u32]);
            let mut rng = GameRng::new(4);
            let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            let script: Vec<String> = if reroll {
                vec!["reroll|0:0".to_owned()]
            } else {
                vec!["decline".to_owned()]
            };
            let decider = RecordingScripted {
                inner: crate::choice::Scripted::new(script),
                seen: seen.clone(),
            };
            let mut table = Table::with_default(Box::new(decider));
            let mut resolver = armed_resolver(&state, vec![a.clone(), b.clone()]);
            let mut event_sequence = crate::event::EventSequence::new();
            let mut window =
                InvasionWindow::new(&mut state, content, POK, &mut dice, &mut rng, &a, &system);
            let mut ctx = Resolving {
                content,
                sources: POK,
                dice: &mut dice,
                rng: &mut rng,
                table: &mut table,
                timing: Some(crate::choice::TimingHandle {
                    resolver: &mut resolver,
                    sequence: &mut event_sequence,
                    galaxy: None,
                }),
            };
            while !window.is_done() {
                window
                    .drive(&mut state, &mut ctx)
                    .expect("the script only picks what is offered");
                while window.take_scoring_occurrence().is_some() {}
                window.settle(&mut state, &mut ctx);
            }
            let asks = seen.borrow().clone();
            let _ = window;
            (asks, state, dice)
        };

        // Rerolled: the 3 is re-drawn to an 8 (seed 4) and takes one infantry.
        let (asks, state, dice) = run(true);
        assert_eq!(
            asks,
            vec![vec!["reroll|0:0".to_owned(), "decline".to_owned()]],
            "one optional die question, asked of the roller; got {asks:?}"
        );
        let left: Vec<Unit> = state.system_state(&system).on_planet(&planet).to_vec();
        assert_eq!(left.len(), 1, "the reroll landed its one hit");
        assert_eq!(dice.rolled("jolnar commander").len(), 1);
        assert!(state.reroll_staging.is_empty());

        // Declined: the 3 rolls no hits and nothing is rerolled.
        let (asks, state, dice) = run(false);
        assert_eq!(
            asks,
            vec![vec!["reroll|0:0".to_owned(), "decline".to_owned()]],
            "the question is offered either way; got {asks:?}"
        );
        let left: Vec<Unit> = state.system_state(&system).on_planet(&planet).to_vec();
        assert_eq!(left.len(), 2, "a declined reroll changes nothing");
        assert!(dice.rolled("jolnar commander").is_empty());
        assert_eq!(dice.count(), 1);
    }
}
