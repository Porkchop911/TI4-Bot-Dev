//! Production and payment (LRR 68, 75, 34, 47).
//!
//! Ported from the oracle's `engine/production.py`: `spendable_planets`, `available`, `pay`,
//! `producers`, `capacity`, `structure_allowed`, `placements`, `buildable_for` and `resolve`.
//!
//! Choices are asked inline through a [`Table`], matching `combat.rs` and `invasion.rs`.

use ti4_content::ContentStore;
use ti4_content::units::{UnitType, catalogue};
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlanetId, PlayerId, SystemId, UnitTypeId};
use ti4_model::state::GameState;
use ti4_model::units::Unit;

use crate::choice::{Choice, ChoiceOption, IllegalChoice, Table};

/// The two things a planet card can be exhausted for (LRR 75.2, 47).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spend {
    Resources,
    Influence,
}

/// The choice kind for exhausting a planet to pay.
pub const PAY_KIND: &str = "pay";
/// The choice kind for producing one unit.
pub const PRODUCE_KIND: &str = "produce";
/// The choice kind for placing a produced unit.
pub const PLACE_KIND: &str = "place";
/// The id standing for a system's space area.
pub const SPACE: &str = "space";

/// What may be produced at all.
pub const BUILDABLE: [&str; 8] = [
    "fighter",
    "infantry",
    "carrier",
    "cruiser",
    "destroyer",
    "dreadnought",
    "spacedock",
    "pds",
];

/// A war sun cannot be produced without the technology that unlocks it (67.x).
pub const UNLOCKED_BY: [(&str, &str); 1] = [("warsun", "ws")];

/// How many of a structure one planet may hold.
///
/// A planet takes one space dock and two PDS. (The second PDS needs Space Dock II in the base
/// game, which is not modelled — the cap is what matters, and it is a cap either way.)
#[must_use]
pub fn structure_limit(base_type: &str) -> Option<usize> {
    match base_type {
        "spacedock" => Some(1),
        "pds" => Some(2),
        _ => None,
    }
}

/// A planet's printed resources or influence.
#[must_use]
pub fn planet_value(
    content: &ContentStore,
    sources: SourceSet,
    planet: &PlanetId,
    kind: Spend,
) -> i64 {
    ti4_content::galaxy::all_planets(content, sources)
        .get(planet.as_str())
        .map_or(0, |record| match kind {
            Spend::Resources => record.resources(),
            Spend::Influence => record.influence(),
        })
}

/// Controlled planets whose cards are still readied (LRR 34, 75.2).
#[must_use]
pub fn spendable_planets(state: &GameState, player: &PlayerId) -> Vec<PlanetId> {
    state
        .controlled_planets(player)
        .into_iter()
        .map(|(_, planet)| planet.clone())
        .filter(|planet| !state.exhausted_planets.contains(planet))
        .collect()
}

/// Spendable resources or influence, counting trade goods (LRR 75.3, 47.3).
#[must_use]
pub fn available(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    kind: Spend,
) -> i64 {
    let from_planets: i64 = spendable_planets(state, player)
        .iter()
        .map(|planet| planet_value(content, sources, planet, kind))
        .sum();
    let goods = state
        .player(player)
        .map_or(0, |seat| i64::from(seat.trade_goods));
    from_planets + goods
}

