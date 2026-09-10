//! Legendary planet abilities.
//!
//! Sixteen legendary planets exist, described by nineteen corpus records -- Mallice has a tile
//! face each side of the Nexus flip, and Mirage carries two alternate printings that nothing
//! places. Each has a `legendaryAbilityName` and its text, and until this module nothing read
//! either: `Planet::is_legendary` answered a bool used for excluding them from
//! Stellar Converter and counting them for objectives, and the printed ability did nothing at all.
//!
//! # Shape
//!
//! A registry keyed by planet, like [`crate::action_cards::effect_for`] and the relic table: a
//! planet with no entry is a visible gap rather than a silent one. Most of these are "you may
//! exhaust this card at the end of your turn", which is a window the engine already drives --
//! `technology::end_turn` runs it for Bio-Stims and Predictive Intelligence -- so the abilities
//! hang off that rather than needing timing machinery of their own.
//!
//! The ability card exhausts separately from the planet (`seat.exhausted_legendary`, readied in
//! the status phase beside technologies and relics), because a seat can spend Primor for its two
//! resources and still use The Atrament in the same round.
//!
//! # Not here
//!
//! Avernus, Ordinian and Custodia Vigilia are faction-specific -- they borrow another faction's
//! ability, sit in a faction's starting position, or arrive with a faction technology -- and are
//! deliberately left out rather than half-modelled.

use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlanetId, PlayerId};
use ti4_model::state::GameState;

use crate::choice::{Choice, ChoiceOption, IllegalChoice, Observed, Table};
use crate::decision_context::{DecisionContext, DecisionSource};

/// Whether this seat holds the planet and has not spent its ability card this round.
#[must_use]
pub fn available(state: &GameState, player: &PlayerId, planet: &PlanetId) -> bool {
    let holds = state
        .controlled_planets(player)
        .into_iter()
        .any(|(_, held)| held == planet);
    holds
        && state
            .player(player)
            .is_some_and(|seat| !seat.exhausted_legendary.contains(planet))
}

/// Spend the ability card.
fn exhaust(state: &mut GameState, player: &PlayerId, planet: &PlanetId) {
    if let Some(seat) = state.player_mut(player) {
        seat.exhausted_legendary.insert(planet.clone());
    }
}

/// Every legendary ability this seat could use at the end of its turn, as options.
///
/// One option per planet rather than one per outcome: the choice of *which* ability is separate
/// from the choice each ability then offers, and folding them together would make declining one
/// ability read as declining all of them.
fn end_of_turn_offers(state: &GameState, player: &PlayerId) -> Vec<PlanetId> {
    END_OF_TURN
        .iter()
        .map(|(planet, _)| PlanetId::new(*planet))
        .filter(|planet| available(state, player, planet))
        .collect()
}

/// Planets whose ability is used at the end of the holder's turn, with the label to offer.
///
/// Six planets, seven rows: Mallice appears twice because the Nexus has two faces and the planet
/// on the locked tile is `lockedmallice`, so whichever face is up is the id in play.
///
/// The corpus also carries `illusion` and `phantasm` -- alternate printings of Mirage, same stats
/// and same ability with the name changed. They are not here because nothing places them: the
/// Mirage frontier card places `mirage` and only `mirage` (`exploration.rs`). Sixteen legendary
/// planets exist; nineteen records describe them.
const END_OF_TURN: [(&str, &str); 7] = [
    ("hopesend", "Imperial Arms Vault"),
    ("primor", "The Atrament"),
    ("mallice", "Exterrix Headquarters"),
    ("lockedmallice", "Exterrix Headquarters"),
    ("mirage", "Mirage Flight Academy"),
    ("emelpar", "The Acropolis"),
    ("mrte", "The Galactic Council"),
];

/// Offer this seat's end-of-turn legendary abilities, one at a time.
///
/// # Errors
/// [`IllegalChoice`] if a decider answers with something that was not offered.
pub fn end_turn(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    table: &mut Table,
    player: &PlayerId,
) -> Result<(), IllegalChoice> {
    loop {
        let offers = end_of_turn_offers(state, player);
        if offers.is_empty() {
            return Ok(());
        }
        let mut options: Vec<ChoiceOption> = offers
            .iter()
            .map(|planet| {
                let label = END_OF_TURN
                    .iter()
                    .find(|(id, _)| *id == planet.as_str())
                    .map_or("legendary ability", |(_, label)| *label);
                ChoiceOption::labelled(planet.to_string(), "legendary", label)
            })
            .collect();
        options.push(ChoiceOption::decline());
        let choice = Choice::new(
            player.clone(),
            "use a legendary planet ability",
            options,
        )
        .contextualized(DecisionContext::new(
            player.clone(),
            DecisionSource::Content("legendary".to_owned()),
            "legendary_end_of_turn",
            state.phase,
            state.round,
        ));
        let answer = table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
        if answer.is_decline() {
            return Ok(());
        }
        let planet = PlanetId::new(answer.id);
        // Exhausted first: an ability that asks a follow-up question must not be re-offered
        // inside its own resolution.
        exhaust(state, player, &planet);
        resolve(state, content, sources, galaxy, table, player, &planet)?;
    }
}


