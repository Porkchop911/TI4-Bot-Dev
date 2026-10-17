//! What playing a reaction card would do right now (display only).
//!
//! A reaction window tells the asked seat *that* a card can be played. This module says *what it
//! would do*, with the concrete context the engine already holds: the ship a Direct Hit would
//! destroy, the hits a Shields Holding would cancel, the planets a Deadly Plot would exhaust.
//!
//! Pure reads over the state and the event that opened the window; nothing here mutates, rolls a
//! die or reads a hand other than the asker's own. The result rides in
//! [`DecisionContext::previews`](crate::decision_context::DecisionContext::previews), which is
//! stripped from every recorded decision and from the replay fingerprint, so it cannot change a
//! hash, a save or a replay.
//!
//! Every fact named is public at the table: units on the board, the card just played, the agenda
//! just revealed, commodities, strategy cards. The asker's own hand is never described beyond the
//! card being offered.

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{ActionCardId, PlanetId, PlayerId, SystemId};
use ti4_model::state::GameState;
use ti4_model::units::Unit;

use crate::decision_context::{EffectPreview, TriggerUnits};
use crate::event::Event;
use crate::phrases::{Phrase, Subject};

/// The printed names of the cards that have a preview (kept in step with `preview_for` by a test).
pub const PREVIEWED: &[&str] = &[
    "Black Market Dealings",
    "Blitz",
    "Bunker",
    "Confounding Legal Text",
    "Counterstroke",
    "Coup d'Etat",
    "Crisis",
    "Deadly Plot",
    "Direct Hit",
    "Disable",
    "Distinguished Councilor",
    "Emergency Repairs",
    "Extreme Duress",
    "Fighter Prototype",
    "Flank Speed",
    "Hack Election",
    "Harness Energy",
    "Infiltrate",
    "Intercept",
    "Lost Star Chart",
    "Maneuvering Jets",
    "Master Plan",
    "Morale Boost",
    "Nav Suite",
    "Parley",
    "Political Stability",
    "Puppets on a String",
    "Rally",
    "Reflective Shielding",
    "Reverse Engineer",
    "Rout",
    "Sabotage",
    "Salvage",
    "Scramble Frequency",
    "Shields Holding",
    "Solar Flare",
    "Veto",
    "War Machine",
    "Waylay",
];

fn blank(option: &str, card: &ActionCardId, effect: &str) -> EffectPreview {
    EffectPreview {
        option: option.to_owned(),
        card: card.to_string(),
        effect: effect.to_owned(),
        seats: Vec::new(),
        system: None,
        planets: Vec::new(),
        units: Vec::new(),
        amount: None,
        of: None,
        refs: Vec::new(),
        fizzles: None,
        phrase: Phrase::default(),
    }
}

/// Count `units` by owner and type, in a stable order.
fn tally<'a>(units: impl IntoIterator<Item = &'a Unit>) -> Vec<TriggerUnits> {
    let mut counts: BTreeMap<(PlayerId, String), u32> = BTreeMap::new();
    for unit in units {
        *counts
            .entry((unit.owner.clone(), unit.type_id.to_string()))
            .or_insert(0) += 1;
    }
    counts
        .into_iter()
        .map(|((owner, unit_type), count)| TriggerUnits {
            owner,
            unit_type,
            count,
        })
        .collect()
}

fn seats_of(units: &[TriggerUnits]) -> Vec<PlayerId> {
    let mut seats: Vec<PlayerId> = units.iter().map(|entry| entry.owner.clone()).collect();
    seats.sort();
    seats.dedup();
    seats
}

/// The system the window is about: the event's own, else the active one.
fn system_of(state: &GameState, event: &Event) -> Option<SystemId> {
    event
        .text("system")
        .filter(|system| !system.is_empty())
        .map(SystemId::new)
        .or_else(|| state.active_system.clone())
}

/// Every unit in a system: the space area and the planets.
fn all_units(state: &GameState, system: &SystemId) -> Vec<Unit> {
    let board = state.system_state(system);
    board
        .units
        .iter()
        .chain(board.planet_units.values().flatten())
        .cloned()
        .collect()
}

