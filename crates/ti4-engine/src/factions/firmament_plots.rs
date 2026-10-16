//! The five plot cards' effects (Thunder's Edge; `genericcards.json`, `cardType` plot), for The
//! Obsidian whose in-play plot cards are faceup. See `plans/evidence/BF-obsidian.md`.
//!
//! > Enervate: "You can perform the secondary abilities of the puppeted player's strategy cards
//! > without spending command tokens; for "Leadership," you can perform the primary ability instead."
//! >
//! > Siphon: "When the puppeted player gains commodities, you gain an equal number of trade goods."
//! >
//! > Seethe: "When this card is revealed, destroy all units on a non-home planet controlled by the
//! > puppeted player. At the start of the status phase, destroy 1 of the puppeted player's infantry
//! > in any system."
//! >
//! > Assail: "Apply +1 to the results of each of your combat and unit ability rolls against the
//! > puppeted player."
//! >
//! > Extract: "When this card is revealed, gain 1 non-faction technology owned by the puppeted
//! > player. When the puppeted player gains a non-faction technology, you may spend 4 resources to
//! > gain that technology."
//!
//! Marionettes: "The player or players whose control tokens are on each plot card are the puppeted
//! players for that plot", so every effect reads the tokens on its own card
//! ([`firmament::puppets_of`]) and applies to each player with a token on it. A facedown plot has no
//! effect; a card is *revealed* when the flip (`firmament_flip::become_obsidian`) turns it faceup,
//! which is announced as `FACTION_FLIPPED`, where the "when this card is revealed" effects hang.
//! Only "may" clauses are optional (Extract's purchase).
//!
//! # Seams
//!
//! * Enervate: `StrategyHooks::secondary_waivers` (a token-free follow) and `substitutes_primary`
//!   (Leadership's primary instead of its secondary, chosen through the extra follow option
//!   [`PRIMARY_INSTEAD_ID`] the follower window offers).
//! * Siphon: the staged event `COMMODITIES_GAINED` (`player`, `amount`), staged by
//!   `supply::note_commodities_gained` at every site that gives commodities.
//! * Assail: [`assail_space`] and [`assail_ground`] beside `unit_roll_modifier` in the combat
//!   thresholds, and [`assail_against`] at the unit-ability rolls (space cannon, anti-fighter
//!   barrage, bombardment, space cannon defense).
//! * Extract's purchase: `TECHNOLOGY_GAINED`, paid through `production::pay_seeing`.

use std::collections::BTreeSet;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_content::units::UnitType;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlanetId, PlayerId, SystemId, TechnologyId};
use ti4_model::state::GameState;
use ti4_model::units::Unit;

use super::firmament::{self, ask};
use super::hooks_strategy::SecondaryWaiver;
use super::obsidian::{self, Victim};
use crate::choice::ChoiceOption;
use crate::production::Spend;
use crate::timing::{Ability, Relation, TimingContext, TimingError};

/// Enervate.
pub const ENERVATE: &str = "enervate";
/// Siphon.
pub const SIPHON: &str = "siphon";
/// Seethe.
pub const SEETHE: &str = "seethe";
/// Assail.
pub const ASSAIL: &str = "assail";
/// Extract.
pub const EXTRACT: &str = "extract";

/// The follow option that performs Leadership's primary ability instead of its secondary (Enervate).
pub const PRIMARY_INSTEAD_ID: &str = "follow|enervate_primary";
/// `firmament:enervate` = the follower who chose the primary instead, until the next follower answers.
const ENERVATE_MARK: &str = "firmament:enervate";
/// The staged event a commodity gain announces (read by Siphon).
pub const COMMODITIES_GAINED: &str = "COMMODITIES_GAINED";
/// The staged event the flip announces (the cards are revealed).
const FLIPPED: &str = "FACTION_FLIPPED";

/// Resource price of Extract's purchase.
const EXTRACT_COST: i64 = 4;

fn illegal(error: crate::choice::IllegalChoice) -> TimingError {
    TimingError::IllegalChoice(error)
}

