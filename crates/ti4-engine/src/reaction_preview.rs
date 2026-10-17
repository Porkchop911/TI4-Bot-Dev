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
    Some(result)
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