/// What playing `alias` (in `option`) would do in the window `event` opened, for `player`; `None`
/// for a card whose effect needs no stated context beyond its printed text.
#[must_use]
pub fn preview_for(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    event: &Event,
    alias: &ActionCardId,
    option: &str,
) -> Option<EffectPreview> {
    let name = crate::action_cards::name_of(content, alias);
    let preview = |effect: &str| blank(option, alias, effect);
    // Only the cards that look at unit kinds need the catalogue.
    let types = std::cell::LazyCell::new(|| ti4_content::units::catalogue(content, sources));
    let base = |unit: &Unit| {
        types
            .get(unit.type_id.as_str())
            .map(|kind| kind.base_type().to_owned())
    };
    let result = match name.as_str() {
        "Sabotage" => {
            let mut p = preview("cancel_card");
            p.seats = event
                .text("player")
                .map(|seat| vec![PlayerId::new(seat)])
                .unwrap_or_default();
            p.refs = event.text("card").map(str::to_owned).into_iter().collect();
            p
        }
        "Direct Hit" => {
            let mut p = preview("destroy_unit");
            let (system, victim, unit_type, _, _) = state.last_sustain.clone()?;
            p.seats = vec![victim.clone()];
            p.units = vec![TriggerUnits {
                owner: victim.clone(),
                unit_type: unit_type.to_string(),
                count: 1,
            }];
            if !crate::combat::direct_hittable_at(
                state,
                content,
                sources,
                &victim,
                &system,
                unit_type.as_str(),
            ) {
                p.fizzles = Some("immune".to_owned());
            }
            p.system = Some(system);
            p
        }
        "Shields Holding" | "Maneuvering Jets" => {
            let mut p = preview("cancel_hits");
            p.amount = Some(if name == "Shields Holding" { 2 } else { 1 });
            p.of = event.integer("hits");
            p.system = system_of(state, event);
            if name == "Maneuvering Jets" {
                p.seats = event
                    .text("gunner")
                    .map(|seat| vec![PlayerId::new(seat)])
                    .unwrap_or_default();
            }
            p
        }
        "Reflective Shielding" => {
            let mut p = preview("produce_hits");
            let (system, _, _, producer, _) = state.last_sustain.clone()?;
            p.seats = vec![producer];
            p.system = Some(system);
            p.amount = Some(2);
            p
        }
        "Salvage" => {
            let mut p = preview("take_commodities");
            let (system, opponents) = state.last_combat_sides.clone()?;
            let opponents: Vec<PlayerId> = opponents
                .into_iter()
                .filter(|seat| seat != player)
                .collect();
            let total: i64 = opponents
                .iter()
                .filter_map(|seat| state.player(seat))
                .map(|seat| i64::from(seat.commodities))
                .sum();
            p.seats = opponents;
            p.amount = Some(total);
            p.system = Some(system);
            if total == 0 {
                p.fizzles = Some("nothing_there".to_owned());
            }
            p
        }
        "Parley" => {
            let mut p = preview("return_committed_unit");
            let (owner, system, planet, unit) = state.last_committed_unit.clone()?;
            p.seats = vec![owner];
            p.units = tally([&unit]);
            p.system = Some(system);
            p.planets = vec![planet];
            p
        }
        "Infiltrate" => {
            let mut p = preview("replace_structures");
            let (system, planet, _, _) = state.last_control_gained.clone()?;
            // The rival PDS and space docks the card would swap out, as the effect reads them.
            let standing: Vec<Unit> = if state.rules_version
                >= ti4_model::state::RULES_INFILTRATE_FIX
            {
                crate::action_cards::infiltrate_swaps(
                    state,
                    content,
                    ti4_model::content_types::SourceSet::all(),
                    player,
                    &system,
                    &planet,
                )
                .into_iter()
                .map(|(old, _)| old)
                .collect()
            } else {
                // Rules version 0: the card replaced the player's own structures.
                state
                    .system_state(&system)
                    .on_planet(&planet)
                    .iter()
                    .filter(|unit| {
                        &unit.owner == player
                            && base(unit).is_some_and(|kind| kind == "pds" || kind == "spacedock")
                    })
                    .cloned()
                    .collect()
            };
            p.units = tally(&standing);
            if standing.is_empty() {
                p.fizzles = Some("nothing_there".to_owned());
            }
            p.system = Some(system);
            p.planets = vec![planet];
            p
        }
        "Deadly Plot" => {
            let mut p = preview("discard_agenda");
            let planets = state
                .board
                .values()
                .flat_map(|system| system.planet_control.iter())
                .filter(|(planet, owner)| {
                    *owner == player && !state.exhausted_planets.contains(*planet)
                })
                .count();
            p.amount = i64::try_from(planets).ok();
            p.refs = event
                .text("agenda")
                .map(str::to_owned)
                .into_iter()
                .collect();
            p
        }
        "Counterstroke" => {
            let mut p = preview("return_command_token");
            p.system = state.active_system.clone();
            p.amount = Some(1);
            p
        }
        "Rout" | "Intercept" => {
            let mut p = preview(if name == "Rout" {
                "force_retreat"
            } else {
                "bar_retreat"
            });
            if let Some((system, attacker, defender)) = state.active_space_combat.clone() {
                p.seats = [attacker, defender]
                    .into_iter()
                    .filter(|seat| seat != player)
                    .collect();
                p.system = Some(system);
            }
            p
        }
        "Waylay" => {
            let mut p = preview("barrage_hits_all_ships");
            p.system = system_of(state, event);
            p
        }
        "Emergency Repairs" => {
            let mut p = preview("repair_units");
            let system = state.active_system.clone()?;
            let hurt: Vec<Unit> = all_units(state, &system)
                .into_iter()
                .filter(|unit| &unit.owner == player && unit.sustained_damage)
                .collect();
            p.units = tally(&hurt);
            if hurt.is_empty() {
                p.fizzles = Some("nothing_there".to_owned());
            }
            p.system = Some(system);
            p
        }
        "Disable" => {
            let mut p = preview("disable_pds");
            let system = state.active_system.clone()?;
            let guns: Vec<Unit> = all_units(state, &system)
                .into_iter()
                .filter(|unit| {
                    &unit.owner != player && base(unit).is_some_and(|kind| kind == "pds")
                })
                .collect();
            p.units = tally(&guns);
            p.seats = seats_of(&p.units);
            if guns.is_empty() {
                p.fizzles = Some("nothing_there".to_owned());
            }
            p.system = Some(system);
            p
        }
        "Bunker" => {
            let mut p = preview("bombardment_penalty");
            let system = state.active_system.clone()?;
            p.planets = state
                .system_state(&system)
                .planet_control
                .iter()
                .filter(|(_, owner)| *owner == player)
                .map(|(planet, _)| planet.clone())
                .collect::<Vec<PlanetId>>();
            p.amount = Some(-4);
            p.system = Some(system);
            p
        }
        "Blitz" => {
            let mut p = preview("grant_bombardment");
            let system = state.active_system.clone()?;
            let ships: Vec<Unit> = state
                .system_state(&system)
                .units
                .iter()
                .filter(|unit| {
                    &unit.owner == player
                        && types.get(unit.type_id.as_str()).is_some_and(|kind| {
                            kind.is_ship()
                                && kind.base_type() != "fighter"
                                && !kind.has_bombardment()
                        })
                })
                .cloned()
                .collect();
            p.units = tally(&ships);
            if ships.is_empty() {
                p.fizzles = Some("nothing_there".to_owned());
            }
            p.amount = Some(6);
            p.system = Some(system);
            p
        }
        "Fighter Prototype" => {
            let mut p = preview("fighter_bonus");
            let system = state.active_system.clone()?;
            let fighters: Vec<Unit> = state
                .system_state(&system)
                .units
                .iter()
                .filter(|unit| {
                    &unit.owner == player && base(unit).is_some_and(|kind| kind == "fighter")
                })
                .cloned()
                .collect();
            p.units = tally(&fighters);
            if fighters.is_empty() {
                p.fizzles = Some("nothing_there".to_owned());
            }
            p.amount = Some(2);
            p.system = Some(system);
            p
        }
        "Rally" => {
            let mut p = preview("gain_fleet_tokens");
            p.amount = Some(2);
            if let Some(system) = state.active_system.clone() {
                let others: Vec<TriggerUnits> = tally(
                    state
                        .system_state(&system)
                        .units
                        .iter()
                        .filter(|unit| &unit.owner != player),
                );
                p.seats = seats_of(&others);
                p.system = Some(system);
            }
            p
        }
        "Harness Energy" => {
            let mut p = preview("replenish_commodities");
            let limit = crate::strategy_cards::commodity_limit(state, content, player);
            let held = state.player(player).map_or(0, |seat| seat.commodities);
            p.amount = Some(i64::from((limit - held).max(0)));
            p.of = Some(i64::from(limit));
            p.system = state.active_system.clone();
            p
        }
        "Hack Election" => {
            let mut p = preview("vote_last");
            p.refs = event
                .text("agenda")
                .map(str::to_owned)
                .into_iter()
                .collect();
            p
        }
        "Distinguished Councilor" => {
            let mut p = preview("extra_votes");
            p.amount = Some(5);
            p
        }
        "Political Stability" => {
            let mut p = preview("keep_strategy_cards");
            p.refs = state
                .player(player)
                .map(|seat| {
                    seat.strategy_cards
                        .iter()
                        .map(ToString::to_string)
                        .collect()
                })
                .unwrap_or_default();
            p
        }
        "Coup d'Etat" => {
            let mut p = preview("cancel_strategic_action");
            p.seats = event
                .text("player")
                .map(|seat| vec![PlayerId::new(seat)])
                .unwrap_or_default();
            p.refs = event.text("card").map(str::to_owned).into_iter().collect();
            p
        }
        "Confounding Legal Text" => {
            let mut p = preview("become_elected");
            p.seats = event
                .text("elected_player")
                .or_else(|| event.text("player"))
                .map(|seat| vec![PlayerId::new(seat)])
                .unwrap_or_default();
            p.refs = event
                .text("agenda")
                .map(str::to_owned)
                .into_iter()
                .collect();
            p
        }
        "Reverse Engineer" => {
            let mut p = preview("take_discarded_card");
            let (by, discarded) = state.last_action_discarded.clone()?;
            p.seats = vec![by];
            p.refs = vec![discarded.to_string()];
            if !crate::action_cards::is_component_action(content, &discarded) {
                p.fizzles = Some("no_component_action".to_owned());
            }
            p
        }
        "Scramble Frequency" => {
            let mut p = preview("reroll_all_dice");
            p.seats = state.last_reroll_player.clone().into_iter().collect();
            p.system = system_of(state, event);
            p
        }
        "Veto" => {
            let mut p = preview("replace_agenda");
            p.refs = event
                .text("agenda")
                .map(str::to_owned)
                .into_iter()
                .collect();
            p
        }
        "Morale Boost" => {
            let mut p = preview("combat_roll_bonus");
            p.amount = Some(1);
            p.system = system_of(state, event);
            p
        }
        "Flank Speed" => {
            let mut p = preview("move_bonus");
            p.amount = Some(1);
            p.system = system_of(state, event);
            p
        }
        "Lost Star Chart" => preview("wormholes_adjacent"),
        "Nav Suite" => preview("ignore_anomalies"),
        "Solar Flare" => {
            let mut p = preview("no_space_cannon");
            p.system = system_of(state, event);
            p
        }
        "War Machine" => {
            let mut p = preview("production_boost");
            p.amount = Some(4);
            p
        }
        "Master Plan" | "Puppets on a String" => preview(if name == "Master Plan" {
            "extra_action"
        } else {
            "puppet_action"
        }),
        "Extreme Duress" => {
            let mut p = preview("duress");
            p.seats = state.active.clone().into_iter().collect();
            p
        }
        "Crisis" => preview("skip_next_turn"),
        "Black Market Dealings" => {
            let mut p = preview("black_market");
            p.seats = [event.text("player"), event.text("partner")]
                .into_iter()
                .flatten()
                .filter(|seat| *seat != player.as_str())
                .map(PlayerId::new)
                .collect();
            p
        }
        _ => return None,
    };
    let phrase = phrase_of(&result);
    Some(EffectPreview { phrase, ..result })
}

