//! Acquired commander effects whose original factions are outside BF seating scope.
use crate::timing::Ability;
use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;

pub(crate) fn timing_abilities(owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    vec![
        Ability::stateful(
            format!("leader:{owner_name}:titanscommander:PRODUCTION_USED:when"),
            seat.clone(),
            "PRODUCTION_USED",
            crate::timing::Relation::When,
            std::sync::Arc::new(move |_, resolver, context| {
                crate::supply::gain_trade_goods_via(
                    context,
                    resolver,
                    &owner,
                    1,
                    "titanscommander",
                )?;
                Ok(())
            }),
        )
        .with_optional(true)
        .with_stateful_condition(std::sync::Arc::new(move |event, _, context| {
            event.text("player") == Some(condition_owner.as_str())
                && crate::promissory::has_commander_ability(
                    context.state,
                    &condition_owner,
                    "titanscommander",
                )
        })),
        claire_gibson(owner_name, seat),
        nekro_commander(owner_name, seat),
        keleres_commander(owner_name, seat),
        ralnel_commander(owner_name, seat),
        crimson_commander(owner_name, seat, "SPACE_COMBAT_ENDED"),
        crimson_commander(owner_name, seat, "GROUND_COMBAT_ENDED"),
    ]
}

/// Whether the Crimson commander's holder can gain a commodity and whether they can convert one.
fn crimson_payments(state: &GameState, content: &ContentStore, player: &PlayerId) -> (bool, bool) {
    let limit = crate::strategy_cards::commodity_limit(state, content, player);
    let held = state.player(player).map_or(0, |seat| seat.commodities);
    (held < limit, held > 0)
}

/// Ask the Crimson commander's holder which payment to take when both are possible.
fn crimson_ask(
    context: &mut crate::timing::TimingContext<'_>,
    owner: &PlayerId,
) -> Result<bool, crate::choice::IllegalChoice> {
    let held = context
        .state
        .player(owner)
        .map_or(0, |seat| seat.commodities);
    let goods = context
        .state
        .player(owner)
        .map_or(0, |seat| seat.trade_goods);
    let limit = crate::strategy_cards::commodity_limit(context.state, context.content, owner);
    let choice = crate::strategy_cards::commander_payment_offer(
        crate::choice::Choice::new(
            owner.clone(),
            "Crimson commander: gain 1 commodity or convert 1 commodity to a trade good",
            vec![
                crate::choice::ChoiceOption::labelled(
                    "gain".to_owned(),
                    "economy",
                    "gain 1 commodity".to_owned(),
                ),
                crate::choice::ChoiceOption::labelled(
                    "convert".to_owned(),
                    "economy",
                    "convert 1 commodity to a trade good".to_owned(),
                ),
            ],
        ),
        context.content,
        "crimsoncommander",
        held,
        limit,
        goods,
    )
    .contextualized(crate::decision_context::DecisionContext::new(
        owner.clone(),
        crate::decision_context::DecisionSource::Content("crimsoncommander".to_owned()),
        "crimson_payment",
        context.state.phase,
        context.state.round,
    ));
    Ok(context.ask_seeing(&choice)?.id == "convert")
}

/// Crimson Rebellion (`crimsoncommander`): "At the end of a combat between any players: Gain 1
/// commodity or convert 1 of your commodities to a trade good." Not optional ("may" is absent);
/// the holder chooses only when both payments are possible, and nothing happens when neither is
/// (no room for a commodity and none to convert). One ability per combat kind, since a space and
/// a ground combat each end separately.
fn crimson_commander(owner_name: &str, seat: &PlayerId, event_type: &'static str) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("leader:{owner_name}:crimsoncommander:{event_type}:after"),
        seat.clone(),
        event_type,
        crate::timing::Relation::After,
        std::sync::Arc::new(move |_, _, context| {
            let (can_gain, can_convert) = crimson_payments(context.state, context.content, &owner);
            let convert = if can_gain && can_convert {
                crimson_ask(context, &owner).map_err(crate::timing::TimingError::IllegalChoice)?
            } else {
                can_convert
            };
            if let Some(seat) = context.state.player_mut(&owner) {
                if convert {
                    seat.commodities -= 1;
                    seat.trade_goods += 1;
                } else {
                    seat.commodities += 1;
                }
            }
            if convert {
                crate::supply::note_trade_goods_gained(
                    context.state,
                    &owner,
                    1,
                    "crimsoncommander",
                );
            } else {
                crate::supply::note_commodities_gained(context.state, &owner, 1);
            }
            Ok(())
        }),
    )
    .with_stateful_condition(std::sync::Arc::new(move |_, _, context| {
        let (can_gain, can_convert) =
            crimson_payments(context.state, context.content, &condition_owner);
        (can_gain || can_convert)
            && crate::promissory::has_commander_ability(
                context.state,
                &condition_owner,
                "crimsoncommander",
            )
    }))
}

