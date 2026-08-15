//! Strategy-card abilities (LRR 52, 91, 92).
//!
//! All eight ordinary cards are resolved here.  Decisions are made through `ask_seeing`, so a
//! learned policy receives the same public board observation for strategy-card choices that it
//! receives for tactical choices.  Thunder's Edge Construction and Warfare are dispatched by
//! card id because they share printed names with materially different cards.

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{PlanetId, PlayerId, SystemId, TechnologyId, UnitTypeId};
use ti4_model::state::{GameState, TokenPool};
use ti4_model::units::Unit;

use crate::choice::{Choice, ChoiceOption, IllegalChoice, Observed, Table};
use crate::production::Spend;

pub const LEADERSHIP_TOKENS: u32 = 3;
pub const INFLUENCE_PER_TOKEN: i64 = 3;
pub const TECHNOLOGY_PRIMARY_SECOND_COST: i64 = 6;
pub const TECHNOLOGY_SECONDARY_COST: i64 = 4;
pub const RESEARCH_KIND: &str = "research";

/// Result of a primary.  TE Warfare hands its free activation back to the stepped game driver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ability {
    Resolved,
    Unresolved,
    FreeTactical(SystemId),
}

#[must_use]
pub fn registered_cards() -> Vec<&'static str> {
    vec![
        "Leadership",
        "Diplomacy",
        "Politics",
        "Construction",
        "Trade",
        "Warfare",
        "Technology",
        "Imperial",
    ]
}

#[must_use]
pub fn card_name(content: &ContentStore, card: &str) -> Option<String> {
    content
        .get(ContentType::StrategyCards, card)
        .and_then(|record| record.text("name"))
        .map(ToOwned::to_owned)
}

fn ask(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    choice: &Choice,
) -> Result<ChoiceOption, IllegalChoice> {
    table.ask_seeing(choice, &Observed::new(state, content, sources, galaxy))
}

fn gain_tokens(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    count: u32,
) -> Result<(), IllegalChoice> {
    for _ in 0..count {
        let choice = Choice::new(
            player.clone(),
            "gain a command token into which pool",
            vec![
                ChoiceOption::labelled("tactic_tokens", "pool", "tactic pool"),
                ChoiceOption::labelled("fleet_tokens", "pool", "fleet pool"),
                ChoiceOption::labelled("strategic_tokens", "pool", "strategy pool"),
            ],
        );
        let answer = ask(state, content, sources, galaxy, table, &choice)?;
        let pool = match answer.id.as_str() {
            "tactic_tokens" => TokenPool::Tactic,
            "fleet_tokens" => TokenPool::Fleet,
            _ => TokenPool::Strategic,
        };
        if let Some(seat) = state.player_mut(player) {
            seat.gain_token(pool, 1);
        }
    }
    Ok(())
}

fn buy_tokens_with_influence(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
) -> Result<(), IllegalChoice> {
    while crate::payment::affordable(
        state,
        content,
        sources,
        player,
        INFLUENCE_PER_TOKEN,
        Spend::Influence,
    ) {
        let choice = Choice::new(
            player.clone(),
            format!("spend {INFLUENCE_PER_TOKEN} influence for a command token"),
            vec![
                ChoiceOption::labelled("buy", "spend", "buy token"),
                ChoiceOption::decline(),
            ],
        );
        if ask(state, content, sources, galaxy, table, &choice)?.is_decline() {
            break;
        }
        let Some(plan) = crate::payment::plans(
            state,
            content,
            sources,
            player,
            INFLUENCE_PER_TOKEN,
            Spend::Influence,
        )
        .into_iter()
        .next() else {
            break;
        };
        if !crate::payment::apply(state, player, &plan) {
            break;
        }
        gain_tokens(state, content, sources, galaxy, table, player, 1)?;
    }
    Ok(())
}