/// This seat's own version of a unit type, falling back to the generic one.
fn unit_of(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    base: &str,
) -> Option<ti4_model::id::UnitTypeId> {
    let faction = state
        .player(player)
        .map(|seat| seat.faction.to_string())
        .unwrap_or_default();
    let generic = ti4_content::units::catalogue(content, sources)
        .get(base)
        .map(|unit| unit.id().to_owned());
    ti4_content::units::faction_unit(content, &faction, base, sources)
        .map(|unit| unit.id().to_owned())
        .or(generic)
        .map(ti4_model::id::UnitTypeId::new)
}

/// Ask which of this seat's planets to place on, then put `count` of `base` there.
///
/// Reinforcements are finite, so the number placed is whatever `supply` allows rather than what
/// the card asks for. Both cards that use this say "up to", which is the same thing said aloud.
fn place_on_own_planet(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    base: &str,
    count: usize,
    prompt: &str,
) -> Result<(), IllegalChoice> {
    let Some(type_id) = unit_of(state, content, sources, player, base) else {
        return Ok(());
    };
    let placeable = crate::supply::allowed(state, content, sources, player, &type_id, count);
    let options: Vec<ChoiceOption> = state
        .controlled_planets(player)
        .into_iter()
        .map(|(system, planet)| {
            ChoiceOption::labelled(
                format!("{system}|{planet}"),
                "legendary",
                format!("place on {planet}"),
            )
        })
        .collect();
    if placeable == 0 || options.is_empty() {
        return Ok(());
    }
    let choice = Choice::new(player.clone(), prompt, options).contextualized(DecisionContext::new(
        player.clone(),
        DecisionSource::Content("legendary".to_owned()),
        "legendary_place",
        state.phase,
        state.round,
    ));
    let answer = table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
    let mut parts = answer.id.split('|');
    let (Some(system), Some(planet), None) = (parts.next(), parts.next(), parts.next()) else {
        return Ok(());
    };
    let held = state
        .system_mut(&ti4_model::id::SystemId::new(system))
        .planet_units
        .entry(PlanetId::new(planet))
        .or_default();
    for _ in 0..placeable {
        held.push(ti4_model::units::Unit::new(type_id.clone(), player.clone()));
    }
    Ok(())
}

