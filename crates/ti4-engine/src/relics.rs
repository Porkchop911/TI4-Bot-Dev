//! Relic effects (LRR 35.9's reward, M06-007).
//!
//! Ported from the oracle's `engine/relics.py`: `_dynamis_core`, `_book_of_latvinia`,
//! `_purge`, and the Circlet's standing gravity-rift immunity.
//!
//! A first tranche. A relic with no registered handler is held but does nothing, and
//! [`unimplemented`] reports which — the same design used for objectives, agendas and laws.

use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{PlanetId, PlayerId, RelicId};
use ti4_model::state::GameState;

use crate::objectives::VICTORY_TARGET;

/// The Circlet of the Void: its owner's units do not roll for gravity rifts.
pub const CIRCLET: &str = "circletofthevoid";

/// What using a relic did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Used {
    /// The relic resolved and was purged.
    Purged { relic: RelicId },
    /// The player does not hold it.
    NotHeld { relic: RelicId },
    /// Held, but this engine has no handler for it.
    Unresolved { relic: RelicId },
}

/// Relics this engine can resolve.
#[must_use]
pub fn registered_aliases() -> Vec<&'static str> {
    let mut all = action_aliases();
    // Passive ability grants.
    all.extend([
        "emelpar",
        "lightrailordnance",
        "metalivoidarmaments",
        "metalivoidshielding",
        "thetriad",
    ]);
    // Passive relics: they change a standing rule and are never *used* as an action. Keeping them
    // out of `action_aliases` is what stops them being offered as an action that does nothing --
    // `available_actions` offers exactly what `use_relic` can resolve.
    all.extend([
        "circletofthevoid", // ignores gravity rifts and other anomalies on movement
        "nanoforge",        // the attached planet is worth two more of each
        "obsidian",         // one additional secret objective
        "heartofixth",      // exhaust to shift a rolled die by one
        "prophetstears",    // exhaust to ignore one research prerequisite
        "thalnos",          // reroll with +1, and lose the units that still miss
        "quantumcore",      // synergy across all four technology types
        "shard",            // a victory point while held
    ]);
    all.sort_unstable();
    all
}

/// Relics whose printed ACTION this engine can resolve.
///
/// Exactly the arms of [`use_relic`]. `available_actions` offers from this list rather than from
/// [`registered_aliases`], because 22.3 says an action that cannot fully resolve is never offered,
/// and a passive relic has no action to resolve at all.
#[must_use]
pub fn action_aliases() -> Vec<&'static str> {
    vec![
        "bookoflatvinia",
        "codex",
        "dynamiscore",
        "enigmaticdevice",
        "mawofworlds",
        "stellarconverter",
        "thesilverflame",
    ]
}

/// The Triad: readied and spent as if it were a planet card.
///
/// "Its resource and influence values are equal to 3 plus the number of **different types** of
/// relic fragments you own" — types, not fragments, so three cultural fragments are worth one, not
/// three. Returns `None` when the holder does not have it, so a caller can tell "not held" from
/// "held and worth three".
#[must_use]
pub fn triad_value(state: &GameState, player: &PlayerId) -> Option<i64> {
    if !holds(state, player, &RelicId::new("thetriad")) {
        return None;
    }
    let kinds = state.player(player).map_or(0, |seat| {
        seat.relic_fragments
            .iter()
            .filter(|(_, count)| **count > 0)
            .count()
    });
    Some(3 + i64::try_from(kinds).unwrap_or(0))
}

/// Scepter of Emelpar: a strategy-pool spend may come from reinforcements instead.
///
/// The card is exhausted to use it, and this engine has no exhausted-relic state, so — as with The
/// Prophet's Tears — the substitution is available whenever the relic is held rather than once per
/// round. Recorded where it is returned rather than left to be discovered.
#[must_use]
pub fn substitutes_strategy_token(state: &GameState, player: &PlayerId) -> bool {
    holds(state, player, &RelicId::new("emelpar"))
}

/// Metali Void Shielding: a non-fighter ship may sustain as if it had the ability.
///
/// "Each time hits are produced against 1 or more of your non-fighter ships, **1 of those ships**
/// may use SUSTAIN DAMAGE as if it had that ability." The card grants the ability to a ship that
/// lacks it, so it is asked where sustain is offered rather than where the unit type is defined --
/// a dreadnought already sustains, and this must not give it a second one.
#[must_use]
pub fn grants_sustain(state: &GameState, player: &PlayerId) -> bool {
    holds(state, player, &RelicId::new("metalivoidshielding"))
}

/// Metali Void Armaments: ANTI-FIGHTER BARRAGE 6 (x3) during the barrage step.
///
/// Granted to the *player*, not to a unit: the card says "you may resolve ANTI-FIGHTER BARRAGE 6
/// (x3) against your opponent's units", so it fires once for its holder rather than once per ship.
#[must_use]
pub fn extra_barrage(state: &GameState, player: &PlayerId) -> Option<(u32, usize)> {
    holds(state, player, &RelicId::new("metalivoidarmaments")).then_some((6, 3))
}

/// Lightrail Ordnance: space docks gain SPACE CANNON 5 (x2).
#[must_use]
pub fn space_dock_cannon(state: &GameState, player: &PlayerId) -> Option<(i64, i64)> {
    holds(state, player, &RelicId::new("lightrailordnance")).then_some((5, 2))
}

/// The Prophet's Tears: exhaust to ignore one research prerequisite.
///
/// The card offers "ignore 1 prerequisite **or** draw 1 action card"; this is the first half. It
/// feeds the same waiver budget the faction abilities and the Research Team laws use, because all
/// three are the same sentence and the requirement is only checked once.
#[must_use]
pub fn prerequisite_waivers(state: &GameState, player: &PlayerId) -> usize {
    let relic = RelicId::new("prophetstears");
    if !holds(state, player, &relic) {
        return 0;
    }
    // The card exhausts to use it. This engine has no exhausted-relic set -- relics are held or
    // purged -- so the waiver is available whenever the card is held. Recorded rather than hidden:
    // it makes the Tears usable once per *check* instead of once per round, which is more generous
    // than the card, and closing it needs an exhaustion state relics do not yet have.
    1
}

