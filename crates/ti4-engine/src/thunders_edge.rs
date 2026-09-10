//! Thunder's Edge expedition actions needed by the learned opening policy.

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{BreakthroughId, PlanetId, PlayerId, SystemId, UnitTypeId};
use ti4_model::state::GameState;
use ti4_model::units::Unit;

use crate::choice::{Choice, ChoiceOption, IllegalChoice, Observed, Table};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::preview::{Delta, Preview, Quantity};
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
///
/// Offered whether or not the seat already holds its faction breakthrough. LRR permits it -- "a
/// player can claim multiple expedition slices in a game" -- and this used to suppress it anyway,
/// on the reasoning that a later slice bought nothing but a tie-break on who places the infantry.
///
/// That reasoning no longer holds. The placer takes control of Thunder's Edge: six influence, a
/// legendary card, and a planet every objective counting controlled planets can see. Deciding who
/// places is worth a real decision, and the count that decides it is the slices each seat claimed.
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
            .with("grants_breakthrough", true)
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
                let before = i64::try_from(hand.len()).unwrap_or(0);
                let options: Vec<ChoiceOption> = hand
                    .iter()
                    .enumerate()
                    .map(|(index, alias)| {
                        let label = content
                            .get(ContentType::ActionCards, alias.as_str())
                            .and_then(|record| record.text("name"))
                            .unwrap_or_else(|| alias.as_str());
                        ChoiceOption::labelled(index.to_string(), "discard", label).previewed(
                            Preview::certain(vec![Delta::new(
                                Quantity::ActionCardsHeld,
                                before,
                                before - 1,
                            )]),
                        )
                    })
                    .collect();
                let choice = Choice::new(
                    player.clone(),
                    "discard an action card for the expedition",
                    options,
                )
                .contextualized(DecisionContext::new(
                    player.clone(),
                    DecisionSource::Content("thunders_edge".to_owned()),
                    "expedition_discard_action_card",
                    state.phase,
                    state.round,
                ));
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
            let before =
                i64::try_from(crate::secrets::held_count(state, content, player)).unwrap_or(0);
            let choice = Choice::new(
                player.clone(),
                "discard a secret objective for the expedition",
                held.iter()
                    .map(|alias| {
                        let label = content
                            .get(ContentType::SecretObjectives, alias.as_str())
                            .and_then(|record| record.text("name"))
                            .unwrap_or_else(|| alias.as_str());
                        ChoiceOption::labelled(alias.to_string(), "return", label).previewed(
                            Preview::certain(vec![Delta::new(
                                Quantity::SecretObjectivesHeld,
                                before,
                                before - 1,
                            )]),
                        )
                    })
                    .collect(),
            )
            .contextualized(DecisionContext::new(
                player.clone(),
                DecisionSource::Content("thunders_edge".to_owned()),
                "expedition_discard_secret",
                state.phase,
                state.round,
            ));
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
    if state.expedition_slices.len() == SLICES.len() {
        complete_expedition(state, content, sources, galaxy, table, player)?;
    }
    Ok(true)
}

/// A system this game's map offers to place Thunder's Edge in: no planet, no supernova, no
/// printed wormhole ("a system of their choice that does not contain a planet, a supernova, or a
/// printed wormhole"), and not in the Fracture ("Thunder's Edge cannot be placed in the
/// Fracture").
fn eligible_systems(content: &ContentStore, sources: SourceSet, galaxy: &Galaxy) -> Vec<SystemId> {
    let mut systems: Vec<SystemId> = galaxy
        .system_ids()
        .into_iter()
        .filter(|id| {
            ti4_content::galaxy::system(content, id, sources).is_some_and(|record| {
                record.planets().is_empty()
                    && !record.is_supernova()
                    && record.wormholes().is_empty()
            })
        })
        .map(SystemId::new)
        .filter(|system| !crate::fracture::is_fracture_system(content, sources, system))
        .collect();
    systems.sort();
    systems
}