/// The players with a token on a faceup `card` of `owner`, if `owner` plays the Obsidian.
#[must_use]
pub fn puppets(state: &GameState, owner: &PlayerId, card: &str) -> BTreeSet<PlayerId> {
    if obsidian::is_obsidian(state, owner) {
        firmament::puppets_of(state, owner, card)
    } else {
        BTreeSet::new()
    }
}

/// Every Obsidian seat that has `card` faceup with `player`'s token on it.
fn watchers(state: &GameState, card: &str, player: &PlayerId) -> Vec<PlayerId> {
    state
        .seating_order
        .iter()
        .filter(|owner| puppets(state, owner, card).contains(player))
        .cloned()
        .collect()
}

// -- Enervate --------------------------------------------------------------------------------------

/// Whether the Obsidian `follower` may follow `primary`'s strategic action without a command token.
pub(crate) fn secondary_waivers(
    state: &GameState,
    _content: &ContentStore,
    follower: &PlayerId,
    primary: &PlayerId,
    card: &str,
) -> Vec<SecondaryWaiver> {
    if follower != primary
        && !card.eq_ignore_ascii_case("leadership")
        && puppets(state, follower, ENERVATE).contains(primary)
    {
        vec![SecondaryWaiver {
            id: "enervate".to_owned(),
            label: format!("resolve {primary}'s secondary without a command token (Enervate)"),
        }]
    } else {
        Vec::new()
    }
}

/// The waiver costs nothing.
pub(crate) fn secondary_waived(
    _state: &mut GameState,
    _content: &ContentStore,
    _follower: &PlayerId,
    _primary: &PlayerId,
    _waiver: &str,
) {
}

/// Whether `follower` is offered Leadership's primary ability instead of its secondary: they play
/// the Obsidian, `primary` is a puppeted player of their Enervate, and the card is Leadership.
#[must_use]
pub fn primary_instead_offered(
    state: &GameState,
    follower: &PlayerId,
    primary: &PlayerId,
    card_name: Option<&str>,
) -> bool {
    follower != primary
        && card_name.is_some_and(|name| name.eq_ignore_ascii_case("leadership"))
        && puppets(state, follower, ENERVATE).contains(primary)
}

/// The follower window recorded `follower`'s answer: remember that they chose the primary instead
/// (or forget any earlier choice).
pub fn record_follow_answer(state: &mut GameState, follower: &PlayerId, option: &str) {
    if option == PRIMARY_INSTEAD_ID {
        state
            .faction_marks
            .insert(ENERVATE_MARK.to_owned(), follower.to_string());
    } else {
        state.faction_marks.remove(ENERVATE_MARK);
    }
}

/// Leadership's secondary is resolved as its primary for a follower who chose that.
pub(crate) fn substitutes_primary(
    state: &GameState,
    _content: &ContentStore,
    player: &PlayerId,
    card: &str,
) -> bool {
    card.eq_ignore_ascii_case("leadership")
        && state.faction_marks.get(ENERVATE_MARK).map(String::as_str) == Some(player.as_str())
}

// -- Siphon ----------------------------------------------------------------------------------------

/// Whether a Siphon watches `player`'s commodities, so a gain is worth announcing.
#[must_use]
pub fn siphon_watches(state: &GameState, player: &PlayerId) -> bool {
    !watchers(state, SIPHON, player).is_empty()
}

/// Siphon: when a puppeted player gains commodities, the owner gains as many trade goods.
fn siphon(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("ability:{owner_name}:plot_{SIPHON}:{COMMODITIES_GAINED}:after"),
        seat.clone(),
        COMMODITIES_GAINED,
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let amount = event
                .integer("amount")
                .and_then(|amount| i32::try_from(amount).ok())
                .unwrap_or(0);
            if amount > 0 {
                crate::supply::gain_trade_goods_staged(context.state, &owner, amount, SIPHON);
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event.text("player").is_some_and(|gainer| {
            puppets(context.state, &condition_owner, SIPHON).contains(&PlayerId::new(gainer))
        }) && event.integer("amount").is_some_and(|amount| amount > 0)
    }))
}

// -- Assail ----------------------------------------------------------------------------------------

