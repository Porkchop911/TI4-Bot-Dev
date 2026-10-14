//! Faction technologies and faction unit abilities whose effect happens at a fixed point of the turn.
//!
//! Each is called from the one place in the engine where its printed timing happens, the same way
//! Minister of Peace and the Dominus Orb are, rather than through a registry: a faction technology is
//! owned by one seat and read by one call site.

use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlayerId, SystemId, TechnologyId};
use ti4_model::state::{GameState, TokenPool};

use crate::choice::{Choice, ChoiceOption, Observed, Table};
use crate::decision_context::{DecisionContext, DecisionSource};

/// Whether `player` holds `alias` and it is not exhausted.
fn ready(state: &GameState, player: &PlayerId, alias: &str) -> bool {
    crate::technology::technology_text_ready(state, player, alias)
}

/// The other seats that have at least one ship in `system`, in seating order.
fn others_with_ships(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
    active: &PlayerId,
) -> Vec<PlayerId> {
    state
        .players
        .iter()
        .map(|seat| seat.id.clone())
        .filter(|seat| seat != active)
        .filter(|seat| !crate::combat::ships_of(state, content, sources, seat, system).is_empty())
        .collect()
}

/// E-Res Siphons (Jol-Nar): "After another player activates a system that contains 1 or more of
/// your ships, gain 4 trade goods." Not optional and not exhausted. Returns who gained.
pub fn e_res_siphons(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
    active: &PlayerId,
) -> Vec<PlayerId> {
    let gained: Vec<PlayerId> = others_with_ships(state, content, sources, system, active)
        .into_iter()
        .filter(|seat| {
            state
                .player(seat)
                .is_some_and(|_| crate::technology::has_technology_text(state, seat, "ers"))
        })
        .collect();
    for seat in &gained {
        if let Some(holder) = state.player_mut(seat) {
            holder.trade_goods += 4;
        }
        crate::supply::note_trade_goods_gained(state, seat, 4, "faction_technology");
    }
    gained
}

/// Nullification Field (Xxcha): "After another player activates a system that contains 1 or more
/// of your ships, you may exhaust this card and spend 1 token from your strategy pool; immediately
/// end that player's turn." Asks each eligible holder in seating order; returns the one who used it.
pub fn offer_nullification_field(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut Table,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    system: &SystemId,
    active: &PlayerId,
) -> Option<PlayerId> {
    let holders: Vec<PlayerId> = others_with_ships(state, content, sources, system, active)
        .into_iter()
        .filter(|seat| ready(state, seat, "nf"))
        .filter(|seat| {
            state
                .player(seat)
                .is_some_and(|holder| holder.strategic_tokens > 0)
        })
        .collect();
    for holder in holders {
        let choice = Choice::new(
            holder.clone(),
            format!(
                "Nullification Field: exhaust and spend a strategy token to end {active}'s turn"
            ),
            vec![
                ChoiceOption::labelled("use".to_owned(), "technology", "end their turn".to_owned()),
                ChoiceOption::decline(),
            ],
        )
        .contextualized(DecisionContext::new(
            holder.clone(),
            DecisionSource::Content("nf".to_owned()),
            "nullification_field_end_turn",
            state.phase,
            state.round,
        ));
        let Ok(answer) = table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))
        else {
            continue;
        };
        if answer.is_decline() {
            continue;
        }
        let Some(seat) = state.player_mut(&holder) else {
            continue;
        };
        if !seat.spend_token(TokenPool::Strategic) {
            continue;
        }
        crate::technology::exhaust_technology_text(state, &holder, "nf");
        crate::supply::note_strategy_token_spent(state, &holder, "nullification_field");
        return Some(holder);
    }
    None
}