/// The finishing player's choice of where to place Thunder's Edge, or `None` when this game's
/// map has nowhere legal for it.
fn choose_system(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    finisher: &PlayerId,
) -> Result<Option<SystemId>, IllegalChoice> {
    let Some(galaxy) = galaxy else {
        return Ok(None); // no board to place it on
    };
    let candidates = eligible_systems(content, sources, galaxy);
    match candidates.as_slice() {
        [] => Ok(None),
        [only] => Ok(Some(only.clone())),
        many => {
            let choice = Choice::new(
                finisher.clone(),
                "Thunder's Edge expedition: choose a system with no planet, no supernova, and \
                 no printed wormhole"
                    .to_owned(),
                many.iter()
                    .map(|system| {
                        ChoiceOption::labelled(
                            system.to_string(),
                            "component",
                            format!("place Thunder's Edge in system {system}"),
                        )
                    })
                    .collect(),
            )
            .contextualized(DecisionContext::new(
                finisher.clone(),
                DecisionSource::Content("thunders_edge".to_owned()),
                "expedition_place_system",
                state.phase,
                state.round,
            ));
            let answer = ask_seeing(state, content, sources, Some(galaxy), table, &choice)?;
            Ok(Some(SystemId::new(answer.id)))
        }
    }
}

/// The player with the most claimed slices places the infantry; the finisher breaks a tie
/// ("In the case of a tie, the player who claimed the final slice chooses which tied player
/// places infantry on Thunder's Edge").
fn choose_placer(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    finisher: &PlayerId,
) -> Result<PlayerId, IllegalChoice> {
    let mut counts: std::collections::BTreeMap<PlayerId, usize> = std::collections::BTreeMap::new();
    for owner in state.expedition_slices.values() {
        *counts.entry(owner.clone()).or_insert(0) += 1;
    }
    let max = counts.values().copied().max().unwrap_or(0);
    let tied: Vec<PlayerId> = counts
        .into_iter()
        .filter(|(_, count)| *count == max)
        .map(|(player, _)| player)
        .collect();
    if let [only] = tied.as_slice() {
        return Ok(only.clone());
    }
    let choice = Choice::new(
        finisher.clone(),
        "Thunder's Edge expedition: which tied player places the infantry".to_owned(),
        tied.iter()
            .map(|player| ChoiceOption::labelled(player.as_str(), "component", player.to_string()))
            .collect(),
    )
    .contextualized(DecisionContext::new(
        finisher.clone(),
        DecisionSource::Content("thunders_edge".to_owned()),
        "expedition_placement_tiebreak",
        state.phase,
        state.round,
    ));
    let answer = ask_seeing(state, content, sources, galaxy, table, &choice)?;
    Ok(PlayerId::new(answer.id))
}

/// Place `count` of `player`'s infantry (their faction's own version, when the corpus has one)
/// on `planet`.
fn place_infantry(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
    count: usize,
) {
    if count == 0 {
        return;
    }
    let faction = state
        .player(player)
        .map(|seat| seat.faction.to_string())
        .unwrap_or_default();
    let generic = ti4_content::units::catalogue(content, sources)
        .get("infantry")
        .map(|unit| unit.id().to_owned());
    let Some(id) = ti4_content::units::faction_unit(content, &faction, "infantry", sources)
        .map(|unit| unit.id().to_owned())
        .or(generic)
    else {
        return;
    };
    let type_id = UnitTypeId::new(id);
    // 31.4: infantry is cardboard and effectively uncapped, but the door is the same one every
    // other placement in this engine goes through.
    let placeable = crate::supply::allowed(state, content, sources, player, &type_id, count);
    let held = state
        .system_mut(system)
        .planet_units
        .entry(planet.clone())
        .or_default();
    for _ in 0..placeable {
        held.push(Unit::new(type_id.clone(), player.clone()));
    }
}