/// +1 to `owner`'s combat and unit ability rolls against any of `targets`, if `owner` has Assail
/// faceup with a token of one of them on it.
#[must_use]
pub fn assail_against<'a>(
    state: &GameState,
    owner: &PlayerId,
    targets: impl IntoIterator<Item = &'a PlayerId>,
) -> i64 {
    let puppeted = puppets(state, owner, ASSAIL);
    i64::from(targets.into_iter().any(|target| puppeted.contains(target)))
}

/// Assail for `player`'s ship rolls in the combat in `system`: against the other side.
#[must_use]
pub fn assail_space(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: Option<&SystemId>,
) -> i64 {
    let Some(system) = system else {
        return 0;
    };
    if puppets(state, player, ASSAIL).is_empty() {
        return 0;
    }
    let sides = crate::combat::combatants(state, content, sources, system);
    assail_against(state, player, sides.iter().filter(|side| *side != player))
}

/// Assail for `player`'s ground rolls on `planet`: against the other players' units there.
#[must_use]
pub fn assail_ground(
    state: &GameState,
    player: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) -> i64 {
    if puppets(state, player, ASSAIL).is_empty() {
        return 0;
    }
    let others: BTreeSet<PlayerId> = state
        .system_state(system)
        .on_planet(planet)
        .iter()
        .filter(|unit| &unit.owner != player)
        .map(|unit| unit.owner.clone())
        .collect();
    assail_against(state, player, others.iter())
}

// -- Seethe ----------------------------------------------------------------------------------------

/// The non-home planets `puppet` controls: planets outside every home system.
fn seethe_planets(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    puppet: &PlayerId,
) -> Vec<(SystemId, PlanetId)> {
    let homes = ti4_content::galaxy::home_systems(content, sources);
    let own_home = state
        .player(puppet)
        .and_then(|seat| seat.home_system.clone());
    state
        .controlled_planets(puppet)
        .into_iter()
        .filter(|(system, _)| {
            !homes.contains(system.as_str()) && Some(*system) != own_home.as_ref()
        })
        .map(|(system, planet)| (system.clone(), planet.clone()))
        .collect()
}

/// Destroy every unit on `planet`, announcing the ground forces among them.
fn destroy_all_on(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
    planet: &PlanetId,
) {
    let types = ti4_content::units::catalogue(content, sources);
    let units: Vec<Unit> = state.system_state(system).on_planet(planet).to_vec();
    state.system_mut(system).remove_from_planet(planet, &units);
    for unit in &units {
        if types
            .get(unit.type_id.as_str())
            .is_some_and(UnitType::is_ground_force)
        {
            super::hooks_ground::stage_ground_force_destroyed(
                state,
                system,
                planet,
                unit,
                "plot_seethe",
            );
        }
    }
}

/// Seethe, revealed: the owner chooses a non-home planet `puppet` controls (asked only when there
/// are several) and every unit on it is destroyed.
fn seethe_reveal(
    context: &mut TimingContext<'_>,
    owner: &PlayerId,
    puppet: &PlayerId,
) -> Result<(), TimingError> {
    let planets = seethe_planets(context.state, context.content, context.sources, puppet);
    let (system, planet) = match planets.as_slice() {
        [] => return Ok(()),
        [only] => only.clone(),
        _ => {
            let options = planets
                .iter()
                .map(|(system, planet)| {
                    ChoiceOption::labelled(
                        format!("{system}|{planet}"),
                        "planet",
                        format!("destroy every unit on {planet} in {system}"),
                    )
                })
                .collect();
            let answer = ask(
                context,
                owner,
                format!("Seethe: destroy all units on a non-home planet {puppet} controls"),
                firmament_card(SEETHE),
                "seethe_planet",
                options,
            )?;
            let Some(chosen) = planets
                .into_iter()
                .find(|(system, planet)| format!("{system}|{planet}") == answer.id)
            else {
                return Ok(());
            };
            chosen
        }
    };
    destroy_all_on(
        context.state,
        context.content,
        context.sources,
        &system,
        &planet,
    );
    Ok(())
}

/// The id a plot card's decisions carry.
fn firmament_card(card: &str) -> &str {
    match card {
        SEETHE => "plot_seethe",
        EXTRACT => "plot_extract",
        _ => "plot",
    }
}