fn offer_research(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
) -> Result<Option<TechnologyId>, IllegalChoice> {
    let open = crate::technology::researchable(state, content, sources, player);
    if open.is_empty() {
        return Ok(None);
    }
    let choice = Choice::new(
        player.clone(),
        "research a technology",
        open.iter()
            .map(|id| {
                ChoiceOption::labelled(
                    id.to_string(),
                    RESEARCH_KIND,
                    crate::technology::name(content, id),
                )
            })
            .chain(std::iter::once(ChoiceOption::decline()))
            .collect(),
    );
    let answer = ask(state, content, sources, galaxy, table, &choice)?;
    if answer.is_decline() {
        return Ok(None);
    }
    let technology = TechnologyId::new(answer.id);
    Ok(
        crate::technology::research(state, content, sources, player, &technology)
            .then_some(technology),
    )
}

fn paid_research(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    cost: i64,
) -> Result<(), IllegalChoice> {
    if !crate::payment::affordable(state, content, sources, player, cost, Spend::Resources) {
        return Ok(());
    }
    // Choose before paying, but take the payment before mutating the technology set.
    let open = crate::technology::researchable(state, content, sources, player);
    if open.is_empty() {
        return Ok(());
    }
    let choice = Choice::new(
        player.clone(),
        "research a technology",
        open.iter()
            .map(|id| {
                ChoiceOption::labelled(
                    id.to_string(),
                    RESEARCH_KIND,
                    crate::technology::name(content, id),
                )
            })
            .chain(std::iter::once(ChoiceOption::decline()))
            .collect(),
    );
    let answer = ask(state, content, sources, galaxy, table, &choice)?;
    if answer.is_decline() {
        return Ok(());
    }
    let Some(plan) = crate::payment::plans(state, content, sources, player, cost, Spend::Resources)
        .into_iter()
        .next()
    else {
        return Ok(());
    };
    if crate::payment::apply(state, player, &plan) {
        crate::technology::research(
            state,
            content,
            sources,
            player,
            &TechnologyId::new(answer.id),
        );
    }
    Ok(())
}

fn ready_planets(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    maximum: usize,
) -> Result<(), IllegalChoice> {
    for _ in 0..maximum {
        let controlled: Vec<PlanetId> = state
            .controlled_planets(player)
            .into_iter()
            .map(|(_, planet)| planet.clone())
            .filter(|planet| state.exhausted_planets.contains(planet))
            .collect();
        if controlled.is_empty() {
            break;
        }
        let choice = Choice::new(
            player.clone(),
            "ready an exhausted planet",
            controlled
                .iter()
                .map(|planet| {
                    ChoiceOption::labelled(planet.to_string(), "ready", planet.to_string())
                })
                .chain(std::iter::once(ChoiceOption::decline()))
                .collect(),
        );
        let answer = ask(state, content, sources, galaxy, table, &choice)?;
        if answer.is_decline() {
            break;
        }
        state.ready_planet(&PlanetId::new(answer.id));
    }
    Ok(())
}

fn diplomacy_primary(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
) -> Result<(), IllegalChoice> {
    let systems: Vec<SystemId> = state
        .controlled_planets(player)
        .into_iter()
        .map(|(system, _)| system.clone())
        .filter(|system| system.as_str() != crate::seating::MECATOL)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    if !systems.is_empty() {
        let choice = Choice::new(
            player.clone(),
            "choose a system for Diplomacy",
            systems
                .iter()
                .map(|system| {
                    ChoiceOption::labelled(system.to_string(), "system", system.to_string())
                })
                .collect(),
        );
        let chosen = SystemId::new(ask(state, content, sources, galaxy, table, &choice)?.id);
        for other in state.seating_order.clone() {
            if &other != player {
                state.system_mut(&chosen).command_tokens.insert(other);
            }
        }
    }
    ready_planets(state, content, sources, galaxy, table, player, 2)
}