/// Spend resources or influence, the player choosing what to exhaust.
///
/// A planet card is exhausted for one or the other, **never both** (34.3, 75.2), and a trade
/// good stands in for either (75.3, 47.3). Returns `false` without spending anything if the
/// cost cannot be met.
///
/// # Errors
/// [`IllegalChoice`] when a decider answers with something not offered.
pub fn pay(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut Table,
    player: &PlayerId,
    cost: i64,
    kind: Spend,
) -> Result<bool, IllegalChoice> {
    if cost <= 0 {
        return Ok(true);
    }
    if available(state, content, sources, player, kind) < cost {
        return Ok(false);
    }

    let mut paid = 0;
    while paid < cost {
        let spendable = spendable_planets(state, player);
        let goods = state
            .player(player)
            .map_or(0, |seat| i64::from(seat.trade_goods));

        // What each option is worth and what is still owed travel on the option, not in the
        // label: a decider needs both numbers, and parsing them back out of
        // "exhaust jord for 4 resources" works until someone rewords the string.
        let mut options: Vec<ChoiceOption> = spendable
            .iter()
            .map(|planet| {
                let worth = planet_value(content, sources, planet, kind);
                ChoiceOption::labelled(
                    format!("exhaust|{planet}"),
                    PAY_KIND,
                    format!("exhaust {planet} for {worth}"),
                )
                .with("worth", worth)
                .with("owed", cost - paid)
            })
            .collect();
        if goods > 0 {
            options.push(
                ChoiceOption::labelled("trade_good", PAY_KIND, "spend a trade good")
                    .with("worth", 1)
                    .with("owed", cost - paid),
            );
        }
        if options.is_empty() {
            return Ok(false);
        }

        let choice = Choice::new(player.clone(), format!("pay {cost}"), options);
        let answer = table.ask(&choice)?;
        if answer.id == "trade_good" {
            if let Some(seat) = state.player_mut(player) {
                seat.trade_goods -= 1;
            }
            paid += 1;
        } else if let Some(planet) = answer.id.strip_prefix("exhaust|") {
            let planet = PlanetId::new(planet);
            paid += planet_value(content, sources, &planet, kind);
            state.exhaust_planet(planet);
        } else {
            return Ok(false);
        }
    }
    Ok(true)
}

/// What one production step costs, and how many units it yields.
///
/// 68.2: a unit whose printed cost is below one — a fighter or an infantry — is produced
/// **two at a time** for that one resource. Charging `ceil` and yielding one would make the
/// two commonest units in the game cost double what the rules ask, which is not a rounding
/// detail: it is most of an early fleet.
#[must_use]
pub fn price_of(kind: &UnitType<'_>) -> (i64, usize) {
    let printed = kind.cost();
    if printed > 0.0 && printed < 1.0 {
        return (1, 2);
    }
    // Costs are small printed integers; anything that is not finite is treated as free rather
    // than wrapping to a nonsense charge.
    // Costs are small printed integers. Counting up rather than casting keeps this free of
    // float-to-int truncation entirely, and a cost the corpus never prints simply stops at the
    // cap rather than wrapping.
    let rounded = printed.ceil().max(0.0);
    let mut charge = 0_i64;
    while f64::from(u32::try_from(charge).unwrap_or(u32::MAX)) < rounded && charge < 64 {
        charge += 1;
    }
    (charge, 1)
}

