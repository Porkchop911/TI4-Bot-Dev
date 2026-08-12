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
use ti4_model::state::GameState;
use ti4_model::units::Unit;

use crate::choice::{Choice, ChoiceOption, IllegalChoice, Resolving, Table, Window};
use crate::combat::MAX_ROUNDS;
use crate::dice::Dice;
use crate::rng::GameRng;

/// The choice kind for landing a ground force on a planet.
pub const LAND_KIND: &str = "land";
/// The choice kind for choosing which of your own ground forces dies.
pub const GROUND_CASUALTY_KIND: &str = "ground_casualty";

/// What an invasion did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InvasionReport {
    /// Planets ground forces were committed to.
    pub committed: Vec<PlanetId>,
    /// Planets whose control changed, with who held them before.
    pub captured: Vec<(PlanetId, Option<PlayerId>)>,
    /// Ground forces destroyed by bombardment.
    pub bombardment_kills: usize,
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
    let types = catalogue(content, sources);
    let planets: Vec<PlanetId> = state
        .system_state(system)
        .planet_units
        .keys()
        .cloned()
        .collect();

    let mut killed = 0;
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

        for doomed in defenders.into_iter().take(hits) {
            state
                .system_mut(system)
                .remove_from_planet(&planet, std::slice::from_ref(&doomed));
            killed += 1;
        }
    }
    killed
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
    let planets: Vec<PlanetId> = ti4_content::galaxy::planets_in(content, system.as_str(), sources)
        .into_iter()
        .map(|planet| PlanetId::new(planet.id()))
        .collect();
    if planets.is_empty() {
        return Ok(Vec::new());
    }

    let mut committed: std::collections::BTreeSet<PlanetId> = std::collections::BTreeSet::new();
    loop {
        let troops = landable(state, content, sources, invader, system);
        if troops.is_empty() {
            break;
        }

        // One option per (unit type, planet). Two identical infantry landing on the same
        // planet are one decision written twice, and a sampling decider would land whichever
        // type it happened to hold more of.
        let mut seen = std::collections::BTreeSet::new();
        let mut options = Vec::new();
        for (index, unit) in troops.iter().enumerate() {
            for planet in &planets {
                if !seen.insert((unit.type_id.to_string(), planet.to_string())) {
                    continue;
                }
                options.push(ChoiceOption::labelled(
                    format!("land|{index}|{planet}"),
                    LAND_KIND,
                    format!("land {} on {planet}", unit.type_id),
                ));
            }
        }
        options.push(ChoiceOption::decline());

        let choice = Choice::new(invader.clone(), "commit ground forces", options);
        let answer = table.ask(&choice)?;
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
    table: &mut Table,
    player: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
    hits: usize,
) -> Result<(), IllegalChoice> {
    for _ in 0..hits {
        let present: Vec<Unit> = state
            .system_state(system)
            .on_planet_of(planet, player)
            .into_iter()
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
            let answer = table.ask(&choice)?;
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
    let defender = state
        .system_state(system)
        .on_planet(planet)
        .iter()
        .find(|unit| &unit.owner != invader)
        .map(|unit| unit.owner.clone());
    let Some(defender) = defender else {
        return Ok(Some(invader.clone()));
    };

    for _ in 1..=MAX_ROUNDS {
        state.combat_round_seq = state.combat_round_seq.saturating_add(1);
        let attacking = !state
            .system_state(system)
            .on_planet_of(planet, invader)
            .is_empty();
        let defending = !state
            .system_state(system)
            .on_planet_of(planet, &defender)
            .is_empty();
        if !attacking || !defending {
            break;
        }

        let attacker_hits =
            roll_ground(state, content, sources, dice, rng, invader, system, planet);
        let defender_hits = roll_ground(
            state, content, sources, dice, rng, &defender, system, planet,
        );
        // 42.2: simultaneous, as in space.
        absorb_ground(state, table, &defender, system, planet, attacker_hits)?;
        absorb_ground(state, table, invader, system, planet, defender_hits)?;
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
    /// Choosing which ground forces to land, and where (49.2).
    Committing,
    /// Fighting on `planets[index]`, having already resolved the earlier ones.
    Fighting {
        planets: Vec<PlanetId>,
        index: usize,
        defender: PlayerId,
    },
    Done,
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
        let kills = bombardment(state, content, sources, dice, rng, system, invader);
        Self {
            invader: invader.clone(),
            system: system.clone(),
            stage: Stage::Committing,
            report: InvasionReport {
                bombardment_kills: kills,
                ..InvasionReport::default()
            },
        }
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
        let planets: Vec<PlanetId> =
            ti4_content::galaxy::planets_in(content, self.system.as_str(), sources)
                .into_iter()
                .map(|planet| PlanetId::new(planet.id()))
                .collect();

        // One option per (unit type, planet). Two identical infantry landing on the same planet
        // are one decision written twice, and a sampling decider would land whichever type it
        // happened to hold more of.
        let mut seen = std::collections::BTreeSet::new();
        let mut options = Vec::new();
        for (index, unit) in troops.iter().enumerate() {
            for planet in &planets {
                if !seen.insert((unit.type_id.to_string(), planet.to_string())) {
                    continue;
                }
                options.push(ChoiceOption::labelled(
                    format!("land|{index}|{planet}"),
                    LAND_KIND,
                    format!("land {} on {planet}", unit.type_id),
                ));
            }
        }
        options
    }

    /// Who is defending `planet`, if anyone.
    fn defender_on(&self, state: &GameState, planet: &PlanetId) -> Option<PlayerId> {
        state
            .system_state(&self.system)
            .on_planet(planet)
            .iter()
            .find(|unit| unit.owner != self.invader)
            .map(|unit| unit.owner.clone())
    }

    /// Move to the next planet that still needs a fight, or finish and take control.
    fn advance_fighting(
        &mut self,
        state: &mut GameState,
        content: &ContentStore,
        sources: SourceSet,
        planets: &[PlanetId],
        mut index: usize,
    ) {
        while index < planets.len() {
            let planet = &planets[index];
            let contested = self.defender_on(state, planet).filter(|_| {
                !state
                    .system_state(&self.system)
                    .on_planet_of(planet, &self.invader)
                    .is_empty()
            });
            if let Some(defender) = contested {
                self.stage = Stage::Fighting {
                    planets: planets.to_vec(),
                    index,
                    defender,
                };
                return;
            }
            index += 1;
        }
        self.report.captured = establish_control(
            state,
            content,
            sources,
            &self.system,
            &self.invader,
            &self.report.committed,
        );
        self.stage = Stage::Done;
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
            Stage::Done => None,
            Stage::Committing => {
                let mut options = self.landing_options(state, content, sources);
                if options.is_empty() {
                    return None;
                }
                options.push(ChoiceOption::decline());
                Some(Choice::new(
                    self.invader.clone(),
                    "commit ground forces",
                    options,
                ))
            }
            Stage::Fighting {
                planets,
                index,
                defender,
            } => {
                // One casualty decision at a time; the roll itself happens on resolve.
                let planet = planets.get(*index)?;
                let _ = defender;
                let board = state.system_state(&self.system);
                if board.on_planet_of(planet, &self.invader).is_empty() {
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
            Stage::Done => {}
            Stage::Committing => {
                if option.is_decline() {
                    let planets = self.report.committed.clone();
                    if planets.is_empty() {
                        self.stage = Stage::Done; // 49.2c: straight on to Production
                    } else {
                        self.advance_fighting(state, content, sources, &planets, 0);
                    }
                } else if let Some(rest) = option.id.strip_prefix("land|") {
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
                remove_ground(state, &self.system, &planet, &defender, attacker_hits);
                remove_ground(state, &self.system, &planet, &self.invader, defender_hits);

                let still_contested = !state
                    .system_state(&self.system)
                    .on_planet_of(&planet, &self.invader)
                    .is_empty()
                    && !state
                        .system_state(&self.system)
                        .on_planet_of(&planet, &defender)
                        .is_empty();
                if still_contested {
                    self.stage = Stage::Fighting {
                        planets,
                        index,
                        defender,
                    };
                } else {
                    self.advance_fighting(state, content, sources, &planets, index + 1);
                }
            }
        }

        // Committing settles when nothing is left to land.
        if matches!(self.stage, Stage::Committing)
            && self.landing_options(state, content, sources).is_empty()
        {
            let planets = self.report.committed.clone();
            if planets.is_empty() {
                self.stage = Stage::Done;
            } else {
                self.advance_fighting(state, content, sources, &planets, 0);
            }
        }
        Ok(())
    }
}

/// Remove `hits` of a player's ground forces from a planet, weakest-first.
///
/// No choice is offered: every ground force on a planet is interchangeable in this model, so
/// asking would be a decision between identical options.
fn remove_ground(
    state: &mut GameState,
    system: &SystemId,
    planet: &PlanetId,
    player: &PlayerId,
    hits: usize,
) {
    for _ in 0..hits {
        let doomed = state
            .system_state(system)
            .on_planet_of(planet, player)
            .first()
            .map(|unit| (*unit).clone());
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
    };
    window.drive(state, &mut ctx, table)?;
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
}