/// Systems adjacent to `system` that hold no other player's ships, for Ral Nel's commander.
fn ralnel_destinations(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &ti4_content::galaxy::Galaxy,
    player: &PlayerId,
    system: &ti4_model::id::SystemId,
) -> Vec<ti4_model::id::SystemId> {
    let types = ti4_content::units::catalogue(content, sources);
    crate::movement::PlayerAdjacency::new(state, content, sources, galaxy, player)
        .neighbours(system.as_str())
        .into_iter()
        .map(ti4_model::id::SystemId::new)
        .filter(|adjacent| {
            !state.system_state(adjacent).units.iter().any(|unit| {
                &unit.owner != player
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(ti4_content::units::UnitType::is_ship)
            })
        })
        .collect()
}

/// The seat's ships in `system` that move under their own power, in board order.
fn ralnel_ships(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &ti4_model::id::SystemId,
) -> Vec<ti4_model::units::Unit> {
    let types = ti4_content::units::catalogue(content, sources);
    state
        .system_state(system)
        .units_of(player)
        .into_iter()
        .filter(|unit| {
            types
                .get(unit.type_id.as_str())
                .is_some_and(|kind| kind.is_ship() && kind.move_value() > 0)
        })
        .cloned()
        .collect()
}

/// Ral Nel (`ralnelcommander`): "When you declare a retreat: Immediately retreat up to 2 of your
/// ships from the active system to an adjacent system that does not contain another player's
/// ships. Place a command token from your reinforcements into that system." The seat picks the
/// destination, then up to two ships one at a time (stopping after the first is allowed); a ship
/// that moves carries nothing, since the card moves ships, not cargo. Atomic: a refused answer
/// leaves the board as it was.
fn ralnel_commander(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("leader:{owner_name}:ralnelcommander:RETREAT_DECLARED:when"),
        seat.clone(),
        "RETREAT_DECLARED",
        crate::timing::Relation::When,
        std::sync::Arc::new(move |event, _, context| {
            let (Some(system), Some(galaxy)) = (event.text("system"), context.galaxy) else {
                return Ok(());
            };
            let system = ti4_model::id::SystemId::new(system);
            let before = context.state.clone();
            let result = ralnel_resolve(context, galaxy, &owner, &system);
            if let Err(error) = result {
                *context.state = before;
                return Err(crate::timing::TimingError::IllegalChoice(error));
            }
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(std::sync::Arc::new(move |event, _, context| {
        let (Some(system), Some(galaxy)) = (event.text("system"), context.galaxy) else {
            return false;
        };
        let system = ti4_model::id::SystemId::new(system);
        event.text("player") == Some(condition_owner.as_str())
            && crate::promissory::has_commander_ability(
                context.state,
                &condition_owner,
                "ralnelcommander",
            )
            && !ralnel_ships(
                context.state,
                context.content,
                context.sources,
                &condition_owner,
                &system,
            )
            .is_empty()
            && !ralnel_destinations(
                context.state,
                context.content,
                context.sources,
                galaxy,
                &condition_owner,
                &system,
            )
            .is_empty()
    }))
}

fn ralnel_ask(
    context: &mut crate::timing::TimingContext<'_>,
    owner: &PlayerId,
    prompt: &str,
    subtype: &str,
    options: Vec<crate::choice::ChoiceOption>,
) -> Result<crate::choice::ChoiceOption, crate::choice::IllegalChoice> {
    let choice = crate::choice::Choice::new(owner.clone(), prompt, options).contextualized(
        crate::decision_context::DecisionContext::new(
            owner.clone(),
            crate::decision_context::DecisionSource::Content("ralnelcommander".to_owned()),
            subtype,
            context.state.phase,
            context.state.round,
        ),
    );
    context.ask_seeing(&choice)
}

fn ralnel_resolve(
    context: &mut crate::timing::TimingContext<'_>,
    galaxy: &ti4_content::galaxy::Galaxy,
    owner: &PlayerId,
    system: &ti4_model::id::SystemId,
) -> Result<(), crate::choice::IllegalChoice> {
    let destinations = ralnel_destinations(
        context.state,
        context.content,
        context.sources,
        galaxy,
        owner,
        system,
    );
    let options = destinations
        .iter()
        .map(|to| {
            crate::choice::ChoiceOption::labelled(
                format!("to|{to}"),
                "retreat",
                format!("retreat up to 2 ships to {to}"),
            )
        })
        .collect();
    let answer = ralnel_ask(
        context,
        owner,
        "Ral Nel commander: where do up to 2 ships retreat now?",
        "ralnel_retreat_destination",
        options,
    )?;
    let Some(to) = answer.id.strip_prefix("to|") else {
        return Ok(());
    };
    let to = ti4_model::id::SystemId::new(to);
    for moved in 0..2 {
        let ships = ralnel_ships(
            context.state,
            context.content,
            context.sources,
            owner,
            system,
        );
        if ships.is_empty() {
            break;
        }
        let mut options: Vec<crate::choice::ChoiceOption> = ships
            .iter()
            .enumerate()
            .map(|(index, unit)| {
                crate::choice::ChoiceOption::labelled(
                    format!("ship|{index}|{}", unit.type_id),
                    "retreat",
                    format!("retreat {}", unit.type_id),
                )
            })
            .collect();
        if moved > 0 {
            options.push(crate::choice::ChoiceOption::decline());
        }
        let answer = ralnel_ask(
            context,
            owner,
            "Ral Nel commander: retreat which ship now?",
            "ralnel_retreat_ship",
            options,
        )?;
        let Some(index) = answer
            .id
            .strip_prefix("ship|")
            .and_then(|rest| rest.split('|').next())
            .and_then(|index| index.parse::<usize>().ok())
        else {
            break;
        };
        let unit = ships[index].clone();
        context
            .state
            .move_units(system, &to, std::slice::from_ref(&unit));
    }
    context.state.system_mut(&to).place_token(owner.clone());
    Ok(())
}

/// Keleres (`kelerescommander`): "After you perform a component action: You may perform an
/// additional action." Arms the same retained-turn flag Master Plan uses, on the typed
/// `ACTION_COMPLETED` whose `component` flag the game sets for component actions.
fn keleres_commander(owner_name: &str, seat: &PlayerId) -> Ability {
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("leader:{owner_name}:kelerescommander:ACTION_COMPLETED:after"),
        seat.clone(),
        "ACTION_COMPLETED",
        crate::timing::Relation::After,
        std::sync::Arc::new(move |_, _, context| {
            context
                .state
                .transient_flags
                .set(ti4_model::state::TransientFlags::ADDITIONAL_ACTION);
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(std::sync::Arc::new(move |event, _, context| {
        event.text("player") == Some(condition_owner.as_str())
            && event.boolean("component") == Some(true)
            && !context
                .state
                .transient_flags
                .has(ti4_model::state::TransientFlags::ADDITIONAL_ACTION)
            && crate::promissory::has_commander_ability(
                context.state,
                &condition_owner,
                "kelerescommander",
            )
    }))
}

/// Nekro (`nekrocommander`): "After you gain a technology: You may draw 1 action card." On the
/// typed `TECHNOLOGY_GAINED` the game announces for every new technology id.
fn nekro_commander(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("leader:{owner_name}:nekrocommander:TECHNOLOGY_GAINED:after"),
        seat.clone(),
        "TECHNOLOGY_GAINED",
        crate::timing::Relation::After,
        std::sync::Arc::new(move |_, _, context| {
            crate::action_cards::draw(context.state, context.content, context.table, &owner, 1)
                .map_err(crate::timing::TimingError::IllegalChoice)?;
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(std::sync::Arc::new(move |event, _, context| {
        event.text("player") == Some(condition_owner.as_str())
            && !context.state.action_card_deck.is_empty()
            && crate::promissory::has_commander_ability(
                context.state,
                &condition_owner,
                "nekrocommander",
            )
    }))
}

/// Claire Gibson (`solcommander`): "At the start of a ground combat on a planet you control: You
/// may place 1 infantry from your reinforcements on that planet." For the commander's own seat, a
/// faceup Alliance, or a Yin grant. Offered only while an infantry remains in reinforcements.
fn claire_gibson(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("leader:{owner_name}:solcommander:GROUND_COMBAT_STARTED:after"),
        seat.clone(),
        "GROUND_COMBAT_STARTED",
        crate::timing::Relation::After,
        std::sync::Arc::new(move |event, _, context| {
            let (Some(system), Some(planet)) = (event.text("system"), event.text("planet")) else {
                return Ok(());
            };
            let system = ti4_model::id::SystemId::new(system);
            let planet = ti4_model::id::PlanetId::new(planet);
            crate::action_cards::place_units(
                context,
                &owner,
                &system,
                Some(&planet),
                "infantry",
                1,
            );
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(std::sync::Arc::new(move |event, _, context| {
        let (Some(system), Some(planet)) = (event.text("system"), event.text("planet")) else {
            return false;
        };
        let system = ti4_model::id::SystemId::new(system);
        let planet = ti4_model::id::PlanetId::new(planet);
        context
            .state
            .system_state(&system)
            .planet_control
            .get(&planet)
            == Some(&condition_owner)
            && crate::promissory::has_commander_ability(
                context.state,
                &condition_owner,
                "solcommander",
            )
            && crate::supply::remaining(
                context.state,
                context.content,
                context.sources,
                &condition_owner,
                &ti4_model::id::UnitTypeId::new("infantry"),
            ) > 0
    }))
}

/// Obsidian (`obsidiancommander`): "At any time: Apply +1 to the result of each of your units'
/// combat rolls in The Fracture."
///
/// A per-unit shift (positive = better) for the shared `factions::unit_roll_modifier` sum. It reads
/// the combat's own system, so it covers space and ground rolls alike; the Fracture must be in play
/// and the system must be one of its tiles. Zero for a seat without the ability.
///
/// NOTE: `factions::unit_roll_modifier` only sums the `MODULES` hooks, so this is not yet called;
/// see `plans/evidence/BF-BORROWED-COMMANDERS-A.md` for the one-line dispatch the coordinator adds.
pub(crate) fn unit_roll_modifier(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    unit: &crate::factions::CombatUnit<'_>,
) -> i64 {
    let Some(system) = unit.system else { return 0 };
    if state.fracture_in_play
        && crate::fracture::is_fracture_system(content, sources, system)
        && crate::promissory::has_commander_ability(state, unit.player, "obsidiancommander")
    {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::choice::{Scripted, Table};
    use ti4_content::ContentStore;
    use ti4_model::content_types::DEFAULT;
    use ti4_model::id::LeaderId;
    use ti4_model::state::{GameState, LeaderStatus};

    fn arena() -> GameState {
        crate::fixtures::seated_game(&[("a", "yin"), ("b", "hacan")], DEFAULT)
    }
    fn fire(state: &mut GameState, producing: &str, answer: &str) {
        let mut resolver = crate::fixtures::armed_resolver(state);
        let mut table = Table::with_default(Box::new(Scripted::new([answer])));
        crate::fixtures::with_context(state, DEFAULT, None, &mut table, |ctx| {
            let event = ctx
                .event_sequence
                .next(
                    "PRODUCTION_USED",
                    [("player".to_owned(), producing.into())].into(),
                )
                .unwrap();
            resolver.emit_with_context(ctx, event, |_, _| {}).unwrap();
        });
    }
    fn ground_start(state: &mut GameState, system: &str, planet: &str, answer: &str) {
        let mut resolver = crate::fixtures::armed_resolver(state);
        let mut table = Table::with_default(Box::new(Scripted::new([answer])));
        crate::fixtures::with_context(state, DEFAULT, None, &mut table, |ctx| {
            let event = ctx
                .event_sequence
                .next(
                    "GROUND_COMBAT_STARTED",
                    [
                        ("system".to_owned(), system.into()),
                        ("planet".to_owned(), planet.into()),
                        ("attacker".to_owned(), "b".into()),
                        ("defender".to_owned(), "a".into()),
                    ]
                    .into(),
                )
                .unwrap();
            resolver.emit_with_context(ctx, event, |_, _| {}).unwrap();
        });
    }

    #[test]
    fn the_keleres_commander_grants_an_additional_action_after_a_component_action_only() {
        use ti4_model::state::TransientFlags;
        let completed = |state: &mut GameState, component: bool, answer: &str| {
            let mut resolver = crate::fixtures::armed_resolver(state);
            let mut table = Table::with_default(Box::new(Scripted::new([answer])));
            crate::fixtures::with_context(state, DEFAULT, None, &mut table, |ctx| {
                let event = ctx
                    .event_sequence
                    .next(
                        "ACTION_COMPLETED",
                        [
                            ("player".to_owned(), "a".into()),
                            ("component".to_owned(), serde_json::Value::Bool(component)),
                        ]
                        .into(),
                    )
                    .unwrap();
                resolver.emit_with_context(ctx, event, |_, _| {}).unwrap();
            });
        };
        let ability = "leader:yin:kelerescommander:ACTION_COMPLETED:after";
        let mut state = arena();
        completed(&mut state, true, ability);
        assert!(
            !state.transient_flags.has(TransientFlags::ADDITIONAL_ACTION),
            "no ability"
        );
        assert!(crate::promissory::grant_commander_ability(
            &mut state,
            ContentStore::embedded(),
            &PlayerId::new("a"),
            "kelerescommander"
        ));
        completed(&mut state, false, ability);
        assert!(
            !state.transient_flags.has(TransientFlags::ADDITIONAL_ACTION),
            "a tactical or strategic action does not qualify"
        );
        completed(&mut state, true, "decline");
        assert!(!state.transient_flags.has(TransientFlags::ADDITIONAL_ACTION));
        completed(&mut state, true, ability);
        assert!(state.transient_flags.has(TransientFlags::ADDITIONAL_ACTION));
    }

    #[test]
    fn the_ral_nel_commander_retreats_two_ships_at_once_and_places_a_token() {
        use ti4_model::id::{SystemId, UnitTypeId};
        use ti4_model::units::Unit;
        let hub = crate::fixtures::plain_hub();
        let mut state = arena();
        let active = SystemId::new(hub.centre.clone());
        let safe = SystemId::new(hub.outer[0].clone());
        for id in std::iter::once(&hub.centre).chain(hub.outer.iter()) {
            let board = state.system_mut(&SystemId::new(id.clone()));
            board.units.clear();
            board.planet_units.clear();
            board.command_tokens.clear();
        }
        // Every neighbour but `safe` holds an enemy ship, so `safe` is the only destination.
        for id in hub.outer.iter().skip(1) {
            state
                .system_mut(&SystemId::new(id.clone()))
                .units
                .push(Unit::new(UnitTypeId::new("destroyer"), PlayerId::new("b")));
        }
        let a = PlayerId::new("a");
        for _ in 0..3 {
            state
                .system_mut(&active)
                .units
                .push(Unit::new(UnitTypeId::new("cruiser"), a.clone()));
        }
        state
            .system_mut(&active)
            .units
            .push(Unit::new(UnitTypeId::new("carrier"), PlayerId::new("b")));
        let declare = |state: &mut GameState, answers: Vec<String>| {
            let mut resolver = crate::fixtures::armed_resolver(state);
            let mut table = Table::with_default(Box::new(Scripted::new(answers)));
            crate::fixtures::with_context(state, DEFAULT, Some(&hub.galaxy), &mut table, |ctx| {
                let event = ctx
                    .event_sequence
                    .next(
                        "RETREAT_DECLARED",
                        [
                            ("system".to_owned(), hub.centre.clone().into()),
                            ("player".to_owned(), "a".into()),
                            ("round".to_owned(), 1.into()),
                        ]
                        .into(),
                    )
                    .unwrap();
                resolver.emit_with_context(ctx, event, |_, _| {}).unwrap();
            });
        };
        let cruisers = |state: &GameState, system: &SystemId| {
            state
                .system_state(system)
                .units
                .iter()
                .filter(|unit| unit.owner == a && unit.type_id.as_str() == "cruiser")
                .count()
        };
        let ability = "leader:yin:ralnelcommander:RETREAT_DECLARED:when".to_owned();
        declare(&mut state, vec![ability.clone()]);
        assert_eq!(cruisers(&state, &active), 3, "no ability: nothing moves");
        assert!(crate::promissory::grant_commander_ability(
            &mut state,
            ContentStore::embedded(),
            &a,
            "ralnelcommander"
        ));
        declare(
            &mut state,
            vec![
                ability,
                format!("to|{safe}"),
                "ship|0|cruiser".to_owned(),
                "ship|0|cruiser".to_owned(),
            ],
        );
        assert_eq!(cruisers(&state, &active), 1);
        assert_eq!(cruisers(&state, &safe), 2);
        assert!(state.system_state(&safe).command_tokens.contains(&a));
    }

    #[test]
    fn the_crimson_commander_pays_at_the_end_of_any_combat() {
        let ended = |state: &mut GameState, event: &str, answers: Vec<&str>| {
            let mut resolver = crate::fixtures::armed_resolver(state);
            let mut table = Table::with_default(Box::new(Scripted::new(answers)));
            crate::fixtures::with_context(state, DEFAULT, None, &mut table, |ctx| {
                let event = ctx
                    .event_sequence
                    .next(event, [("system".to_owned(), "18".into())].into())
                    .unwrap();
                resolver.emit_with_context(ctx, event, |_, _| {}).unwrap();
            });
        };
        let a = PlayerId::new("a");
        let mut state = arena();
        state.player_mut(&a).unwrap().commodities = 0;
        ended(&mut state, "SPACE_COMBAT_ENDED", vec![]);
        assert_eq!(
            state.player(&a).unwrap().commodities,
            0,
            "no ability, nothing"
        );
        assert!(crate::promissory::grant_commander_ability(
            &mut state,
            ContentStore::embedded(),
            &a,
            "crimsoncommander"
        ));
        // No commodity to convert: the gain is automatic.
        ended(&mut state, "SPACE_COMBAT_ENDED", vec![]);
        assert_eq!(state.player(&a).unwrap().commodities, 1);
        // Both possible: the holder chooses; a ground combat counts too.
        let goods = state.player(&a).unwrap().trade_goods;
        ended(&mut state, "GROUND_COMBAT_ENDED", vec!["convert"]);
        assert_eq!(state.player(&a).unwrap().commodities, 0);
        assert_eq!(state.player(&a).unwrap().trade_goods, goods + 1);
    }

    /// The gain-or-convert question is an offer card: the commander as printed, the holder's
    /// commodities and trade goods, and what each answer does (display only).
    #[test]
    fn the_crimson_commander_payment_is_an_offer_card() {
        let a = PlayerId::new("a");
        let mut state = arena();
        assert!(crate::promissory::grant_commander_ability(
            &mut state,
            ContentStore::embedded(),
            &a,
            "crimsoncommander"
        ));
        let limit = crate::strategy_cards::commodity_limit(&state, ContentStore::embedded(), &a);
        state.player_mut(&a).unwrap().commodities = 1;
        state.player_mut(&a).unwrap().trade_goods = 4;
        let (decider, seen) = crate::choice::Capturing::new(Box::new(Scripted::new(["gain"])));
        let mut table = Table::with_default(Box::new(decider));
        let mut resolver = crate::fixtures::armed_resolver(&state);
        crate::fixtures::with_context(&mut state, DEFAULT, None, &mut table, |ctx| {
            let event = ctx
                .event_sequence
                .next(
                    "GROUND_COMBAT_ENDED",
                    [("system".to_owned(), "18".into())].into(),
                )
                .unwrap();
            resolver.emit_with_context(ctx, event, |_, _| {}).unwrap();
        });
        let asked = seen.borrow();
        let offer = asked
            .iter()
            .find(|choice| {
                choice
                    .context
                    .as_ref()
                    .is_some_and(|context| context.subtype == "crimson_payment")
            })
            .expect("the holder was asked");
        assert_eq!(offer.details["kind"], "offer");
        assert_eq!(offer.details["card"]["title"], "Ahk Siever");
        assert_eq!(offer.details["card"]["tag"], "commander");
        assert_eq!(offer.details["facts"][0]["value"], format!("1 of {limit}"));
        assert_eq!(offer.details["facts"][1]["value"], 4);
        assert_eq!(
            offer.details["captions"]["convert"]["hint"],
            "Commodities 1 → 0, trade goods 4 → 5"
        );
        assert_eq!(
            offer
                .options
                .iter()
                .map(|o| o.id.as_str())
                .collect::<Vec<_>>(),
            ["gain", "convert"]
        );
    }

    #[test]
    fn the_nekro_commander_draws_one_after_a_technology_is_gained() {
        let mut state = arena();
        state.action_card_deck = vec![ti4_model::id::ActionCardId::new("sabo1")];
        let hand = |state: &GameState| {
            state
                .player(&PlayerId::new("a"))
                .unwrap()
                .action_cards
                .len()
        };
        let gained = |state: &mut GameState, answer: &str| {
            let mut resolver = crate::fixtures::armed_resolver(state);
            let mut table = Table::with_default(Box::new(Scripted::new([answer])));
            crate::fixtures::with_context(state, DEFAULT, None, &mut table, |ctx| {
                let event = ctx
                    .event_sequence
                    .next(
                        "TECHNOLOGY_GAINED",
                        [
                            ("player".to_owned(), "a".into()),
                            ("technology".to_owned(), "nm".into()),
                        ]
                        .into(),
                    )
                    .unwrap();
                resolver.emit_with_context(ctx, event, |_, _| {}).unwrap();
            });
        };
        let ability = "leader:yin:nekrocommander:TECHNOLOGY_GAINED:after";
        let before = hand(&state);
        gained(&mut state, ability);
        assert_eq!(hand(&state), before, "no ability, no draw");
        assert!(crate::promissory::grant_commander_ability(
            &mut state,
            ContentStore::embedded(),
            &PlayerId::new("a"),
            "nekrocommander"
        ));
        gained(&mut state, "decline");
        assert_eq!(hand(&state), before, "declined");
        gained(&mut state, ability);
        assert_eq!(hand(&state), before + 1);
        assert!(state.action_card_deck.is_empty());
    }

    #[test]
    fn claire_gibson_places_an_infantry_on_a_defended_planet_for_a_granted_seat() {
        let mut state = arena();
        let system = ti4_model::id::SystemId::new("sol-test");
        let planet = ti4_model::id::PlanetId::new("sol-test-planet");
        state
            .system_mut(&system)
            .planet_control
            .insert(planet.clone(), PlayerId::new("a"));
        let infantry = |state: &GameState| {
            state
                .system_state(&system)
                .planet_units
                .get(&planet)
                .map_or(0, Vec::len)
        };
        let ability = "leader:yin:solcommander:GROUND_COMBAT_STARTED:after";
        // Without the ability nothing is offered (Scripted would fail on an unexpected ask).
        let before = state.clone();
        ground_start(&mut state, "sol-test", "sol-test-planet", ability);
        assert_eq!(infantry(&state), infantry(&before));
        assert!(crate::promissory::grant_commander_ability(
            &mut state,
            ContentStore::embedded(),
            &PlayerId::new("a"),
            "solcommander"
        ));
        ground_start(&mut state, "sol-test", "sol-test-planet", ability);
        assert_eq!(infantry(&state), 1);
        // Declining places nothing.
        ground_start(&mut state, "sol-test", "sol-test-planet", "decline");
        assert_eq!(infantry(&state), 1);
    }

    #[test]
    fn sol_alliance_places_infantry_for_its_recipient_when_ground_combat_starts() {
        let mut state = arena();
        let recipient = PlayerId::new("a");
        let sol = PlayerId::new("b");
        state.player_mut(&sol).unwrap().faction = ti4_model::id::FactionId::new("sol");
        let system = ti4_model::id::SystemId::new("sol-alliance-test");
        let planet = ti4_model::id::PlanetId::new("sol-alliance-planet");
        state.system_mut(&system).set_control(planet.clone(), recipient.clone());
        let note = crate::promissory::note_id("an", "sol");
        crate::promissory::take(&mut state, ContentStore::embedded(), &recipient, &note);
        let count = |state: &GameState| {
            state.system_state(&system).on_planet_of(&planet, &recipient).len()
        };
        let ability = "leader:yin:solcommander:GROUND_COMBAT_STARTED:after";

        ground_start(&mut state, system.as_str(), planet.as_str(), ability);
        assert_eq!(count(&state), 0, "locked Sol commander gives no benefit");
        state.player_mut(&sol).unwrap().leaders.insert(
            LeaderId::new("solcommander"), LeaderStatus::Unlocked,
        );
        ground_start(&mut state, system.as_str(), planet.as_str(), ability);
        assert_eq!(count(&state), 1, "the recipient places their infantry");
        crate::promissory::give_back(&mut state, &note);
        ground_start(&mut state, system.as_str(), planet.as_str(), ability);
        assert_eq!(count(&state), 1, "returning Alliance removes that benefit");
    }

    fn grant(state: &mut GameState, who: &str) {
        assert!(crate::promissory::grant_commander_ability(
            state,
            ContentStore::embedded(),
            &PlayerId::new(who),
            "titanscommander"
        ));
    }
    const OFFER: &str = "leader:yin:titanscommander:PRODUCTION_USED:when";

    #[test]
    fn ownerless_titans_grant_gains_once_per_production_use_for_recipient() {
        let mut state = arena();
        grant(&mut state, "a");
        let before = state.player(&PlayerId::new("a")).unwrap().trade_goods;
        fire(&mut state, "a", OFFER);
        assert_eq!(
            state.player(&PlayerId::new("a")).unwrap().trade_goods,
            before + 1
        );
        fire(&mut state, "a", OFFER);
        assert_eq!(
            state.player(&PlayerId::new("a")).unwrap().trade_goods,
            before + 2
        );
        assert!(
            !state
                .player(&PlayerId::new("a"))
                .unwrap()
                .leaders
                .contains_key(&LeaderId::new("titanscommander"))
        );
    }
    #[test]
    fn titans_grant_does_not_apply_to_another_players_production() {
        let mut state = arena();
        grant(&mut state, "a");
        let before = state.clone();
        fire(&mut state, "b", OFFER);
        assert_eq!(state, before);
    }
    #[test]
    fn titans_gain_can_be_declined_and_no_right_is_inert() {
        let mut state = arena();
        let before = state.clone();
        fire(&mut state, "a", OFFER);
        assert_eq!(state, before);
        grant(&mut state, "a");
        let before = state.clone();
        fire(&mut state, "a", "decline");
        assert_eq!(state, before);
    }
    #[test]
    fn ordinary_titans_alliance_requires_its_seated_owner_to_unlock() {
        let mut state = arena();
        state.player_mut(&PlayerId::new("b")).unwrap().faction =
            ti4_model::id::FactionId::new("titans");
        state
            .player_mut(&PlayerId::new("b"))
            .unwrap()
            .leaders
            .insert(LeaderId::new("titanscommander"), LeaderStatus::Locked);
        state
            .promissory_notes
            .insert("an:titans".to_owned(), PlayerId::new("a"));
        state.promissory_faceup.insert("an:titans".to_owned());
        let before = state.player(&PlayerId::new("a")).unwrap().trade_goods;
        fire(&mut state, "a", OFFER);
        assert_eq!(
            state.player(&PlayerId::new("a")).unwrap().trade_goods,
            before
        );
        state
            .player_mut(&PlayerId::new("b"))
            .unwrap()
            .leaders
            .insert(LeaderId::new("titanscommander"), LeaderStatus::Unlocked);
        fire(&mut state, "a", OFFER);
        assert_eq!(
            state.player(&PlayerId::new("a")).unwrap().trade_goods,
            before + 1
        );
    }

    mod obsidian {
        use super::*;
        use crate::factions::CombatUnit;
        use ti4_model::content_types::FULL;
        use ti4_model::id::SystemId;

        fn shift(state: &GameState, who: &PlayerId, system: &str, context: &str) -> i64 {
            let system = SystemId::new(system);
            unit_roll_modifier(
                state,
                ContentStore::embedded(),
                FULL,
                &CombatUnit {
                    player: who,
                    system: Some(&system),
                    planet: None,
                    unit_type: "cruiser",
                    context,
                },
            )
        }
        fn grant_obsidian(state: &mut GameState, who: &str) {
            assert!(crate::promissory::grant_commander_ability(
                state,
                ContentStore::embedded(),
                &PlayerId::new(who),
                "obsidiancommander"
            ));
        }

        #[test]
        fn obsidian_grant_adds_one_to_own_rolls_in_the_fracture_only() {
            let mut state = arena();
            state.fracture_in_play = true;
            grant_obsidian(&mut state, "a");
            let a = PlayerId::new("a");
            assert_eq!(shift(&state, &a, "fracture1", "space"), 1);
            assert_eq!(shift(&state, &a, "fracture1", "ground"), 1);
            assert_eq!(shift(&state, &a, "19", "space"), 0, "outside the Fracture");
            assert_eq!(
                shift(&state, &PlayerId::new("b"), "fracture1", "space"),
                0,
                "another player's units"
            );
        }

        #[test]
        fn obsidian_is_inert_without_the_ability_or_before_the_fracture_is_in_play() {
            let mut state = arena();
            state.fracture_in_play = true;
            let a = PlayerId::new("a");
            assert_eq!(shift(&state, &a, "fracture1", "space"), 0, "no ability");
            grant_obsidian(&mut state, "a");
            state.fracture_in_play = false;
            assert_eq!(shift(&state, &a, "fracture1", "space"), 0, "not in play");
        }
    }
}