/// Genesis, the Sol flagship: "At the end of the status phase, place 1 infantry from your
/// reinforcements in this system's space area." One infantry per flagship, of the type the owner
/// currently builds (Spec Ops once upgraded), and only while the box has one left. Returns the
/// systems that got one.
pub fn genesis(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
) -> Vec<(PlayerId, SystemId)> {
    let flagships: Vec<(PlayerId, SystemId)> = state
        .board
        .iter()
        .flat_map(|(system, here)| {
            here.units
                .iter()
                .filter(|unit| {
                    crate::factions::flagship_has_text(
                        state,
                        &unit.owner,
                        unit.type_id.as_str(),
                        "sol_flagship",
                    )
                })
                .map(|unit| (unit.owner.clone(), system.clone()))
                .collect::<Vec<_>>()
        })
        .collect();
    let mut placed = Vec::new();
    for (owner, system) in flagships {
        let Some(infantry) = crate::production::buildable_for(state, content, sources, &owner)
            .into_iter()
            .find(|kind| {
                ti4_content::units::catalogue(content, sources)
                    .get(kind.as_str())
                    .is_some_and(|record| record.base_type() == "infantry")
            })
        else {
            continue;
        };
        let infantry = ti4_model::id::UnitTypeId::new(infantry);
        if crate::supply::remaining(state, content, sources, &owner, &infantry) < 1 {
            continue;
        }
        state
            .system_mut(&system)
            .units
            .push(ti4_model::units::Unit::new(infantry, owner.clone()));
        placed.push((owner, system));
    }
    placed
}

/// Spec Ops II (Sol): "After this unit is destroyed, roll 1 die. If the result is 5 or greater,
/// place the unit on this card." Called wherever a ground force is destroyed; the die is rolled
/// by [`roll_spec_ops`] on the engine's next step, where the game's dice are in hand.
pub fn note_destroyed(state: &mut GameState, unit: &ti4_model::units::Unit) {
    if unit.type_id.as_str() != "sol_infantry2" {
        return;
    }
    if let Some(seat) = state.player_mut(&unit.owner) {
        seat.spec_ops_destroyed += 1;
    }
}

/// What a Spec Ops II survival roll needs.
pub const SPEC_OPS_SURVIVES_ON: u32 = 5;

/// Roll for every Spec Ops II destroyed since the last step. Returns (owner, survived) per die.
pub fn roll_spec_ops(
    state: &mut GameState,
    dice: &mut crate::dice::Dice,
    rng: &mut crate::rng::GameRng,
) -> Vec<(PlayerId, bool)> {
    let mut rolled = Vec::new();
    let pending: Vec<(PlayerId, u32)> = state
        .players
        .iter()
        .filter(|seat| seat.spec_ops_destroyed > 0)
        .map(|seat| (seat.id.clone(), seat.spec_ops_destroyed))
        .collect();
    for (owner, count) in pending {
        let roll = dice.roll_by(
            rng,
            usize::try_from(count).unwrap_or(0),
            "spec ops II",
            Some(SPEC_OPS_SURVIVES_ON),
            &owner,
        );
        let survived = u32::try_from(roll.hits()).unwrap_or(0);
        if let Some(seat) = state.player_mut(&owner) {
            seat.spec_ops_destroyed = 0;
            seat.spec_ops_card += survived;
        }
        for face in &roll.faces {
            rolled.push((owner.clone(), *face >= SPEC_OPS_SURVIVES_ON));
        }
    }
    rolled
}

/// "At the start of your next turn, place each unit that is on this card on a planet you control
/// in your home system." On the first such planet; with none controlled they wait on the card.
/// Returns how many were placed.
pub fn return_spec_ops(state: &mut GameState, player: &PlayerId) -> u32 {
    let Some(seat) = state.player(player) else {
        return 0;
    };
    let waiting = seat.spec_ops_card;
    if waiting == 0 {
        return 0;
    }
    let Some(home) = seat.home_system.clone() else {
        return 0;
    };
    let Some(planet) = state
        .controlled_planets(player)
        .into_iter()
        .find(|(system, _)| **system == home)
        .map(|(_, planet)| planet.clone())
    else {
        return 0;
    };
    for _ in 0..waiting {
        state
            .system_mut(&home)
            .planet_units
            .entry(planet.clone())
            .or_default()
            .push(ti4_model::units::Unit::new(
                ti4_model::id::UnitTypeId::new("sol_infantry2"),
                player.clone(),
            ));
    }
    if let Some(seat) = state.player_mut(player) {
        seat.spec_ops_card = 0;
    }
    waiting
}