/// "When all slices are claimed" (rules.json "expeditions"): flip Thunder's Edge to its planet
/// side, place it in a system of the finishing player's choice, then give the player with the
/// most claimed slices that many infantry on it.
///
/// Nothing did this before: `thunders_edge_system` existed and `fracture::brings_into_play`
/// already read it for Rule 13's ingress token, but the expedition's own completion never wrote
/// it, so the trigger fired -- the sixth slice really was claimed -- and nothing was ever placed.
fn complete_expedition(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    finisher: &PlayerId,
) -> Result<(), IllegalChoice> {
    let Some(system) = choose_system(state, content, sources, galaxy, table, finisher)? else {
        return Ok(()); // this game's map has nowhere legal to place it
    };
    let placer = choose_placer(state, content, sources, galaxy, table, finisher)?;
    let count = state
        .expedition_slices
        .values()
        .filter(|owner| *owner == &placer)
        .count();

    let planet = PlanetId::new("thundersedge");
    state.placed_planets.insert(planet.clone(), system.clone());
    state.board.entry(system.clone()).or_default();
    place_infantry(state, content, sources, &placer, &system, &planet, count);
    // The planet arrives with nobody on it, and ground forces landing on an uncontrolled planet
    // take it. Without this Thunder's Edge sat on the board held by no one: its six influence
    // never voted, its legendary card never resolved, and no objective counting controlled
    // planets ever saw it. Guarded on "uncontrolled" so this can never flip a planet someone
    // already holds -- taking those is invasion's job, not placement's.
    if !state
        .board
        .get(&system)
        .is_some_and(|here| here.planet_control.contains_key(&planet))
    {
        state
            .system_mut(&system)
            .set_control(planet.clone(), placer.clone());
    }
    state.thunders_edge_system = Some(system);
    Ok(())
}

#[cfg(test)]
mod tests {
    use ti4_model::content_types::DEFAULT;
    use ti4_model::id::{FactionId, SecretObjectiveId};

    use super::*;
    use crate::fixtures::game;

    /// While legal to invest further, offering the expedition to a faction that already holds
    /// its breakthrough is not fun: the one thing every slice is for has already happened.
    #[test]
    fn a_player_who_already_holds_the_breakthrough_may_still_claim_slices() {
        let content = ContentStore::embedded();
        let player = PlayerId::new("a");
        let mut state = game(&["a"]);
        let seat = state.player_mut(&player).unwrap();
        seat.faction = FactionId::new("letnev");
        seat.breakthrough = Some(BreakthroughId::new("letnevbt"));
        // Affordable by every slice, so an empty result here proves the breakthrough gate, not a
        // gate on the ability to pay.
        seat.trade_goods = 10;
        seat.action_cards = vec![
            ti4_model::id::ActionCardId::new("sabotage"),
            ti4_model::id::ActionCardId::new("upgrade"),
        ];
        seat.secret_objectives
            .push(SecretObjectiveId::new("destroy_their_greatest_ship"));

        // Every further slice raises this seat's claim on placing the infantry, and the placer
        // takes the planet -- so the decision is live even once the breakthrough is in hand.
        assert!(
            !available_actions(&state, content, DEFAULT, &player).is_empty(),
            "holding the breakthrough does not close the expedition"
        );
    }