/// Apply one ability. A planet with no arm here is an honest gap, not a silent one.
fn resolve(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    planet: &PlanetId,
) -> Result<(), IllegalChoice> {
    match planet.as_str() {
        // "gain 2 trade goods or convert all of your commodities into trade goods"
        "mallice" | "lockedmallice" => {
            let commodities = state.player(player).map_or(0, |seat| seat.commodities);
            let options = vec![
                ChoiceOption::labelled("goods", "legendary", "gain 2 trade goods"),
                ChoiceOption::labelled(
                    "convert",
                    "legendary",
                    format!("convert {commodities} commodities to trade goods"),
                ),
            ];
            let choice = Choice::new(player.clone(), "Exterrix Headquarters", options)
                .contextualized(DecisionContext::new(
                    player.clone(),
                    DecisionSource::Content("legendary".to_owned()),
                    "legendary_exterrix",
                    state.phase,
                    state.round,
                ));
            let answer =
                table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
            if let Some(seat) = state.player_mut(player) {
                if answer.id == "convert" {
                    seat.trade_goods += seat.commodities;
                    seat.commodities = 0;
                } else {
                    seat.trade_goods += 2;
                }
            }
        }
        // "place up to 2 infantry from your reinforcements on any planet you control"
        "primor" => place_on_own_planet(
            state,
            content,
            sources,
            galaxy,
            table,
            player,
            "infantry",
            2,
            "The Atrament: place infantry where",
        )?,
        // "place 1 mech from your reinforcements on any planet you control, or draw 1 action card"
        "hopesend" => {
            let options = vec![
                ChoiceOption::labelled("mech", "legendary", "place 1 mech"),
                ChoiceOption::labelled("card", "legendary", "draw 1 action card"),
            ];
            let choice = Choice::new(player.clone(), "Imperial Arms Vault", options)
                .contextualized(DecisionContext::new(
                    player.clone(),
                    DecisionSource::Content("legendary".to_owned()),
                    "legendary_arms_vault",
                    state.phase,
                    state.round,
                ));
            let answer =
                table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
            if answer.id == "card" {
                crate::action_cards::draw(state, content, table, player, 1)?;
            } else {
                place_on_own_planet(
                    state,
                    content,
                    sources,
                    galaxy,
                    table,
                    player,
                    "mech",
                    1,
                    "Imperial Arms Vault: place the mech where",
                )?;
            }
        }
        // "place up to 2 fighters in any system that contains 1 or more of your ships" -- a
        // system rather than a planet, and only one this seat is already in.
        "mirage" => {
            let Some(type_id) = unit_of(state, content, sources, player, "fighter") else {
                return Ok(());
            };
            let placeable = crate::supply::allowed(state, content, sources, player, &type_id, 2);
            let types = ti4_content::units::catalogue(content, sources);
            let options: Vec<ChoiceOption> = state
                .board
                .iter()
                .filter(|(_, here)| {
                    here.units_of(player).into_iter().any(|unit| {
                        types
                            .get(unit.type_id.as_str())
                            .is_some_and(ti4_content::units::UnitType::is_ship)
                    })
                })
                .map(|(system, _)| {
                    ChoiceOption::labelled(
                        system.to_string(),
                        "legendary",
                        format!("place fighters in {system}"),
                    )
                })
                .collect();
            if placeable == 0 || options.is_empty() {
                return Ok(());
            }
            let choice = Choice::new(
                player.clone(),
                "Mirage Flight Academy: place fighters where",
                options,
            )
            .contextualized(DecisionContext::new(
                player.clone(),
                DecisionSource::Content("legendary".to_owned()),
                "legendary_place",
                state.phase,
                state.round,
            ));
            let answer =
                table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
            let system = ti4_model::id::SystemId::new(answer.id);
            let units = &mut state.system_mut(&system).units;
            for _ in 0..placeable {
                units.push(ti4_model::units::Unit::new(type_id.clone(), player.clone()));
            }
        }
        // "discard 1 secret objective to draw 1 secret objective"
        "mrte" => {
            let held: Vec<ti4_model::id::SecretObjectiveId> = state
                .player(player)
                .map(|seat| seat.secret_objectives.clone())
                .unwrap_or_default();
            if held.is_empty() {
                return Ok(());
            }
            let options: Vec<ChoiceOption> = held
                .iter()
                .map(|secret| {
                    ChoiceOption::labelled(secret.to_string(), "legendary", "discard this secret")
                })
                .collect();
            let choice = Choice::new(
                player.clone(),
                "The Galactic Council: discard which secret",
                options,
            )
            .contextualized(DecisionContext::new(
                player.clone(),
                DecisionSource::Content("legendary".to_owned()),
                "legendary_galactic_council",
                state.phase,
                state.round,
            ));
            let answer =
                table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
            let discarded = ti4_model::id::SecretObjectiveId::new(answer.id);
            if let Some(seat) = state.player_mut(player) {
                seat.secret_objectives.retain(|s| s != &discarded);
            }
            // Back to the deck before the draw, so a one-card deck still yields a card and the
            // swap cannot silently lose the discarded objective.
            state.secret_deck.push(discarded);
            crate::secrets::draw(state, content, table, player)?;
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::content_types::POK;

    fn content() -> &'static ContentStore {
        ContentStore::embedded()
    }

    /// A seat holding Mallice, with the commodities the ability can convert.
    fn holding_mallice(commodities: i32) -> (GameState, PlayerId, PlanetId) {
        let player = PlayerId::new("a");
        let planet = PlanetId::new("mallice");
        let mut state = crate::fixtures::game(&["a", "b"]);
        let system = ti4_model::id::SystemId::new("82b");
        state.board.entry(system.clone()).or_default();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player.clone());
        if let Some(seat) = state.player_mut(&player) {
            seat.commodities = commodities;
            seat.trade_goods = 0;
        }
        (state, player, planet)
    }


    /// A seat holding `planet`, with a board entry for `system`.
    fn holding(planet: &str, system: &str) -> (GameState, PlayerId) {
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a", "b"]);
        let system = ti4_model::id::SystemId::new(system);
        state.board.entry(system.clone()).or_default();
        state
            .system_mut(&system)
            .set_control(PlanetId::new(planet), player.clone());
        (state, player)
    }

    #[test]
    fn the_atrament_places_two_infantry_on_a_planet_you_control() {
        let (mut state, player) = holding("primor", "45");
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([
            "primor",
            "45|primor",
            "decline",
        ])));
        end_turn(&mut state, content(), POK, None, &mut table, &player).unwrap();

        let placed = state
            .system_state(&ti4_model::id::SystemId::new("45"))
            .planet_units
            .get(&PlanetId::new("primor"))
            .cloned()
            .unwrap_or_default();
        assert_eq!(placed.len(), 2, "two infantry: {placed:?}");
        assert!(placed.iter().all(|unit| unit.owner == player));
    }

    #[test]
    fn the_imperial_arms_vault_draws_a_card_when_that_branch_is_taken() {
        let (mut state, player) = holding("hopesend", "45");
        let before = state.player(&player).unwrap().action_cards.len();
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([
            "hopesend", "card", "decline",
        ])));
        end_turn(&mut state, content(), POK, None, &mut table, &player).unwrap();

        assert_eq!(
            state.player(&player).unwrap().action_cards.len(),
            before + 1,
            "the other branch of the same ability"
        );
    }

    #[test]
    fn the_galactic_council_swaps_one_secret_for_another() {
        let (mut state, player) = holding("mrte", "112");
        let discarded = ti4_model::id::SecretObjectiveId::new("mrm");
        if let Some(seat) = state.player_mut(&player) {
            seat.secret_objectives = vec![discarded.clone()];
        }
        state.secret_deck = vec![ti4_model::id::SecretObjectiveId::new("baf")];
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([
            "mrte", "mrm", "decline",
        ])));
        end_turn(&mut state, content(), POK, None, &mut table, &player).unwrap();

        let held = state.player(&player).unwrap().secret_objectives.clone();
        assert_eq!(held.len(), 1, "still one secret, a different one: {held:?}");
        assert!(
            !held.contains(&discarded),
            "the discarded objective is gone from the hand"
        );
    }

    #[test]
    fn an_ability_is_offered_once_a_round() {
        // Exhausted before it resolves, so an ability that asks a follow-up question cannot be
        // re-offered inside its own resolution -- and cannot be used twice in one turn.
        let (mut state, player) = holding("primor", "45");
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([
            "primor",
            "45|primor",
            "decline",
        ])));
        end_turn(&mut state, content(), POK, None, &mut table, &player).unwrap();
        assert!(!available(&state, &player, &PlanetId::new("primor")));

        // The status phase readies it, beside technologies and relics.
        for seat in &mut state.players {
            seat.exhausted_legendary.clear();
        }
        assert!(available(&state, &player, &PlanetId::new("primor")));
    }

    #[test]
    fn exterrix_headquarters_gains_two_trade_goods() {
        let (mut state, player, planet) = holding_mallice(4);
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([
            "mallice", "goods", "decline",
        ])));
        end_turn(&mut state, content(), POK, None, &mut table, &player).unwrap();

        let seat = state.player(&player).unwrap();
        assert_eq!(seat.trade_goods, 2, "two trade goods");
        assert_eq!(seat.commodities, 4, "commodities untouched on this branch");
        assert!(
            !available(&state, &player, &planet),
            "the ability card is spent for the round"
        );
    }

    #[test]
    fn exterrix_headquarters_converts_every_commodity() {
        let (mut state, player, _planet) = holding_mallice(5);
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([
            "mallice", "convert", "decline",
        ])));
        end_turn(&mut state, content(), POK, None, &mut table, &player).unwrap();

        let seat = state.player(&player).unwrap();
        assert_eq!(seat.trade_goods, 5, "all five converted");
        assert_eq!(seat.commodities, 0, "and none left behind");
    }

    #[test]
    fn a_seat_that_does_not_hold_the_planet_is_offered_nothing() {
        // The ability belongs to the planet, so losing it takes the ability with it.
        let (mut state, player, _planet) = holding_mallice(3);
        let system = ti4_model::id::SystemId::new("82b");
        state.system_mut(&system).planet_control.clear();
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new(
            Vec::<String>::new(),
        )));
        end_turn(&mut state, content(), POK, None, &mut table, &player).unwrap();
        assert_eq!(
            state.player(&player).unwrap().trade_goods,
            0,
            "nothing was offered, so nothing was gained"
        );
    }
}