/// Quantum Datahub Node (Hacan): "At the end of the strategy phase, you may spend 1 token from
/// your strategy pool and give another player 3 of your trade goods. If you do, give 1 of your
/// strategy cards to that player and take 1 of their strategy cards."
///
/// One question: every (partner, card given, card taken) the holder could name, or decline.
/// Returns the partner when it was used.
pub fn offer_quantum_datahub(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut Table,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
) -> Option<(PlayerId, PlayerId)> {
    let holder = state
        .players
        .iter()
        .find(|seat| {
            crate::technology::has_technology_text(state, &seat.id, "qdn")
                && seat.strategic_tokens > 0
                && seat.trade_goods >= QUANTUM_DATAHUB_GOODS
                && !seat.strategy_cards.is_empty()
        })?
        .id
        .clone();
    let mine = state.player(&holder)?.strategy_cards.clone();
    let mut options = Vec::new();
    for seat in state.players.iter().filter(|seat| seat.id != holder) {
        for given in &mine {
            for taken in &seat.strategy_cards {
                options.push(
                    ChoiceOption::labelled(
                        format!("qdn|{}|{given}|{taken}", seat.id),
                        "strategy_card",
                        format!("give {} 3 trade goods and {given}, take {taken}", seat.id),
                    )
                    .with("partner", seat.id.to_string())
                    .with("give", given.to_string())
                    .with("take", taken.to_string()),
                );
            }
        }
    }
    if options.is_empty() {
        return None;
    }
    options.push(ChoiceOption::decline());
    let choice = Choice::new(
        holder.clone(),
        "Quantum Datahub Node: spend a strategy token and 3 trade goods to swap a strategy card",
        options,
    )
    .contextualized(DecisionContext::new(
        holder.clone(),
        DecisionSource::Content("qdn".to_owned()),
        "quantum_datahub_swap",
        state.phase,
        state.round,
    ));
    let answer = table
        .ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))
        .ok()?;
    if answer.is_decline() {
        return None;
    }
    let mut parts = answer.id.split('|').skip(1);
    let (partner, given, taken) = (parts.next()?, parts.next()?, parts.next()?);
    let partner = PlayerId::new(partner);
    let given = ti4_model::id::StrategyCardId::new(given);
    let taken = ti4_model::id::StrategyCardId::new(taken);
    let seat = state.player_mut(&holder)?;
    if !seat.spend_token(TokenPool::Strategic) {
        return None;
    }
    seat.trade_goods -= QUANTUM_DATAHUB_GOODS;
    state.player_mut(&partner)?.trade_goods += QUANTUM_DATAHUB_GOODS;
    crate::supply::note_strategy_token_spent(state, &holder, "quantum_datahub");
    crate::supply::note_trade_goods_gained(
        state,
        &partner,
        QUANTUM_DATAHUB_GOODS,
        "quantum_datahub",
    );
    state.swap_strategy_card(&holder, &given, taken.clone());
    state.swap_strategy_card(&partner, &taken, given);
    Some((holder, partner))
}

/// What Quantum Datahub Node hands the other player.
pub const QUANTUM_DATAHUB_GOODS: i32 = 3;

/// Spatial Conduit Cylinders (Jol-Nar): "You may exhaust this card after you activate a system
/// that contains 1 or more of your units; that system is adjacent to all other systems that
/// contain 1 or more of your units during this activation."
///
/// Offered to the active player right after activation. Returns whether it was used; the
/// adjacency itself is [`spatial_conduit_links`], read into the galaxy every step.
pub fn offer_spatial_conduit(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut Table,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    system: &SystemId,
    active: &PlayerId,
) -> bool {
    if !ready(state, active, "scc") || !state.system_state(system).has_units_of(active) {
        return false;
    }
    // 22.3: nothing to connect unless the player has units somewhere else too.
    if !state
        .board
        .iter()
        .any(|(other, here)| other != system && here.has_units_of(active))
    {
        return false;
    }
    let choice = Choice::new(
        active.clone(),
        format!(
            "Spatial Conduit Cylinders: make {system} adjacent to every system with your units"
        ),
        vec![
            ChoiceOption::labelled("use".to_owned(), "technology", "exhaust it".to_owned()),
            ChoiceOption::decline(),
        ],
    )
    .contextualized(DecisionContext::new(
        active.clone(),
        DecisionSource::Content("scc".to_owned()),
        "spatial_conduit_link",
        state.phase,
        state.round,
    ));
    let Ok(answer) = table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))
    else {
        return false;
    };
    if answer.is_decline() {
        return false;
    }
    let activation = state.activation_seq;
    crate::technology::exhaust_technology_text(state, active, "scc");
    let Some(seat) = state.player_mut(active) else {
        return false;
    };
    seat.spatial_conduit = Some(activation);
    true
}

