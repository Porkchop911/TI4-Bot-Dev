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
    !board.on_planet(planet).iter().any(|unit| {
        types
            .get(unit.type_id.as_str())
            .is_some_and(UnitType::planetary_shield)
    })
}

/// 49.1: the invader's bombarding ships fire at ground forces on the planets below.
///
/// Returns how many ground forces were destroyed.
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
    system: &SystemId,
    invader: &PlayerId,
) -> usize {
    let occurrence = state.begin_feat_occurrence();
    bombardment_at(
        state, content, sources, dice, rng, system, invader, occurrence,
    )
    .0
}

#[allow(
    clippy::too_many_arguments,
    reason = "one parameter per distinct invasion input"
)]
fn bombardment_at(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    dice: &mut Dice,
    rng: &mut GameRng,
    system: &SystemId,
    invader: &PlayerId,
    occurrence: FeatOccurrence,
) -> (usize, bool) {
    let types = catalogue(content, sources);
    let planets: Vec<PlanetId> = state
        .system_state(system)
        .planet_units
        .keys()
        .cloned()
        .collect();

    let mut killed = 0;
    let mut noted = false;
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

        let mut hits = 0;
        for unit in state.system_state(system).units_of(invader) {
            let Some(kind) = types.get(unit.type_id.as_str()) else {
                continue;
            };
            let Some(value) = kind.bombard_hits_on() else {
                continue;
            };
            let count = usize::try_from(kind.bombard_dice()).unwrap_or(0);
            if count == 0 {
                continue;
            }
            let roll = dice.roll(
                rng,
                count,
                "bombardment",
                Some(u32::try_from(value).unwrap_or(u32::MAX)),
            );
            hits += roll.hits();
        }

        // Make an Example of Their World asks for the last ground force on a planet and asks for
        // it during this step. Counted here rather than after the invasion, because ground
        // combat clears planets too and the empty planet afterwards does not say which step
        // emptied it.
        let held = defenders.len();
        let victims: std::collections::BTreeSet<PlayerId> =
            defenders.iter().map(|unit| unit.owner.clone()).collect();
        let mut taken = 0;
        for doomed in defenders.into_iter().take(hits) {
            state
                .system_mut(system)
                .remove_from_planet(&planet, std::slice::from_ref(&doomed));
            killed += 1;
            taken += 1;
        }
        if taken == held
            && victims.len() == 1
            && state
                .system_state(system)
                .on_planet(&planet)
                .iter()
                .all(|unit| &unit.owner == invader)
        {
            state.record_event_feat(invader, Feat::BombardedOutTheLastGroundForces, occurrence);
            noted = true;
        }
    }
    (killed, noted)
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
    ti4_content::galaxy::planets_in(content, system.as_str(), sources)
        .into_iter()
        .map(|planet| PlanetId::new(planet.id()))
        .filter(|planet| planet.as_str() != "mr" || state.custodians_removed)
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
    if ti4_content::galaxy::planets_in(content, system.as_str(), sources).is_empty() {
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
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    dice: &mut Dice,
    rng: &mut GameRng,
    player: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) -> usize {
    let types = catalogue(content, sources);
    let mut fighting: std::collections::BTreeMap<i64, i64> = std::collections::BTreeMap::new();
    for unit in state.system_state(system).on_planet_of(planet, player) {
        let Some(kind) = types.get(unit.type_id.as_str()) else {
            continue;
        };
        let Some(value) = kind.combat_hits_on() else {
            continue;
        };
        *fighting.entry(value).or_insert(0) += kind.combat_dice();
    }
    let mut hits = 0;
    for (value, count) in fighting {
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
        let harrow = crate::faction_abilities::ground_combat_round_ended(
            state, content, sources, dice, rng, invader, system,
        );
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
        captured.push((planet.clone(), previous));
    }
    captured
}

/// Where an open invasion has reached.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Stage {
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
/// Bombardment happens when the window opens: it involves no choices, and 49.1 puts it before
/// ground forces are committed, so deferring it would let a player commit knowing what a
/// bombardment they had not yet suffered was going to do.
#[derive(Debug, Clone)]
pub struct InvasionWindow {
    invader: PlayerId,
    system: SystemId,
    stage: Stage,
    report: InvasionReport,
    pending_scoring_occurrences: std::collections::VecDeque<(FeatOccurrence, bool)>,
    current_ground_occurrence: Option<FeatOccurrence>,
    notes_at_tactical_start: crate::combat::NoteHoldings,
}