/// The Obsidian: one extra secret objective, scored or unscored.
#[must_use]
pub fn secret_objective_bonus(state: &GameState, player: &PlayerId) -> usize {
    usize::from(holds(state, player, &RelicId::new("obsidian")))
}

/// Nano-Forge: the attached planet is worth two more of each, and is legendary.
///
/// Attached rather than held, so the bonus follows the planet and not the owner. Read through the
/// same `planet_value_now` path the three attachment laws use.
#[must_use]
pub fn nanoforge_bonus(state: &GameState, planet: &ti4_model::id::PlanetId) -> i64 {
    i64::from(
        state
            .planet_attachments
            .get(planet)
            .is_some_and(|attached| attached.iter().any(|card| card == "nanoforge")),
    ) * 2
}

/// Ask which technology to gain, and gain it without checking prerequisites.
///
/// Maw of Worlds and Enigmatic Device both say *gain* or *research 1 technology* as the whole of
/// their effect, with the price already paid, so neither routes through `can_research`: the cost
/// they charge is the cost, and a prerequisite check would charge twice.
///
/// `colour` narrows the offer when a card demands one.
pub(crate) fn grant_chosen_technology(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut crate::choice::Table,
    player: &PlayerId,
    colour: Option<&str>,
) -> bool {
    let held: std::collections::BTreeSet<String> = state
        .player(player)
        .map(|seat| {
            seat.technologies
                .iter()
                .map(|alias| alias.as_str().to_owned())
                .collect()
        })
        .unwrap_or_default();
    let faction = state.player(player).map(|seat| seat.faction.to_string());

    let options: Vec<crate::choice::ChoiceOption> = content
        .from_sources(ti4_model::content_types::ContentType::Technologies, sources)
        .filter(|record| {
            // 90.11: a faction technology belongs to that faction alone.
            record.text("faction").is_none_or(|owner| {
                faction.as_deref().is_some_and(|mine| mine == owner)
            })
        })
        .filter_map(|record| record.text("alias"))
        .filter(|alias| !held.contains(*alias))
        .filter(|alias| {
            colour.is_none_or(|wanted| {
                crate::technology::colour_type(content, &ti4_model::id::TechnologyId::new(*alias))
                    .is_some_and(|had| had == wanted)
            })
        })
        .map(|alias| {
            crate::choice::ChoiceOption::labelled(
                alias.to_owned(),
                "technology",
                format!("gain {alias}"),
            )
        })
        .collect();
    if options.is_empty() {
        return false;
    }
    let choice =
        crate::choice::Choice::new(player.clone(), "gain which technology", options);
    let Ok(answer) = table.ask(&choice) else {
        return false;
    };
    if let Some(seat) = state.player_mut(player) {
        seat.technologies
            .insert(ti4_model::id::TechnologyId::new(answer.id));
    }
    true
}

/// The Silver Flame: a ten scores, anything else consumes your home system.
///
/// Purges the relic itself on both branches -- the roll happens either way -- so it returns the
/// finished `Used` rather than a flag.
fn the_silver_flame(
    state: &mut GameState,
    content: &ContentStore,
    dice: &mut crate::dice::Dice,
    rng: &mut crate::rng::GameRng,
    player: &PlayerId,
    relic: &RelicId,
) -> Used {

    // A ten scores; anything else consumes the home system and bars this player from
    // public objectives for the rest of the game. The roll happens either way, so the
    // card is purged before the branch rather than in one arm of it.
    let roll = dice
        .roll(rng, 1, "silver_flame", None)
        .faces
        .first()
        .copied()
        .unwrap_or(0);
    purge(state, player, relic);
    if roll == 10 {
        if let Some(seat) = state.player_mut(player) {
            seat.victory_points = (seat.victory_points + 1).min(VICTORY_TARGET);
        }
        return Used::Purged {
            relic: relic.clone(),
        };
    }
    let home = state.player(player).and_then(|seat| {
        seat.home_system.clone().or_else(|| {
            ti4_content::factions::get(content, seat.faction.as_str())
                .and_then(|faction| faction.home_system())
                .map(ti4_model::id::SystemId::new)
        })
    });
    if let Some(seat) = state.player_mut(player) {
        seat.public_objectives_forbidden = true;
    }
    if let Some(home) = home {
        state.board.remove(&home);
        state.purged_systems.insert(home);
    }
    Used::Purged {
        relic: relic.clone(),
    }
}

/// The Codex: take up to three action cards from the discard pile.
///
/// Asked one at a time so a shrinking pile is offered honestly, rather than three questions put to
/// the pile as it stood when the card was played.
fn codex(state: &mut GameState, table: &mut crate::choice::Table, player: &PlayerId) {

    // "Take up to 3 action cards of your choice from the action card discard pile."
    //
    // Up to three, and taken one at a time so a shrinking pile is offered honestly rather
    // than three questions asked against the pile as it stood.
    for _ in 0..3 {
        let options: Vec<crate::choice::ChoiceOption> = state
            .discarded_action_cards
            .clone()
            .into_iter()
            .map(|alias| {
                crate::choice::ChoiceOption::labelled(
                    alias.to_string(),
                    "action_card",
                    format!("take {alias}"),
                )
            })
            .chain(std::iter::once(crate::choice::ChoiceOption::decline()))
            .collect();
        if options.len() == 1 {
            break; // nothing but the decline: the pile is empty
        }
        let choice = crate::choice::Choice::new(
            player.clone(),
            "The Codex: take which action card",
            options,
        );
        let Ok(answer) = table.ask(&choice) else {
            break;
        };
        if answer.is_decline() {
            break;
        }
        let taken = ti4_model::id::ActionCardId::new(answer.id);
        if let Some(at) = state
            .discarded_action_cards
            .iter()
            .position(|held| *held == taken)
        {
            state.discarded_action_cards.remove(at);
            if let Some(seat) = state.player_mut(player) {
                seat.action_cards.push(taken);
            }
        }
    }
}