/// The adjacency Spatial Conduit Cylinders adds for the activation in flight, both directions.
#[must_use]
pub fn spatial_conduit_links(
    state: &GameState,
) -> std::collections::BTreeMap<String, std::collections::BTreeSet<String>> {
    let mut links: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    let (Some(active), Some(system)) = (state.active.as_ref(), state.active_system.as_ref()) else {
        return links;
    };
    if state
        .player(active)
        .is_none_or(|seat| seat.spatial_conduit != Some(state.activation_seq))
    {
        return links;
    }
    for (other, here) in &state.board {
        if other == system || !here.has_units_of(active) {
            continue;
        }
        links
            .entry(system.to_string())
            .or_default()
            .insert(other.to_string());
        links
            .entry(other.to_string())
            .or_default()
            .insert(system.to_string());
    }
    links
}

/// Dacxive Animators: "After you win a ground combat, you may place 1 infantry from your
/// reinforcements on that planet." Always taken when the box has one: an extra infantry on a
/// planet just won is never worse than one in reinforcements. Returns whether it was placed.
pub fn dacxive_animators(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    winner: &PlayerId,
    system: &SystemId,
    planet: &ti4_model::id::PlanetId,
) -> bool {
    if !state
        .player(winner)
        .is_some_and(|seat| seat.technologies.contains(&TechnologyId::new("dxa")))
    {
        return false;
    }
    let Some(infantry) = crate::production::buildable_for(state, content, sources, winner)
        .into_iter()
        .find(|kind| {
            ti4_content::units::catalogue(content, sources)
                .get(kind.as_str())
                .is_some_and(|record| record.base_type() == "infantry")
        })
    else {
        return false;
    };
    let infantry = ti4_model::id::UnitTypeId::new(infantry);
    if crate::supply::remaining(state, content, sources, winner, &infantry) < 1 {
        return false;
    }
    state
        .system_mut(system)
        .planet_units
        .entry(planet.clone())
        .or_default()
        .push(ti4_model::units::Unit::new(infantry, winner.clone()));
    true
}

/// Scanlink Drone Network: "When you activate a system, you may explore 1 planet in that system
/// which contains 1 or more of your units." Asks which planet, or none; returns the choice. The
/// exploration itself runs in the game step, where its dice and table are.
pub fn offer_scanlink(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut Table,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    system: &SystemId,
    active: &PlayerId,
) -> Option<ti4_model::id::PlanetId> {
    if !state
        .player(active)
        .is_some_and(|seat| seat.technologies.contains(&TechnologyId::new("sdn")))
    {
        return None;
    }
    let planets: Vec<ti4_model::id::PlanetId> =
        crate::planets::in_system(state, content, sources, system)
            .into_iter()
            .filter(|planet| {
                !state
                    .system_state(system)
                    .on_planet_of(planet, active)
                    .is_empty()
            })
            .filter(|planet| {
                !crate::planets::traits_now(state, content, sources, planet).is_empty()
            })
            .collect();
    if planets.is_empty() {
        return None;
    }
    let mut options: Vec<ChoiceOption> = planets
        .iter()
        .map(|planet| {
            ChoiceOption::labelled(planet.to_string(), "planet", format!("explore {planet}"))
                .with_planet(planet.as_str(), Some(system.as_str()))
        })
        .collect();
    options.push(ChoiceOption::decline());
    let choice = Choice::new(
        active.clone(),
        "Scanlink Drone Network: explore a planet here with your units on it",
        options,
    )
    .contextualized(DecisionContext::new(
        active.clone(),
        DecisionSource::Content("sdn".to_owned()),
        "scanlink_explore",
        state.phase,
        state.round,
    ));
    let answer = table
        .ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))
        .ok()?;
    planets
        .into_iter()
        .find(|planet| planet.as_str() == answer.id)
}