fn politics_primary(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
) -> Result<(), IllegalChoice> {
    let candidates: Vec<PlayerId> = state
        .seating_order
        .iter()
        .filter(|candidate| **candidate != state.speaker)
        .cloned()
        .collect();
    if !candidates.is_empty() {
        // engine/strategy.py:736–748 offers the candidates by faction name — Python player ids
        // are factions — so the surface names them and the answer maps back to a seat.
        let named: Vec<(String, PlayerId)> = candidates
            .iter()
            .map(|candidate| {
                (
                    crate::promissory::faction_name(state, candidate),
                    candidate.clone(),
                )
            })
            .collect();
        let choice = Choice::new(
            player.clone(),
            "who becomes speaker",
            named
                .iter()
                .map(|(name, _)| {
                    ChoiceOption::labelled(
                        name.clone(),
                        "speaker",
                        format!("{name} becomes speaker"),
                    )
                })
                .collect(),
        );
        let chosen = ask(state, content, sources, galaxy, table, &choice)?.id;
        state.speaker = named
            .iter()
            .find(|(name, _)| *name == chosen)
            .map(|(_, seat)| seat.clone())
            .ok_or_else(|| IllegalChoice::NotOffered {
                player: player.clone(),
                offered: named.iter().map(|(name, _)| name.clone()).collect(),
                chosen,
            })?;
    }
    crate::action_cards::draw(state, content, table, player, 2)?;
    let looked: Vec<String> = (0..state.agenda_deck.len().min(2))
        .map(|_| state.agenda_deck.remove(0))
        .collect();
    for agenda in looked {
        let choice = Choice::new(
            player.clone(),
            format!("place {agenda} where"),
            vec![
                ChoiceOption::labelled("top", "agenda", "on top of the deck"),
                ChoiceOption::labelled("bottom", "agenda", "on the bottom"),
            ],
        );
        if ask(state, content, sources, galaxy, table, &choice)?.id == "top" {
            state.agenda_deck.insert(0, agenda);
        } else {
            state.agenda_deck.push(agenda);
        }
    }
    Ok(())
}

fn structure_options(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    only_pds: bool,
) -> Vec<ChoiceOption> {
    state
        .controlled_planets(player)
        .into_iter()
        .flat_map(|(system, planet)| {
            ["pds", "spacedock"]
                .into_iter()
                .filter(move |kind| !only_pds || *kind == "pds")
                .filter(move |kind| {
                    let unit = UnitTypeId::new(*kind);
                    crate::production::structure_allowed(
                        state, content, sources, player, planet, kind,
                    ) && crate::supply::allowed(state, content, sources, player, &unit, 1) == 1
                })
                .map(move |kind| {
                    ChoiceOption::labelled(
                        format!("{kind}|{system}|{planet}"),
                        "build",
                        format!("place {kind} on {planet}"),
                    )
                })
        })
        .collect()
}

fn place_structure(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    only_pds: bool,
) -> Result<Option<SystemId>, IllegalChoice> {
    let mut options = structure_options(state, content, sources, player, only_pds);
    if options.is_empty() {
        return Ok(None);
    }
    options.push(ChoiceOption::decline());
    let choice = Choice::new(player.clone(), "place a structure", options);
    let answer = ask(state, content, sources, galaxy, table, &choice)?;
    if answer.is_decline() {
        return Ok(None);
    }
    let mut parts = answer.id.split('|');
    let (Some(kind), Some(system), Some(planet), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Ok(None);
    };
    let system = SystemId::new(system);
    let planet = PlanetId::new(planet);
    let controlled =
        state
            .controlled_planets(player)
            .into_iter()
            .any(|(candidate_system, candidate_planet)| {
                candidate_system == &system && candidate_planet == &planet
            });
    if !controlled {
        return Ok(None);
    }
    state
        .system_mut(&system)
        .planet_units
        .entry(planet)
        .or_default()
        .push(Unit::new(UnitTypeId::new(kind), player.clone()));
    Ok(Some(system))
}

fn commodity_limit(state: &GameState, content: &ContentStore, player: &PlayerId) -> i32 {
    state
        .player(player)
        .and_then(|seat| ti4_content::factions::get(content, seat.faction.as_str()))
        .map_or(0, |faction| faction.commodities())
}

fn replenish(state: &mut GameState, content: &ContentStore, player: &PlayerId) {
    let limit = commodity_limit(state, content, player);
    if let Some(seat) = state.player_mut(player) {
        seat.commodities = limit;
    }
}