/// The Stellar Converter's ACTION: choose a planet in range and destroy it.
///
/// Returns whether it resolved. Split out of `use_relic` because the target search, the offer and
/// the destruction are three separate steps and the match arm was the only thing holding them
/// together.
fn stellar_converter(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut crate::choice::Table,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    player: &PlayerId,
) -> bool {
    // "Choose 1 non-home, non-legendary planet other than Mecatol Rex in a system that is
    // adjacent to 1 or more of your units that have BOMBARDMENT, destroy all units on that
    // planet and purge its attachments and its planet card."
    //
    // 6.4 -- a system is not adjacent to itself -- so a planet sharing a system with the
    // bombarding ship is *not* a target. That reads oddly and it is what the rule says;
    // `Galaxy::adjacent` already excludes the system itself, so this gets it for free.
    let Some(galaxy) = galaxy else {
        return false; // 22.3: without a board there is no adjacency, so nothing can be chosen
    };
    let targets = stellar_converter_targets(state, content, sources, galaxy, player);
    if targets.is_empty() {
        return false;
    }
    let options: Vec<crate::choice::ChoiceOption> = targets
        .iter()
        .map(|(_, planet)| {
            crate::choice::ChoiceOption::labelled(
                planet.to_string(),
                "planet",
                format!("destroy {planet}"),
            )
        })
        .collect();
    let choice = crate::choice::Choice::new(
        player.clone(),
        "Stellar Converter: destroy which planet",
        options,
    );
    let Ok(answer) = table.ask(&choice) else {
        return false;
    };
    let chosen = PlanetId::new(answer.id);
    let Some((system, _)) = targets.iter().find(|(_, planet)| *planet == chosen) else {
        return false;
    };
    if let Some(here) = state.board.get_mut(system) {
        here.purge_planet(&chosen);
    }
    state.planet_attachments.remove(&chosen);
    true
}

/// Planets the Stellar Converter may be aimed at.
///
/// The three exclusions are printed on the card; the adjacency is measured from every system
/// holding one of this player's BOMBARDMENT units, which is a property of the unit type rather
/// than of the unit, so it comes from the catalogue.
fn stellar_converter_targets(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &ti4_content::galaxy::Galaxy,
    player: &PlayerId,
) -> Vec<(ti4_model::id::SystemId, PlanetId)> {
    let types = ti4_content::units::catalogue(content, sources);
    let mut reach: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for (system, here) in &state.board {
        let bombards = here
            .units_of(player)
            .into_iter()
            .any(|unit| types.get(unit.type_id.as_str()).is_some_and(ti4_content::UnitType::has_bombardment));
        if bombards {
            reach.extend(galaxy.adjacent(system.as_str()));
        }
    }
    let mut targets = Vec::new();
    for system in reach {
        let id = ti4_model::id::SystemId::new(system);
        if !state.board.contains_key(&id) {
            continue; // purged by the Silver Flame, or never on this board
        }
        for planet in ti4_content::galaxy::planets_in(content, system, sources) {
            let alias = planet.id();
            if planet.homeworld_of().is_some() || planet.is_legendary() || alias == "mecatol_rex" {
                continue;
            }
            let target = PlanetId::new(alias);
            if state
                .board
                .get(&id)
                .is_some_and(|here| here.purged_planets.contains(&target))
            {
                continue; // already destroyed
            }
            targets.push((id.clone(), target));
        }
    }
    targets
}

/// Exhaust a held relic for an ability that says "exhaust this card".
///
/// Returns whether it was exhausted -- `false` if it is not held, or is exhausted already. The
/// three cards that say it are once per round, and the status phase readies them.
pub fn exhaust(state: &mut GameState, player: &PlayerId, relic: &str) -> bool {
    let id = RelicId::new(relic);
    if !holds(state, player, &id) {
        return false;
    }
    state
        .player_mut(player)
        .is_some_and(|seat| seat.exhausted_relics.insert(id))
}

/// Whether a held relic is ready to be exhausted.
#[must_use]
pub fn ready(state: &GameState, player: &PlayerId, relic: &str) -> bool {
    let id = RelicId::new(relic);
    holds(state, player, &id)
        && state
            .player(player)
            .is_some_and(|seat| !seat.exhausted_relics.contains(&id))
}

/// Whether a player holds a relic.
#[must_use]
pub fn holds(state: &GameState, player: &PlayerId, relic: &RelicId) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.relics.contains(relic))
}

/// 41.2 immunity: the Circlet's owner never rolls for a gravity rift.
///
/// Read where the roll happens rather than at the card, so it cannot be honoured in one place
/// and forgotten in another — the mistake Nav Suite nearly made in `transit`.
#[must_use]
pub fn ignores_gravity_rifts(state: &GameState, player: &PlayerId) -> bool {
    holds(state, player, &RelicId::new(CIRCLET))
}

fn purge(state: &mut GameState, player: &PlayerId, relic: &RelicId) {
    if let Some(seat) = state.player_mut(player) {
        seat.relics.retain(|held| held != relic);
    }
}