#[cfg(test)]
mod tests {
    use ti4_model::content_types::POK;

    use super::*;
    use crate::fixtures::{game, put};

    fn setup(tech: &str) -> (GameState, SystemId, PlayerId, PlayerId) {
        let mut state = game(&["a", "b"]);
        let system = SystemId::new("19");
        let (active, holder) = (PlayerId::new("a"), PlayerId::new("b"));
        put(&mut state, &system, "cruiser", &holder, 1);
        let seat = state.player_mut(&holder).unwrap();
        seat.technologies.insert(TechnologyId::new(tech));
        seat.strategic_tokens = 2;
        (state, system, active, holder)
    }

    /// Scanlink's planet options carry `planet` + `system`; its decline does not.
    #[test]
    fn scanlink_planet_options_carry_planet_and_system_payloads() {
        use crate::choice::planet_payload::{assert_locates, assert_not_a_planet, offered};
        let mut state = game(&["a", "b"]);
        let a = PlayerId::new("a");
        let system = SystemId::new("28");
        state
            .player_mut(&a)
            .unwrap()
            .technologies
            .insert(TechnologyId::new("sdn"));
        for planet in ["tequran", "torkan"] {
            crate::fixtures::put_on_planet(
                &mut state,
                &system,
                &ti4_model::id::PlanetId::new(planet),
                "infantry",
                &a,
                1,
            );
        }
        let (decider, seen) = crate::choice::Capturing::new(Box::new(crate::choice::AlwaysDecline));
        let mut table = Table::with_default(Box::new(decider));
        offer_scanlink(
            &state,
            ContentStore::embedded(),
            POK,
            &mut table,
            None,
            &system,
            &a,
        );
        let choice = &seen.borrow()[0];
        assert_eq!(choice.context.as_ref().unwrap().subtype, "scanlink_explore");
        assert_locates(offered(choice, "tequran"), "tequran", "28");
        assert_locates(offered(choice, "torkan"), "torkan", "28");
        assert_not_a_planet(offered(choice, crate::choice::DECLINE_ID));
    }

    #[test]
    fn e_res_siphons_pays_four_when_another_player_activates_your_ships_system() {
        let (mut state, system, active, holder) = setup("ers");
        let before = state.player(&holder).unwrap().trade_goods;
        let gained = e_res_siphons(&mut state, ContentStore::embedded(), POK, &system, &active);
        assert_eq!(gained, vec![holder.clone()]);
        assert_eq!(state.player(&holder).unwrap().trade_goods, before + 4);

        // Its own activation pays nothing, and neither does a system without its ships.
        let mut again = state.clone();
        assert!(
            e_res_siphons(&mut again, ContentStore::embedded(), POK, &system, &holder).is_empty()
        );
        assert!(
            e_res_siphons(
                &mut again,
                ContentStore::embedded(),
                POK,
                &SystemId::new("20"),
                &active
            )
            .is_empty()
        );
    }

    #[test]
    fn nullification_field_costs_a_token_and_the_exhausted_card() {
        let (mut state, system, active, holder) = setup("nf");
        let mut table = Table::with_default(Box::new(crate::choice::FirstOption));
        let used = offer_nullification_field(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            None,
            &system,
            &active,
        );
        assert_eq!(used, Some(holder.clone()));
        let seat = state.player(&holder).unwrap();
        assert_eq!(seat.strategic_tokens, 1);
        assert!(
            seat.exhausted_technologies
                .contains(&TechnologyId::new("nf"))
        );

        // Exhausted, it is not offered again.
        let mut table = Table::with_default(Box::new(crate::choice::FirstOption));
        assert_eq!(
            offer_nullification_field(
                &mut state,
                ContentStore::embedded(),
                POK,
                &mut table,
                None,
                &system,
                &active,
            ),
            None
        );
    }