fn trade_primary(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
) -> Result<(), IllegalChoice> {
    if let Some(seat) = state.player_mut(player) {
        seat.trade_goods += 3;
    }
    replenish(state, content, player);
    let mut remaining: Vec<PlayerId> = state
        .seating_order
        .iter()
        .filter(|other| *other != player)
        .cloned()
        .collect();
    loop {
        remaining.retain(|other| {
            state
                .player(other)
                .is_some_and(|seat| seat.commodities < commodity_limit(state, content, other))
        });
        if remaining.is_empty() {
            break;
        }
        let choice = Choice::new(
            player.clone(),
            "grant free Trade replenishment",
            remaining
                .iter()
                .map(|other| {
                    ChoiceOption::labelled(other.to_string(), "replenish", other.to_string())
                })
                .chain(std::iter::once(ChoiceOption::decline()))
                .collect(),
        );
        let answer = ask(state, content, sources, galaxy, table, &choice)?;
        if answer.is_decline() {
            break;
        }
        let other = PlayerId::new(answer.id);
        replenish(state, content, &other);
        remaining.retain(|candidate| candidate != &other);
    }
    Ok(())
}

fn home_production(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
) -> Result<(), IllegalChoice> {
    let home = state
        .player(player)
        .and_then(|seat| seat.home_system.clone());
    if let Some(home) = home
        && crate::production::capacity(state, content, sources, player, &home) > 0
    {
        crate::production::resolve(state, content, sources, galaxy, table, player, &home)?;
    }
    Ok(())
}

fn warfare_primary(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
) -> Result<(), IllegalChoice> {
    let systems: Vec<SystemId> = state
        .systems_with_token(player)
        .into_iter()
        .cloned()
        .collect();
    if systems.is_empty() {
        return Ok(());
    }
    let choice = Choice::new(
        player.clone(),
        "recall a command token",
        systems
            .iter()
            .map(|system| ChoiceOption::labelled(system.to_string(), "recall", system.to_string()))
            .collect(),
    );
    let system = SystemId::new(ask(state, content, sources, galaxy, table, &choice)?.id);
    state.system_mut(&system).command_tokens.remove(player);
    gain_tokens(state, content, sources, galaxy, table, player, 1)
}

fn imperial_primary(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
) -> Result<(), IllegalChoice> {
    let scoreable = crate::objectives::scoreable_on(state, content, sources, player, galaxy);
    if !scoreable.is_empty() {
        let choice = Choice::new(
            player.clone(),
            "score a public objective with Imperial",
            scoreable
                .iter()
                .map(|objective| {
                    ChoiceOption::labelled(
                        objective.to_string(),
                        "objective",
                        objective.to_string(),
                    )
                })
                .chain(std::iter::once(ChoiceOption::decline()))
                .collect(),
        );
        let answer = ask(state, content, sources, galaxy, table, &choice)?;
        if !answer.is_decline() {
            let _ = crate::objectives::award(
                state,
                content,
                sources,
                player,
                &ti4_model::id::ObjectiveId::new(answer.id),
            );
        }
    }
    let controls_mecatol = state
        .controlled_planets(player)
        .into_iter()
        .any(|(system, _)| system.as_str() == crate::seating::MECATOL);
    if controls_mecatol {
        if let Some(seat) = state.player_mut(player) {
            seat.victory_points = (seat.victory_points + 1).min(crate::objectives::VICTORY_TARGET);
        }
    } else {
        crate::secrets::draw(state, content, table, player)?;
    }
    state.finished = crate::objectives::winner(state).is_some();
    Ok(())
}

