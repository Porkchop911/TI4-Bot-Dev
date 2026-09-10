//! Legendary planet abilities.
//!
//! Nineteen planets in the corpus carry a `legendaryAbilityName` and its text, and until this
//! module nothing read either: `Planet::is_legendary` answered a bool used for excluding them from
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
/// Three Flight Academies carry identical text on different planets, and both faces of Mallice
/// carry Exterrix Headquarters, so the table has more rows than there are distinct abilities.
const END_OF_TURN: [(&str, &str); 9] = [
    ("hopesend", "Imperial Arms Vault"),
    ("primor", "The Atrament"),
    ("mallice", "Exterrix Headquarters"),
    ("lockedmallice", "Exterrix Headquarters"),
    ("mirage", "Mirage Flight Academy"),
    ("illusion", "Illusion Flight Academy"),
    ("phantasm", "Phantasm Flight Academy"),
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