/// Whether this player controls planets covering all four technology specialties.
fn controls_all_four_specialties(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> bool {
    let catalogue = ti4_content::galaxy::all_planets(content, sources);
    let mut found = std::collections::BTreeSet::new();
    for (_, planet) in state.controlled_planets(player) {
        if let Some(record) = catalogue.get(planet.as_str()) {
            for specialty in record.tech_specialties() {
                found.insert(specialty.to_ascii_uppercase());
            }
        }
    }
    found.len() >= 4
}

/// A faction's printed commodity value (21.1).
fn commodity_value(state: &GameState, content: &ContentStore, player: &PlayerId) -> i32 {
    state.player(player).map_or(0, |seat| {
        ti4_content::factions::get(content, seat.faction.as_str())
            .map_or(0, |faction| faction.commodities())
    })
}

/// The Shard of the Throne, which is worth a victory point simply for being held.
pub const SHARD: &str = "shard";

/// Draw the top relic (73.2).
///
/// Every path that hands a player a relic goes through here, because a relic can be worth a
/// point the moment it arrives: the Shard was worth nothing when exploration drew it straight
/// off the deck, and would have been worth nothing again for the next path written.
pub fn gain(state: &mut GameState, player: &PlayerId) -> Option<RelicId> {
    let top = state.relic_deck.first().cloned()?; // 73.2a: an empty deck yields nothing
    state.relic_deck.remove(0);
    if let Some(seat) = state.player_mut(player) {
        seat.relics.push(top.clone());
    }
    if top.as_str() == SHARD
        && let Some(seat) = state.player_mut(player)
    {
        seat.victory_points = (seat.victory_points + 1).min(VICTORY_TARGET);
    }
    Some(top)
}

/// Use a relic's action, purging it.
pub fn use_relic(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    dice: &mut crate::dice::Dice,
    rng: &mut crate::rng::GameRng,
    table: &mut crate::choice::Table,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    player: &PlayerId,
    relic: &RelicId,
) -> Used {
    if !holds(state, player, relic) {
        return Used::NotHeld {
            relic: relic.clone(),
        };
    }
    match relic.as_str() {
        "enigmaticdevice" => {
            // "You may spend 6 resources and purge this card to research 1 technology."
            //
            // The research is a choice and the cost is a gate, so the cost is checked before the
            // question is asked: 22.3 does not offer an action that cannot fully resolve, and
            // asking which technology before knowing it can be paid for would let a decider spend
            // a decision on nothing.
            if !crate::production::pay(
                state,
                content,
                sources,
                table,
                player,
                6,
                crate::production::Spend::Resources,
            )
            .unwrap_or(false)
            {
                return Used::Unresolved {
                    relic: relic.clone(),
                };
            }
            grant_chosen_technology(state, content, sources, table, player, None);
        }
        "mawofworlds" => {
            // "Purge this card and exhaust all of your planets to gain any 1 technology."
            //
            // Exhausting is the cost and is paid whatever is chosen, so it happens before the
            // question. Prerequisites are waived: the card says *gain*, not research.
            let planets: Vec<ti4_model::id::PlanetId> = state
                .controlled_planets(player)
                .into_iter()
                .map(|(_, planet)| planet.clone())
                .collect();
            for planet in planets {
                state.exhaust_planet(planet);
            }
            grant_chosen_technology(state, content, sources, table, player, None);
        }
        "codex" => codex(state, table, player),
        "stellarconverter" => {
            if !stellar_converter(state, content, sources, table, galaxy, player) {
                return Used::Unresolved {
                    relic: relic.clone(),
                };
            }
        }
        "dynamiscore" => {
            // "Gain trade goods equal to your commodity value, then purge this card." The
            // card's other half — commodity value increased by 2 — is a standing modifier, and
            // is applied here to the gain so the two halves cannot disagree about the number.
            // Commodity *value* is the faction's printed number, not how many commodities the
            // player happens to be holding. Reading the holding pays a full seat nothing and an
            // empty one two, which is the card backwards.
            let value = commodity_value(state, content, player) + 2;
            if let Some(seat) = state.player_mut(player) {
                seat.trade_goods += value;
            }
        }
        "bookoflatvinia" => {
            // All four specialties gains a victory point; otherwise the speaker token.
            if controls_all_four_specialties(state, content, sources, player) {
                if let Some(seat) = state.player_mut(player) {
                    seat.victory_points = (seat.victory_points + 1).min(VICTORY_TARGET);
                }
            } else {
                state.speaker = player.clone();
            }
        }
        "thesilverflame" => {
            return the_silver_flame(state, content, dice, rng, player, relic);
        }
        _ => {
            return Used::Unresolved {
                relic: relic.clone(),
            };
        }
    }
    purge(state, player, relic);
    Used::Purged {
        relic: relic.clone(),
    }
}

// -- the component action (22) -----------------------------------------------------------------

/// The kind of a relic component action.
pub const ACTION_KIND: &str = "component";

/// The prefix of an option that purges fragments, and of one that uses a held relic.
const PURGE_PREFIX: &str = "purge|";
const USE_PREFIX: &str = "relic|";

/// Component actions this player could take with relics and fragments right now.
///
/// Two kinds, and only the first ever existed here: purging three fragments for a new relic,
/// and using a relic already in the play area. Without the second a relic could be drawn,
/// held and counted while being unusable for the whole game.
#[must_use]
pub fn available_actions(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    player: &PlayerId,
) -> Vec<crate::choice::ChoiceOption> {
    let mut options: Vec<crate::choice::ChoiceOption> =
        crate::exploration::purgeable(state, player)
            .into_iter()
            .map(|trait_name| {
                crate::choice::ChoiceOption::labelled(
                    format!("{PURGE_PREFIX}{trait_name}"),
                    ACTION_KIND,
                    format!(
                        "purge 3 {} relic fragments for a relic",
                        trait_name.to_lowercase()
                    ),
                )
            })
            .collect();

    let held = state
        .player(player)
        .map(|seat| seat.relics.clone())
        .unwrap_or_default();
    let known = action_aliases();
    // 22.3 again: two of these actions have a precondition that the card itself states, and an
    // action that cannot fully resolve is never offered. Checked here rather than only inside
    // `use_relic`, so a decider is never handed a choice that resolves to nothing.
    let resolvable = |alias: &str| -> bool {
        match alias {
            // Six resources, and no way to take the action without them.
            "enigmaticdevice" => {
                crate::production::available(
                    state,
                    content,
                    sources,
                    player,
                    crate::production::Spend::Resources,
                ) >= 6
            }
            // Something in range that the card is allowed to destroy. Without a board there is no
            // adjacency and therefore no target.
            "stellarconverter" => galaxy.is_some_and(|galaxy| {
                !stellar_converter_targets(state, content, sources, galaxy, player).is_empty()
            }),
            _ => true,
        }
    };
    options.extend(
        held.into_iter()
            // 22.3: an action that cannot fully resolve is never offered, and a relic with no
            // handler cannot resolve at all.
            .filter(|relic| known.contains(&relic.as_str()) && resolvable(relic.as_str()))
            .filter(|relic| relic.as_str() != SHARD) // held for its point; it has no action
            .map(|relic| {
                crate::choice::ChoiceOption::labelled(
                    format!("{USE_PREFIX}{relic}"),
                    ACTION_KIND,
                    format!("use {relic}"),
                )
            }),
    );
    let _ = (content, sources);
    options
}

/// Perform a relic component action. Returns `false` for an option that is not one.
pub fn perform(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    dice: &mut crate::dice::Dice,
    rng: &mut crate::rng::GameRng,
    table: &mut crate::choice::Table,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    player: &PlayerId,
    option: &crate::choice::ChoiceOption,
) -> bool {
    if let Some(trait_name) = option.id.strip_prefix(PURGE_PREFIX) {
        // The fragments are spent by `purge_for_relic`, which draws straight from the deck, so
        // the Shard's point is applied here rather than being lost on this one path.
        let before = state.player(player).map(|seat| seat.relics.len());
        let gained = crate::exploration::purge_for_relic(state, player, trait_name);
        if let (Some(relic), Some(_)) = (gained.as_ref(), before)
            && relic.as_str() == SHARD
            && let Some(seat) = state.player_mut(player)
        {
            seat.victory_points = (seat.victory_points + 1).min(VICTORY_TARGET);
        }
        return gained.is_some();
    }
    if let Some(alias) = option.id.strip_prefix(USE_PREFIX) {
        let relic = RelicId::new(alias);
        return matches!(
            use_relic(state, content, sources, dice, rng, table, galaxy, player, &relic),
            Used::Purged { .. }
        );
    }
    false
}

/// Relics in the corpus that nothing here resolves.
#[must_use]
pub fn unimplemented(content: &ContentStore, sources: SourceSet) -> Vec<RelicId> {
    let known = registered_aliases();
    content
        .from_sources(ContentType::Relics, sources)
        .filter_map(|record| record.text("alias"))
        .filter(|alias| !known.contains(alias) && *alias != CIRCLET)
        .map(RelicId::new)
        .collect()
}

#[cfg(test)]
mod tests {
    /// Every relic offered as an action must have an arm that resolves it (22.3).
    ///
    /// This exists because adding five *passive* relics to `registered_aliases` for coverage very
    /// nearly offered all five as actions that would have done nothing when taken. Coverage and
    /// "has a printed ACTION" are different questions and were briefly the same list.
    #[test]
    fn every_offered_relic_action_actually_resolves() {
        let content = ti4_content::ContentStore::embedded();
        let mut state = crate::fixtures::game(&["a"]);
        let player = PlayerId::new("a");

        // The Stellar Converter needs a board: a BOMBARDMENT unit, and an adjacent system holding
        // a planet it is allowed to aim at. A dreadnought in the centre of a hub reaches the whole
        // ring, and 6.4 -- a system is not adjacent to itself -- is why the ship is not parked in
        // the system it is shooting at.
        let hub = crate::fixtures::hub_with_outer(an_ordinary_planet().0.as_str());
        let centre = ti4_model::id::SystemId::new(&hub.centre);
        for id in std::iter::once(&hub.centre).chain(hub.outer.iter()) {
            state
                .board
                .entry(ti4_model::id::SystemId::new(id))
                .or_default();
        }
        crate::fixtures::put(&mut state, &centre, "dreadnought", &player, 1);

        for alias in action_aliases() {
            if let Some(seat) = state.player_mut(&player) {
                seat.relics = vec![RelicId::new(alias)];
                // Enigmatic Device costs six resources. `available_actions` declines to offer it
                // unpaid-for (22.3), so a seat that cannot pay would never reach this arm in play;
                // giving the fixture the means keeps every arm exercised rather than skipped.
                seat.trade_goods = 6;
            }
            let used = use_relic(
                &mut state,
                content,
                ti4_model::content_types::DEFAULT,
                &mut crate::dice::Dice::new(),
                &mut crate::rng::GameRng::new(1),
                &mut crate::choice::Table::new(),
                Some(&hub.galaxy),
                &player,
                &RelicId::new(alias),
            );
            assert!(
                !matches!(used, Used::Unresolved { .. }),
                "{alias} is offered as an action but does not resolve"
            );
        }
    }

    /// A planet the Stellar Converter is allowed to aim at, with its system.
    ///
    /// Not `fixtures::a_placed_planet`: the first placed planet in the corpus is a homeworld, and
    /// the card excludes those.
    fn an_ordinary_planet() -> (ti4_model::id::SystemId, PlanetId) {
        ti4_content::galaxy::all_planets(
            ti4_content::ContentStore::embedded(),
            ti4_model::content_types::DEFAULT,
        )
        .iter()
        .find(|(id, planet)| {
            planet.homeworld_of().is_none()
                && !planet.is_legendary()
                && !planet.is_placed_during_play()
                && !id.eq_ignore_ascii_case("mecatol_rex")
                && planet.system_id().is_some()
        })
        .map(|(id, planet)| {
            (
                ti4_model::id::SystemId::new(planet.system_id().unwrap_or_default()),
                PlanetId::new(*id),
            )
        })
        .expect("the corpus has an ordinary planet")
    }

    /// The Stellar Converter destroys a planet, and the destruction sticks.
    ///
    /// Two halves, and the second is the one worth testing: purging the planet card means there is
    /// nothing left to take, so an invader who lands there afterwards gains nothing. A version that
    /// only cleared the current occupants would pass a units-are-gone check and still let the next
    /// player take the planet on the following turn.
    #[test]
    fn the_stellar_converter_destroys_a_planet_for_good() {
        let content = ti4_content::ContentStore::embedded();
        let sources = ti4_model::content_types::DEFAULT;
        let mut state = crate::fixtures::game(&["a", "b"]);
        let (attacker, victim) = (PlayerId::new("a"), PlayerId::new("b"));

        let (target_system, target) = an_ordinary_planet();
        let hub = crate::fixtures::hub_with_outer(target_system.as_str());
        let centre = ti4_model::id::SystemId::new(&hub.centre);
        for id in std::iter::once(&hub.centre).chain(hub.outer.iter()) {
            state
                .board
                .entry(ti4_model::id::SystemId::new(id))
                .or_default();
        }
        crate::fixtures::put(&mut state, &centre, "dreadnought", &attacker, 1);
        crate::fixtures::put_on_planet(&mut state, &target_system, &target, "infantry", &victim, 2);
        if let Some(here) = state.board.get_mut(&target_system) {
            here.set_control(target.clone(), victim.clone());
        }
        state
            .planet_attachments
            .insert(target.clone(), vec!["demilitarizedzone".to_owned()]);
        if let Some(seat) = state.player_mut(&attacker) {
            seat.relics = vec![RelicId::new("stellarconverter")];
        }

        let mut table = crate::choice::Table::new();
        table.seat(
            attacker.clone(),
            Box::new(crate::choice::Scripted::new(vec![target.to_string()])),
        );
        let used = use_relic(
            &mut state,
            content,
            sources,
            &mut crate::dice::Dice::new(),
            &mut crate::rng::GameRng::new(1),
            &mut table,
            Some(&hub.galaxy),
            &attacker,
            &RelicId::new("stellarconverter"),
        );
        assert!(matches!(used, Used::Purged { .. }), "the relic is purged");

        let here = state.board.get(&target_system).expect("the system");
        assert!(here.on_planet(&target).is_empty(), "the units are destroyed");
        assert!(
            !here.planet_control.contains_key(&target),
            "and nobody controls it"
        );
        assert!(
            !state.planet_attachments.contains_key(&target),
            "and its attachments are purged"
        );

        // The half that matters: there is no card left to take.
        let here = state.board.get_mut(&target_system).expect("the system");
        here.set_control(target.clone(), attacker.clone());
        crate::fixtures::put_on_planet(
            &mut state,
            &target_system,
            &target,
            "infantry",
            &attacker,
            1,
        );
        let here = state.board.get(&target_system).expect("the system");
        assert!(
            !here.planet_control.contains_key(&target),
            "a destroyed planet cannot be taken afterwards"
        );
    }

    /// A passive relic is implemented but never offered as an action.
    #[test]
    fn a_passive_relic_is_not_offered_as_an_action() {
        let content = ti4_content::ContentStore::embedded();
        let mut state = crate::fixtures::game(&["a"]);
        let player = PlayerId::new("a");
        if let Some(seat) = state.player_mut(&player) {
            seat.relics = vec![RelicId::new("obsidian")];
        }

        let offered = available_actions(&state, content, ti4_model::content_types::DEFAULT, None, &player);
        assert!(
            !offered.iter().any(|option| option.id.contains("obsidian")),
            "the Obsidian has no printed ACTION and must not be offered as one"
        );
        assert!(
            registered_aliases().contains(&"obsidian"),
            "but it is implemented, and coverage must say so"
        );
    }

    use ti4_model::content_types::DEFAULT as ALL_SOURCES;

    fn holding(alias: &str) -> (GameState, PlayerId) {
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a", "b"]);
        if let Some(seat) = state.player_mut(&player) {
            seat.relics.push(RelicId::new(alias));
        }
        (state, player)
    }

    /// The Prophet's Tears waives one research prerequisite.
    ///
    /// Driven through `can_research`, the function the rest of the engine reads, so a waiver that
    /// fed a budget nobody consults would fail here.
    #[test]
    fn the_prophets_tears_pays_one_prerequisite() {
        let content = ti4_content::ContentStore::embedded();
        let wanted = ti4_model::id::TechnologyId::new("td"); // two cybernetic

        let (mut state, player) = holding("prophetstears");
        if let Some(seat) = state.player_mut(&player) {
            // One cybernetic against a two-cybernetic requirement: exactly one short, so the
            // waiver is the whole difference.
            seat.technologies = [ti4_model::id::TechnologyId::new("st")]
                .into_iter()
                .collect();
        }
        let with_tears = crate::technology::can_research(
            &state, content, ALL_SOURCES, &player, &wanted,
        );

        // The same seat without the relic is one prerequisite short.
        if let Some(seat) = state.player_mut(&player) {
            seat.relics.clear();
        }
        let without = crate::technology::can_research(
            &state, content, ALL_SOURCES, &player, &wanted,
        );

        assert!(
            with_tears && !without,
            "the Tears must be what closes the gap (with: {with_tears}, without: {without})"
        );
    }

    /// The Quantumcore joins all four colours rather than a pair.
    #[test]
    fn the_quantumcore_joins_every_colour() {
        let content = ti4_content::ContentStore::embedded();
        let (state, player) = holding("quantumcore");
        let joined = crate::synergy::joined(&state, content, ALL_SOURCES, &player);
        assert_eq!(joined.len(), 4, "all four technology types: {joined:?}");

        let (plain, other) = holding("shard");
        assert!(
            crate::synergy::joined(&plain, content, ALL_SOURCES, &other).is_empty(),
            "and no other relic does that"
        );
    }

    /// Nano-Forge adds two of each to the planet it is attached to, not to its holder.
    #[test]
    fn the_nano_forge_enriches_the_planet_it_is_attached_to() {
        let content = ti4_content::ContentStore::embedded();
        let mut state = crate::fixtures::game(&["a"]);
        let planet = ti4_model::id::PlanetId::new("bellatrix");
        let printed = crate::production::planet_value(
            content,
            ALL_SOURCES,
            &planet,
            crate::production::Spend::Resources,
        );

        assert_eq!(
            crate::production::planet_value_now(
                &state, content, ALL_SOURCES, &planet, crate::production::Spend::Resources
            ),
            printed,
            "unattached, the planet is worth its printed value"
        );

        state
            .planet_attachments
            .entry(planet.clone())
            .or_default()
            .push("nanoforge".to_owned());

        assert_eq!(
            crate::production::planet_value_now(
                &state, content, ALL_SOURCES, &planet, crate::production::Spend::Resources
            ),
            printed + 2
        );
    }

    /// The Triad is worth three plus the number of fragment *types*, not fragments.
    ///
    /// Driven through `production::available`, which is what a payment reads, so a value that
    /// existed but reached no spending path would fail here.
    #[test]
    fn the_triad_spends_as_a_planet_worth_three_plus_its_fragment_types() {
        let content = ti4_content::ContentStore::embedded();
        let (mut state, player) = holding("thetriad");

        let base = crate::production::available(
            &state,
            content,
            ALL_SOURCES,
            &player,
            crate::production::Spend::Resources,
        );
        assert!(base >= 3, "three even with no fragments, saw {base}");

        // Three of one type is one type.
        if let Some(seat) = state.player_mut(&player) {
            seat.relic_fragments.insert("CULTURAL".to_owned(), 3);
        }
        let one_type = crate::production::available(
            &state,
            content,
            ALL_SOURCES,
            &player,
            crate::production::Spend::Resources,
        );
        assert_eq!(one_type, base + 1, "three fragments of one type count once");

        if let Some(seat) = state.player_mut(&player) {
            seat.relic_fragments.insert("HAZARDOUS".to_owned(), 1);
        }
        let two_types = crate::production::available(
            &state,
            content,
            ALL_SOURCES,
            &player,
            crate::production::Spend::Resources,
        );
        assert_eq!(two_types, base + 2);
    }

    /// The Triad is not a planet, and must not be counted as one.
    #[test]
    fn the_triad_is_not_a_planet() {
        let (state, player) = holding("thetriad");
        assert!(
            crate::production::spendable_planets(&state, &player).is_empty(),
            "spending like a planet is not being one"
        );
    }

    /// Metali Void Shielding grants sustain to a ship that lacks it, and to nobody else.
    #[test]
    fn metali_void_shielding_grants_sustain_only_to_its_holder() {
        let (state, player) = holding("metalivoidshielding");
        assert!(grants_sustain(&state, &player));
        assert!(!grants_sustain(&state, &PlayerId::new("b")));
    }

    /// The two ability-granting relics report the values printed on them.
    #[test]
    fn the_ability_grants_carry_their_printed_numbers() {
        let (state, player) = holding("metalivoidarmaments");
        assert_eq!(extra_barrage(&state, &player), Some((6, 3)), "AFB 6 (x3)");
        assert_eq!(extra_barrage(&state, &PlayerId::new("b")), None);

        let (state, player) = holding("lightrailordnance");
        assert_eq!(
            space_dock_cannon(&state, &player),
            Some((5, 2)),
            "SPACE CANNON 5 (x2)"
        );
    }

    /// The Obsidian raises the secret-objective limit rather than exempting a card.
    #[test]
    fn the_obsidian_raises_the_secret_limit_by_one() {
        let (state, player) = holding("obsidian");
        assert_eq!(secret_objective_bonus(&state, &player), 1);
        assert_eq!(
            secret_objective_bonus(&state, &PlayerId::new("b")),
            0,
            "and only for its holder"
        );
    }

    use ti4_model::content_types::POK;

    use super::*;
    use crate::fixtures::game;

    fn player() -> PlayerId {
        PlayerId::new("a")
    }

    fn give(state: &mut GameState, alias: &str) -> RelicId {
        let relic = RelicId::new(alias);
        state
            .player_mut(&player())
            .unwrap()
            .relics
            .push(relic.clone());
        relic
    }

    #[test]
    fn the_shard_is_worth_a_point_the_moment_it_arrives() {
        let mut state = game(&["a"]);
        state.relic_deck = vec![RelicId::new(SHARD)];
        let before = state.player(&player()).unwrap().victory_points;

        let gained = gain(&mut state, &player());

        assert_eq!(gained, Some(RelicId::new(SHARD)));
        assert_eq!(
            state.player(&player()).unwrap().victory_points,
            before + 1,
            "held, not used: the point comes with the card"
        );
    }

    #[test]
    fn an_ordinary_relic_is_worth_no_points() {
        let mut state = game(&["a"]);
        state.relic_deck = vec![RelicId::new("dynamiscore")];
        let before = state.player(&player()).unwrap().victory_points;

        gain(&mut state, &player());

        assert_eq!(state.player(&player()).unwrap().victory_points, before);
    }

    #[test]
    fn an_empty_relic_deck_gives_nothing() {
        let mut state = game(&["a"]);
        state.relic_deck.clear();
        assert_eq!(gain(&mut state, &player()), None);
    }

    #[test]
    fn the_silver_flame_scores_on_a_ten_and_burns_you_otherwise() {
        // Both halves are reachable, so the roll is forced rather than hoped for: a test that
        // takes whatever the stream gives would exercise one branch and call it the card.
        for (face, scored) in [(10, true), (1, false)] {
            let mut state = game(&["a"]);
            let relic = give(&mut state, "thesilverflame");
            state.player_mut(&player()).unwrap().faction = ti4_model::id::FactionId::new("sol");
            let home = ti4_content::factions::get(ContentStore::embedded(), "sol")
                .and_then(|faction| faction.home_system())
                .map(ti4_model::id::SystemId::new)
                .expect("sol has a home system");
            state.system_mut(&home);
            let before = state.player(&player()).unwrap().victory_points;

            let mut dice = crate::dice::Dice::from_faces([face]);
            use_relic(
                &mut state,
                ContentStore::embedded(),
                POK,
                &mut dice,
                &mut crate::rng::GameRng::new(0),
                &mut crate::choice::Table::new(),
                None,
                &player(),
                &relic,
            );

            let seat = state.player(&player()).unwrap();
            assert!(!holds(&state, &player(), &relic), "purged either way");
            if scored {
                assert_eq!(seat.victory_points, before + 1);
                assert!(!seat.public_objectives_forbidden);
                assert!(state.purged_systems.is_empty());
            } else {
                assert_eq!(seat.victory_points, before);
                assert!(
                    seat.public_objectives_forbidden,
                    "the price is every public objective for the rest of the game"
                );
                assert!(state.purged_systems.contains(&home), "and the home system");
            }
        }
    }

    #[test]
    fn a_relic_with_no_handler_is_never_offered_as_an_action() {
        // 22.3: an action that cannot fully resolve is not offered. A relic the engine cannot
        // resolve would otherwise be a turn spent on nothing.
        let mut state = game(&["a"]);
        let unknown = unimplemented(ContentStore::embedded(), POK)
            .into_iter()
            .next()
            .expect("some relic is still unimplemented");
        state.player_mut(&player()).unwrap().relics = vec![unknown.clone()];

        let offered = available_actions(&state, ContentStore::embedded(), POK, None, &player());

        assert!(
            offered.is_empty(),
            "{unknown} has no handler and must not be offered: {offered:?}"
        );
    }

    #[test]
    fn a_held_relic_is_offered_and_using_it_purges_it() {
        let mut state = game(&["a"]);
        let relic = give(&mut state, "dynamiscore");

        let offered = available_actions(&state, ContentStore::embedded(), POK, None, &player());
        let option = offered
            .iter()
            .find(|option| option.id.contains(relic.as_str()))
            .cloned()
            .expect("the relic is offered");

        assert!(perform(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut crate::dice::Dice::new(),
            &mut crate::rng::GameRng::new(0),
            &mut crate::choice::Table::new(),
                None,
            &player(),
            &option,
        ));
        assert!(!holds(&state, &player(), &relic));
    }

    #[test]
    fn fragments_are_offered_as_an_action_and_buy_a_relic() {
        let mut state = game(&["a"]);
        state.relic_deck = vec![RelicId::new("dynamiscore")];
        state.player_mut(&player()).unwrap().relic_fragments =
            [("CULTURAL".to_owned(), 3)].into_iter().collect();

        let offered = available_actions(&state, ContentStore::embedded(), POK, None, &player());
        let option = offered
            .iter()
            .find(|option| option.id.starts_with("purge|"))
            .cloned()
            .expect("three fragments buy a relic");

        assert!(perform(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut crate::dice::Dice::new(),
            &mut crate::rng::GameRng::new(0),
            &mut crate::choice::Table::new(),
                None,
            &player(),
            &option,
        ));
        assert_eq!(state.player(&player()).unwrap().relics.len(), 1);
    }

    #[test]
    fn a_relic_you_do_not_hold_does_nothing() {
        let mut state = game(&["a"]);
        let before = state.clone();
        let used = use_relic(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut crate::dice::Dice::new(),
            &mut crate::rng::GameRng::new(0),
            &mut crate::choice::Table::new(),
                None,
            &player(),
            &RelicId::new("dynamiscore"),
        );
        assert!(matches!(used, Used::NotHeld { .. }));
        assert!(state.identical(&before));
    }

    #[test]
    fn an_unregistered_relic_is_held_but_reports_unresolved() {
        let mut state = game(&["a"]);
        let relic = give(&mut state, "nanoforge");

        let used = use_relic(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut crate::dice::Dice::new(),
            &mut crate::rng::GameRng::new(0),
            &mut crate::choice::Table::new(),
                None,
            &player(),
            &relic,
        );

        assert!(matches!(used, Used::Unresolved { .. }));
        assert!(holds(&state, &player(), &relic), "it was not purged");
    }

    #[test]
    fn dynamis_core_counts_its_own_bonus_into_the_gain() {
        // The card's standing half raises commodity *value* by 2, and its action gains that
        // value. Value is the faction's printed number: reading the commodities in hand pays a
        // full seat nothing extra and an empty one two, which is the card backwards.
        let mut state = game(&["a"]);
        let relic = give(&mut state, "dynamiscore");
        let seat = state.player_mut(&player()).unwrap();
        seat.faction = ti4_model::id::FactionId::new("sol");
        seat.commodities = 0;
        seat.trade_goods = 0;
        let printed = ti4_content::factions::get(ContentStore::embedded(), "sol")
            .expect("sol is a faction")
            .commodities();

        use_relic(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut crate::dice::Dice::new(),
            &mut crate::rng::GameRng::new(0),
            &mut crate::choice::Table::new(),
                None,
            &player(),
            &relic,
        );

        assert_eq!(
            state.player(&player()).unwrap().trade_goods,
            printed + 2,
            "the printed value plus the card's own two, with none in hand"
        );
        assert!(!holds(&state, &player(), &relic), "and it purged itself");
    }

    #[test]
    fn the_book_gives_the_speaker_token_without_all_four_specialties() {
        let mut state = game(&["a", "b"]);
        state.speaker = PlayerId::new("b");
        let relic = give(&mut state, "bookoflatvinia");

        use_relic(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut crate::dice::Dice::new(),
            &mut crate::rng::GameRng::new(0),
            &mut crate::choice::Table::new(),
                None,
            &player(),
            &relic,
        );

        assert_eq!(state.speaker, player());
        assert_eq!(state.player(&player()).unwrap().victory_points, 0);
        assert!(!holds(&state, &player(), &relic));
    }

    #[test]
    fn the_book_gives_a_victory_point_with_all_four() {
        let mut state = game(&["a", "b"]);
        state.speaker = PlayerId::new("b");
        let relic = give(&mut state, "bookoflatvinia");

        // Control a planet of each specialty, if the corpus offers them.
        let mut covered = std::collections::BTreeSet::new();
        for (id, record) in &ti4_content::galaxy::all_planets(ContentStore::embedded(), POK) {
            let specialties = record.tech_specialties();
            if specialties.is_empty() || record.is_placed_during_play() {
                continue;
            }
            let system = ti4_model::id::SystemId::new(record.system_id().unwrap_or("18"));
            state
                .system_mut(&system)
                .set_control(ti4_model::id::PlanetId::new(*id), player());
            for specialty in specialties {
                covered.insert(specialty.to_ascii_uppercase());
            }
            if covered.len() >= 4 {
                break;
            }
        }
        if covered.len() < 4 {
            return;
        }

        use_relic(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut crate::dice::Dice::new(),
            &mut crate::rng::GameRng::new(0),
            &mut crate::choice::Table::new(),
                None,
            &player(),
            &relic,
        );

        assert_eq!(state.player(&player()).unwrap().victory_points, 1);
        assert_eq!(state.speaker, PlayerId::new("b"), "the token did not move");
    }

    #[test]
    fn the_circlet_makes_its_owner_immune_to_gravity_rifts() {
        let mut state = game(&["a", "b"]);
        assert!(!ignores_gravity_rifts(&state, &player()));

        give(&mut state, CIRCLET);

        assert!(ignores_gravity_rifts(&state, &player()));
        assert!(
            !ignores_gravity_rifts(&state, &PlayerId::new("b")),
            "it protects its owner, not the table"
        );
    }

    #[test]
    fn the_unresolved_relics_are_reported() {
        let missing = unimplemented(ContentStore::embedded(), POK);
        assert!(!missing.is_empty(), "most relics are still unresolved");
        for alias in registered_aliases() {
            assert!(!missing.contains(&RelicId::new(alias)));
        }
    }

    #[test]
    fn every_registered_alias_is_a_real_relic() {
        for alias in registered_aliases().into_iter().chain([CIRCLET]) {
            assert!(
                ContentStore::embedded()
                    .get(ContentType::Relics, alias)
                    .is_some(),
                "{alias} is not a relic the corpus knows"
            );
        }
    }
}