/// Resolve a primary ability.
///
/// # Errors
/// Returns [`IllegalChoice`] if a decider selects an option that was not offered.
#[allow(
    clippy::too_many_lines,
    reason = "the public dispatcher keeps all card-id and printed-name routing visible in one place"
)]
pub fn primary(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    card: &str,
) -> Result<Ability, IllegalChoice> {
    if card == "te6warfare" {
        if state.phase != ti4_model::state::Phase::Action {
            return Ok(Ability::Resolved);
        }
        let Some(galaxy) = galaxy else {
            return Ok(Ability::Resolved);
        };
        let systems: Vec<String> = galaxy
            .system_ids()
            .into_iter()
            .map(ToOwned::to_owned)
            .collect();
        if systems.is_empty() {
            return Ok(Ability::Resolved);
        }
        let choice = Choice::new(
            player.clone(),
            "Warfare: a tactical action without a command token",
            systems
                .iter()
                .map(|system| {
                    ChoiceOption::labelled(
                        system,
                        "activate",
                        format!("free tactical action in {system}"),
                    )
                })
                .collect(),
        );
        let answer = ask(state, content, sources, Some(galaxy), table, &choice)?;
        return Ok(Ability::FreeTactical(SystemId::new(answer.id)));
    }
    if card == "te4construction" {
        // Keep the oracle's two-stage choice shape exactly.  The deployed explicit policy was
        // trained with one abstract `structure` option competing against each dock's
        // `produce|system` option.  Flattening the structure branch into every legal PDS/dock
        // placement changes both the feature names and the softmax denominator before the
        // policy has chosen which ability to resolve.
        let production_systems: Vec<SystemId> = state
            .board
            .keys()
            .filter(|system| {
                crate::production::capacity(state, content, sources, player, system) > 0
            })
            .cloned()
            .collect();
        let mut options = vec![ChoiceOption::labelled(
            "structure",
            "build",
            "place a structure",
        )];
        options.extend(production_systems.iter().map(|system| {
            ChoiceOption::labelled(
                format!("produce|{system}"),
                "build",
                format!("use PRODUCTION in {system}"),
            )
        }));
        let choice = Choice::new(
            player.clone(),
            "Construction: a structure or a production",
            options,
        );
        let answer = ask(state, content, sources, galaxy, table, &choice)?;
        if let Some(system) = answer.id.strip_prefix("produce|") {
            crate::production::resolve(
                state,
                content,
                sources,
                galaxy,
                table,
                player,
                &SystemId::new(system),
            )?;
        } else {
            place_structure(state, content, sources, galaxy, table, player, false)?;
        }
        place_structure(state, content, sources, galaxy, table, player, false)?;
        return Ok(Ability::Resolved);
    }

    let Some(name) = card_name(content, card) else {
        return Ok(Ability::Unresolved);
    };
    match name.as_str() {
        "Leadership" => {
            gain_tokens(
                state,
                content,
                sources,
                galaxy,
                table,
                player,
                LEADERSHIP_TOKENS,
            )?;
            buy_tokens_with_influence(state, content, sources, galaxy, table, player)?;
        }
        "Diplomacy" => diplomacy_primary(state, content, sources, galaxy, table, player)?,
        "Politics" => politics_primary(state, content, sources, galaxy, table, player)?,
        "Construction" => {
            place_structure(state, content, sources, galaxy, table, player, false)?;
            place_structure(state, content, sources, galaxy, table, player, true)?;
        }
        "Trade" => trade_primary(state, content, sources, galaxy, table, player)?,
        "Warfare" => warfare_primary(state, content, sources, galaxy, table, player)?,
        "Technology" => {
            offer_research(state, content, sources, galaxy, table, player)?;
            paid_research(
                state,
                content,
                sources,
                galaxy,
                table,
                player,
                TECHNOLOGY_PRIMARY_SECOND_COST,
            )?;
        }
        "Imperial" => imperial_primary(state, content, sources, galaxy, table, player)?,
        _ => return Ok(Ability::Unresolved),
    }
    Ok(Ability::Resolved)
}

