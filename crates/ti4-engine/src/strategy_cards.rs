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

/// The 52.4 token gain, as a question: one choice per token, each into a pool of the
/// player's. The status phase (81.5) asks it through a window; action cards that say
/// "gain command tokens" without a pool — Summit gains two — ask it straight from their
/// timing context.
pub(crate) fn gain_tokens(
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
        state.gain_token(player, pool, 1);
    }
    Ok(())
}

/// Whether a follower may buy command tokens with influence in the Leadership window.
///
/// Oracle parity (`_buy_tokens_with_influence`): affordability *is* the gate. A seat that
/// cannot pay three influence makes zero decisions; there is no separate decline/follow
/// prompt for unaffordable followers.
#[must_use]
pub fn leadership_influence_eligible(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> bool {
    crate::production::available(state, content, sources, player, Spend::Influence)
        >= INFLUENCE_PER_TOKEN
}

/// The 52.3 purchase loop: one token per three influence, for as long as the seat wants and
/// can pay.
///
/// Oracle identity (`_buy_tokens_with_influence`): the question offers `no` ("spend nothing
/// further") and `yes` ("spend 3 influence"), both kind `strategy`; any non-`yes` answer stops
/// this seat entirely; each accepted token is paid through the ordinary ask-based payment loop.
fn buy_tokens_with_influence(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
) -> Result<(), IllegalChoice> {
    while leadership_influence_eligible(state, content, sources, player) {
        let answer = ask(
            state,
            content,
            sources,
            galaxy,
            table,
            &influence_purchase_choice(player),
        )?;
        if answer.id != "yes" {
            return Ok(());
        }
        if !pay_influence_and_gain_one(state, content, sources, galaxy, table, player)? {
            return Ok(());
        }
    }
    Ok(())
}

/// Same purchase loop after the strategic-secondary window already recorded a `yes`.
///
/// The window's question carries the oracle identity, so this variant pays for the accepted
/// first token and then re-asks while the seat is still affordable — never asking twice in a row.
fn buy_tokens_first_yes_assumed(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
) -> Result<(), IllegalChoice> {
    if !pay_influence_and_gain_one(state, content, sources, galaxy, table, player)? {
        return Ok(());
    }
    while leadership_influence_eligible(state, content, sources, player) {
        let answer = ask(
            state,
            content,
            sources,
            galaxy,
            table,
            &influence_purchase_choice(player),
        )?;
        if answer.id != "yes" {
            return Ok(());
        }
        if !pay_influence_and_gain_one(state, content, sources, galaxy, table, player)? {
            return Ok(());
        }
    }
    Ok(())
}

fn influence_purchase_choice(player: &PlayerId) -> Choice {
    // Oracle wording and ids (`_buy_tokens_with_influence`): both options kind `strategy`.
    Choice::new(
        player.clone(),
        format!("spend {INFLUENCE_PER_TOKEN} influence for a command token"),
        vec![
            ChoiceOption::labelled(
                "no",
                crate::strategy::STRATEGY_KIND,
                "spend nothing further",
            ),
            ChoiceOption::labelled(
                "yes",
                crate::strategy::STRATEGY_KIND,
                format!("spend {INFLUENCE_PER_TOKEN} influence"),
            ),
        ],
    )
}

/// Pay three influence through the ask-based payment loop and gain one command token.
///
/// Returns `false` when the payment could not be completed; per the oracle a failed payment
/// stops the whole purchase loop, so callers must treat it as terminal.
fn pay_influence_and_gain_one(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
) -> Result<bool, IllegalChoice> {
    if !crate::production::pay_seeing(
        state,
        content,
        sources,
        galaxy,
        table,
        player,
        INFLUENCE_PER_TOKEN,
        Spend::Influence,
    )? {
        return Ok(false);
    }
    gain_tokens(state, content, sources, galaxy, table, player, 1)?;
    Ok(true)
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

/// Jol-Nar's Specialist Compounds: exhaust a specialty planet instead of paying, and research a
/// technology of that colour.
///
/// Returns whether it took over the research. Two questions rather than one: which specialty to
/// spend, and then which technology of its colour -- because the colour is a consequence of the
/// first answer, and offering the pair together would ask for a combination the player cannot see
/// the shape of. Declining the first falls through to the ordinary paid research.
fn specialist_compounds(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
) -> Result<bool, IllegalChoice> {
    let specialties =
        crate::breakthroughs::specialty_research_planets(state, content, sources, player);
    if specialties.is_empty() {
        return Ok(false);
    }
    // 22.3: only offer a specialty that can actually buy something. A colour whose technologies
    // are all researched already would exhaust a planet for nothing.
    let open = crate::technology::researchable(state, content, sources, player);
    let usable: Vec<(PlanetId, &'static str)> = specialties
        .into_iter()
        .filter(|(_, colour)| {
            open.iter().any(|id| {
                crate::technology::colour_type(content, id).is_some_and(|had| had == *colour)
            })
        })
        .collect();
    if usable.is_empty() {
        return Ok(false);
    }

    let choice = Choice::new(
        player.clone(),
        "Specialist Compounds: exhaust a specialty instead of paying",
        usable
            .iter()
            .map(|(planet, colour)| {
                ChoiceOption::labelled(
                    format!("{planet}:{colour}"),
                    "planet",
                    format!("exhaust {planet} to research {} technology", colour.to_lowercase()),
                )
            })
            .chain(std::iter::once(ChoiceOption::decline()))
            .collect(),
    );
    let answer = ask(state, content, sources, galaxy, table, &choice)?;
    if answer.is_decline() {
        return Ok(false);
    }
    let Some((planet, colour)) = usable
        .iter()
        .find(|(planet, colour)| answer.id == format!("{planet}:{colour}"))
    else {
        return Ok(false);
    };

    // "must research a technology of that color" -- so the second offer carries no decline. The
    // planet is exhausted first: the price is paid whichever technology is chosen.
    let of_colour: Vec<TechnologyId> = open
        .into_iter()
        .filter(|id| {
            crate::technology::colour_type(content, id).is_some_and(|had| had == *colour)
        })
        .collect();
    state.exhaust_planet(planet.clone());
    let choice = Choice::new(
        player.clone(),
        format!("research which {} technology", colour.to_lowercase()),
        of_colour
            .iter()
            .map(|id| {
                ChoiceOption::labelled(
                    id.to_string(),
                    RESEARCH_KIND,
                    crate::technology::name(content, id),
                )
            })
            .collect(),
    );
    let answer = ask(state, content, sources, galaxy, table, &choice)?;
    crate::technology::research(state, content, sources, player, &TechnologyId::new(answer.id));
    Ok(true)
}

/// Doctor Sucaban: infantry removed from the board pay for research, one resource each.
///
/// > When a player spends resources to research: You may exhaust this card to allow that player to
/// > remove any number of their infantry from the game board. For each unit removed, reduce the
/// > resources spent by 1.
///
/// Two seats are involved and they are usually not the same one. The Jol-Nar player owns the card
/// and decides whether to exhaust it; the *researching* player decides how many of their own
/// infantry to give up. Asking the owner first is what the card says and also what keeps the
/// researcher from being offered a discount nobody has agreed to pay for.
///
/// Returns the reduced cost. Infantry are removed one at a time, each naming where it comes from:
/// units are interchangeable but their *locations* are not, and a seat losing a garrison it needed
/// is a real decision rather than an accounting detail.
fn doctor_sucaban(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    cost: i64,
) -> Result<i64, IllegalChoice> {
    if cost <= 0 {
        return Ok(cost);
    }
    let agent = ti4_model::id::LeaderId::new("jolnaragent");
    let Some(owner) = state
        .players
        .iter()
        .find(|seat| {
            seat.leaders.get(&agent) == Some(&ti4_model::state::LeaderStatus::Readied)
        })
        .map(|seat| seat.id.clone())
    else {
        return Ok(cost);
    };
    if infantry_sites(state, player).is_empty() {
        return Ok(cost); // nothing to trade, so nothing to ask about
    }

    let choice = Choice::new(
        owner.clone(),
        format!("Doctor Sucaban: exhaust to let {player} trade infantry for research"),
        vec![
            ChoiceOption::labelled("yes".to_owned(), "leader", "exhaust the agent".to_owned()),
            ChoiceOption::decline(),
        ],
    );
    let answer = ask(state, content, sources, galaxy, table, &choice)?;
    if answer.is_decline() || !crate::leaders::exhaust(state, &owner, &agent) {
        return Ok(cost);
    }

    let mut reduced = cost;
    while reduced > 0 {
        let sites = infantry_sites(state, player);
        if sites.is_empty() {
            break;
        }
        let choice = Choice::new(
            player.clone(),
            "remove an infantry to reduce the cost by 1",
            sites
                .iter()
                .map(|(system, planet)| {
                    let (id, label) = planet.as_ref().map_or_else(
                        || {
                            (
                                format!("{system}:"),
                                format!("an infantry in space at {system}"),
                            )
                        },
                        |planet| {
                            (
                                format!("{system}:{planet}"),
                                format!("an infantry on {planet}"),
                            )
                        },
                    );
                    ChoiceOption::labelled(id, "unit", label)
                })
                .chain(std::iter::once(ChoiceOption::decline()))
                .collect(),
        );
        let answer = ask(state, content, sources, galaxy, table, &choice)?;
        if answer.is_decline() {
            break;
        }
        let Some((system, planet)) = sites.into_iter().find(|(system, planet)| {
            let key = planet
                .as_ref()
                .map_or_else(|| format!("{system}:"), |planet| format!("{system}:{planet}"));
            key == answer.id
        }) else {
            break;
        };
        // Remove the unit that is actually standing there rather than constructing one to match:
        // a faction's infantry carries its own type id (Sol's Spec Ops, Letnev's ...), and building
        // the wrong id would remove nothing while still granting the discount.
        let removed = state.board.get_mut(&system).is_some_and(|here| {
            let stack = match planet.as_ref() {
                Some(planet) => here.planet_units.get_mut(planet),
                None => Some(&mut here.units),
            };
            stack.is_some_and(|stack| {
                stack
                    .iter()
                    .position(|unit| {
                        unit.owner == *player && unit.type_id.as_str().contains("infantry")
                    })
                    .is_some_and(|at| {
                        stack.remove(at);
                        true
                    })
            })
        });
        if !removed {
            break;
        }
        reduced -= 1;
    }
    Ok(reduced)
}

/// Where this player has infantry: `(system, Some(planet))` on the ground, `(system, None)` in
/// space. One entry per location, not per unit -- a stack of three offers one place to take from.
fn infantry_sites(state: &GameState, player: &PlayerId) -> Vec<(SystemId, Option<PlanetId>)> {
    let mut sites = Vec::new();
    for (system, here) in &state.board {
        if here
            .units
            .iter()
            .any(|unit| unit.owner == *player && unit.type_id.as_str().contains("infantry"))
        {
            sites.push((system.clone(), None));
        }
        for (planet, standing) in &here.planet_units {
            if standing
                .iter()
                .any(|unit| unit.owner == *player && unit.type_id.as_str().contains("infantry"))
            {
                sites.push((system.clone(), Some(planet.clone())));
            }
        }
    }
    sites
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
    // Jol-Nar's Specialist Compounds pays with a planet instead of resources, so it is offered
    // before the affordability gate -- a seat that cannot pay the price can still research this
    // way, and testing affordability first would close the window the card exists to open.
    if specialist_compounds(state, content, sources, galaxy, table, player)? {
        return Ok(());
    }
    // Doctor Sucaban discounts the bill, so he is asked before the affordability gate: the whole
    // point of the card is to make a research affordable that was not.
    let cost = doctor_sucaban(state, content, sources, galaxy, table, player, cost)?;
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
        // engine/strategy.py:633–654 offers every exhausted controlled planet and nothing else:
        // no decline, no done — each of the `maximum` iterations is a forced choice until the
        // player has none left.
        let choice = Choice::new(
            player.clone(),
            "ready which planet",
            controlled
                .iter()
                .map(|planet| {
                    ChoiceOption::labelled(planet.to_string(), "ready", format!("ready {planet}"))
                })
                .collect(),
        );
        let answer = ask(state, content, sources, galaxy, table, &choice)?;
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

pub(crate) fn structure_options(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    only_pds: bool,
) -> Vec<ChoiceOption> {
    state
        .controlled_planets(player)
        // Space stations rule 5: no structures on a space station either.
        .into_iter()
        .filter(|(_, planet)| {
            !ti4_content::galaxy::is_space_station(content, planet.as_str(), sources)
        })
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

pub(crate) fn place_structure(
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

    // Minister of Industry: "When the owner of this card places a space dock in a system, their
    // units in that system may use their PRODUCTION abilities." A space dock specifically, so a
    // PDS placed under the same law produces nothing.
    if kind.contains("space_dock")
        && crate::laws::industry_produces_on_placement(state, player)
    {
        produce_all(state, content, sources, galaxy, table, player, &system)?;
    }
    Ok(Some(system))
}

/// Use every PRODUCTION ability in a system, until the player stops buying.
///
/// Minister of Industry grants a production window rather than a single unit, so this keeps
/// offering until `produce_one` finds nothing to sell or the player declines.
fn produce_all(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    system: &SystemId,
) -> Result<(), IllegalChoice> {
    let limit = crate::production::capacity(state, content, sources, player, system);
    for _ in 0..limit.max(0) {
        if !crate::production::produce_one(
            state, content, sources, galaxy, table, player, system,
        )? {
            break;
        }
    }
    Ok(())
}

pub(crate) fn commodity_limit(state: &GameState, content: &ContentStore, player: &PlayerId) -> i32 {
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
        // engine/strategy.py:206–246 _replenishable: seats below their printed commodity value,
        // generic factions excluded (they have no printed value to replenish to). The limit
        // lookup already yields zero for them, but the oracle states it outright.
        remaining.retain(|other| {
            state.player(other).is_some_and(|seat| {
                seat.faction.as_str() != "generic"
                    && seat.commodities < commodity_limit(state, content, other)
            })
        });
        if remaining.is_empty() {
            break;
        }
        // The oracle names each option after the faction — its player ids *are* factions — so a
        // duplicate-faction table is first-match-in-seating-order here, as in the speaker ask.
        let named: Vec<(String, PlayerId)> = remaining
            .iter()
            .map(|other| (crate::promissory::faction_name(state, other), other.clone()))
            .collect();
        let choice = Choice::new(
            player.clone(),
            "let another player replenish commodities",
            named
                .iter()
                .map(|(name, _)| {
                    ChoiceOption::labelled(
                        name.clone(),
                        "replenish",
                        format!("{name} replenishes commodities"),
                    )
                })
                .chain(std::iter::once(ChoiceOption::labelled(
                    "done",
                    crate::choice::DECLINE_KIND,
                    "nobody else replenishes",
                )))
                .collect(),
        );
        let answer = ask(state, content, sources, galaxy, table, &choice)?;
        if answer.is_decline() {
            break;
        }
        let chosen = answer.id.clone();
        let other = named
            .iter()
            .find(|(name, _)| *name == chosen)
            .map(|(_, seat)| seat.clone())
            .ok_or_else(|| crate::choice::IllegalChoice::NotOffered {
                player: player.clone(),
                offered: named.iter().map(|(name, _)| name.clone()).collect(),
                chosen,
            })?;
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
    gain_tokens(state, content, sources, galaxy, table, player, 1)?;
    // "Then, the active player can redistribute their command tokens." A separate sentence from the
    // gain above and a separate decision: the token just recovered goes into a pool of the player's
    // choice, and *then* every token they hold may be moved between pools.
    redistribute_tokens(state, content, sources, galaxy, table, player)?;
    Ok(())
}

/// Warfare's second half: move any number of command tokens between your own pools.
///
/// Offered one move at a time until the player declines, which is what "redistribute" allows and
/// what keeps each move a decision a policy can see. Bounded by the tokens actually held, so a
/// decider that never declines still terminates.
pub(crate) fn redistribute_tokens(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
) -> Result<Ability, IllegalChoice> {
    use ti4_model::state::TokenPool;
    const POOLS: [(TokenPool, &str); 3] = [
        (TokenPool::Tactic, "tactic"),
        (TokenPool::Fleet, "fleet"),
        (TokenPool::Strategic, "strategy"),
    ];
    let held = |state: &GameState, pool: TokenPool| -> i32 {
        state.player(player).map_or(0, |seat| match pool {
            TokenPool::Tactic => seat.tactic_tokens,
            TokenPool::Fleet => seat.fleet_tokens,
            TokenPool::Strategic => seat.strategic_tokens,
        })
    };
    let total: i32 = POOLS.iter().map(|(pool, _)| held(state, *pool)).sum();
    for _ in 0..total.max(0) {
        let mut options = Vec::new();
        for (from, from_name) in POOLS {
            if held(state, from) <= 0 {
                continue;
            }
            for (to, to_name) in POOLS {
                if from_name == to_name {
                    continue;
                }
                let _ = to;
                options.push(ChoiceOption::labelled(
                    format!("move|{from_name}|{to_name}"),
                    "redistribute",
                    format!("move a token from {from_name} to {to_name}"),
                ));
            }
        }
        if options.is_empty() {
            break;
        }
        options.push(ChoiceOption::decline());
        let choice = Choice::new(
            player.clone(),
            "Warfare: redistribute your command tokens",
            options,
        );
        let answer = ask(state, content, sources, galaxy, table, &choice)?;
        if answer.is_decline() {
            break;
        }
        let Some((from_name, to_name)) = answer
            .id
            .strip_prefix("move|")
            .and_then(|rest| rest.split_once('|'))
        else {
            break;
        };
        let pool_of = |name: &str| {
            POOLS
                .iter()
                .find(|(_, label)| *label == name)
                .map(|(pool, _)| *pool)
        };
        let (Some(from), Some(to)) = (pool_of(from_name), pool_of(to_name)) else {
            break;
        };
        if let Some(seat) = state.player_mut(player)
            && seat.spend_token(from)
        {
            seat.gain_token_uncapped(to, 1); // moved, not gained
        }
    }
    Ok(Ability::Resolved)
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
        // The window's `yes` already asked the oracle question; pay for it and continue.
        "Leadership" => {
            buy_tokens_first_yes_assumed(state, content, sources, galaxy, table, player)?;
        }
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

    /// Warfare's primary redistributes tokens between pools, which is its second sentence.
    ///
    /// "The active player removes any one of their command tokens from the game board. Then, that
    /// player gains that command token... Then, the active player can redistribute their command
    /// tokens." The recall was implemented and the redistribution was not, so a Warfare was worth
    /// one token and never the pool shuffle that makes the card interesting.
    #[test]
    fn warfare_lets_the_player_move_tokens_between_pools() {
        use ti4_model::state::TokenPool;
        let content = ContentStore::embedded();
        let player = PlayerId::new("a");
        let mut state = game(&["a"]);
        if let Some(seat) = state.player_mut(&player) {
            seat.tactic_tokens = 1;
            seat.fleet_tokens = 0;
            seat.strategic_tokens = 0;
        }

        // One move then stop: the first option moves tactic -> fleet, the second answer declines.
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new(vec![
            "move|tactic|fleet".to_owned(),
        ])));
        redistribute_tokens(
            &mut state,
            content,
            ti4_model::content_types::DEFAULT,
            None,
            &mut table,
            &player,
        )
        .expect("redistribution resolves");

        let seat = state.player(&player).expect("seated");
        assert_eq!(seat.tactic_tokens, 0, "the token left the tactic pool");
        assert_eq!(seat.fleet_tokens, 1, "and arrived in the fleet pool");
        assert_eq!(
            seat.tactic_tokens + seat.fleet_tokens + seat.strategic_tokens,
            1,
            "redistribution moves tokens, it does not mint them"
        );
        let _ = TokenPool::Tactic;
    }
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

    /// Specialist Compounds researches by exhausting a specialty, with nothing to spend.
    ///
    /// The seat is deliberately broke: no trade goods, no other planets. That is the whole point of
    /// the card, and it is also what proves the research did not quietly go through the ordinary
    /// paid path -- which would have found nothing to pay with and done nothing at all.
    #[test]
    fn specialist_compounds_researches_by_exhausting_a_specialty() {
        let content = ContentStore::embedded();
        let sources = ti4_model::content_types::DEFAULT;
        let player = PlayerId::new("a");
        let mut state = game(&["a"]);

        let (specialty, colour) = ti4_content::galaxy::all_planets(content, sources)
            .iter()
            .find_map(|(id, record)| {
                record.tech_specialties().first().and_then(|specialty| {
                    let upper = specialty.to_ascii_uppercase();
                    crate::technology::COLOURS
                        .iter()
                        .find(|c| ***c == *upper.as_str())
                        .map(|colour| (PlanetId::new(*id), *colour))
                })
            })
            .expect("the corpus has a technology specialty");
        let system = ti4_content::galaxy::planet(content, specialty.as_str(), sources)
            .and_then(|record| record.system_id().map(SystemId::new))
            .expect("it sits on a tile");

        state.board.entry(system.clone()).or_default();
        if let Some(here) = state.board.get_mut(&system) {
            here.set_control(specialty.clone(), player.clone());
        }
        if let Some(seat) = state.player_mut(&player) {
            seat.breakthrough = Some(ti4_model::id::BreakthroughId::new("jolnarbt"));
            seat.trade_goods = 0;
        }
        let before = state.player(&player).unwrap().technologies.clone();

        // Take the first option at every question: the specialty, then a technology of its colour.
        let mut table = Table::with_default(Box::new(crate::choice::FirstOption));
        secondary(
            &mut state,
            content,
            sources,
            None,
            &mut table,
            &player,
            &card("Technology"),
        )
        .unwrap();

        let seat = state.player(&player).unwrap();
        let gained: Vec<&TechnologyId> = seat
            .technologies
            .iter()
            .filter(|id| !before.contains(*id))
            .collect();
        assert_eq!(gained.len(), 1, "exactly one technology researched");
        assert_eq!(
            crate::technology::colour_type(content, gained[0]),
            Some(colour),
            "and it is the colour of the specialty that paid for it"
        );
        assert!(
            state.exhausted_planets.contains(&specialty),
            "the specialty is exhausted"
        );
        assert_eq!(seat.trade_goods, 0, "and nothing was spent");
    }

    /// Doctor Sucaban lets a broke seat research by spending infantry instead of resources.
    ///
    /// The agent belongs to a *different* player, which is the shape of the card and the reason the
    /// owner is asked first. The researcher holds nothing spendable, so the four infantry are
    /// demonstrably what paid for it.
    #[test]
    fn doctor_sucaban_trades_infantry_for_research() {
        let content = ContentStore::embedded();
        let sources = ti4_model::content_types::DEFAULT;
        let (researcher, owner) = (PlayerId::new("a"), PlayerId::new("b"));
        let mut state = game(&["a", "b"]);

        let system = SystemId::new(crate::fixtures::plain_systems(1)[0].clone());
        state.board.entry(system.clone()).or_default();
        crate::fixtures::put(&mut state, &system, "infantry", &researcher, 4);
        if let Some(seat) = state.player_mut(&researcher) {
            seat.trade_goods = 0;
        }
        if let Some(seat) = state.player_mut(&owner) {
            seat.leaders.insert(
                ti4_model::id::LeaderId::new("jolnaragent"),
                ti4_model::state::LeaderStatus::Readied,
            );
        }
        let before = state.player(&researcher).unwrap().technologies.len();

        // Yes to everything: exhaust the agent, then take an infantry at each offer.
        let mut table = Table::with_default(Box::new(crate::choice::FirstOption));
        secondary(
            &mut state,
            content,
            sources,
            None,
            &mut table,
            &researcher,
            &card("Technology"),
        )
        .unwrap();

        assert_eq!(
            state.player(&researcher).unwrap().technologies.len(),
            before + 1,
            "the research happened with nothing to spend"
        );
        assert_eq!(
            state.system_state(&system).units_of(&researcher).len(),
            0,
            "and all four infantry paid for it"
        );
        assert_eq!(
            state
                .player(&owner)
                .unwrap()
                .leaders
                .get(&ti4_model::id::LeaderId::new("jolnaragent")),
            Some(&ti4_model::state::LeaderStatus::Exhausted),
            "the agent is exhausted"
        );
        assert_eq!(
            state.player(&researcher).unwrap().trade_goods,
            0,
            "and no resources were spent"
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

    /// Two distinct placed planets, in the order `controlled_planets` yields them.
    fn two_controlled_candidates() -> Vec<(SystemId, PlanetId)> {
        let content = ContentStore::embedded();
        ti4_content::galaxy::all_planets(content, POK)
            .iter()
            .filter(|(_, planet)| planet.system_id().is_some() && !planet.is_placed_during_play())
            .map(|(id, planet)| {
                (
                    SystemId::new(planet.system_id().unwrap_or("18")),
                    PlanetId::new(*id),
                )
            })
            .take(2)
            .collect::<Vec<_>>()
    }

    #[test]
    fn ready_planets_uses_the_oracle_wording_and_offers_no_decline() {
        // engine/strategy.py:633–654 asks "ready which planet" with one option per exhausted
        // controlled planet — label "ready {p}" and no decline/done option; every iteration is a
        // forced choice until the count is spent or nothing stays exhausted.
        let mut state = game(&["a"]);
        let player = PlayerId::new("a");
        let candidates: Vec<(SystemId, PlanetId)> = two_controlled_candidates();
        for (system, planet) in &candidates {
            state
                .system_mut(system)
                .set_control(planet.clone(), player.clone());
            state.exhaust_planet(planet.clone());
        }

        let script: Vec<&str> = candidates
            .iter()
            .map(|(_, planet)| planet.as_str())
            .collect();
        let (recorder, seen) = SpeakerRecording::new(&script);
        let mut table = Table::with_default(Box::new(recorder));

        ready_planets(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            &player,
            2,
        )
        .unwrap();

        let asks = seen.borrow();
        assert_eq!(asks.len(), 2, "one forced choice per planet");
        for ask in &*asks {
            assert_eq!(ask.0, "ready which planet");
            assert!(
                !ask.1.iter().any(|(id, _, _)| id == "decline"),
                "the oracle offers no decline here"
            );
        }
        let p1 = &candidates[0].1;
        let p2 = &candidates[1].1;
        assert_eq!(
            asks[0].1,
            vec![
                (p1.to_string(), "ready".to_owned(), format!("ready {p1}")),
                (p2.to_string(), "ready".to_owned(), format!("ready {p2}"))
            ]
        );
        assert_eq!(
            asks[1].1,
            vec![(p2.to_string(), "ready".to_owned(), format!("ready {p2}"))]
        );
        for (_, planet) in &candidates {
            assert!(!state.exhausted_planets.contains(planet));
        }
    }

    #[test]
    fn free_trade_replenishment_uses_the_oracle_identity() {
        // engine/strategy.py:219–246 asks "let another player replenish commodities" with one
        // option per eligible seat — id = the faction name, label "{name} replenishes
        // commodities" — plus ("done", "decline", "nobody else replenishes"). Generic-faction
        // seats are never offered.
        let content = ContentStore::embedded();
        let mut state = game(&["a", "b", "c"]);
        let player = PlayerId::new("a");
        let other = PlayerId::new("b");
        state.player_mut(&player).unwrap().faction = ti4_model::id::FactionId::new("sol");
        state.player_mut(&other).unwrap().faction = ti4_model::id::FactionId::new("hacan");
        // "c" keeps its default generic faction: never offered, whatever its commodities.
        let limit = commodity_limit(&state, content, &other);
        assert!(limit > 0, "the test needs a faction with a printed value");
        state.player_mut(&other).unwrap().commodities = 0;

        let (recorder, seen) = SpeakerRecording::new(&["hacan"]);
        let mut table = Table::with_default(Box::new(recorder));

        trade_primary(&mut state, content, POK, None, &mut table, &player).unwrap();

        let asks = seen.borrow();
        assert_eq!(asks.len(), 1, "one grant, then nothing left to offer");
        assert_eq!(asks[0].0, "let another player replenish commodities");
        assert_eq!(
            asks[0].1,
            vec![
                (
                    "hacan".to_owned(),
                    "replenish".to_owned(),
                    "hacan replenishes commodities".to_owned()
                ),
                (
                    "done".to_owned(),
                    "decline".to_owned(),
                    "nobody else replenishes".to_owned()
                )
            ]
        );
        assert_eq!(state.player(&other).unwrap().commodities, limit);
    }

    #[test]
    fn leadership_influence_purchase_uses_the_oracle_questions() {
        // engine/strategy.py:_leadership_primary = gain three tokens, then the
        // _buy_tokens_with_influence loop: "spend 3 influence for a command token" with
        // ("no","strategy","spend nothing further") and ("yes","strategy","spend 3
        // influence"); an accepted token is paid through the ordinary payment loop — which,
        // with trade goods as the only asset, offers one lone option per step and therefore
        // never asks (oracle pay() auto-picks) — then one pool choice follows.
        let content = ContentStore::embedded();
        let mut state = game(&["a", "b"]);
        let actor = PlayerId::new("a");
        state.player_mut(&actor).unwrap().trade_goods = 6; // exactly two purchasable tokens
        let before = state.player(&actor).unwrap().clone();

        let (recorder, seen) = SpeakerRecording::new(&[
            "fleet_tokens",
            "strategic_tokens",
            "tactic_tokens",
            "yes",
            "fleet_tokens",
            "no",
        ]);
        let mut table = Table::with_default(Box::new(recorder));

        primary(
            &mut state,
            content,
            POK,
            None,
            &mut table,
            &actor,
            &card("Leadership"),
        )
        .unwrap();

        let asks = seen.borrow();
        assert_eq!(
            asks.len(),
            6,
            "three pool gains, a silently paid purchase plus its pool gain, then the loop stops on no"
        );
        for index in [0usize, 1, 2] {
            assert_eq!(asks[index].0, "gain a command token into which pool");
        }
        // Oracle pay(): with trade goods as the only asset every step is a lone option and no
        // payment question reaches the table at all.
        assert!(!asks.iter().any(|(prompt, _)| prompt.starts_with("pay ")));
        for index in [3usize, 5] {
            assert_eq!(
                asks[index].0, "spend 3 influence for a command token",
                "re-asked while still affordable"
            );
            assert_eq!(
                asks[index].1,
                vec![
                    (
                        "no".to_owned(),
                        "strategy".to_owned(),
                        "spend nothing further".to_owned()
                    ),
                    (
                        "yes".to_owned(),
                        "strategy".to_owned(),
                        "spend 3 influence".to_owned()
                    )
                ]
            );
        }
        assert_eq!(asks[4].0, "gain a command token into which pool");
        let seat = state.player(&actor).unwrap();
        assert_eq!(seat.fleet_tokens, before.fleet_tokens + 2);
        assert_eq!(seat.strategic_tokens, before.strategic_tokens + 1);
        assert_eq!(seat.tactic_tokens, before.tactic_tokens + 1);
        assert_eq!(seat.trade_goods, 3, "one token was paid in influence");
    }

    #[test]
    fn leadership_influence_purchase_stops_on_no() {
        // The oracle `return`s on any non-yes answer: no payment ask, no purchased token.
        let content = ContentStore::embedded();
        let mut state = game(&["a", "b"]);
        let actor = PlayerId::new("a");
        state.player_mut(&actor).unwrap().trade_goods = 9; // could buy three
        let before = state.clone();

        let (recorder, seen) =
            SpeakerRecording::new(&["fleet_tokens", "strategic_tokens", "tactic_tokens", "no"]);
        let mut table = Table::with_default(Box::new(recorder));
        primary(
            &mut state,
            content,
            POK,
            None,
            &mut table,
            &actor,
            &card("Leadership"),
        )
        .unwrap();

        let asks = seen.borrow();
        assert_eq!(asks.len(), 4, "the first no ends the loop for this seat");
        assert_eq!(asks[3].0, "spend 3 influence for a command token");
        let seat = state.player(&actor).unwrap();
        assert_eq!(seat.trade_goods, before.player(&actor).unwrap().trade_goods);
    }
}