    /// Claiming the sixth and final slice flips Thunder's Edge onto the board and gives the
    /// player with the most claimed slices that many infantry on it.
    ///
    /// Before `complete_expedition` existed, `perform` claimed the slice and stopped:
    /// `thunders_edge_system` stayed `None` forever, so `fracture::brings_into_play`'s Rule-13
    /// hookup for it -- which already read the field -- had nothing to find, and no infantry ever
    /// reached the board no matter how many slices were claimed.
    #[test]
    fn completing_the_expedition_places_thunders_edge_and_its_infantry() {
        let content = ContentStore::embedded();
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let mut state = game(&["a", "b"]);
        state.player_mut(&a).unwrap().faction = FactionId::new("letnev");
        state.player_mut(&a).unwrap().trade_goods = 3;

        // Five slices already claimed elsewhere in the game, split 2/3 so the sixth ties the two
        // players and exercises the finisher's tie-break.
        state.claim_slice("resources", &a);
        state.claim_slice("influence", &a);
        state.claim_slice("action_cards", &b);
        state.claim_slice("secret", &b);
        state.claim_slice("tech_planet", &b);

        // A tiny map with exactly one legal placement: 41 (Gravity Rift) has no planet, no
        // supernova and no printed wormhole; the padding tile has a real planet and is filtered
        // out, so only "41" is ever offered.
        let padding = ti4_content::galaxy::all_systems(content, DEFAULT)
            .into_iter()
            .find(|(_, system)| !system.is_hyperlane() && !system.planets().is_empty())
            .map(|(id, _)| id.to_owned())
            .expect("the corpus has a system with a planet");
        let galaxy = Galaxy::build(content, &[padding.as_str(), "41"], DEFAULT, 1).unwrap();

        let option = available_actions(&state, content, DEFAULT, &a)
            .into_iter()
            .find(|option| option.id == "component|expedition|trade_goods")
            .expect("the trade_goods slice is still unclaimed and affordable");

        // Only the placer tie-break needs scripting: with one legal system, `choose_system`
        // never asks.
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new(["a"])));
        assert!(
            perform(
                &mut state,
                content,
                DEFAULT,
                Some(&galaxy),
                &mut table,
                &a,
                &option,
            )
            .unwrap(),
            "the sixth slice completes the expedition"
        );

        let system = SystemId::new("41");
        assert_eq!(
            state.thunders_edge_system,
            Some(system.clone()),
            "Thunder's Edge was placed, and the engine remembers where"
        );
        let planet = PlanetId::new("thundersedge");
        assert_eq!(
            state.placed_planets.get(&planet),
            Some(&system),
            "the planet joins the system it was placed in"
        );
        let infantry = state
            .board
            .get(&system)
            .and_then(|board| board.planet_units.get(&planet))
            .cloned()
            .unwrap_or_default();
        assert_eq!(
            infantry.len(),
            3,
            "the tied player the finisher chose places their own claimed-slice count"
        );
        assert!(
            infantry.iter().all(|unit| unit.owner == a),
            "the finisher broke the tie in their own favor, as scripted"
        );
        // Placing ground forces on an uncontrolled planet takes it (LRR control). Thunder's Edge
        // arrives with nobody on it, so the player who lands is the player who holds it -- and
        // without this the planet's six influence and its legendary card sat outside the game.
        assert_eq!(
            state
                .board
                .get(&system)
                .and_then(|board| board.planet_control.get(&planet)),
            Some(&a),
            "the player who placed the infantry controls Thunder's Edge"
        );
    }

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

    /// OBS-008h3: the expedition's action-card discard previews the seat's own hand count
    /// falling by exactly one, on the first of its two iterations (the second offers no choice).
    #[test]
    fn obs008h3_expedition_action_card_discard_previews_the_exact_loss() {
        let content = ContentStore::embedded();
        let player = PlayerId::new("a");
        let mut state = game(&["a"]);
        state.player_mut(&player).unwrap().action_cards = vec![
            ti4_model::id::ActionCardId::new("sabotage"),
            ti4_model::id::ActionCardId::new("upgrade"),
        ];
        let (decider, seen) =
            crate::choice::Capturing::new(Box::new(crate::choice::Scripted::new(["0"])));
        let mut table = Table::with_default(Box::new(decider));

        pay(
            &mut state,
            content,
            DEFAULT,
            None,
            &mut table,
            &player,
            "action_cards",
        )
        .unwrap();

        let ask = seen.borrow();
        let asked = ask.first().expect("the first discard asked");
        let option = asked.option("0").expect("the first card was offered");
        assert_eq!(
            option.preview,
            Some(Preview::certain(vec![Delta::new(
                Quantity::ActionCardsHeld,
                2,
                1,
            )]))
        );
    }

    /// OBS-008h3: the expedition's secret discard previews the seat's own held-secret count
    /// falling by exactly one.
    #[test]
    fn obs008h3_expedition_secret_discard_previews_the_exact_loss() {
        let content = ContentStore::embedded();
        let player = PlayerId::new("a");
        let mut state = game(&["a"]);
        state.player_mut(&player).unwrap().secret_objectives =
            vec![SecretObjectiveId::new("s0"), SecretObjectiveId::new("s1")];
        let (decider, seen) =
            crate::choice::Capturing::new(Box::new(crate::choice::Scripted::new(["s0"])));
        let mut table = Table::with_default(Box::new(decider));

        pay(
            &mut state, content, DEFAULT, None, &mut table, &player, "secret",
        )
        .unwrap();

        let ask = seen.borrow();
        let asked = ask.first().expect("the secret discard asked");
        let option = asked.option("s0").expect("the secret was offered");
        assert_eq!(
            option.preview,
            Some(Preview::certain(vec![Delta::new(
                Quantity::SecretObjectivesHeld,
                2,
                1,
            )]))
        );
    }
}