    #[test]
    fn genesis_places_one_infantry_beside_each_sol_flagship() {
        let mut state = game(&["a"]);
        let sol = PlayerId::new("a");
        state.player_mut(&sol).unwrap().faction = ti4_model::id::FactionId::new("sol");
        let system = SystemId::new("19");
        put(&mut state, &system, "sol_flagship", &sol, 1);
        let placed = genesis(&mut state, ContentStore::embedded(), POK);
        assert_eq!(placed, vec![(sol.clone(), system.clone())]);
        let infantry: Vec<String> = state
            .system_state(&system)
            .units
            .iter()
            .filter(|unit| unit.type_id.as_str() != "sol_flagship")
            .map(|unit| unit.type_id.to_string())
            .collect();
        assert_eq!(
            infantry,
            vec!["sol_infantry".to_owned()],
            "Sol builds Spec Ops"
        );
    }

    #[test]
    fn spec_ops_two_survives_on_five_and_comes_home_next_turn() {
        let mut state = game(&["a"]);
        let sol = PlayerId::new("a");
        let unit = ti4_model::units::Unit::new(
            ti4_model::id::UnitTypeId::new("sol_infantry2"),
            sol.clone(),
        );
        note_destroyed(&mut state, &unit);
        note_destroyed(&mut state, &unit);
        // A plain infantry is not Spec Ops II.
        note_destroyed(
            &mut state,
            &ti4_model::units::Unit::new(ti4_model::id::UnitTypeId::new("infantry"), sol.clone()),
        );
        let mut dice = crate::dice::Dice::from_faces([5, 4]);
        let mut rng = crate::rng::GameRng::new(1);
        let rolled = roll_spec_ops(&mut state, &mut dice, &mut rng);
        assert_eq!(rolled, vec![(sol.clone(), true), (sol.clone(), false)]);
        let seat = state.player(&sol).unwrap();
        assert_eq!((seat.spec_ops_destroyed, seat.spec_ops_card), (0, 1));

        let (system, planet) = crate::fixtures::a_placed_planet();
        state.player_mut(&sol).unwrap().home_system = Some(system.clone());
        state
            .system_mut(&system)
            .set_control(planet.clone(), sol.clone());
        assert_eq!(return_spec_ops(&mut state, &sol), 1);
        assert_eq!(
            state
                .system_state(&system)
                .on_planet_of(&planet, &sol)
                .len(),
            1
        );
        assert_eq!(state.player(&sol).unwrap().spec_ops_card, 0);
    }