/// Every infantry of `puppet`, in space or on a planet, as destroyable victims.
fn infantry_of(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    puppet: &PlayerId,
) -> Vec<Victim> {
    let types = ti4_content::units::catalogue(content, sources);
    let mut found: BTreeSet<Victim> = BTreeSet::new();
    for (system, board) in &state.board {
        for unit in &board.units {
            if &unit.owner == puppet && obsidian::is_infantry(&types, unit) {
                found.insert((
                    system.clone(),
                    None,
                    puppet.clone(),
                    unit.type_id.to_string(),
                ));
            }
        }
        for (planet, units) in &board.planet_units {
            for unit in units {
                if &unit.owner == puppet && obsidian::is_infantry(&types, unit) {
                    found.insert((
                        system.clone(),
                        Some(planet.clone()),
                        puppet.clone(),
                        unit.type_id.to_string(),
                    ));
                }
            }
        }
    }
    found.into_iter().collect()
}

/// Seethe, start of the status phase: destroy 1 of `puppet`'s infantry in any system; the owner
/// chooses which when there is a choice.
fn seethe_infantry(
    context: &mut TimingContext<'_>,
    owner: &PlayerId,
    puppet: &PlayerId,
) -> Result<(), TimingError> {
    let victims = infantry_of(context.state, context.content, context.sources, puppet);
    let chosen = match victims.as_slice() {
        [] => return Ok(()),
        [only] => only.clone(),
        _ => {
            let options = victims
                .iter()
                .map(|victim| {
                    let (system, planet, who, kind) = victim;
                    ChoiceOption::labelled(
                        obsidian::victim_id(victim),
                        "infantry",
                        format!(
                            "destroy {who}'s {kind} {}",
                            planet.as_ref().map_or_else(
                                || format!("in the space area of {system}"),
                                |planet| format!("on {planet} in {system}"),
                            )
                        ),
                    )
                })
                .collect();
            let answer = ask(
                context,
                owner,
                format!("Seethe: destroy 1 of {puppet}'s infantry"),
                firmament_card(SEETHE),
                "seethe_infantry",
                options,
            )?;
            let Some(victim) = victims
                .into_iter()
                .find(|victim| obsidian::victim_id(victim) == answer.id)
            else {
                return Ok(());
            };
            victim
        }
    };
    obsidian::destroy_infantry(context.state, &chosen, "plot_seethe");
    Ok(())
}

/// Seethe's second sentence: at the start of the status phase, for each puppeted player.
fn seethe_status(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("ability:{owner_name}:plot_{SEETHE}:STATUS_PHASE_BEGAN:after"),
        seat.clone(),
        "STATUS_PHASE_BEGAN",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            for puppet in puppets(context.state, &owner, SEETHE) {
                seethe_infantry(context, &owner, &puppet)?;
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        puppets(context.state, &condition_owner, SEETHE)
            .iter()
            .any(|puppet| {
                !infantry_of(context.state, context.content, context.sources, puppet).is_empty()
            })
    }))
}

// -- Extract ---------------------------------------------------------------------------------------

/// Non-faction technologies `puppet` owns that `owner` does not.
fn extractable(
    state: &GameState,
    content: &ContentStore,
    owner: &PlayerId,
    puppet: &PlayerId,
) -> Vec<TechnologyId> {
    let (Some(mine), Some(theirs)) = (state.player(owner), state.player(puppet)) else {
        return Vec::new();
    };
    theirs
        .technologies
        .iter()
        .filter(|tech| {
            !mine.technologies.contains(*tech)
                && crate::technology::faction_of(content, tech).is_none()
        })
        .cloned()
        .collect()
}

/// Gain (not research) a technology: it is granted, and a unit upgrade reaches the units on the board.
fn gain_technology(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    owner: &PlayerId,
    tech: &TechnologyId,
) {
    crate::technology::gain(state, content, sources, owner, tech);
}

