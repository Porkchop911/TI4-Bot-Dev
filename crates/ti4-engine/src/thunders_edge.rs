//! Thunder's Edge expedition actions needed by the learned opening policy.

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{BreakthroughId, PlanetId, PlayerId};
use ti4_model::state::GameState;

use crate::choice::{Choice, ChoiceOption, IllegalChoice, Observed, Table};
use crate::production::Spend;

const SLICES: [(&str, &str, f64); 6] = [
    ("resources", "spend 5 resources", 10.0),
    ("influence", "spend 5 influence", 7.5),
    ("trade_goods", "spend 3 trade goods", 9.0),
    ("action_cards", "discard 2 action cards", 10.0),
    ("secret", "discard 1 unscored secret objective", 14.0),
    ("tech_planet", "exhaust 1 technology specialty planet", 4.0),
];

fn specialty_planets(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<PlanetId> {
    let planets = ti4_content::galaxy::all_planets(content, sources);
    state
        .controlled_planets(player)
        .into_iter()
        .filter_map(|(_, planet)| {
            (!state.exhausted_planets.contains(planet)
                && planets
                    .get(planet.as_str())
                    .is_some_and(|record| !record.tech_specialties().is_empty()))
            .then_some(planet.clone())
        })
        .collect()
}

fn can_pay(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    slice: &str,
) -> bool {
    let Some(seat) = state.player(player) else {
        return false;
    };
    match slice {
        "resources" => {
            crate::production::available(state, content, sources, player, Spend::Resources) >= 5
        }
        "influence" => {
            crate::production::available(state, content, sources, player, Spend::Influence) >= 5
        }
        "trade_goods" => seat.trade_goods >= 3,
        "action_cards" => seat.action_cards.len() >= 2,
        "secret" => !seat.secret_objectives.is_empty(),
        "tech_planet" => !specialty_planets(state, content, sources, player).is_empty(),
        _ => false,
    }
}

/// Expedition slices this player can currently claim.
#[must_use]
pub fn available_actions(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<ChoiceOption> {
    if !sources.contains(ti4_model::content_types::Source::ThundersEdge) {
        return Vec::new();
    }
    let first = state
        .player(player)
        .is_some_and(|seat| seat.breakthrough.is_none());
    SLICES
        .iter()
        .filter(|(slice, _, _)| !state.expedition_slices.contains_key(*slice))
        .filter(|(slice, _, _)| can_pay(state, content, sources, player, slice))
        .map(|(slice, label, opportunity_cost)| {
            ChoiceOption::labelled(
                format!("component|expedition|{slice}"),
                "component",
                format!("Thunder's Edge expedition: {label}"),
            )
            .with("slice", *slice)
            .with("grants_breakthrough", first)
            .with("opportunity_cost", *opportunity_cost)
        })
        .collect()
}

fn ask_seeing(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    choice: &Choice,
) -> Result<ChoiceOption, IllegalChoice> {
    table.ask_seeing(choice, &Observed::new(state, content, sources, galaxy))
}

#[allow(
    clippy::too_many_lines,
    reason = "the six mutually exclusive printed expedition costs are clearest as one match"
)]
fn pay(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    slice: &str,
) -> Result<bool, IllegalChoice> {
    if !can_pay(state, content, sources, player, slice) {
        return Ok(false);
    }
    match slice {
        "resources" => crate::production::pay_seeing(
            state,
            content,
            sources,
            galaxy,
            table,
            player,
            5,
            Spend::Resources,
        ),
        "influence" => crate::production::pay_seeing(
            state,
            content,
            sources,
            galaxy,
            table,
            player,
            5,
            Spend::Influence,
        ),
        "trade_goods" => {
            state.player_mut(player).expect("player exists").trade_goods -= 3;
            Ok(true)
        }
        "action_cards" => {
            for _ in 0..2 {
                let hand = state
                    .player(player)
                    .expect("player exists")
                    .action_cards
                    .clone();
                let options: Vec<ChoiceOption> = hand
                    .iter()
                    .enumerate()
                    .map(|(index, alias)| {
                        let label = content
                            .get(ContentType::ActionCards, alias.as_str())
                            .and_then(|record| record.text("name"))
                            .unwrap_or_else(|| alias.as_str());
                        ChoiceOption::labelled(index.to_string(), "discard", label)
                    })
                    .collect();
                let choice = Choice::new(
                    player.clone(),
                    "discard an action card for the expedition",
                    options,
                );
                let chosen = if choice.options.len() == 1 {
                    choice.options[0].clone()
                } else {
                    ask_seeing(state, content, sources, galaxy, table, &choice)?
                };
                let index = chosen.id.parse::<usize>().unwrap_or(0);
                crate::action_cards::discard(state, player, index);
            }
            Ok(true)
        }
        "secret" => {
            let held = state
                .player(player)
                .expect("player exists")
                .secret_objectives
                .clone();
            let choice = Choice::new(
                player.clone(),
                "discard a secret objective for the expedition",
                held.iter()
                    .map(|alias| {
                        let label = content
                            .get(ContentType::SecretObjectives, alias.as_str())
                            .and_then(|record| record.text("name"))
                            .unwrap_or_else(|| alias.as_str());
                        ChoiceOption::labelled(alias.to_string(), "return", label)
                    })
                    .collect(),
            );
            let chosen = if choice.options.len() == 1 {
                choice.options[0].clone()
            } else {
                ask_seeing(state, content, sources, galaxy, table, &choice)?
            };
            if let Some(index) = state
                .player(player)
                .expect("player exists")
                .secret_objectives
                .iter()
                .position(|alias| alias.as_str() == chosen.id)
            {
                let returned = state
                    .player_mut(player)
                    .expect("player exists")
                    .secret_objectives
                    .remove(index);
                state.secret_deck.push(returned);
            }
            Ok(true)
        }
        "tech_planet" => {
            if let Some(planet) = specialty_planets(state, content, sources, player).first() {
                state.exhaust_planet(planet.clone());
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn breakthrough_for(
    content: &ContentStore,
    _sources: SourceSet,
    faction: &str,
) -> Option<BreakthroughId> {
    content
        .records(ContentType::Breakthroughs)
        .iter()
        .find(|record| record.text("faction") == Some(faction))
        .and_then(|record| record.text("alias"))
        .map(BreakthroughId::new)
}

/// Claim and pay one expedition slice, gaining the faction breakthrough on the first claim.
///
/// # Errors
/// Returns [`IllegalChoice`] when a nested discard or payment choice is invalid.
pub fn perform(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    option: &ChoiceOption,
) -> Result<bool, IllegalChoice> {
    let Some(slice) = option.id.strip_prefix("component|expedition|") else {
        return Ok(false);
    };
    if state.expedition_slices.contains_key(slice)
        || !pay(state, content, sources, galaxy, table, player, slice)?
    {
        return Ok(false);
    }
    state.claim_slice(slice, player);
    let first = state
        .expedition_slices
        .values()
        .filter(|owner| *owner == player)
        .count()
        == 1;
    if first {
        let Some(faction) = state.player(player).map(|seat| seat.faction.to_string()) else {
            return Ok(false);
        };
        if let Some(breakthrough) = breakthrough_for(content, sources, &faction)
            && let Some(seat) = state.player_mut(player)
        {
            seat.breakthrough = Some(breakthrough);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use ti4_model::content_types::DEFAULT;
    use ti4_model::id::{FactionId, SecretObjectiveId};

    use super::*;
    use crate::fixtures::game;

    #[test]
    fn a_first_expedition_slice_grants_the_faction_breakthrough() {
        let content = ContentStore::embedded();
        assert_eq!(
            breakthrough_for(content, DEFAULT, "letnev"),
            Some(BreakthroughId::new("letnevbt"))
        );

        let player = PlayerId::new("a");
        let mut state = game(&["a"]);
        let seat = state.player_mut(&player).unwrap();
        seat.faction = FactionId::new("letnev");
        seat.secret_objectives
            .push(SecretObjectiveId::new("destroy_their_greatest_ship"));
        let mut table = Table::new();
        let option = available_actions(&state, content, DEFAULT, &player)
            .into_iter()
            .find(|option| option.id == "component|expedition|secret")
            .unwrap();

        assert!(
            perform(
                &mut state, content, DEFAULT, None, &mut table, &player, &option
            )
            .unwrap()
        );
        assert_eq!(
            state.player(&player).unwrap().breakthrough,
            Some(BreakthroughId::new("letnevbt"))
        );
        assert_eq!(state.expedition_slices.get("secret"), Some(&player));
    }
}