impl InvasionWindow {
    /// Open an invasion, resolving bombardment immediately.
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
        let (kills, noted) = bombardment_at(
            state, content, sources, dice, rng, system, invader, occurrence,
        );
        Self {
            invader: invader.clone(),
            system: system.clone(),
            stage: Stage::Custodians,
            report: InvasionReport {
                bombardment_kills: kills,
                ..InvasionReport::default()
            },
            pending_scoring_occurrences: noted.then_some((occurrence, false)).into_iter().collect(),
            current_ground_occurrence: None,
            notes_at_tactical_start,
        }
    }

    #[must_use]
    pub fn take_scoring_occurrence(&mut self) -> Option<(FeatOccurrence, bool)> {
        self.pending_scoring_occurrences.pop_front()
    }

    pub fn settle(&mut self, state: &mut GameState, ctx: &mut Resolving<'_>) {
        loop {
            match self.stage.clone() {
                Stage::Advancing { planets, index } => {
                    self.advance_fighting(state, ctx, &planets, index);
                    return;
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

    fn resolve_ground_round(
        &mut self,
        state: &mut GameState,
        ctx: &mut Resolving<'_>,
        planets: Vec<PlanetId>,
        index: usize,
        defender: PlayerId,
    ) {
        let (content, sources) = (ctx.content, ctx.sources);
        // 42.2: hits are simultaneous, so both sides roll before either loses anything.
        let planet = planets[index].clone();
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

        // 42.3: the fight ends when one side has no ground forces left on the planet — not
        // when its last structure falls, because structures never fight (KD-2).
        let owners = ground_force_owners(state, content, sources, &self.system, &planet);
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
            if let Some(defender) = owners.iter().find(|owner| **owner != self.invader).cloned() {
                self.current_ground_occurrence = Some(state.begin_feat_occurrence());
                self.stage = Stage::Fighting {
                    planets: planets.to_vec(),
                    index,
                    defender,
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
        // announced before the exploration that follows it.
        let mut payload = std::collections::BTreeMap::new();
        payload.insert("player".to_owned(), self.invader.to_string().into());
        payload.insert("planet".to_owned(), planet.to_string().into());
        payload.insert("system".to_owned(), self.system.to_string().into());
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
    fn pending_choice(
        &self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
    ) -> Option<Choice> {
        match &self.stage {
            Stage::Done | Stage::Advancing { .. } | Stage::FinalizingControl { .. } => None,
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
            Stage::Done | Stage::Advancing { .. } | Stage::FinalizingControl { .. } => {}
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
                        state
                            .system_mut(&self.system)
                            .remove(std::slice::from_ref(&unit));
                        state
                            .system_mut(&self.system)
                            .planet_units
                            .entry(planet.clone())
                            .or_default()
                            .push(unit);
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
    use ti4_model::content_types::POK;
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
        let (_, mut dice, mut rng) = kit();

        bombardment(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut dice,
            &mut rng,
            &system,
            &invader(),
        );

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

        for _ in 0..2 {
            on_planet(&mut state, &system, &planet, "infantry", &holder(), 1);
            assert_eq!(
                bombardment(
                    &mut state,
                    ContentStore::embedded(),
                    POK,
                    &mut dice,
                    &mut rng,
                    &system,
                    &invader(),
                ),
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
        let (_, mut dice, mut rng) = kit();

        let killed = bombardment(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut dice,
            &mut rng,
            &system,
            &invader(),
        );

        assert_eq!(killed, 0);
        assert_eq!(dice.count(), 0, "nothing to shoot at, so no dice");
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
            let planets: Vec<PlanetId> = ti4_content::galaxy::planets_in(content, system, POK)
                .iter()
                .map(|planet| PlanetId::new(planet.id()))
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
        let (_table, mut dice, mut rng) = kit();

        let window = InvasionWindow::new(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut dice,
            &mut rng,
            &invader(),
            &system,
        );
        let choice = window
            .pending_choice(&state, ContentStore::embedded(), POK)
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
}