/// Extract, revealed: gain 1 non-faction technology `puppet` owns; the owner chooses which.
fn extract_reveal(
    context: &mut TimingContext<'_>,
    owner: &PlayerId,
    puppet: &PlayerId,
) -> Result<(), TimingError> {
    let techs = extractable(context.state, context.content, owner, puppet);
    let chosen = match techs.as_slice() {
        [] => return Ok(()),
        [only] => only.clone(),
        _ => {
            let options = techs
                .iter()
                .map(|tech| {
                    ChoiceOption::labelled(
                        tech.to_string(),
                        "technology",
                        format!(
                            "gain {} from {puppet}",
                            crate::technology::name(context.content, tech)
                        ),
                    )
                })
                .collect();
            let answer = ask(
                context,
                owner,
                format!("Extract: gain 1 non-faction technology {puppet} owns"),
                firmament_card(EXTRACT),
                "extract_technology",
                options,
            )?;
            let Some(tech) = techs.into_iter().find(|tech| tech.as_str() == answer.id) else {
                return Ok(());
            };
            tech
        }
    };
    gain_technology(
        context.state,
        context.content,
        context.sources,
        owner,
        &chosen,
    );
    Ok(())
}

/// Every plot's "when this card is revealed" effect, as the flip turns the cards faceup: in the
/// order the cards were placed, for each puppeted player of the card.
fn plots_revealed(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("ability:{owner_name}:plots_revealed:{FLIPPED}:after"),
        seat.clone(),
        FLIPPED,
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            for plot in firmament::plots(context.state, &owner) {
                for puppet in &plot.tokens {
                    if plot.is(SEETHE) {
                        seethe_reveal(context, &owner, puppet)?;
                    } else if plot.is(EXTRACT) {
                        extract_reveal(context, &owner, puppet)?;
                    }
                }
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event.text("to") == Some(obsidian::FACTION)
            && event.text("player") == Some(condition_owner.as_str())
            && obsidian::is_obsidian(context.state, &condition_owner)
            && firmament::plots(context.state, &condition_owner)
                .iter()
                .any(|plot| plot.faceup && (plot.is(SEETHE) || plot.is(EXTRACT)))
    }))
}

/// Extract's second sentence: when a puppeted player gains a non-faction technology the owner may
/// spend 4 resources to gain it too. Choosing the ability is the consent; the payment is then asked.
fn extract_purchase(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    let wanted = |event: &crate::event::Event,
                  state: &GameState,
                  content: &ContentStore,
                  owner: &PlayerId|
     -> Option<TechnologyId> {
        let gainer = PlayerId::new(event.text("player")?);
        let tech = TechnologyId::new(event.text("technology")?);
        (puppets(state, owner, EXTRACT).contains(&gainer)
            && crate::technology::faction_of(content, &tech).is_none()
            && state
                .player(owner)
                .is_some_and(|seat| !seat.technologies.contains(&tech))
            && state
                .player(&gainer)
                .is_some_and(|seat| seat.technologies.contains(&tech)))
        .then_some(tech)
    };
    Ability::stateful(
        format!("ability:{owner_name}:plot_{EXTRACT}:TECHNOLOGY_GAINED:after"),
        seat.clone(),
        "TECHNOLOGY_GAINED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let Some(tech) = wanted(event, context.state, context.content, &owner) else {
                return Ok(());
            };
            let before = context.state.clone();
            match crate::production::pay_seeing(
                context.state,
                context.content,
                context.sources,
                context.galaxy,
                context.table,
                &owner,
                EXTRACT_COST,
                Spend::Resources,
            ) {
                Ok(true) => {
                    gain_technology(
                        context.state,
                        context.content,
                        context.sources,
                        &owner,
                        &tech,
                    );
                    Ok(())
                }
                Ok(false) => {
                    *context.state = before;
                    Ok(())
                }
                Err(error) => {
                    *context.state = before;
                    Err(illegal(error))
                }
            }
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        wanted(event, context.state, context.content, &condition_owner).is_some()
            && crate::payment::affordable(
                context.state,
                context.content,
                context.sources,
                &condition_owner,
                EXTRACT_COST,
                Spend::Resources,
            )
    }))
}

/// The plots' timing abilities, registered by the Obsidian module.
pub(crate) fn abilities(owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    vec![
        plots_revealed(owner_name, seat),
        seethe_status(owner_name, seat),
        siphon(owner_name, seat),
        extract_purchase(owner_name, seat),
    ]
}