    #[test]
    fn quantum_datahub_trades_three_goods_and_a_token_for_a_card_swap() {
        let mut state = game(&["a", "b"]);
        let (hacan, other) = (PlayerId::new("a"), PlayerId::new("b"));
        let (low, high) = (
            ti4_model::id::StrategyCardId::new("leadership"),
            ti4_model::id::StrategyCardId::new("imperial"),
        );
        let seat = state.player_mut(&hacan).unwrap();
        seat.technologies.insert(TechnologyId::new("qdn"));
        seat.trade_goods = 5;
        seat.strategic_tokens = 1;
        seat.strategy_cards = vec![high.clone()];
        let seat = state.player_mut(&other).unwrap();
        seat.trade_goods = 0;
        seat.strategy_cards = vec![low.clone()];
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([format!(
            "qdn|b|{high}|{low}"
        )])));
        let used =
            offer_quantum_datahub(&mut state, ContentStore::embedded(), POK, &mut table, None);
        assert_eq!(used, Some((hacan.clone(), other.clone())));
        let (a, b) = (state.player(&hacan).unwrap(), state.player(&other).unwrap());
        assert_eq!(
            (a.trade_goods, a.strategic_tokens, b.trade_goods),
            (2, 0, 3)
        );
        assert_eq!(a.strategy_cards, vec![low]);
        assert_eq!(b.strategy_cards, vec![high]);
    }

    #[test]
    fn spatial_conduit_links_the_active_system_to_every_system_with_your_units() {
        let mut state = game(&["a"]);
        let jolnar = PlayerId::new("a");
        let (here, far, empty) = (
            SystemId::new("19"),
            SystemId::new("40"),
            SystemId::new("41"),
        );
        put(&mut state, &here, "cruiser", &jolnar, 1);
        put(&mut state, &far, "carrier", &jolnar, 1);
        state.board.entry(empty.clone()).or_default();
        state
            .player_mut(&jolnar)
            .unwrap()
            .technologies
            .insert(TechnologyId::new("scc"));
        state.active = Some(jolnar.clone());
        state.active_system = Some(here.clone());
        state.activation_seq = 7;
        assert!(
            spatial_conduit_links(&state).is_empty(),
            "nothing until it is used"
        );

        let mut table = Table::with_default(Box::new(crate::choice::FirstOption));
        assert!(offer_spatial_conduit(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            None,
            &here,
            &jolnar,
        ));
        let links = spatial_conduit_links(&state);
        assert!(links[here.as_str()].contains(far.as_str()));
        assert!(links[far.as_str()].contains(here.as_str()));
        assert!(
            !links.contains_key(empty.as_str()),
            "only systems with the player's units"
        );

        // The next activation is ordinary again.
        state.activation_seq = 8;
        assert!(spatial_conduit_links(&state).is_empty());
    }

    #[test]
    fn dacxive_animators_adds_an_infantry_to_the_planet_won() {
        let mut state = game(&["a"]);
        let winner = PlayerId::new("a");
        let (system, planet) = crate::fixtures::a_placed_planet();
        assert!(!dacxive_animators(
            &mut state,
            ContentStore::embedded(),
            POK,
            &winner,
            &system,
            &planet
        ));
        state
            .player_mut(&winner)
            .unwrap()
            .technologies
            .insert(TechnologyId::new("dxa"));
        assert!(dacxive_animators(
            &mut state,
            ContentStore::embedded(),
            POK,
            &winner,
            &system,
            &planet
        ));
        assert_eq!(
            state
                .system_state(&system)
                .on_planet_of(&planet, &winner)
                .len(),
            1
        );
    }

    #[test]
    fn hyper_metabolism_gains_one_more_status_token() {
        let mut state = game(&["a"]);
        let player = PlayerId::new("a");
        let without =
            crate::faction_abilities::status_tokens(&state, ContentStore::embedded(), &player, 2);
        state
            .player_mut(&player)
            .unwrap()
            .technologies
            .insert(TechnologyId::new("hm"));
        assert_eq!(
            crate::faction_abilities::status_tokens(&state, ContentStore::embedded(), &player, 2),
            without + 1
        );
    }

    #[test]
    fn scanlink_offers_only_planets_with_the_players_units() {
        let mut state = game(&["a"]);
        let player = PlayerId::new("a");
        let (system, planet) = crate::fixtures::a_placed_planet();
        let mut table = Table::with_default(Box::new(crate::choice::FirstOption));
        state
            .player_mut(&player)
            .unwrap()
            .technologies
            .insert(TechnologyId::new("sdn"));
        assert_eq!(
            offer_scanlink(
                &state,
                ContentStore::embedded(),
                POK,
                &mut table,
                None,
                &system,
                &player
            ),
            None,
            "no units on the planet, nothing to explore"
        );
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &player, 1);
        let chosen = offer_scanlink(
            &state,
            ContentStore::embedded(),
            POK,
            &mut table,
            None,
            &system,
            &player,
        );
        if crate::exploration::trait_of(ContentStore::embedded(), POK, &planet).is_some() {
            assert_eq!(chosen, Some(planet));
        } else {
            assert_eq!(chosen, None, "a planet with no trait cannot be explored");
        }
    }

    #[test]
    fn a_nekro_flagship_with_the_sol_z_token_places_an_infantry_in_its_system() {
        let system = SystemId::new("19");
        let run = |lent: &[&str]| {
            let mut state = crate::fixtures::nekro_with_z(&[("a", "nekro"), ("b", "sol")], lent);
            put(
                &mut state,
                &system,
                "nekro_flagship",
                &PlayerId::new("a"),
                1,
            );
            let placed = genesis(&mut state, ContentStore::embedded(), POK);
            (placed.len(), state.system_state(&system).units.len())
        };
        assert_eq!(run(&[]), (0, 1), "off by default");
        assert_eq!(run(&["sol"]), (1, 2));
    }
}