/// "1 hit", "2 hits".
fn plural(count: i64, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// What `preview` would do, as one or two sentences: its facts in words.
#[allow(clippy::too_many_lines)]
fn phrase_of(preview: &EffectPreview) -> Phrase {
    let new = Phrase::default;
    let system = preview.system.as_ref();
    let unit = preview.units.first();
    let seat = preview.seats.first();
    let planet = preview.planets.first();
    let amount = preview.amount.unwrap_or(0);
    // The printed size when the facts have none.
    let or = |default: i64| {
        preview
            .amount
            .filter(|value| *value != 0)
            .unwrap_or(default)
    };
    let fizzles = preview.fizzles.is_some();
    let nothing = |phrase: Phrase| phrase.say(" Playing it now would change nothing.");
    let the_planet = |phrase: Phrase, none: &str| match planet {
        Some(planet) => phrase.name(Subject::planet(planet)),
        None => phrase.say(none),
    };
    let action_card = |phrase: Phrase, none: &str| match preview.refs.first() {
        Some(card) => phrase.name(Subject::action_card(&ActionCardId::new(card))),
        None => phrase.say(none),
    };
    let strategy_card = |card: &String| Subject::StrategyCard { card: card.clone() };
    let seat_or = |phrase: Phrase, none: &str| match seat {
        Some(seat) => phrase.name(Subject::seat(seat)),
        None => phrase.say(none),
    };
    // The owner's unit: "Bob's dreadnought".
    let owned_unit = |phrase: Phrase| match unit {
        Some(unit) => phrase
            .name(Subject::possessive(&unit.owner))
            .say(" ")
            .name(Subject::Unit {
                unit_type: unit.unit_type.clone(),
            }),
        None => phrase.say("the unit"),
    };
    match preview.effect.as_str() {
        "cancel_card" => action_card(
            new().say("Cancels ").possessive(seat, "a player's").say(" "),
            "action card",
        )
        .say(". The card stays spent and its effect does not happen."),
        "destroy_unit" => {
            if preview.fizzles.as_deref() == Some("immune") {
                nothing(
                    owned_unit(new())
                        .in_system(system)
                        .say(" cannot be destroyed by Direct Hit."),
                )
            } else {
                owned_unit(new().say("Destroys "))
                    .in_system(system)
                    .say(", the ship that just sustained damage.")
            }
        }
        "cancel_hits" => {
            let from = |phrase: Phrase| match seat {
                Some(seat) => phrase
                    .say(" from ")
                    .name(Subject::possessive(seat))
                    .say(" space cannon fire"),
                None => phrase,
            };
            match preview.of {
                None => from(new().say(&format!("Cancels up to {}", plural(amount, "hit", "hits"))))
                    .say("."),
                Some(of) => {
                    let cancelled = amount.min(of);
                    let rest = if cancelled < of {
                        format!(", leaving {}.", of - cancelled)
                    } else {
                        ", so none are assigned.".to_owned()
                    };
                    from(new().say(&format!(
                        "Cancels {cancelled} of the {}",
                        plural(of, "hit", "hits")
                    )))
                    .say(" about to be assigned")
                    .in_system(system)
                    .say(&rest)
                }
            }
        }
        "produce_hits" => new()
            .say(&format!("Produces {} against ", plural(amount, "hit", "hits")))
            .possessive(seat, "a player's")
            .say(" ships")
            .in_system(system)
            .say("."),
        "take_commodities" => {
            if fizzles {
                nothing(
                    new()
                        .seats(&preview.seats, "an opponent")
                        .say(" hold no commodities."),
                )
            } else {
                let gain = new().say(&format!(
                    "You would gain {}",
                    plural(amount, "commodity", "commodities")
                ));
                let gain = if preview.seats.is_empty() {
                    gain
                } else {
                    gain.say(" (").seats(&preview.seats, "").say(")")
                };
                gain.say(", as trade goods.")
            }
        }
        "return_committed_unit" => the_planet(owned_unit(new().say("Returns ")).say(" from "), "the planet")
            .say(" to the space area")
            .in_system(system)
            .say("; it does not land."),
        "replace_structures" => {
            if fizzles {
                nothing(the_planet(new().say("You have no PDS or space dock on "), "that planet").say("."))
            } else {
                the_planet(new().say("Replaces your ").units(&preview.units).say(" on "), "that planet")
                    .say(" with matching units from your reinforcements.")
            }
        }
        "discard_agenda" => {
            let discards = new().say("Discards ");
            let discards = match preview.refs.first() {
                Some(agenda) => discards.name(Subject::Agenda {
                    card: agenda.clone(),
                }),
                None => discards.say("the agenda"),
            };
            discards.say(&format!(
                " with no effect and no replacement, then exhausts {} of yours.",
                plural(amount, "ready planet", "ready planets")
            ))
        }
        "return_command_token" => {
            let phrase = new().say("Returns your command token in ");
            match system {
                Some(system) => phrase.name(Subject::system(system)),
                None => phrase.say("that system"),
            }
            .say(" to your tactic pool.")
        }
        "force_retreat" => new()
            .seats(&preview.seats, "an opponent")
            .say(" must announce a retreat")
            .in_system(system)
            .say(", if able."),
        "bar_retreat" => new()
            .seats(&preview.seats, "an opponent")
            .say(" cannot retreat")
            .in_system(system)
            .say(" this combat round."),
        "barrage_hits_all_ships" => new().say(
            "The hits of your anti-fighter barrage roll are produced against all ships, not just fighters.",
        ),
        "repair_units" => {
            if fizzles {
                nothing(
                    new()
                        .say("None of your units")
                        .in_system(system)
                        .say(" has sustained damage."),
                )
            } else {
                new()
                    .say("Repairs your ")
                    .units(&preview.units)
                    .in_system(system)
                    .say(".")
            }
        }
        "disable_pds" => {
            if fizzles {
                nothing(new().say("No opposing PDS").in_system(system).say("."))
            } else {
                new()
                    .possessive(seat, "a player's")
                    .say(" ")
                    .units(&preview.units)
                    .in_system(system)
                    .say(" lose Planetary Shield and Space Cannon during this invasion.")
            }
        }
        "bombardment_penalty" => {
            let phrase = new().say(&format!("Applies {amount} to every bombardment roll against "));
            let phrase = if preview.planets.is_empty() {
                phrase.say("your planets")
            } else {
                preview
                    .planets
                    .iter()
                    .enumerate()
                    .fold(phrase, |phrase, (index, planet)| {
                        let phrase = if index == 0 { phrase } else { phrase.say(", ") };
                        phrase.name(Subject::planet(planet))
                    })
            };
            phrase.say(" during this invasion.")
        }
        "grant_bombardment" => {
            if fizzles {
                nothing(
                    new()
                        .say("You have no non-fighter ship without bombardment")
                        .in_system(system)
                        .say("."),
                )
            } else {
                new()
                    .say("Your ")
                    .units(&preview.units)
                    .in_system(system)
                    .say(&format!(
                        " gain Bombardment {} until the end of this invasion.",
                        or(6)
                    ))
            }
        }
        "fighter_bonus" => {
            if fizzles {
                nothing(new().say("You have no fighters").in_system(system).say("."))
            } else {
                new()
                    .say(&format!("+{} to each of your ", or(2)))
                    .units(&preview.units)
                    .say("' combat rolls this combat round.")
            }
        }
        "gain_fleet_tokens" => {
            let phrase = new().say(&format!(
                "You gain {} in your fleet pool (",
                plural(or(2), "command token", "command tokens")
            ));
            let phrase = if seat.is_some() {
                phrase
                    .seats(&preview.seats, "")
                    .say(if preview.seats.len() > 1 {
                        " have ships"
                    } else {
                        " has ships"
                    })
            } else {
                phrase.say("activated")
            };
            phrase.in_system(system).say(").")
        }
        "replenish_commodities" => {
            if amount == 0 {
                nothing(new().say("Your commodities are already full."))
            } else {
                let to = match preview.of {
                    Some(of) if of != 0 => format!(" (to {of})"),
                    _ => String::new(),
                };
                new().say(&format!("Replenishes your commodities: +{amount}{to}."))
            }
        }
        "vote_last" => new().say("You vote last on this agenda, after the speaker."),
        "extra_votes" => new().say(&format!(
            "Casts {} additional votes for the outcome you just chose.",
            or(5)
        )),
        "keep_strategy_cards" => {
            let phrase = new().say("You keep ");
            let phrase = if preview.refs.is_empty() {
                phrase.say("your strategy cards")
            } else {
                preview
                    .refs
                    .iter()
                    .enumerate()
                    .fold(phrase, |phrase, (index, card)| {
                        let phrase = if index == 0 { phrase } else { phrase.say(", ") };
                        phrase.name(strategy_card(card))
                    })
            };
            phrase.say(&format!(
                " instead of returning {}, and you do not choose a strategy card next strategy phase.",
                if preview.refs.len() == 1 { "it" } else { "them" }
            ))
        }
        "cancel_strategic_action" => {
            let phrase = new().say("Cancels ").possessive(seat, "a player's").say(" ");
            match preview.refs.first() {
                Some(card) => phrase.name(strategy_card(card)),
                None => phrase.say("strategy card"),
            }
            .say(" strategic action: it is not resolved, the card is not exhausted and their turn ends.")
        }
        "become_elected" => {
            let phrase = new().say("You are the elected player instead");
            match seat {
                Some(seat) => phrase.say(" of ").name(Subject::seat(seat)),
                None => phrase,
            }
            .say(", and the agenda's effect applies to you.")
        }
        "take_discarded_card" => {
            if fizzles {
                nothing(action_card(new(), "that card").say(" has no component action."))
            } else {
                action_card(new().say("Takes "), "that card")
                    .say(" from the discard pile into your hand.")
            }
        }
        "reroll_all_dice" => seat_or(new(), "That player")
            .say(" must reroll all of their dice for that roll."),
        "replace_agenda" => new().say(
            "Discards the revealed agenda and reveals the next one from the deck; players vote on that one instead.",
        ),
        "combat_roll_bonus" => new().say(&format!(
            "+{} to the result of each of your combat rolls during this combat round.",
            or(1)
        )),
        "move_bonus" => new().say(&format!(
            "+{} to the move value of each of your ships during this tactical action.",
            or(1)
        )),
        "wormholes_adjacent" => new().say(
            "Systems with alpha and beta wormholes count as adjacent to each other during this tactical action.",
        ),
        "ignore_anomalies" => {
            new().say("You ignore the effects of anomalies while moving during this tactical action.")
        }
        "no_space_cannon" => new()
            .say("Other players cannot use Space Cannon against your ships during the movement step")
            .in_system(system)
            .say("."),
        "production_boost" => new().say(&format!(
            "+{} to the total PRODUCTION value of your units and -1 to the combined cost of the units you produce.",
            or(4)
        )),
        "extra_action" => {
            new().say("After this action you perform one additional action before the turn passes.")
        }
        "puppet_action" => new().say("You perform one more action even though you have passed."),
        "duress" => new()
            .say("If ")
            .possessive(seat, "that player's")
            .say(
                " next action is not a strategic action, they discard all action cards, give you all trade goods and show you their secret objectives.",
            ),
        "skip_next_turn" => new().say("The next player's turn is skipped."),
        "black_market" => {
            let phrase = new().say("Your transaction");
            match seat {
                Some(seat) => phrase.say(" with ").name(Subject::seat(seat)),
                None => phrase,
            }
            .say(
                " may also include relic fragments, action cards and unscored secret objectives. It cannot be canceled.",
            )
        }
        _ => new(),
    }
}

/// The previews for every card in `cards`, keyed to `option_for(card)`.
#[must_use]
pub fn previews_for<'a>(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    event: &Event,
    cards: impl IntoIterator<Item = (&'a ActionCardId, String)>,
) -> Vec<EffectPreview> {
    cards
        .into_iter()
        .filter_map(|(card, option)| {
            preview_for(state, content, sources, player, event, card, &option)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::content_types::POK;
    use ti4_model::id::UnitTypeId;

    fn event(kind: &str, pairs: &[(&str, serde_json::Value)]) -> Event {
        Event::new(
            1,
            kind,
            pairs
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect(),
        )
    }

    fn preview(state: &GameState, who: &str, event: &Event, alias: &str) -> Option<EffectPreview> {
        preview_for(
            state,
            ContentStore::embedded(),
            POK,
            &PlayerId::new(who),
            event,
            &ActionCardId::new(alias),
            alias,
        )
    }

    #[test]
    fn direct_hit_names_the_ship_it_destroys_and_the_system() {
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.last_sustain = Some((
            SystemId::new("27"),
            PlayerId::new("b"),
            UnitTypeId::new("dreadnought"),
            PlayerId::new("a"),
            true,
        ));
        let found = preview(&state, "a", &event("SUSTAIN_DAMAGE_USED", &[]), "dh1").unwrap();
        assert_eq!(found.effect, "destroy_unit");
        assert_eq!(found.seats, vec![PlayerId::new("b")]);
        assert_eq!(found.units[0].unit_type, "dreadnought");
        assert_eq!(found.system, Some(SystemId::new("27")));
    }

    /// A preview of `effect` with only the facts that `edit` sets, as a window would give them.
    fn facts(effect: &str, edit: impl FnOnce(&mut EffectPreview)) -> String {
        let mut preview = blank("x", &ActionCardId::new("sabo1"), effect);
        edit(&mut preview);
        crate::phrases::said(&phrase_of(&preview), "a")
    }

    fn owned(owner: &str, unit_type: &str, count: u32) -> TriggerUnits {
        TriggerUnits {
            owner: PlayerId::new(owner),
            unit_type: unit_type.to_owned(),
            count,
        }
    }

    #[test]
    fn sabotage_names_the_card_it_cancels_and_whose_it_is() {
        let said = facts("cancel_card", |p| {
            p.seats = vec![PlayerId::new("b")];
            p.refs = vec!["fs1".to_owned()];
        });
        assert_eq!(
            said,
            "Cancels b's fs1. The card stays spent and its effect does not happen."
        );
        let mine = facts("cancel_card", |p| p.seats = vec![PlayerId::new("a")]);
        assert_eq!(
            mine,
            "Cancels your action card. The card stays spent and its effect does not happen."
        );
    }

    #[test]
    fn direct_hit_names_the_ship_or_says_that_it_is_immune() {
        let hit = |p: &mut EffectPreview| {
            p.seats = vec![PlayerId::new("b")];
            p.system = Some(SystemId::new("27"));
            p.units = vec![owned("b", "dreadnought", 1)];
        };
        assert_eq!(
            facts("destroy_unit", hit),
            "Destroys b's dreadnought in System 27, the ship that just sustained damage."
        );
        assert_eq!(
            facts("destroy_unit", |p| {
                hit(p);
                p.fizzles = Some("immune".to_owned());
            }),
            "b's dreadnought in System 27 cannot be destroyed by Direct Hit. Playing it now would \
             change nothing."
        );
    }

    #[test]
    fn shields_holding_states_how_many_of_the_incoming_hits_it_cancels() {
        let shields = |of: Option<i64>| {
            facts("cancel_hits", |p| {
                p.amount = Some(2);
                p.of = of;
                p.system = Some(SystemId::new("27"));
            })
        };
        assert_eq!(
            shields(Some(5)),
            "Cancels 2 of the 5 hits about to be assigned in System 27, leaving 3."
        );
        assert_eq!(
            shields(Some(1)),
            "Cancels 1 of the 1 hit about to be assigned in System 27, so none are assigned."
        );
        assert_eq!(shields(None), "Cancels up to 2 hits.");
        let jets = facts("cancel_hits", |p| {
            p.amount = Some(1);
            p.of = Some(2);
            p.seats = vec![PlayerId::new("b")];
        });
        assert_eq!(
            jets,
            "Cancels 1 of the 2 hits from b's space cannon fire about to be assigned, leaving 1."
        );
    }

    #[test]
    fn deadly_plot_counts_what_it_exhausts_and_names_the_agenda() {
        let said = facts("discard_agenda", |p| {
            p.amount = Some(4);
            p.refs = vec!["minister_of_peace".to_owned()];
        });
        assert_eq!(
            said,
            "Discards minister_of_peace with no effect and no replacement, then exhausts 4 ready \
             planets of yours."
        );
    }

    #[test]
    fn a_card_that_would_change_nothing_says_why() {
        assert_eq!(
            facts("take_commodities", |p| {
                p.seats = vec![PlayerId::new("b")];
                p.fizzles = Some("nothing_there".to_owned());
            }),
            "b hold no commodities. Playing it now would change nothing."
        );
        assert_eq!(
            facts("take_commodities", |p| {
                p.seats = vec![PlayerId::new("b"), PlayerId::new("c")];
                p.amount = Some(3);
            }),
            "You would gain 3 commodities (b and c), as trade goods."
        );
        assert_eq!(
            facts("repair_units", |p| {
                p.system = Some(SystemId::new("27"));
                p.fizzles = Some("nothing_there".to_owned());
            }),
            "None of your units in System 27 has sustained damage. Playing it now would change \
             nothing."
        );
        assert_eq!(
            facts("replenish_commodities", |p| p.amount = Some(0)),
            "Your commodities are already full. Playing it now would change nothing."
        );
    }

    #[test]
    fn units_are_counted_subjects_in_a_list() {
        let mut preview = blank("x", &ActionCardId::new("emergency"), "repair_units");
        preview.units = vec![owned("a", "cruiser", 2), owned("a", "dreadnought", 1)];
        let phrase = phrase_of(&preview);
        assert_eq!(phrase.text, "Repairs your {0}, {1}.");
        assert_eq!(
            phrase.refs["0"],
            Subject::Units {
                unit_type: "cruiser".to_owned(),
                count: 2
            }
        );
        let json = serde_json::to_value(&phrase).unwrap();
        assert_eq!(json["refs"]["1"]["kind"], "units");
        assert_eq!(json["refs"]["1"]["count"], 1);
    }

    #[test]
    fn harness_energy_counts_the_commodities_it_would_refill() {
        let mut state = crate::fixtures::game(&["a", "b"]);
        let content = ContentStore::embedded();
        let limit = crate::strategy_cards::commodity_limit(&state, content, &PlayerId::new("a"));
        state.player_mut(&PlayerId::new("a")).unwrap().commodities = 0;
        let found = preview(&state, "a", &event("SYSTEM_ACTIVATED", &[]), "harness").unwrap();
        assert_eq!(found.amount, Some(i64::from(limit)));
    }

    #[test]
    fn deadly_plot_counts_only_planets_that_are_ready() {
        let mut state = crate::fixtures::game(&["a", "b"]);
        for planet in ["p1", "p2"] {
            state
                .system_mut(&SystemId::new("27"))
                .planet_control
                .insert(PlanetId::new(planet), PlayerId::new("a"));
        }
        let mine: Vec<PlanetId> = vec![PlanetId::new("p1"), PlanetId::new("p2")];
        let ready = i64::try_from(mine.len()).unwrap();
        let event = event("AGENDA_RESOLVED", &[("agenda", "x".into())]);
        let found = preview(&state, "a", &event, "deadly_plot").unwrap();
        assert_eq!(found.amount, Some(ready));
        state.exhaust_planet(mine[0].clone());
        let found = preview(&state, "a", &event, "deadly_plot").unwrap();
        assert_eq!(found.amount, Some(ready - 1));
    }

    #[test]
    fn a_card_with_nothing_to_say_has_no_preview() {
        let state = crate::fixtures::game(&["a", "b"]);
        assert!(preview(&state, "a", &event("TURN_BEGAN", &[]), "abs").is_none());
    }

    #[test]
    fn the_preview_survives_json_and_an_old_message_without_it_decodes() {
        let state = crate::fixtures::game(&["a", "b"]);
        let found = preview(&state, "a", &event("SYSTEM_ACTIVATED", &[]), "harness").unwrap();
        let json = serde_json::to_value(&found).unwrap();
        assert!(json.get("units").is_none(), "empty fields are omitted");
        let back: EffectPreview = serde_json::from_value(json).unwrap();
        assert_eq!(back, found);
        let old = r#"{"version":1,"actor":"b","source":{"Reaction":"X"},"subtype":"reaction_when_X","phase":"action","round":1,"optional":true,"target":null,"outstanding":[]}"#;
        let context: crate::decision_context::DecisionContext = serde_json::from_str(old).unwrap();
        assert!(context.previews.is_empty());
    }

    /// Every card that has a preview, with a state and window that make its facts available.
    /// `None` of them may come back empty, and the effect kind is the one the client words.
    #[test]
    fn every_reaction_card_with_a_preview_yields_its_effect_kind() {
        use ti4_model::state::Phase;
        let mut state = crate::fixtures::game(&["a", "b"]);
        let (a, b) = (PlayerId::new("a"), PlayerId::new("b"));
        let sys = SystemId::new("27");
        let pds = Unit::new(UnitTypeId::new("pds"), b.clone());
        let dock = Unit::new(UnitTypeId::new("spacedock"), a.clone());
        let fighter = Unit::new(UnitTypeId::new("fighter"), a.clone());
        let mut cruiser = Unit::new(UnitTypeId::new("cruiser"), a.clone());
        cruiser.sustained_damage = true;
        let enemy = Unit::new(UnitTypeId::new("cruiser"), b.clone());
        {
            let board = state.system_mut(&sys);
            board.units = vec![fighter.clone(), cruiser, enemy];
            board
                .planet_units
                .insert(PlanetId::new("p1"), vec![pds, dock.clone()]);
            board.planet_control.insert(PlanetId::new("p1"), a.clone());
            board.command_tokens.insert(a.clone());
        }
        state.active_system = Some(sys.clone());
        state.active = Some(b.clone());
        state.phase = Phase::Action;
        state.active_space_combat = Some((sys.clone(), a.clone(), b.clone()));
        state.last_sustain = Some((
            sys.clone(),
            b.clone(),
            UnitTypeId::new("dreadnought"),
            a.clone(),
            true,
        ));
        state.last_committed_unit = Some((
            b.clone(),
            sys.clone(),
            PlanetId::new("p1"),
            Unit::new(UnitTypeId::new("infantry"), b.clone()),
        ));
        state.last_control_gained =
            Some((sys.clone(), PlanetId::new("p1"), a.clone(), Some(b.clone())));
        state.last_combat_sides = Some((sys.clone(), vec![a.clone(), b.clone()]));
        state.last_action_discarded = Some((b.clone(), ActionCardId::new("unexpected")));
        state.player_mut(&b).unwrap().commodities = 3;
        state.last_reroll_player = Some(b.clone());
        type Case<'a> = (
            &'a str,
            &'a str,
            &'a [(&'a str, serde_json::Value)],
            &'a str,
        );
        let cases: &[Case] = &[
            (
                "sabo1",
                "ACTION_CARD_PLAYED",
                &[("player", "b".into()), ("card", "fs1".into())],
                "cancel_card",
            ),
            ("dh1", "SUSTAIN_DAMAGE_USED", &[], "destroy_unit"),
            (
                "sh1",
                "HITS_TO_ASSIGN",
                &[("hits", 3.into())],
                "cancel_hits",
            ),
            (
                "mjets1",
                "SPACE_CANNON_HITS",
                &[("hits", 2.into()), ("gunner", "b".into())],
                "cancel_hits",
            ),
            ("reflective", "SUSTAIN_DAMAGE_USED", &[], "produce_hits"),
            ("salvage", "SPACE_COMBAT_WON", &[], "take_commodities"),
            ("parley", "UNITS_COMMITTED", &[], "return_committed_unit"),
            (
                "infiltrate",
                "PLANET_CONTROL_GAINED",
                &[],
                "replace_structures",
            ),
            (
                "deadly_plot",
                "AGENDA_RESOLVED",
                &[("agenda", "x".into())],
                "discard_agenda",
            ),
            (
                "counterstroke",
                "SYSTEM_ACTIVATED",
                &[],
                "return_command_token",
            ),
            ("rout", "RETREAT_STEP_STARTED", &[], "force_retreat"),
            ("intercept", "RETREAT_DECLARED", &[], "bar_retreat"),
            (
                "waylay",
                "ANTI_FIGHTER_BARRAGE_STARTED",
                &[],
                "barrage_hits_all_ships",
            ),
            ("emergency", "COMBAT_ROUND_STARTED", &[], "repair_units"),
            ("disable", "INVASION_BEGAN", &[], "disable_pds"),
            ("bunker", "INVASION_BEGAN", &[], "bombardment_penalty"),
            ("blitz", "INVASION_BEGAN", &[], "grant_bombardment"),
            ("f_prototype", "SPACE_COMBAT_STARTED", &[], "fighter_bonus"),
            ("rally", "SYSTEM_ACTIVATED", &[], "gain_fleet_tokens"),
            ("harness", "SYSTEM_ACTIVATED", &[], "replenish_commodities"),
            (
                "hack",
                "AGENDA_REVEALED",
                &[("agenda", "x".into())],
                "vote_last",
            ),
            ("distinguished", "VOTES_CAST", &[], "extra_votes"),
            (
                "stability",
                "STRATEGY_CARDS_WOULD_RETURN",
                &[],
                "keep_strategy_cards",
            ),
            (
                "coup",
                "STRATEGIC_ACTION_BEGAN",
                &[("player", "b".into()), ("card", "warfare".into())],
                "cancel_strategic_action",
            ),
            (
                "confounding",
                "AGENDA_RESOLVED",
                &[("elected_player", "b".into())],
                "become_elected",
            ),
            (
                "reverse_engineer",
                "ACTION_CARD_DISCARDED",
                &[],
                "take_discarded_card",
            ),
            ("scramble", "UNIT_ABILITY_ROLLED", &[], "reroll_all_dice"),
            (
                "veto",
                "AGENDA_REVEALED",
                &[("agenda", "x".into())],
                "replace_agenda",
            ),
            ("mb1", "COMBAT_ROUND_STARTED", &[], "combat_roll_bonus"),
            ("fs1", "SYSTEM_ACTIVATED", &[], "move_bonus"),
            ("lost_star", "SYSTEM_ACTIVATED", &[], "wormholes_adjacent"),
            ("nav_suite", "SYSTEM_ACTIVATED", &[], "ignore_anomalies"),
            ("solar_flare", "SYSTEM_ACTIVATED", &[], "no_space_cannon"),
            ("war_machine1", "PRODUCTION_USED", &[], "production_boost"),
            ("master_plan", "ACTION_COMPLETED", &[], "extra_action"),
            ("puppetsonastring", "PLAYER_PASSED", &[], "puppet_action"),
            ("extremeduress", "TURN_BEGAN", &[], "duress"),
            ("crisis", "TURN_PASSED", &[], "skip_next_turn"),
            (
                "blackmarketdealing",
                "TRANSACTION_OPENED",
                &[("player", "b".into()), ("partner", "a".into())],
                "black_market",
            ),
        ];
        for (alias, kind, pairs, expected) in cases {
            let found = preview(&state, "a", &event(kind, pairs), alias)
                .unwrap_or_else(|| panic!("{alias} has a preview"));
            assert_eq!(found.effect, *expected, "{alias}");
            assert_eq!(found.card, *alias);
            assert!(
                found.phrase.text.ends_with('.'),
                "{alias} is worded: {found:?}"
            );
            for key in found.phrase.refs.keys() {
                assert!(found.phrase.text.contains(&format!("{{{key}}}")), "{alias}");
            }
            assert!(
                found.fizzles.is_none() || *alias == "dh1",
                "{alias} has something to do here: {found:?}"
            );
        }
        // A fighter-less, damage-free, token-less table says so instead of promising an effect.
        let empty = crate::fixtures::game(&["a", "b"]);
        let mut empty = empty;
        empty.active_system = Some(sys.clone());
        for (alias, kind) in [
            ("emergency", "COMBAT_ROUND_STARTED"),
            ("f_prototype", "SPACE_COMBAT_STARTED"),
            ("blitz", "INVASION_BEGAN"),
            ("disable", "INVASION_BEGAN"),
        ] {
            let found = preview(&empty, "a", &event(kind, &[]), alias).unwrap();
            assert_eq!(found.fizzles.as_deref(), Some("nothing_there"), "{alias}");
        }
    }

    /// The reaction cards that have no preview, named, so a card added to a window later is a
    /// conscious choice: give it a preview or list it here. Each of these asks its own question
    /// when played (a pick, a rider outcome, a retreat), which is not what a preview is for.
    #[test]
    fn the_reaction_cards_without_a_preview_are_the_known_ones() {
        let content = ContentStore::embedded();
        let mut without: Vec<String> =
            crate::reactions::reachable(content, ti4_model::content_types::FULL)
                .into_iter()
                .map(|alias| crate::action_cards::name_of(content, &alias))
                .filter(|name| !PREVIEWED.contains(&name.as_str()))
                .collect();
        without.sort();
        without.dedup();
        assert_eq!(
            without,
            [
                "Ancient Burial Sites",
                "Assassinate Representative",
                "Bribery",
                "Confusing Legal Text",
                "Construction Rider",
                "Courageous to the End",
                "Crash Landing",
                "Decoy Operation",
                "Diplomacy Rider",
                "Diplomatic Pressure",
                "Experimental Battlestation",
                "Fire Team",
                "Forward Supply Base",
                "Ghost Squad",
                "Imperial Rider",
                "In The Silence Of Space",
                "Insider Information",
                "Leadership Rider",
                "Lie in Wait",
                "Manipulate Investments",
                "Politics Rider",
                "Public Disgrace",
                "Reparations",
                "Rescue",
                "Reveal Prototype",
                "Sanction",
                "Skilled Retreat",
                "Summit",
                "Technology Rider",
                "Trade Rider",
                "Upgrade",
                "Warfare Rider",
            ]
        );
    }

    /// `PREVIEWED` is exactly the set of cards `preview_for` answers for.
    #[test]
    fn the_previewed_list_matches_the_cards_the_table_test_exercises() {
        let content = ContentStore::embedded();
        for name in PREVIEWED {
            assert!(
                content
                    .from_sources(
                        ti4_model::content_types::ContentType::ActionCards,
                        ti4_model::content_types::FULL
                    )
                    .any(|record| record.text("name") == Some(name)),
                "{name} is a printed card"
            );
        }
        let mut sorted = PREVIEWED.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted, PREVIEWED, "kept sorted");
    }
}