/// The player's units with Production here, paired with the planet they sit on.
#[must_use]
pub fn producers(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> Vec<(Unit, Option<PlanetId>)> {
    let types = catalogue(content, sources);
    let board = state.system_state(system);
    let produces = |unit: &Unit| {
        types
            .get(unit.type_id.as_str())
            .is_some_and(UnitType::has_production)
    };

    let mut found: Vec<(Unit, Option<PlanetId>)> = board
        .units_of(player)
        .into_iter()
        .filter(|unit| produces(unit))
        .map(|unit| (unit.clone(), None))
        .collect();
    for (planet, units) in &board.planet_units {
        found.extend(
            units
                .iter()
                .filter(|unit| &unit.owner == player && produces(unit))
                .map(|unit| (unit.clone(), Some(planet.clone()))),
        );
    }
    found
}

/// 68.1a: the production values of all the player's producing units here, combined.
///
/// A space dock's value depends on the resources of the planet it sits on, which is why the
/// planet travels with the unit rather than the value being read from the unit alone.
#[must_use]
pub fn capacity(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> i64 {
    let types = catalogue(content, sources);
    producers(state, content, sources, player, system)
        .into_iter()
        .filter_map(|(unit, planet)| {
            let kind = types.get(unit.type_id.as_str())?;
            let resources = planet.map_or(0, |planet| {
                planet_value(content, sources, &planet, Spend::Resources)
            });
            Some(kind.production(resources))
        })
        .sum()
}

/// How many of this structure the player already has on that planet.
#[must_use]
pub fn structures_on(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    planet: &PlanetId,
    base_type: &str,
) -> usize {
    let types = catalogue(content, sources);
    state
        .board
        .values()
        .filter_map(|system| system.planet_units.get(planet))
        .flatten()
        .filter(|unit| &unit.owner == player)
        .filter(|unit| {
            types
                .get(unit.type_id.as_str())
                .is_some_and(|kind| kind.base_type() == base_type)
        })
        .count()
}

/// 79.2: whether another of this structure may be built on that planet.
#[must_use]
pub fn structure_allowed(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    planet: &PlanetId,
    base_type: &str,
) -> bool {
    structure_limit(base_type)
        .is_none_or(|cap| structures_on(state, content, sources, player, planet, base_type) < cap)
}

/// Where a produced unit may go. [`SPACE`] denotes the space area.
#[must_use]
pub fn placements(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    kind: &UnitType<'_>,
) -> Vec<String> {
    if kind.is_ship() {
        return vec![SPACE.to_owned()]; // 68.2
    }
    let made = producers(state, content, sources, player, system);
    let mut spots: Vec<String> = made
        .iter()
        .filter_map(|(_, planet)| planet.clone())
        .filter(|planet| {
            structure_allowed(state, content, sources, player, planet, kind.base_type())
        })
        .map(|planet| planet.to_string())
        .collect(); // 68.3, 79.2
    if made.iter().any(|(_, planet)| planet.is_none()) {
        spots.push(SPACE.to_owned()); // 68.4
    }
    spots.dedup();
    spots
}

/// What this player can produce.
///
/// A war sun needs its technology; nothing else is gated. Faction-specific hulls are not
/// resolved — see the evidence for what that costs.
#[must_use]
pub fn buildable_for(state: &GameState, player: &PlayerId) -> Vec<String> {
    let owned = state.player(player).map(|seat| seat.technologies.clone());
    let mut out: Vec<String> = BUILDABLE.iter().map(|id| (*id).to_owned()).collect();
    for (unit, gate) in UNLOCKED_BY {
        let has = owned.as_ref().is_some_and(|held| {
            held.iter()
                .any(|tech| tech.as_str() == gate || tech.as_str().ends_with(gate))
        });
        if has {
            out.push(unit.to_owned());
        }
    }
    out
}

/// What one production step did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProductionReport {
    /// Units produced, with where they were placed.
    pub produced: Vec<(UnitTypeId, String)>,
    /// Production capacity that went unused.
    pub unused_capacity: i64,
}

/// LRR 68: produce units in the active system, up to capacity, paying for each.
///
/// # Errors
/// [`IllegalChoice`] when a decider answers with something not offered.
pub fn resolve(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut Table,
    player: &PlayerId,
    system: &SystemId,
) -> Result<ProductionReport, IllegalChoice> {
    let mut remaining = capacity(state, content, sources, player, system);
    let mut report = ProductionReport::default();
    if remaining <= 0 {
        return Ok(report);
    }

    let types = catalogue(content, sources);
    loop {
        if remaining <= 0 {
            break;
        }
        // One option per affordable, placeable unit type, plus stopping.
        let mut options = Vec::new();
        for id in buildable_for(state, player) {
            let Some(kind) = types.get(id.as_str()) else {
                continue;
            };
            let (cost, _) = price_of(kind);
            if cost > available(state, content, sources, player, Spend::Resources) {
                continue;
            }
            if placements(state, content, sources, player, system, kind).is_empty() {
                continue;
            }
            options.push(
                ChoiceOption::labelled(
                    format!("produce|{id}"),
                    PRODUCE_KIND,
                    format!("produce {id} for {cost}"),
                )
                .with("cost", cost),
            );
        }
        if options.is_empty() {
            break;
        }
        options.push(ChoiceOption::decline());

        let choice = Choice::new(
            player.clone(),
            format!("produce a unit ({remaining} capacity left)"),
            options,
        );
        let answer = table.ask(&choice)?;
        if answer.is_decline() {
            break;
        }
        let Some(id) = answer.id.strip_prefix("produce|").map(ToOwned::to_owned) else {
            break;
        };
        let Some(kind) = types.get(id.as_str()).copied() else {
            break;
        };

        // Paid before placed: a unit that could not be afforded must not reach the board even
        // for an instant, or an ability reacting to placement sees something never bought.
        let (cost, made) = price_of(&kind);
        if !pay(
            state,
            content,
            sources,
            table,
            player,
            cost,
            Spend::Resources,
        )? {
            break;
        }

        let spots = placements(state, content, sources, player, system, &kind);
        let where_to = if let [only] = spots.as_slice() {
            only.clone()
        } else {
            let options: Vec<ChoiceOption> = spots
                .iter()
                .map(|spot| {
                    ChoiceOption::labelled(
                        format!("place|{spot}"),
                        PLACE_KIND,
                        format!("place on {spot}"),
                    )
                })
                .collect();
            let choice = Choice::new(player.clone(), format!("place the {id}"), options);
            let answer = table.ask(&choice)?;
            answer.id.strip_prefix("place|").unwrap_or(SPACE).to_owned()
        };

        for _ in 0..made {
            let unit = Unit::new(UnitTypeId::new(id.clone()), player.clone());
            if where_to == SPACE {
                state.system_mut(system).units.push(unit);
            } else {
                state
                    .system_mut(system)
                    .planet_units
                    .entry(PlanetId::new(where_to.clone()))
                    .or_default()
                    .push(unit);
            }
            report
                .produced
                .push((UnitTypeId::new(id.clone()), where_to.clone()));
        }
        // 68.1a counts production *capacity*, and a two-for-one still uses one of it.
        remaining -= 1;
    }

    report.unused_capacity = remaining.max(0);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use ti4_model::content_types::POK;

    use super::*;
    use crate::fixtures::{a_placed_planet, game, put, put_on_planet};

    fn player() -> PlayerId {
        PlayerId::new("a")
    }

    fn seated() -> (GameState, SystemId, PlanetId) {
        let state = game(&["a", "b"]);
        let (system, planet) = a_placed_planet();
        (state, system, planet)
    }

    #[test]
    fn only_readied_controlled_planets_can_be_spent() {
        // 34, 75.2.
        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        assert_eq!(spendable_planets(&state, &player()), vec![planet.clone()]);

        state.exhaust_planet(planet);
        assert!(spendable_planets(&state, &player()).is_empty());
    }

    #[test]
    fn trade_goods_count_towards_what_can_be_afforded() {
        // 75.3, 47.3.
        let (mut state, _, _) = seated();
        state.player_mut(&player()).unwrap().trade_goods = 3;

        assert_eq!(
            available(
                &state,
                ContentStore::embedded(),
                POK,
                &player(),
                Spend::Resources
            ),
            3
        );
    }

    #[test]
    fn paying_exhausts_the_planet_it_used() {
        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        let worth = planet_value(ContentStore::embedded(), POK, &planet, Spend::Resources);
        assert!(worth > 0, "the fixture planet is worth something");
        let mut table = Table::new();

        let paid = pay(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &player(),
            worth,
            Spend::Resources,
        )
        .unwrap();

        assert!(paid);
        assert!(state.exhausted_planets.contains(&planet));
    }

    #[test]
    fn an_unaffordable_cost_spends_nothing() {
        let (mut state, _, _) = seated();
        let before = state.clone();
        let mut table = Table::new();

        let paid = pay(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &player(),
            99,
            Spend::Resources,
        )
        .unwrap();

        assert!(!paid);
        assert!(state.identical(&before), "nothing was exhausted or spent");
    }

    #[test]
    fn a_planet_pays_for_one_thing_or_the_other_never_both() {
        // 34.3: exhausting for influence leaves nothing to give for resources.
        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        let mut table = Table::new();

        let influence = planet_value(ContentStore::embedded(), POK, &planet, Spend::Influence);
        if influence > 0 {
            pay(
                &mut state,
                ContentStore::embedded(),
                POK,
                &mut table,
                &player(),
                influence,
                Spend::Influence,
            )
            .unwrap();
            assert_eq!(
                available(
                    &state,
                    ContentStore::embedded(),
                    POK,
                    &player(),
                    Spend::Resources
                ),
                0,
                "the card is exhausted, so it gives nothing further"
            );
        }
    }

    #[test]
    fn a_space_dock_produces_and_a_cruiser_does_not() {
        let (mut state, system, planet) = seated();
        put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);
        put(&mut state, &system, "cruiser", &player(), 2);

        let made = producers(&state, ContentStore::embedded(), POK, &player(), &system);
        assert_eq!(made.len(), 1);
        assert_eq!(made[0].1, Some(planet));
    }

    #[test]
    fn a_docks_capacity_follows_the_planet_it_sits_on() {
        // 68.1a: a space dock's production value is read from its planet's resources, which is
        // why the planet travels with the unit rather than the value coming from the unit.
        let (mut state, system, planet) = seated();
        put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);

        let resources = planet_value(ContentStore::embedded(), POK, &planet, Spend::Resources);
        let got = capacity(&state, ContentStore::embedded(), POK, &player(), &system);

        assert!(got > 0);
        assert!(
            got >= resources,
            "a dock is worth its planet's resources plus two"
        );
    }

    #[test]
    fn ships_go_to_space_and_structures_to_a_planet() {
        // 68.2 and 68.3.
        let (mut state, system, planet) = seated();
        put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);
        let types = catalogue(ContentStore::embedded(), POK);

        let ship = types.get("cruiser").unwrap();
        assert_eq!(
            placements(
                &state,
                ContentStore::embedded(),
                POK,
                &player(),
                &system,
                ship
            ),
            vec![SPACE.to_owned()]
        );

        let structure = types.get("pds").unwrap();
        assert!(
            placements(
                &state,
                ContentStore::embedded(),
                POK,
                &player(),
                &system,
                structure
            )
            .contains(&planet.to_string())
        );
    }

    #[test]
    fn one_planet_takes_only_one_space_dock() {
        // 79.2.
        let (mut state, system, planet) = seated();
        assert!(structure_allowed(
            &state,
            ContentStore::embedded(),
            POK,
            &player(),
            &planet,
            "spacedock"
        ));

        put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);
        assert!(
            !structure_allowed(
                &state,
                ContentStore::embedded(),
                POK,
                &player(),
                &planet,
                "spacedock"
            ),
            "a second dock has nowhere to go"
        );
    }

    #[test]
    fn a_fighter_costs_one_and_arrives_in_pairs() {
        // 68.2: printed cost below one means two units for one resource. Charging ceil and
        // yielding one would make the two commonest units cost double.
        let types = catalogue(ContentStore::embedded(), POK);
        let fighter = types.get("fighter").unwrap();
        assert!(fighter.cost() < 1.0, "the corpus prices it below one");
        assert_eq!(price_of(fighter), (1, 2));

        let cruiser = types.get("cruiser").unwrap();
        assert_eq!(price_of(cruiser).1, 1, "a full-cost unit comes singly");
    }

    #[test]
    fn a_war_sun_needs_its_technology() {
        // 67.x.
        let (mut state, _, _) = seated();
        assert!(!buildable_for(&state, &player()).contains(&"warsun".to_owned()));

        state
            .player_mut(&player())
            .unwrap()
            .technologies
            .insert(ti4_model::id::TechnologyId::new("ws"));
        assert!(buildable_for(&state, &player()).contains(&"warsun".to_owned()));
    }

    #[test]
    fn a_system_with_no_producer_produces_nothing() {
        let (mut state, system, _) = seated();
        put(&mut state, &system, "cruiser", &player(), 3);
        let mut table = Table::new();

        let report = resolve(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &player(),
            &system,
        )
        .unwrap();

        assert!(report.produced.is_empty());
    }

    #[test]
    fn production_places_units_and_charges_for_them() {
        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);
        state.player_mut(&player()).unwrap().trade_goods = 10;
        let before_goods = state.player(&player()).unwrap().trade_goods;
        let mut table = Table::new();

        let report = resolve(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &player(),
            &system,
        )
        .unwrap();

        assert!(!report.produced.is_empty(), "the dock built something");
        let spent = before_goods > state.player(&player()).unwrap().trade_goods
            || !state.exhausted_planets.is_empty();
        assert!(spent, "and it was paid for");
    }
}