/// Resolve one follower's secondary after the shared follower window has charged its token.
///
/// # Errors
/// Returns [`IllegalChoice`] if a decider selects an option that was not offered.
pub fn secondary(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    card: &str,
) -> Result<Ability, IllegalChoice> {
    let Some(name) = card_name(content, card) else {
        return Ok(Ability::Unresolved);
    };
    match name.as_str() {
        "Leadership" => buy_tokens_with_influence(state, content, sources, galaxy, table, player)?,
        "Diplomacy" => ready_planets(state, content, sources, galaxy, table, player, 2)?,
        "Politics" => {
            crate::action_cards::draw(state, content, table, player, 2)?;
        }
        "Construction" => {
            place_structure(state, content, sources, galaxy, table, player, false)?;
        }
        "Trade" => replenish(state, content, player),
        "Warfare" => home_production(state, content, sources, galaxy, table, player)?,
        "Technology" => paid_research(
            state,
            content,
            sources,
            galaxy,
            table,
            player,
            TECHNOLOGY_SECONDARY_COST,
        )?,
        "Imperial" => {
            crate::secrets::draw(state, content, table, player)?;
        }
        _ => return Ok(Ability::Unresolved),
    }
    Ok(Ability::Resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{a_placed_planet, game, plain_hub, put_on_planet};
    use ti4_model::content_types::POK;

    fn card(name: &str) -> String {
        ContentStore::embedded()
            .records(ContentType::StrategyCards)
            .iter()
            .find(|record| {
                record.text("name") == Some(name)
                    && record.text("id").is_some_and(|id| id.starts_with("pok"))
            })
            .and_then(|record| record.text("id"))
            .unwrap_or_else(|| panic!("missing {name}"))
            .to_owned()
    }

    #[test]
    fn every_base_card_is_registered_and_resolves() {
        let content = ContentStore::embedded();
        for name in registered_cards() {
            let mut state = game(&["a", "b"]);
            let mut table = Table::with_default(Box::new(crate::choice::AlwaysDecline));
            let result = primary(
                &mut state,
                content,
                POK,
                None,
                &mut table,
                &PlayerId::new("a"),
                &card(name),
            )
            .unwrap();
            assert_ne!(result, Ability::Unresolved, "{name}");
        }
    }

    #[test]
    fn leadership_allocates_three_tokens() {
        let mut state = game(&["a"]);
        let before = state.player(&PlayerId::new("a")).unwrap().total_tokens();
        let mut table = Table::new();
        primary(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            &PlayerId::new("a"),
            &card("Leadership"),
        )
        .unwrap();
        assert_eq!(
            state.player(&PlayerId::new("a")).unwrap().total_tokens(),
            before + 3
        );
    }

    #[test]
    fn trade_gains_goods_and_replenishes() {
        let mut state = game(&["a"]);
        let mut table = Table::new();
        primary(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            &PlayerId::new("a"),
            &card("Trade"),
        )
        .unwrap();
        let seat = state.player(&PlayerId::new("a")).unwrap();
        assert_eq!(seat.trade_goods, 3);
        assert_eq!(
            seat.commodities,
            commodity_limit(&state, ContentStore::embedded(), &PlayerId::new("a"))
        );
    }

    #[test]
    fn warfare_recalls_a_board_token_and_gains_one() {
        let mut state = game(&["a"]);
        let player = PlayerId::new("a");
        let system = SystemId::new("18");
        state
            .system_mut(&system)
            .command_tokens
            .insert(player.clone());
        let before = state.player(&player).unwrap().total_tokens();
        let mut table = Table::new();
        primary(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            &player,
            &card("Warfare"),
        )
        .unwrap();
        assert!(!state.system_state(&system).command_tokens.contains(&player));
        assert_eq!(state.player(&player).unwrap().total_tokens(), before + 1);
    }

    #[test]
    fn diplomacy_locks_the_system_and_readies_planets() {
        let mut state = game(&["a", "b"]);
        let player = PlayerId::new("a");
        let other = PlayerId::new("b");
        let (system, planet) = a_placed_planet();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player.clone());
        state.exhaust_planet(planet.clone());
        let mut table = Table::new();

        primary(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            &player,
            &card("Diplomacy"),
        )
        .unwrap();

        assert!(state.system_state(&system).command_tokens.contains(&other));
        assert!(!state.exhausted_planets.contains(&planet));
    }

    #[test]
    fn politics_moves_the_speaker_and_draws_two() {
        let mut state = game(&["a", "b"]);
        let player = PlayerId::new("a");
        let before = state.player(&player).unwrap().action_cards.len();
        let mut table = Table::new();

        primary(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            &player,
            &card("Politics"),
        )
        .unwrap();

        assert_eq!(state.speaker, PlayerId::new("b"));
        assert_eq!(
            state.player(&player).unwrap().action_cards.len(),
            before + 2
        );
    }

    // P1-e: speaker choice surface aligned to the oracle (engine/strategy.py:736–748 @ 37061c5).

    /// One recorded option surface: id, kind and label.
    type OptionSurface = (String, String, String);
    /// One recorded choice: prompt plus its offered options in order.
    type RecordedAsk = (String, Vec<OptionSurface>);

    /// A decider that records every choice it is asked to answer, answering from a queue of ids.
    struct SpeakerRecording {
        wanted: std::collections::VecDeque<String>,
        seen: std::rc::Rc<std::cell::RefCell<Vec<RecordedAsk>>>,
    }

    impl SpeakerRecording {
        fn new(wanted: &[&str]) -> (Self, std::rc::Rc<std::cell::RefCell<Vec<RecordedAsk>>>) {
            let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            (
                Self {
                    wanted: wanted.iter().map(|id| (*id).to_owned()).collect(),
                    seen: seen.clone(),
                },
                seen,
            )
        }

        fn record(&self, choice: &crate::choice::Choice) {
            self.seen.borrow_mut().push((
                choice.prompt.clone(),
                choice
                    .options
                    .iter()
                    .map(|option| (option.id.clone(), option.kind.clone(), option.label.clone()))
                    .collect(),
            ));
        }
    }

    impl crate::choice::Decider for SpeakerRecording {
        fn choose(
            &mut self,
            choice: &crate::choice::Choice,
        ) -> Result<crate::choice::ChoiceOption, crate::choice::IllegalChoice> {
            self.record(choice);
            let Some(wanted) = self.wanted.pop_front() else {
                return Err(crate::choice::IllegalChoice::ScriptDiverged {
                    player: choice.player.clone(),
                    wanted: "<script exhausted>".to_owned(),
                    offered: choice.ids().into_iter().map(str::to_owned).collect(),
                });
            };
            choice.option(&wanted).cloned().ok_or_else(|| {
                crate::choice::IllegalChoice::ScriptDiverged {
                    player: choice.player.clone(),
                    wanted,
                    offered: choice.ids().into_iter().map(str::to_owned).collect(),
                }
            })
        }
    }

    #[test]
    fn the_speaker_choice_offers_factions_in_the_oracle_wording() {
        // engine/strategy.py:736–748 asks "who becomes speaker" with one option per candidate,
        // id = the faction name, label "{faction} becomes speaker". Rust presented seat ids
        // under "choose the new speaker".
        let mut state = game(&["a", "b"]);
        let player = PlayerId::new("a");
        state.player_mut(&player).unwrap().faction = ti4_model::id::FactionId::new("sol");
        let other = PlayerId::new("b");
        state.player_mut(&other).unwrap().faction = ti4_model::id::FactionId::new("hacan");

        let (recorder, seen) = SpeakerRecording::new(&["hacan", "top", "bottom"]);
        let mut table = Table::with_default(Box::new(recorder));
        primary(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            &player,
            &card("Politics"),
        )
        .unwrap();

        let asks = seen.borrow();
        assert_eq!(asks[0].0, "who becomes speaker");
        assert_eq!(
            asks[0].1,
            vec![(
                "hacan".to_owned(),
                "speaker".to_owned(),
                "hacan becomes speaker".to_owned()
            )]
        );
        // The chosen name maps back to the seat that plays it.
        assert_eq!(state.speaker, other);
    }

    #[test]
    fn construction_places_both_primary_structures() {
        let mut state = game(&["a"]);
        let player = PlayerId::new("a");
        let (system, planet) = a_placed_planet();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player.clone());
        let mut table = Table::new();

        primary(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            &player,
            &card("Construction"),
        )
        .unwrap();

        assert_eq!(
            state
                .system_state(&system)
                .on_planet(&planet)
                .iter()
                .filter(|unit| unit.owner == player && unit.type_id.as_str() == "pds")
                .count(),
            2
        );
    }

    #[test]
    fn technology_researches_the_free_technology() {
        let mut state = game(&["a"]);
        let player = PlayerId::new("a");
        let before = state.player(&player).unwrap().technologies.len();
        let mut table = Table::new();

        primary(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            &player,
            &card("Technology"),
        )
        .unwrap();

        assert_eq!(
            state.player(&player).unwrap().technologies.len(),
            before + 1
        );
    }

    #[test]
    fn imperial_draws_a_secret_without_mecatol() {
        let mut state = game(&["a"]);
        let player = PlayerId::new("a");
        let before = state.player(&player).unwrap().secret_objectives.len();
        let mut table = Table::with_default(Box::new(crate::choice::AlwaysDecline));

        primary(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            &player,
            &card("Imperial"),
        )
        .unwrap();

        assert_eq!(
            state.player(&player).unwrap().secret_objectives.len(),
            before + 1
        );
    }

    #[test]
    fn the_simple_secondaries_apply_their_effects() {
        let content = ContentStore::embedded();
        let player = PlayerId::new("a");

        let mut diplomacy = game(&["a"]);
        let (system, planet) = a_placed_planet();
        diplomacy
            .system_mut(&system)
            .set_control(planet.clone(), player.clone());
        diplomacy.exhaust_planet(planet.clone());
        secondary(
            &mut diplomacy,
            content,
            POK,
            None,
            &mut Table::new(),
            &player,
            &card("Diplomacy"),
        )
        .unwrap();
        assert!(!diplomacy.exhausted_planets.contains(&planet));

        let mut politics = game(&["a"]);
        let before_cards = politics.player(&player).unwrap().action_cards.len();
        secondary(
            &mut politics,
            content,
            POK,
            None,
            &mut Table::new(),
            &player,
            &card("Politics"),
        )
        .unwrap();
        assert_eq!(
            politics.player(&player).unwrap().action_cards.len(),
            before_cards + 2
        );

        let mut construction = game(&["a"]);
        construction
            .system_mut(&system)
            .set_control(planet.clone(), player.clone());
        secondary(
            &mut construction,
            content,
            POK,
            None,
            &mut Table::new(),
            &player,
            &card("Construction"),
        )
        .unwrap();
        assert_eq!(
            construction.system_state(&system).on_planet(&planet).len(),
            1
        );

        let mut trade = game(&["a"]);
        secondary(
            &mut trade,
            content,
            POK,
            None,
            &mut Table::new(),
            &player,
            &card("Trade"),
        )
        .unwrap();
        assert_eq!(
            trade.player(&player).unwrap().commodities,
            commodity_limit(&trade, content, &player)
        );

        let mut imperial = game(&["a"]);
        let before_secrets = imperial.player(&player).unwrap().secret_objectives.len();
        secondary(
            &mut imperial,
            content,
            POK,
            None,
            &mut Table::new(),
            &player,
            &card("Imperial"),
        )
        .unwrap();
        assert_eq!(
            imperial.player(&player).unwrap().secret_objectives.len(),
            before_secrets + 1
        );
    }

    #[test]
    fn warfare_secondary_produces_in_the_home_system() {
        let mut state = game(&["a"]);
        let player = PlayerId::new("a");
        let (system, planet) = a_placed_planet();
        state.player_mut(&player).unwrap().home_system = Some(system.clone());
        state.player_mut(&player).unwrap().trade_goods = 10;
        state
            .system_mut(&system)
            .set_control(planet.clone(), player.clone());
        put_on_planet(&mut state, &system, &planet, "spacedock", &player, 1);
        let before = state.system_state(&system).units.len()
            + state.system_state(&system).on_planet(&planet).len();

        secondary(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut Table::new(),
            &player,
            &card("Warfare"),
        )
        .unwrap();

        let after = state.system_state(&system).units.len()
            + state.system_state(&system).on_planet(&planet).len();
        assert!(after > before);
    }

    #[test]
    fn thunders_edge_construction_uses_its_distinct_two_structure_primary() {
        let mut state = game(&["a"]);
        let player = PlayerId::new("a");
        let (system, planet) = a_placed_planet();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player.clone());

        let result = primary(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut Table::new(),
            &player,
            "te4construction",
        )
        .unwrap();

        assert_eq!(result, Ability::Resolved);
        assert_eq!(state.system_state(&system).on_planet(&planet).len(), 2);
    }

    #[test]
    fn thunders_edge_warfare_returns_a_free_tactical_directive() {
        let mut state = game(&["a"]);
        let player = PlayerId::new("a");
        let hub = plain_hub();
        state.phase = ti4_model::state::Phase::Action;
        let tokens = state.player(&player).unwrap().tactic_tokens;

        let result = primary(
            &mut state,
            ContentStore::embedded(),
            POK,
            Some(&hub.galaxy),
            &mut Table::new(),
            &player,
            "te6warfare",
        )
        .unwrap();

        assert!(matches!(result, Ability::FreeTactical(_)));
        assert_eq!(state.player(&player).unwrap().tactic_tokens, tokens);
    }
}
