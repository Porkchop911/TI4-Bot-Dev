//! Strategy-card abilities (LRR 52, 91, 92).
//!
//! All eight ordinary cards are resolved here.  Decisions are made through `ask_seeing`, so a
//! learned policy receives the same public board observation for strategy-card choices that it
//! receives for tactical choices.  Thunder's Edge Construction and Warfare are dispatched by
//! card id because they share printed names with materially different cards.

use std::collections::BTreeSet;

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{PlanetId, PlayerId, SystemId, TechnologyId, UnitTypeId};
use ti4_model::state::{GameState, TokenPool};
use ti4_model::units::Unit;

use crate::choice::{Choice, ChoiceOption, IllegalChoice, Observed, Table};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::preview::{Delta, Preview, Quantity};
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
        // OBS-008d2: each pool option previews the exact count it would reach, read fresh every
        // iteration since an earlier pick in this same ask already changed it.
        let (tactic, fleet, strategic) = state.player(player).map_or((0, 0, 0), |seat| {
            (seat.tactic_tokens, seat.fleet_tokens, seat.strategic_tokens)
        });
        let choice = Choice::new(
            player.clone(),
            "gain a command token into which pool",
            vec![
                ChoiceOption::labelled("tactic_tokens", "pool", "tactic pool").previewed(
                    Preview::certain(vec![Delta::new(
                        Quantity::TacticTokens,
                        i64::from(tactic),
                        i64::from(tactic + 1),
                    )]),
                ),
                ChoiceOption::labelled("fleet_tokens", "pool", "fleet pool").previewed(
                    Preview::certain(vec![Delta::new(
                        Quantity::FleetTokens,
                        i64::from(fleet),
                        i64::from(fleet + 1),
                    )]),
                ),
                ChoiceOption::labelled("strategic_tokens", "pool", "strategy pool").previewed(
                    Preview::certain(vec![Delta::new(
                        Quantity::StrategicTokens,
                        i64::from(strategic),
                        i64::from(strategic + 1),
                    )]),
                ),
            ],
        )
        .contextualized(DecisionContext::new(
            player.clone(),
            DecisionSource::Rule("52.4".to_owned()),
            "gain_command_token",
            state.phase,
            state.round,
        ));
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
        // Xander Alexin Victori III: commodities as trade goods, offered when the purchase opens.
        || crate::factions::keleres::with_agent_granted(state, player, |granted| {
            crate::production::available(granted, content, sources, player, Spend::Influence)
                >= INFLUENCE_PER_TOKEN
        })
        .unwrap_or(false)
}

/// Offer the Keleres agent for a Leadership purchase when the commodities make a token payable.
/// `true` if it was used; the caller closes the window ([`crate::factions::keleres::close_agent_window`]).
fn offer_agent_for_tokens(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
) -> Result<bool, IllegalChoice> {
    let matters = crate::factions::keleres::with_agent_granted(state, player, |granted| {
        crate::production::available(granted, content, sources, player, Spend::Influence)
            >= INFLUENCE_PER_TOKEN
    })
    .unwrap_or(false);
    if !matters {
        return Ok(false);
    }
    crate::factions::keleres::offer_agent(state, content, sources, galaxy, table, player)
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
    // One purchase is one payment window: the agent is offered once, for all its tokens.
    let opened = offer_agent_for_tokens(state, content, sources, galaxy, table, player)?;
    let result = buy_tokens_loop(state, content, sources, galaxy, table, player);
    if opened {
        crate::factions::keleres::close_agent_window(state, player);
    }
    result
}

fn buy_tokens_loop(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
) -> Result<(), IllegalChoice> {
    let mut credit = 0;
    while leadership_influence_eligible_with_credit(state, content, sources, player, credit) {
        let answer = ask(
            state,
            content,
            sources,
            galaxy,
            table,
            &influence_purchase_choice(state, player, credit),
        )?;
        if answer.id != "yes" {
            return Ok(());
        }
        if !pay_influence_and_gain_one(state, content, sources, galaxy, table, player, &mut credit)?
        {
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
    let opened = offer_agent_for_tokens(state, content, sources, galaxy, table, player)?;
    let result = buy_tokens_first_yes_loop(state, content, sources, galaxy, table, player);
    if opened {
        crate::factions::keleres::close_agent_window(state, player);
    }
    result
}

fn buy_tokens_first_yes_loop(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
) -> Result<(), IllegalChoice> {
    let mut credit = 0;
    if !pay_influence_and_gain_one(state, content, sources, galaxy, table, player, &mut credit)? {
        return Ok(());
    }
    while leadership_influence_eligible_with_credit(state, content, sources, player, credit) {
        let answer = ask(
            state,
            content,
            sources,
            galaxy,
            table,
            &influence_purchase_choice(state, player, credit),
        )?;
        if answer.id != "yes" {
            return Ok(());
        }
        if !pay_influence_and_gain_one(state, content, sources, galaxy, table, player, &mut credit)?
        {
            return Ok(());
        }
    }
    Ok(())
}

fn leadership_influence_eligible_with_credit(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    credit: i64,
) -> bool {
    crate::production::available(state, content, sources, player, Spend::Influence) + credit
        >= INFLUENCE_PER_TOKEN
}

fn influence_purchase_choice(state: &GameState, player: &PlayerId, credit: i64) -> Choice {
    let owed = (INFLUENCE_PER_TOKEN - credit).max(0);
    let prompt = if credit > 0 {
        format!("spend {owed} more influence for a command token")
    } else {
        format!("spend {owed} influence for a command token")
    };
    let yes = if credit > 0 {
        format!("spend {owed} more influence")
    } else {
        format!("spend {owed} influence")
    };
    // Oracle wording and ids (`_buy_tokens_with_influence`): both options kind `strategy`.
    Choice::new(
        player.clone(),
        prompt,
        vec![
            ChoiceOption::labelled(
                "no",
                crate::strategy::STRATEGY_KIND,
                "spend nothing further",
            ),
            ChoiceOption::labelled("yes", crate::strategy::STRATEGY_KIND, yes),
        ],
    )
    .contextualized(DecisionContext::new(
        player.clone(),
        DecisionSource::Rule("52.3".to_owned()),
        "buy_token_with_influence",
        state.phase,
        state.round,
    ))
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
    credit: &mut i64,
) -> Result<bool, IllegalChoice> {
    if !crate::production::pay_seeing_with_credit(
        state,
        content,
        sources,
        galaxy,
        table,
        player,
        INFLUENCE_PER_TOKEN,
        Spend::Influence,
        credit,
    )? {
        return Ok(false);
    }
    gain_tokens(state, content, sources, galaxy, table, player, 1)?;
    Ok(true)
}

/// One research option, carrying what it costs.
///
/// The price goes in the PAYLOAD, not the label. A numeric payload entry becomes a real feature --
/// `payload-number:cost` with its value, and `payload-number-kind:cost:research` -- so a learned
/// policy can weigh the technology against its price. A label is read by people and by nothing
/// else.
///
/// This matters because the same gain has three different prices: the Technology primary's first
/// research is free, its second costs six resources, and the secondary costs a command token plus
/// four. Offering all three as a bare list of technology names asks the policy to decide whether a
/// technology is worth paying for while hiding what it would pay.
///
/// `resources` is the cost AFTER any discount already applied (Doctor Sucaban reduces it before the
/// offer is built), so it is what this seat would actually spend. `tokens` counts command tokens
/// charged at this decision, which is zero everywhere the follower window has already taken one.
///
/// `techs_owned` previews the one exact, universal consequence every research shares regardless
/// of price: the seat's technology count rising by one (OBS-008d2). The resource/token bill
/// itself is not previewed here -- it is already an exact payload fact, and a full payment
/// preview needs the plan machinery `OBS-008c1` gives payment questions, not this one.
fn research_option(
    content: &ContentStore,
    id: &TechnologyId,
    resources: i64,
    tokens: i64,
    techs_owned: i64,
) -> ChoiceOption {
    let mut option = ChoiceOption::labelled(
        id.to_string(),
        RESEARCH_KIND,
        crate::technology::name(content, id),
    )
    .previewed(Preview::certain(vec![Delta::new(
        Quantity::TechnologiesOwned,
        techs_owned,
        techs_owned + 1,
    )]));
    option
        .payload
        .insert("cost".to_owned(), serde_json::Value::from(resources));
    option
        .payload
        .insert("cost_tokens".to_owned(), serde_json::Value::from(tokens));
    option
}

/// The Technology primary's research, inside Doctor Carrina's window (every research route opens
/// it: see `factions/deepwrought_research.rs`).
fn offer_research(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
) -> Result<Option<TechnologyId>, IllegalChoice> {
    use crate::factions::deepwrought_research as carrina;
    let window = carrina::open(
        &mut carrina::Host::new(&mut *state, content, sources, galaxy, &mut *table),
        player,
        &carrina::any,
    )?;
    let result = offer_research_inner(state, content, sources, galaxy, table, player);
    carrina::settle(
        &mut carrina::Host::new(&mut *state, content, sources, galaxy, &mut *table),
        player,
        window,
        result.is_ok(),
    )?;
    result
}

fn offer_research_inner(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
) -> Result<Option<TechnologyId>, IllegalChoice> {
    // A technology whose optional faction waiver was declined is not offered again in this
    // research. Re-offering it let a player select it and decline the waiver forever: an
    // unbounded cycle of decisions inside one step (BF-22 wide-roster evaluation, Cabal cv2).
    // Each pass now researches or shrinks the offer, so the loop ends.
    let mut declined_waivers: BTreeSet<TechnologyId> = BTreeSet::new();
    loop {
        let open: Vec<TechnologyId> =
            crate::technology::researchable(state, content, sources, player)
                .into_iter()
                .filter(|id| !declined_waivers.contains(id))
                .collect();
        if open.is_empty() {
            return Ok(None);
        }
        // No decline. The Technology primary reads "Research 1 technology" -- it is not optional,
        // and the second one, which is, goes through `paid_research`. A declined *optional faction
        // waiver* returns here without paying or researching, so the player can select another
        // legal technology.
        let techs_owned = i64::try_from(
            state
                .player(player)
                .map_or(0, |seat| seat.technologies.len()),
        )
        .unwrap_or(i64::MAX);
        let choice = Choice::new(
            player.clone(),
            "research a technology",
            open.iter()
                .map(|id| research_option(content, id, 0, 0, techs_owned))
                .collect(),
        )
        .contextualized(DecisionContext::new(
            player.clone(),
            DecisionSource::StrategyCard {
                card: "Technology".to_owned(),
                secondary: false,
            },
            "research_technology",
            state.phase,
            state.round,
        ));
        let answer = ask(state, content, sources, galaxy, table, &choice)?;
        if answer.is_decline() {
            return Ok(None);
        }
        let technology = TechnologyId::new(answer.id);
        if resolve_research(state, content, sources, galaxy, table, player, &technology)? {
            return Ok(Some(technology));
        }
        declined_waivers.insert(technology);
    }
}

/// Resolve an already-selected technology. The normal and Inheritance Systems paths retain their
/// existing table-less resolution. When a faction waiver is the only way past prerequisites, the
/// player chooses both the waiver and its exact cost; declining either makes no mutation.
fn resolve_research(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    technology: &TechnologyId,
) -> Result<bool, IllegalChoice> {
    if !crate::technology::faction_waiver_required(state, content, sources, player, technology) {
        return Ok(crate::technology::research(
            state, content, sources, player, technology,
        ));
    }
    let waivers = crate::technology::research_waiver_offers(state, content, player, technology);
    let choice = Choice::new(
        player.clone(),
        format!("research {technology}: choose a prerequisite waiver"),
        waivers
            .iter()
            .map(|(index, waiver)| {
                ChoiceOption::labelled(
                    format!("waiver|{index}"),
                    "research_waiver",
                    waiver.label.clone(),
                )
            })
            .chain(std::iter::once(ChoiceOption::decline()))
            .collect(),
    )
    .contextualized(DecisionContext::new(
        player.clone(),
        DecisionSource::FactionAbility("research_waiver".to_owned()),
        "research_waiver",
        state.phase,
        state.round,
    ));
    let answer = ask(state, content, sources, galaxy, table, &choice)?;
    if answer.is_decline() {
        return Ok(false);
    }
    let Some(index) = answer
        .id
        .strip_prefix("waiver|")
        .and_then(|id| id.parse::<usize>().ok())
    else {
        return Ok(false);
    };
    let Some((_, waiver)) = waivers.iter().find(|(offered, _)| *offered == index) else {
        return Ok(false);
    };
    let payment = Choice::new(
        player.clone(),
        format!("{}: choose the payment", waiver.label),
        waiver
            .payments
            .iter()
            .map(|payment| {
                ChoiceOption::labelled(
                    payment.id.clone(),
                    "research_waiver_payment",
                    payment.label.clone(),
                )
            })
            .chain(std::iter::once(ChoiceOption::decline()))
            .collect(),
    )
    .contextualized(DecisionContext::new(
        player.clone(),
        DecisionSource::FactionAbility("research_waiver".to_owned()),
        "research_waiver_payment",
        state.phase,
        state.round,
    ));
    let answer = ask(state, content, sources, galaxy, table, &payment)?;
    if answer.is_decline() {
        return Ok(false);
    }
    Ok(crate::technology::research_with_waiver(
        state, content, sources, player, technology, index, &answer.id,
    ))
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
                    format!(
                        "exhaust {planet} to research {} technology",
                        colour.to_lowercase()
                    ),
                )
            })
            .chain(std::iter::once(ChoiceOption::decline()))
            .collect(),
    )
    .contextualized(DecisionContext::new(
        player.clone(),
        DecisionSource::FactionAbility("specialist_compounds".to_owned()),
        "specialist_compounds_choose_planet",
        state.phase,
        state.round,
    ));
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
    state.exhaust_planet(planet.clone());
    // Doctor Carrina: this is a research too. Her window opens once the colour is fixed, because
    // the waiver widens the technologies of that colour the player can take.
    let wanted = *colour;
    let window = crate::factions::deepwrought_research::open(
        &mut crate::factions::deepwrought_research::Host::new(
            &mut *state,
            content,
            sources,
            galaxy,
            &mut *table,
        ),
        player,
        &move |content, id| {
            crate::technology::colour_type(content, id).is_some_and(|had| had == wanted)
        },
    )?;
    let open = crate::technology::researchable(state, content, sources, player);
    let of_colour: Vec<TechnologyId> = open
        .into_iter()
        .filter(|id| crate::technology::colour_type(content, id).is_some_and(|had| had == wanted))
        .collect();
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
    )
    .contextualized(DecisionContext::new(
        player.clone(),
        DecisionSource::FactionAbility("specialist_compounds".to_owned()),
        "specialist_compounds_choose_technology",
        state.phase,
        state.round,
    ));
    let answer = ask(state, content, sources, galaxy, table, &choice);
    if let Ok(answer) = &answer {
        crate::technology::research(
            state,
            content,
            sources,
            player,
            &TechnologyId::new(answer.id.clone()),
        );
    }
    crate::factions::deepwrought_research::settle(
        &mut crate::factions::deepwrought_research::Host::new(
            &mut *state,
            content,
            sources,
            galaxy,
            &mut *table,
        ),
        player,
        window,
        answer.is_ok(),
    )?;
    answer?;
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
/// infantry to give up. The holder is therefore asked before that holder's discount is offered to
/// the researcher; when several cards can open the window, their holders follow timing order.
///
/// Returns the reduced cost. Infantry are removed one at a time, each naming where it comes from:
/// units are interchangeable but their *locations* are not, and a seat losing a garrison it needed
/// is a real decision rather than an accounting detail.
struct SucabanSource {
    holder: PlayerId,
    source_owner: PlayerId,
    card: ti4_model::id::LeaderId,
    copied: bool,
}

struct SucabanAdjustment {
    cost: i64,
    borrowed_source: Option<ti4_model::id::LeaderId>,
    rollback: Option<(GameState, crate::choice::DecisionLog)>,
}

impl SucabanAdjustment {
    fn unused(cost: i64) -> Self {
        Self {
            cost,
            borrowed_source: None,
            rollback: None,
        }
    }
}

/// Deepwrought Scholarate's commander (`deepwroughtcommander`): "When another player spends
/// resources to research a technology: That player may reduce the cost by 1; if they do, gain 1
/// commodity or convert 1 of your commodities to a trade good." Asks the researcher once per other
/// seat holding the ability (own commander, faceup Alliance, or a Yin grant), in seat order, while
/// the bill is above zero; each accepted offer pays its holder, who chooses between the two
/// payments when both are possible. Returns how much came off the bill.
fn deepwrought_commander(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    cost: i64,
) -> Result<i64, IllegalChoice> {
    let holders: Vec<PlayerId> = state
        .players
        .iter()
        .map(|seat| seat.id.clone())
        .filter(|holder| {
            holder != player
                && crate::promissory::has_commander_ability(state, holder, "deepwroughtcommander")
        })
        .collect();
    let mut reduced = 0;
    for holder in holders {
        if cost - reduced <= 0 {
            break;
        }
        let limit = commodity_limit(state, content, &holder);
        let held = state.player(&holder).map_or(0, |seat| seat.commodities);
        let (can_gain, can_convert) = (held < limit, held > 0);
        if !can_gain && !can_convert {
            continue; // the holder could be paid nothing, so the condition "if they do" is moot
        }
        let offer = Choice::new(
            player.clone(),
            format!("Deepwrought commander: reduce this research by 1 (pays {holder})"),
            vec![
                ChoiceOption::labelled("reduce".to_owned(), "research", "reduce by 1".to_owned()),
                ChoiceOption::decline(),
            ],
        )
        .contextualized(DecisionContext::new(
            player.clone(),
            DecisionSource::Content("deepwroughtcommander".to_owned()),
            "deepwrought_reduce_research",
            state.phase,
            state.round,
        ));
        if ask(state, content, sources, galaxy, table, &offer)?.is_decline() {
            continue;
        }
        reduced += 1;
        let convert = if can_gain && can_convert {
            let payment = Choice::new(
                holder.clone(),
                "Deepwrought commander: gain 1 commodity or convert 1 to a trade good",
                vec![
                    ChoiceOption::labelled(
                        "gain".to_owned(),
                        "economy",
                        "gain 1 commodity".to_owned(),
                    ),
                    ChoiceOption::labelled(
                        "convert".to_owned(),
                        "economy",
                        "convert 1 commodity to a trade good".to_owned(),
                    ),
                ],
            )
            .contextualized(DecisionContext::new(
                holder.clone(),
                DecisionSource::Content("deepwroughtcommander".to_owned()),
                "deepwrought_payment",
                state.phase,
                state.round,
            ));
            ask(state, content, sources, galaxy, table, &payment)?.id == "convert"
        } else {
            can_convert
        };
        if let Some(seat) = state.player_mut(&holder) {
            if convert {
                seat.commodities -= 1;
                seat.trade_goods += 1;
            } else {
                seat.commodities += 1;
            }
        }
        if convert {
            crate::supply::note_trade_goods_gained(state, &holder, 1, "deepwroughtcommander");
        } else {
            crate::supply::note_commodities_gained(state, &holder, 1);
        }
    }
    Ok(reduced)
}

fn doctor_sucaban(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    cost: i64,
) -> Result<SucabanAdjustment, IllegalChoice> {
    if cost <= 0 {
        return Ok(SucabanAdjustment::unused(cost));
    }
    if infantry_sites(state, player).is_empty() {
        return Ok(SucabanAdjustment::unused(cost)); // nothing to trade, so nothing to ask about
    }
    // Each card is an independent right in the same timing window. The helper returns their
    // holders in the action-phase resolver's order and retains the exact source owner for a copy.
    // A declined or unspendable first offer does not close the next one.
    for source in sucaban_sources(state, content, player) {
        // A borrowed modifier is provisional until paid_research actually lands a technology.
        // Keep the decision log beside state, but deliberately do not clone/rewind decider input.
        let copied_checkpoint = source.copied.then(|| (state.clone(), table.log.clone()));
        let prompt = if source.copied {
            format!(
                "Ssruu: copy {}'s Doctor Sucaban to trade infantry for research",
                source.source_owner
            )
        } else {
            format!("Doctor Sucaban: exhaust to let {player} trade infantry for research")
        };
        let choice = Choice::new(
            source.holder.clone(),
            prompt,
            vec![
                ChoiceOption::labelled("yes".to_owned(), "leader", "exhaust the agent".to_owned()),
                ChoiceOption::decline(),
            ],
        )
        .contextualized(DecisionContext::new(
            source.holder.clone(),
            DecisionSource::Content(
                if source.copied {
                    "yssarilagent"
                } else {
                    "jolnaragent"
                }
                .to_owned(),
            ),
            if source.copied {
                "doctor_sucaban_borrowed_exhaust"
            } else {
                "doctor_sucaban_exhaust"
            },
            state.phase,
            state.round,
        ));
        let answer = match ask(state, content, sources, galaxy, table, &choice) {
            Ok(answer) => answer,
            Err(error) => {
                if let Some((before, log)) = copied_checkpoint {
                    *state = before;
                    table.log = log;
                }
                return Err(error);
            }
        };
        if answer.is_decline() || !crate::leaders::exhaust(state, &source.holder, &source.card) {
            continue;
        }
        // Exhausted for another seat's research: a promise to use this agent for them is kept here.
        // A borrowed copy is spent by its own holder for their own research, so no seat is served
        // by someone else and there is no promise to settle.
        if !source.copied && &source.holder != player {
            crate::diplomacy::evaluate_event(
                state,
                &crate::diplomacy::DiplomacyEventContext::LeaderUsedFor {
                    user: source.holder.clone(),
                    leader: source.card.to_string(),
                    beneficiary: player.clone(),
                },
            )
            .expect("validated diplomacy promises settle deterministically");
        }
        let reduced =
            match trade_infantry_for_research(state, content, sources, galaxy, table, player, cost)
            {
                Ok(reduced) => reduced,
                Err(error) => {
                    if let Some((before, log)) = copied_checkpoint {
                        *state = before;
                        table.log = log;
                    }
                    return Err(error);
                }
            };
        return Ok(SucabanAdjustment {
            cost: reduced,
            borrowed_source: source
                .copied
                .then_some(ti4_model::id::LeaderId::new("jolnaragent")),
            rollback: copied_checkpoint,
        });
    }
    Ok(SucabanAdjustment::unused(cost))
}

/// Who may pay for this research: a seat holding a readied Doctor Sucaban, and `player` themselves
/// if they hold a readied Ssruu and another seat has the agent at all (readied or exhausted).
/// `borrowable_agents` is the project's existing copy contract, so the two routes cannot disagree
/// about what counts as a copyable agent.
fn sucaban_sources(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
) -> Vec<SucabanSource> {
    let agent = ti4_model::id::LeaderId::new("jolnaragent");
    let mut sources = Vec::new();
    if let Some(owner) = state
        .players
        .iter()
        .find(|seat| seat.leaders.get(&agent) == Some(&ti4_model::state::LeaderStatus::Readied))
        .map(|seat| seat.id.clone())
    {
        sources.push(SucabanSource {
            holder: owner.clone(),
            source_owner: owner,
            card: agent.clone(),
            copied: false,
        });
    }
    let ssruu = ti4_model::id::LeaderId::new("yssarilagent");
    for (source_owner, _) in crate::factions::hooks_cards::borrowable_agents(state, content, player)
        .into_iter()
        .filter(|(_, source)| source == &agent)
    {
        sources.push(SucabanSource {
            holder: player.clone(),
            source_owner,
            card: ssruu.clone(),
            copied: true,
        });
    }
    let mut order = state.initiative_order();
    if let Some(active) = state.active.as_ref()
        && let Some(at) = order.iter().position(|seat| seat == active)
    {
        order.rotate_left(at);
    }
    sources.sort_by_key(|source| {
        (
            order
                .iter()
                .position(|seat| seat == &source.holder)
                .unwrap_or(usize::MAX),
            source.copied,
        )
    });
    sources
}

fn trade_infantry_for_research(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    cost: i64,
) -> Result<i64, IllegalChoice> {
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
        )
        .contextualized(DecisionContext::new(
            player.clone(),
            DecisionSource::Content("jolnaragent".to_owned()),
            "doctor_sucaban_remove_infantry",
            state.phase,
            state.round,
        ));
        let answer = ask(state, content, sources, galaxy, table, &choice)?;
        if answer.is_decline() {
            break;
        }
        let Some((system, planet)) = sites.into_iter().find(|(system, planet)| {
            let key = planet.as_ref().map_or_else(
                || format!("{system}:"),
                |planet| format!("{system}:{planet}"),
            );
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
    let mut adjustment = doctor_sucaban(state, content, sources, galaxy, table, player, cost)?;
    // Deepwrought's commander, held by another seat: the researcher may take 1 off the bill, and
    // the holder is paid for it. Provisional like Sucaban: undone if no technology lands.
    let deepwrought_held = state.players.iter().any(|seat| {
        &seat.id != player
            && crate::promissory::has_commander_ability(state, &seat.id, "deepwroughtcommander")
    });
    let before_deepwrought = deepwrought_held.then(|| (state.clone(), table.log.clone()));
    let reduced = deepwrought_commander(
        state,
        content,
        sources,
        galaxy,
        table,
        player,
        adjustment.cost,
    )?;
    if reduced > 0 && adjustment.rollback.is_none() {
        adjustment.rollback = before_deepwrought;
    }
    adjustment.cost -= reduced;
    let cost = adjustment.cost;
    let mut agent_window = false;
    let mut carrina_window = crate::factions::deepwrought_research::Window::default();
    let outcome = (|| -> Result<bool, IllegalChoice> {
        // Xander Alexin Victori III (Keleres): this research is one payment window; the agent is
        // offered before the affordability gate, which is what its commodities can open.
        if cost > 0
            && !crate::payment::affordable(state, content, sources, player, cost, Spend::Resources)
            && crate::factions::keleres::with_agent_granted(state, player, |granted| {
                crate::payment::affordable(
                    granted,
                    content,
                    sources,
                    player,
                    cost,
                    Spend::Resources,
                )
            })
            .unwrap_or(false)
        {
            agent_window = crate::factions::keleres::offer_agent(
                state, content, sources, galaxy, table, player,
            )?;
        }
        if !crate::payment::affordable(state, content, sources, player, cost, Spend::Resources) {
            return Ok(false);
        }
        // Doctor Carrina is asked as the research begins, once it can be paid for.
        carrina_window = crate::factions::deepwrought_research::open(
            &mut crate::factions::deepwrought_research::Host::new(
                &mut *state,
                content,
                sources,
                galaxy,
                &mut *table,
            ),
            player,
            &crate::factions::deepwrought_research::any,
        )?;
        // Choose before paying. A declined optional prerequisite waiver returns here, restoring
        // the resource plan too, so the player may choose another legal technology or decline.
        // That technology is not offered again (see `offer_research_inner`), so the loop ends.
        let mut declined_waivers: BTreeSet<TechnologyId> = BTreeSet::new();
        loop {
            let open: Vec<TechnologyId> =
                crate::technology::researchable(state, content, sources, player)
                    .into_iter()
                    .filter(|id| !declined_waivers.contains(id))
                    .collect();
            if open.is_empty() {
                return Ok(false);
            }
            let techs_owned = i64::try_from(
                state
                    .player(player)
                    .map_or(0, |seat| seat.technologies.len()),
            )
            .unwrap_or(i64::MAX);
            let choice = Choice::new(
                player.clone(),
                "research a technology",
                open.iter()
                    .map(|id| research_option(content, id, cost, 0, techs_owned))
                    .chain(std::iter::once(ChoiceOption::decline()))
                    .collect(),
            )
            .contextualized(DecisionContext::new(
                player.clone(),
                DecisionSource::StrategyCard {
                    card: "Technology".to_owned(),
                    secondary: true,
                },
                "research_technology",
                state.phase,
                state.round,
            ));
            let answer = ask(state, content, sources, galaxy, table, &choice)?;
            if answer.is_decline() {
                return Ok(false);
            }
            let technology = TechnologyId::new(answer.id);
            let waiver_required = crate::technology::faction_waiver_required(
                state,
                content,
                sources,
                player,
                &technology,
            );
            let Some(plan) =
                crate::payment::plans(state, content, sources, player, cost, Spend::Resources)
                    .into_iter()
                    .next()
            else {
                return Ok(false);
            };
            let before = state.clone();
            if !crate::payment::apply(state, player, &plan) {
                return Ok(false);
            }
            if resolve_research(state, content, sources, galaxy, table, player, &technology)? {
                return Ok(true);
            }
            *state = before;
            if !waiver_required {
                return Ok(false);
            }
            declined_waivers.insert(technology);
        }
    })();
    if agent_window {
        crate::factions::keleres::close_agent_window(state, player);
    }
    // Doctor Carrina: close the waiver, and settle the holder's placement for a research that landed.
    let settled = crate::factions::deepwrought_research::settle(
        &mut crate::factions::deepwrought_research::Host::new(
            &mut *state,
            content,
            sources,
            galaxy,
            &mut *table,
        ),
        player,
        carrina_window,
        outcome.is_ok(),
    );
    let outcome = outcome.and_then(|researched| settled.map(|()| researched));
    match outcome {
        Ok(true) => {
            if let Some(source) = adjustment.borrowed_source {
                crate::factions::hooks_cards::borrowed_agent_used_state(state, player, &source);
            }
            Ok(())
        }
        Ok(false) => {
            if let Some((before, log)) = adjustment.rollback {
                *state = before;
                table.log = log;
            }
            Ok(())
        }
        Err(error) => {
            if let Some((before, log)) = adjustment.rollback {
                *state = before;
                table.log = log;
            }
            Err(error)
        }
    }
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
        )
        .contextualized(DecisionContext::new(
            player.clone(),
            DecisionSource::Rule("34.2".to_owned()),
            "ready_planet",
            state.phase,
            state.round,
        ));
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
        .filter(|system| !crate::seating::is_mecatol(system.as_str()))
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
        )
        .contextualized(DecisionContext::new(
            player.clone(),
            DecisionSource::StrategyCard {
                card: "Diplomacy".to_owned(),
                secondary: false,
            },
            "diplomacy_choose_system",
            state.phase,
            state.round,
        ));
        let chosen = SystemId::new(ask(state, content, sources, galaxy, table, &choice)?.id);
        for other in state.seating_order.clone() {
            if &other != player {
                if state
                    .system_mut(&chosen)
                    .command_tokens
                    .insert(other.clone())
                {
                    crate::tokens::stage_command_token_placed(
                        state,
                        &other,
                        &chosen,
                        crate::factions::hooks_cards::TokenPool::Reinforcements,
                    );
                }
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
        )
        .contextualized(DecisionContext::new(
            player.clone(),
            DecisionSource::StrategyCard {
                card: "Politics".to_owned(),
                secondary: false,
            },
            "politics_choose_speaker",
            state.phase,
            state.round,
        ));
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
        )
        .contextualized(DecisionContext::new(
            player.clone(),
            DecisionSource::StrategyCard {
                card: "Politics".to_owned(),
                secondary: false,
            },
            "politics_place_agenda",
            state.phase,
            state.round,
        ));
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
                    // The unit this player actually places: faction unit or upgrade, as an action
                    // card placement resolves it (Titans Hel-Titan, PDS II).
                    let unit = construction_unit(state, content, sources, player, kind);
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

/// The unit Construction places for `kind` (`pds` / `spacedock`): the faction unit or upgrade, as
/// an action card placement resolves it. Construction puts the structure on a planet, so a
/// space-only form (Saar's Floating Factory) falls back to the generic planet unit.
fn construction_unit(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    kind: &str,
) -> UnitTypeId {
    let generic = || UnitTypeId::new(kind);
    let Some(unit) = crate::action_cards::placed_unit_id(state, content, sources, player, kind)
    else {
        return generic();
    };
    let space_only = ti4_content::units::catalogue(content, sources)
        .get(unit.as_str())
        .is_some_and(|record| record.is_space_only_structure());
    if space_only { generic() } else { unit }
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
    // Shared across every card offering the structure ability (Construction, and Politics/Warfare
    // where a law extends it) with which unit `structure_options` prices, so this names the
    // mechanic rather than one card that does not always own it.
    let choice = Choice::new(player.clone(), "place a structure", options).contextualized(
        DecisionContext::new(
            player.clone(),
            DecisionSource::Content("place_structure".to_owned()),
            "place_structure",
            state.phase,
            state.round,
        ),
    );
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
    // "When you would place a PDS on a planet, you may ... instead" (Titans Hecatoncheires): the
    // alternatives are asked before anything is placed, so a refusal leaves the board untouched.
    if kind == "pds" {
        let alternatives = crate::factions::hooks_economy::pds_placement_alternatives(
            state, content, sources, player, &system, &planet,
        );
        if !alternatives.is_empty() {
            let mut offered = vec![ChoiceOption::labelled(
                "pds",
                "build",
                format!("place pds on {planet}"),
            )];
            offered.extend(alternatives);
            let choice = Choice::new(player.clone(), "place a PDS or an alternative", offered)
                .contextualized(DecisionContext::new(
                    player.clone(),
                    DecisionSource::Content("place_structure".to_owned()),
                    "place_structure_pds_alternative",
                    state.phase,
                    state.round,
                ));
            let answer = ask(state, content, sources, galaxy, table, &choice)?;
            if answer.id != "pds" {
                if crate::factions::hooks_economy::perform_pds_placement_alternative(
                    state, content, sources, player, &system, &planet, &answer.id,
                ) {
                    return Ok(Some(system));
                }
                return Ok(None);
            }
        }
    }
    let placed = construction_unit(state, content, sources, player, kind);
    state
        .system_mut(&system)
        .planet_units
        .entry(planet)
        .or_default()
        .push(Unit::new(placed, player.clone()));

    // Minister of Industry: "When the owner of this card places a space dock in a system, their
    // units in that system may use their PRODUCTION abilities." A space dock specifically, so a
    // PDS placed under the same law produces nothing.
    if kind == "spacedock" && crate::laws::industry_produces_on_placement(state, player) {
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
        if !crate::production::produce_one(state, content, sources, galaxy, table, player, system)?
        {
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

pub(crate) fn replenish(state: &mut GameState, content: &ContentStore, player: &PlayerId) {
    let limit = commodity_limit(state, content, player);
    let mut gained = 0;
    if let Some(seat) = state.player_mut(player) {
        gained = (limit - seat.commodities).max(0);
        seat.commodities = limit;
    }
    // Siphon (Obsidian plot): the commodities gained, announced before Trade Agreement can take them.
    crate::supply::note_commodities_gained(state, player, gained);
    // Trade Agreement: "When the <color> player replenishes commodities".
    crate::promissory::trade_agreement_on_replenish(state, player);
}

fn trade_primary(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
) -> Result<(), IllegalChoice> {
    // No timing handle here: the gain is staged and announced by `supply::flush_staged_events`.
    crate::supply::gain_trade_goods_staged(state, player, 3, "trade_primary");
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
        )
        .contextualized(DecisionContext::new(
            player.clone(),
            DecisionSource::StrategyCard {
                card: "Trade".to_owned(),
                secondary: false,
            },
            "trade_choose_replenish",
            state.phase,
            state.round,
        ));
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
        // A promise to spend the Trade primary on somebody else is kept here, and nowhere else:
        // the self-replenish above and the secondary are not favours to anyone.
        crate::diplomacy::evaluate_event(
            state,
            &crate::diplomacy::DiplomacyEventContext::CommoditiesReplenished {
                by: player.clone(),
                beneficiary: other.clone(),
            },
        )
        .expect("validated diplomacy promises settle deterministically");
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
    )
    .contextualized(DecisionContext::new(
        player.clone(),
        DecisionSource::StrategyCard {
            card: "Warfare".to_owned(),
            secondary: false,
        },
        "warfare_recall_token",
        state.phase,
        state.round,
    ));
    let system = SystemId::new(ask(state, content, sources, galaxy, table, &choice)?.id);
    state.system_mut(&system).command_tokens.remove(player);
    gain_tokens(state, content, sources, galaxy, table, player, 1)?;
    // "Then, the active player can redistribute their command tokens." A separate sentence from the
    // gain above and a separate decision: the token just recovered goes into a pool of the player's
    // choice, and *then* every token they hold may be moved between pools.
    redistribute_tokens(
        state,
        content,
        sources,
        galaxy,
        table,
        player,
        &DecisionSource::StrategyCard {
            card: "Warfare".to_owned(),
            secondary: false,
        },
        "warfare_redistribute_tokens",
        "Warfare: redistribute your command tokens",
    )?;
    Ok(())
}

/// Move command tokens between a player's own pools under the caller's named rule/effect.
///
/// Offered as one choice over every legal final distribution, so the policy scores the complete
/// command sheet it is choosing rather than a sequence of isolated one-token moves.
pub(crate) fn redistribute_tokens(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    source: &DecisionSource,
    subtype: &str,
    prompt: &str,
) -> Result<Ability, IllegalChoice> {
    let mut window = crate::tokens::TokenRedistribution::new(player.clone());
    let Some(choice) = window.pending_choice(state) else {
        return Ok(Ability::Resolved);
    };
    let choice =
        Choice::new(player.clone(), prompt, choice.options).contextualized(DecisionContext::new(
            player.clone(),
            source.clone(),
            subtype,
            state.phase,
            state.round,
        ));
    let answer = ask(state, content, sources, galaxy, table, &choice)?;
    window.resolve(state, answer).map_err(|error| match error {
        crate::tokens::RedistributeError::IllegalChoice(error) => error,
        other => IllegalChoice::DeciderFailed {
            player: player.clone(),
            prompt: prompt.to_owned(),
            reason: other.to_string(),
        },
    })?;
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
        )
        .contextualized(DecisionContext::new(
            player.clone(),
            DecisionSource::StrategyCard {
                card: "Imperial".to_owned(),
                secondary: false,
            },
            "imperial_score_objective",
            state.phase,
            state.round,
        ));
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
        .any(|(system, _)| crate::seating::is_mecatol(system.as_str()));
    if controls_mecatol {
        if let Some(seat) = state.player_mut(player) {
            seat.victory_points = (seat.victory_points + 1).min(crate::objectives::VICTORY_TARGET);
        }
        state.note_vp(player, 1, "imperial_primary");
        // Custodian's Favour (Custodia Vigilia): "Gain 2 command tokens when another player scores a
        // victory point with the second clause of the 'Imperial' strategy card."
        crate::factions::keleres::custodians_favour_tokens(
            state, content, sources, galaxy, table, player,
        )?;
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
        )
        .contextualized(DecisionContext::new(
            player.clone(),
            DecisionSource::Content("te6warfare".to_owned()),
            "warfare_free_tactical",
            state.phase,
            state.round,
        ));
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
        )
        .contextualized(DecisionContext::new(
            player.clone(),
            DecisionSource::Content("te4construction".to_owned()),
            "construction_choose_ability",
            state.phase,
            state.round,
        ));
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

        // Choose the complete final sheet with the one token moved from tactic to fleet.
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new(vec![
            "0|1|0".to_owned(),
        ])));
        redistribute_tokens(
            &mut state,
            content,
            ti4_model::content_types::DEFAULT,
            None,
            &mut table,
            &player,
            &DecisionSource::StrategyCard {
                card: "Warfare".to_owned(),
                secondary: false,
            },
            "warfare_redistribute_tokens",
            "Warfare: redistribute your command tokens",
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

    /// Tier-C review remediation: the same redistribution mechanic is also the status-phase
    /// 81.5 step, so the caller supplies its real source instead of this helper always claiming
    /// Warfare's primary.
    #[test]
    fn status_redistribution_carries_the_status_rule_not_warfare() {
        let content = ContentStore::embedded();
        let player = PlayerId::new("a");
        let mut state = game(&["a"]);
        state.player_mut(&player).unwrap().tactic_tokens = 1;
        let (decider, seen) = crate::choice::Capturing::new(Box::new(crate::choice::FirstOption));
        let mut table = Table::with_default(Box::new(decider));

        redistribute_tokens(
            &mut state,
            content,
            ti4_model::content_types::DEFAULT,
            None,
            &mut table,
            &player,
            &DecisionSource::Rule("81.5".to_owned()),
            "status_redistribute_tokens",
            "redistribute your command tokens",
        )
        .expect("status redistribution resolves");

        let asked = seen.borrow();
        assert_eq!(asked[0].prompt, "redistribute your command tokens");
        let context = asked[0].context.clone().expect("typed context");
        assert_eq!(context.source, DecisionSource::Rule("81.5".to_owned()));
        assert_eq!(context.subtype, "status_redistribute_tokens");
    }
    use super::*;
    use crate::fixtures::{a_placed_planet, game, plain_hub, put, put_on_planet, seated_game};
    use ti4_model::content_types::POK;

    /// A decider that answers the first offered option and keeps every `Choice` it was asked,
    /// context included.
    struct FirstOptionCapturing {
        seen: std::rc::Rc<std::cell::RefCell<Vec<Choice>>>,
    }

    impl crate::choice::Decider for FirstOptionCapturing {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            self.seen.borrow_mut().push(choice.clone());
            choice
                .options
                .first()
                .cloned()
                .ok_or_else(|| IllegalChoice::NoOptions {
                    player: choice.player.clone(),
                    prompt: choice.prompt.clone(),
                })
        }
    }

    #[test]
    fn a_technology_whose_waiver_was_declined_is_not_offered_again() {
        // BF-22: a near-greedy bot selected a waiver-only technology and declined the waiver
        // forever (Cabal cv2), an unbounded decision cycle inside one step. The declined
        // technology now leaves the offer, so the research ends.
        let content = ContentStore::embedded();
        let player = PlayerId::new("a");
        let other = PlayerId::new("b");
        let technology = TechnologyId::new("ws");
        let mut state = seated_game(&[("a", "yin"), ("b", "sol")], POK);
        state.player_mut(&player).expect("Yin seat").leaders.insert(
            ti4_model::id::LeaderId::new("yincommander"),
            ti4_model::state::LeaderStatus::Unlocked,
        );
        state
            .player_mut(&other)
            .expect("other seat")
            .technologies
            .insert(technology.clone());
        put(&mut state, &SystemId::new("18"), "infantry", &player, 1);
        assert!(crate::technology::faction_waiver_required(
            &state,
            content,
            POK,
            &player,
            &technology
        ));
        let open = crate::technology::researchable(&state, content, POK, &player);
        assert!(open.contains(&technology), "the waiver makes it researchable");
        let alternative = open
            .iter()
            .find(|id| **id != technology)
            .expect("another technology")
            .clone();

        // Selecting it again after declining its waiver is no longer legal.
        let mut again = state.clone();
        let mut repeat = Table::with_default(Box::new(crate::choice::Scripted::new(vec![
            technology.to_string(),
            "decline".to_owned(),
            technology.to_string(),
        ])));
        // Before the fix the third answer was legal and the script's fallback finished the
        // research; now the third offer lacks it and the script diverges there.
        assert!(
            matches!(
                offer_research_inner(&mut again, content, POK, None, &mut repeat, &player),
                Err(IllegalChoice::ScriptDiverged { .. })
            ),
            "the declined technology was offered again"
        );

        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new(vec![
            technology.to_string(),
            "decline".to_owned(),
            alternative.to_string(),
        ])));
        assert_eq!(
            offer_research_inner(&mut state, content, POK, None, &mut table, &player)
                .expect("researches another"),
            Some(alternative)
        );
    }

    #[test]
    fn yin_commander_chooses_an_infantry_payment_and_decline_is_atomic() {
        let content = ContentStore::embedded();
        let player = PlayerId::new("a");
        let other = PlayerId::new("b");
        let system = SystemId::new("18");
        let technology = TechnologyId::new("ws");
        let mut state = seated_game(&[("a", "yin"), ("b", "sol")], POK);
        state.player_mut(&player).expect("Yin seat").leaders.insert(
            ti4_model::id::LeaderId::new("yincommander"),
            ti4_model::state::LeaderStatus::Unlocked,
        );
        state
            .player_mut(&other)
            .expect("other seat")
            .technologies
            .insert(technology.clone());
        put(&mut state, &system, "infantry", &player, 1);

        let before = state.clone();
        let mut declined = Table::with_default(Box::new(crate::choice::Scripted::new(vec![
            "waiver|0".to_owned(),
            "decline".to_owned(),
        ])));
        assert!(
            !resolve_research(
                &mut state,
                content,
                POK,
                None,
                &mut declined,
                &player,
                &technology,
            )
            .expect("declining a legal optional waiver"),
        );
        assert_eq!(state, before, "declining the waiver spends nothing");

        let mut chosen = Table::with_default(Box::new(crate::choice::Scripted::new(vec![
            "waiver|0".to_owned(),
            "space|18".to_owned(),
        ])));
        assert!(
            resolve_research(
                &mut state,
                content,
                POK,
                None,
                &mut chosen,
                &player,
                &technology,
            )
            .expect("selected legal infantry")
        );
        assert!(
            state
                .player(&player)
                .expect("Yin seat")
                .technologies
                .contains(&technology)
        );
        assert!(
            state.system_state(&system).units.iter().all(|unit| {
                !(unit.owner == player && unit.type_id.as_str().contains("infantry"))
            })
        );
    }

    /// OBS-003e: the production/payment producers this module shares with `OBS-008c` were
    /// already typed; these are the card-specific asks that were not.
    #[test]
    fn obs003e_strategy_card_choices_carry_typed_context() {
        let content = ContentStore::embedded();
        let player = PlayerId::new("a");

        // gain_tokens: a generic LRR 52.4 mechanic, not one card's own text.
        let mut state = game(&["a"]);
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut table = Table::with_default(Box::new(FirstOptionCapturing { seen: seen.clone() }));
        gain_tokens(&mut state, content, POK, None, &mut table, &player, 1).unwrap();
        let context = seen.borrow()[0].context.clone().expect("typed context");
        assert_eq!(context.source, DecisionSource::Rule("52.4".to_owned()));
        assert_eq!(context.subtype, "gain_command_token");

        // offer_research (Technology primary, mandatory) vs paid_research (Technology secondary,
        // optional): the same subtype, distinguished by the source's `secondary` flag.
        let mut state = game(&["a"]);
        state.player_mut(&player).unwrap().trade_goods = 20;
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut table = Table::with_default(Box::new(FirstOptionCapturing { seen: seen.clone() }));
        offer_research(&mut state, content, POK, None, &mut table, &player).unwrap();
        let primary_context = seen.borrow()[0].context.clone().expect("typed context");
        assert_eq!(primary_context.subtype, "research_technology");
        assert_eq!(
            primary_context.source,
            DecisionSource::StrategyCard {
                card: "Technology".to_owned(),
                secondary: false
            }
        );

        let mut state = game(&["a"]);
        state.player_mut(&player).unwrap().trade_goods = 20;
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut table = Table::with_default(Box::new(FirstOptionCapturing { seen: seen.clone() }));
        paid_research(
            &mut state,
            content,
            POK,
            None,
            &mut table,
            &player,
            TECHNOLOGY_SECONDARY_COST,
        )
        .unwrap();
        let secondary_context = seen.borrow()[0].context.clone().expect("typed context");
        assert_eq!(secondary_context.subtype, "research_technology");
        assert_eq!(
            secondary_context.source,
            DecisionSource::StrategyCard {
                card: "Technology".to_owned(),
                secondary: true
            }
        );
        assert_ne!(
            primary_context.source, secondary_context.source,
            "the same subtype, distinguished by which half of the card it is"
        );
    }

    /// OBS-008d2: each command-token pool option previews the exact count it reaches, and a
    /// research option previews the seat's technology count rising by one, regardless of price.
    #[test]
    fn obs008d2_token_gain_and_research_preview_their_exact_consequence() {
        let content = ContentStore::embedded();
        let player = PlayerId::new("a");

        let mut state = game(&["a"]);
        state.player_mut(&player).unwrap().tactic_tokens = 3;
        state.player_mut(&player).unwrap().fleet_tokens = 2;
        state.player_mut(&player).unwrap().strategic_tokens = 1;
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut table = Table::with_default(Box::new(FirstOptionCapturing { seen: seen.clone() }));
        gain_tokens(&mut state, content, POK, None, &mut table, &player, 1).unwrap();
        let asked = seen.borrow();
        let choice = &asked[0];
        for (id, quantity, before) in [
            ("tactic_tokens", Quantity::TacticTokens, 3),
            ("fleet_tokens", Quantity::FleetTokens, 2),
            ("strategic_tokens", Quantity::StrategicTokens, 1),
        ] {
            let option = choice
                .options
                .iter()
                .find(|option| option.id == id)
                .unwrap_or_else(|| panic!("a {id} option"));
            match &option.preview.as_ref().expect("previewed").outcome {
                crate::preview::Outcome::Certain { deltas } => {
                    assert_eq!(deltas, &[Delta::new(quantity, before, before + 1)]);
                }
                other => panic!("a token-pool preview is certain, got {other:?}"),
            }
        }
        drop(asked);

        let mut state = game(&["a"]);
        state.player_mut(&player).unwrap().trade_goods = 20;
        state
            .player_mut(&player)
            .unwrap()
            .technologies
            .insert(TechnologyId::new("gd"));
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut table = Table::with_default(Box::new(FirstOptionCapturing { seen: seen.clone() }));
        offer_research(&mut state, content, POK, None, &mut table, &player).unwrap();
        let research_choice = &seen.borrow()[0];
        match &research_choice.options[0]
            .preview
            .as_ref()
            .expect("previewed")
            .outcome
        {
            crate::preview::Outcome::Certain { deltas } => {
                assert_eq!(
                    deltas,
                    &[Delta::new(Quantity::TechnologiesOwned, 1, 2)],
                    "the seat already owns one technology, so research previews 1 -> 2"
                );
            }
            other => panic!("a research preview is certain, got {other:?}"),
        }
    }

    /// OBS-003e: Politics' speaker choice and its agenda-placement choice are typed distinctly,
    /// though both come from the same primary.
    #[test]
    fn obs003e_politics_primary_types_its_two_choices_distinctly() {
        let content = ContentStore::embedded();
        let player = PlayerId::new("a");
        let mut state = game(&["a", "b"]);
        state.speaker = player.clone();
        for _ in 0..4 {
            state
                .agenda_deck
                .push(format!("agenda{}", state.agenda_deck.len()));
        }
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut table = Table::with_default(Box::new(FirstOptionCapturing { seen: seen.clone() }));

        politics_primary(&mut state, content, POK, None, &mut table, &player).unwrap();

        let asked = seen.borrow();
        let speaker = asked
            .iter()
            .find(|choice| choice.prompt == "who becomes speaker")
            .expect("the speaker ask happened")
            .context
            .clone()
            .expect("typed context");
        assert_eq!(speaker.subtype, "politics_choose_speaker");
        let agenda = asked
            .iter()
            .find(|choice| choice.prompt.starts_with("place "))
            .expect("an agenda-placement ask happened")
            .context
            .clone()
            .expect("typed context");
        assert_eq!(agenda.subtype, "politics_place_agenda");
        assert_ne!(speaker.subtype, agenda.subtype);
    }

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

    fn count_type(state: &GameState, player: &PlayerId, type_id: &str) -> usize {
        state
            .board
            .values()
            .flat_map(|system| system.planet_units.values().flatten())
            .filter(|unit| unit.owner == *player && unit.type_id.as_str() == type_id)
            .count()
    }

    fn titans_seat() -> (GameState, PlayerId) {
        let state =
            crate::fixtures::seated_game(&[("a", "titans")], ti4_model::content_types::DEFAULT);
        (state, PlayerId::new("a"))
    }

    #[test]
    fn construction_places_the_factions_own_pds_and_its_upgrade() {
        let content = ContentStore::embedded();
        let sources = ti4_model::content_types::DEFAULT;
        let (mut state, player) = titans_seat();
        let mut table = Table::new();
        primary(
            &mut state,
            content,
            sources,
            None,
            &mut table,
            &player,
            &card("Construction"),
        )
        .unwrap();
        assert_eq!(count_type(&state, &player, "titans_pds"), 2);
        assert_eq!(
            count_type(&state, &player, "pds"),
            0,
            "never the generic plastic"
        );

        let (mut state, player) = titans_seat();
        state
            .player_mut(&player)
            .unwrap()
            .technologies
            .insert(ti4_model::id::TechnologyId::new("ht2"));
        primary(
            &mut state,
            content,
            sources,
            None,
            &mut table,
            &player,
            &card("Construction"),
        )
        .unwrap();
        assert_eq!(count_type(&state, &player, "titans_pds2"), 2);
        assert_eq!(count_type(&state, &player, "titans_pds"), 0);
    }

    #[test]
    fn a_faction_without_its_own_pds_still_places_the_generic_one() {
        let content = ContentStore::embedded();
        let sources = ti4_model::content_types::DEFAULT;
        let mut state = crate::fixtures::seated_game(&[("a", "sol")], sources);
        let player = PlayerId::new("a");
        let mut table = Table::new();
        primary(
            &mut state,
            content,
            sources,
            None,
            &mut table,
            &player,
            &card("Construction"),
        )
        .unwrap();
        assert_eq!(count_type(&state, &player, "pds"), 2);
    }

    #[test]
    fn saar_construction_places_the_generic_dock_on_a_planet_not_the_floating_factory() {
        let content = ContentStore::embedded();
        let sources = ti4_model::content_types::DEFAULT;
        let mut state = seated_game(&[("a", "saar")], sources);
        let player = PlayerId::new("a");
        // Free the planet of any dock so a space dock is a legal Construction spot.
        for system in state.board.values_mut() {
            for units in system.planet_units.values_mut() {
                units.retain(|unit| !unit.type_id.as_str().contains("spacedock"));
            }
        }
        let spot = structure_options(&state, content, sources, &player, false)
            .into_iter()
            .find(|option| option.id.starts_with("spacedock|"))
            .expect("a Saar space dock spot is offered")
            .id;
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([spot])));
        place_structure(
            &mut state, content, sources, None, &mut table, &player, false,
        )
        .unwrap();
        assert_eq!(count_type(&state, &player, "spacedock"), 1);
        assert_eq!(count_type(&state, &player, "saar_spacedock"), 0);
        assert_eq!(count_type(&state, &player, "saar_spacedock2"), 0);
    }

    #[test]
    fn minister_of_industry_produces_on_a_construction_space_dock() {
        let content = ContentStore::embedded();
        let run = |law: bool| {
            let mut state = game(&["a"]);
            let player = PlayerId::new("a");
            let (system, planet) = a_placed_planet();
            state
                .system_mut(&system)
                .set_control(planet.clone(), player.clone());
            state.player_mut(&player).unwrap().trade_goods = 10;
            if law {
                state
                    .laws
                    .insert("minister_industry".to_owned(), "a".to_owned());
            }
            let spot = format!("spacedock|{system}|{planet}");
            let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([spot])));
            place_structure(&mut state, content, POK, None, &mut table, &player, false).unwrap();
            let board = state.system_state(&system);
            board.units.len() + board.on_planet(&planet).len()
        };
        assert!(
            run(true) > run(false),
            "the placed dock produces under the law"
        );
    }

    // -- "when you would place a PDS ... you may place ... instead" ---------------------------------

    /// Hecatoncheires is the live Titans module hook (`factions::titans`); these tests drive it
    /// through Construction.
    mod hecatoncheires {
        pub(super) const ID: &str = "titans|hecatoncheires";
    }

    fn place_one_pds(
        state: &mut GameState,
        player: &PlayerId,
        second: Option<&str>,
    ) -> (Result<Option<SystemId>, IllegalChoice>, Vec<String>) {
        let content = ContentStore::embedded();
        let sources = ti4_model::content_types::DEFAULT;
        let first = structure_options(state, content, sources, player, true)
            .into_iter()
            .next()
            .expect("a PDS spot")
            .id;
        let mut script = vec![first.as_str()];
        script.extend(second);
        let (recorder, seen) = SpeakerRecording::new(&script);
        let mut table = Table::with_default(Box::new(recorder));
        let result = place_structure(state, content, sources, None, &mut table, player, true);
        let prompts = seen
            .borrow()
            .iter()
            .map(|(prompt, _)| prompt.clone())
            .collect();
        (result, prompts)
    }

    #[test]
    fn the_pds_alternative_is_offered_and_taken_instead_of_the_pds() {
        {
            let (mut state, player) = titans_seat();
            let infantry = count_type(&state, &player, "infantry");
            let (result, prompts) = place_one_pds(&mut state, &player, Some(hecatoncheires::ID));
            assert!(result.unwrap().is_some());
            assert_eq!(prompts.len(), 2, "{prompts:?}");
            assert_eq!(count_type(&state, &player, "titans_pds"), 0);
            assert_eq!(count_type(&state, &player, "titans_mech"), 1);
            assert_eq!(count_type(&state, &player, "infantry"), infantry + 1);
        }
    }

    #[test]
    fn declining_the_alternative_places_the_pds_and_nothing_else() {
        {
            let (mut state, player) = titans_seat();
            let (result, _) = place_one_pds(&mut state, &player, Some("pds"));
            assert!(result.unwrap().is_some());
            assert_eq!(count_type(&state, &player, "titans_pds"), 1);
            assert_eq!(count_type(&state, &player, "titans_mech"), 0);
        }
    }

    #[test]
    fn the_pds_alternative_respects_the_box_and_other_factions_are_not_asked() {
        {
            // The four mechs are already out: nothing to offer, so one question only.
            let (mut state, player) = titans_seat();
            let far = SystemId::new("far");
            put(&mut state, &far, "titans_mech", &player, 4);
            let (result, prompts) = place_one_pds(&mut state, &player, None);
            assert!(result.unwrap().is_some());
            assert_eq!(prompts.len(), 1, "{prompts:?}");
            assert_eq!(count_type(&state, &player, "titans_pds"), 1);

            // A faction without the unit is never asked.
            let mut state =
                crate::fixtures::seated_game(&[("a", "sol")], ti4_model::content_types::DEFAULT);
            let (result, prompts) = place_one_pds(&mut state, &player, None);
            assert!(result.unwrap().is_some());
            assert_eq!(prompts.len(), 1, "{prompts:?}");
        }
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

    #[test]
    fn the_deepwrought_commander_takes_one_off_another_seats_research_and_pays_its_holder() {
        let content = ContentStore::embedded();
        let mut state = seated_game(&[("a", "sol"), ("b", "hacan")], POK);
        let (a, b) = (PlayerId::new("a"), PlayerId::new("b"));
        state.player_mut(&b).unwrap().commodities = 0;
        let mut none =
            Table::with_default(Box::new(crate::choice::Scripted::new(Vec::<String>::new())));
        assert_eq!(
            deepwrought_commander(&mut state, content, POK, None, &mut none, &a, 3).unwrap(),
            0,
            "nobody holds it: nothing is asked"
        );
        assert!(crate::promissory::grant_commander_ability(
            &mut state,
            content,
            &b,
            "deepwroughtcommander"
        ));
        let mut declined = Table::with_default(Box::new(crate::choice::Scripted::new(["decline"])));
        assert_eq!(
            deepwrought_commander(&mut state, content, POK, None, &mut declined, &a, 3).unwrap(),
            0
        );
        assert_eq!(state.player(&b).unwrap().commodities, 0);
        // At zero commodities the holder can only gain one, so only the researcher is asked.
        let mut taken = Table::with_default(Box::new(crate::choice::Scripted::new(["reduce"])));
        assert_eq!(
            deepwrought_commander(&mut state, content, POK, None, &mut taken, &a, 3).unwrap(),
            1
        );
        assert_eq!(state.player(&b).unwrap().commodities, 1);
        // With room and a commodity, the holder chooses: convert pays a trade good.
        let goods = state.player(&b).unwrap().trade_goods;
        let mut convert = Table::with_default(Box::new(crate::choice::Scripted::new([
            "reduce", "convert",
        ])));
        assert_eq!(
            deepwrought_commander(&mut state, content, POK, None, &mut convert, &a, 3).unwrap(),
            1
        );
        assert_eq!(state.player(&b).unwrap().commodities, 0);
        assert_eq!(state.player(&b).unwrap().trade_goods, goods + 1);
        // The holder's own research is not "another player's".
        let mut own =
            Table::with_default(Box::new(crate::choice::Scripted::new(Vec::<String>::new())));
        assert_eq!(
            deepwrought_commander(&mut state, content, POK, None, &mut own, &b, 3).unwrap(),
            0
        );
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

    /// A promise to use Doctor Sucaban for another seat is kept when the agent is exhausted for
    /// that seat's research (operator, 2026-09-23: agent use should be tradeable).
    #[test]
    fn doctor_sucaban_for_another_seat_keeps_a_promise_to_them() {
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
        state.diplomacy = ti4_model::DiplomacyState::for_players(&state.seating_order, true);
        let promise = ti4_model::DealTerm::UseLeaderFor {
            leader: "jolnaragent".to_owned(),
            beneficiary: researcher.clone(),
            deadline_round: state.round.max(1),
        };
        let revision =
            ti4_model::DealRevision::new(0, owner.clone(), vec![promise], vec![], state.round)
                .unwrap();
        let id = state
            .diplomacy
            .create_deal(owner.clone(), researcher.clone(), state.round, revision)
            .unwrap();
        state.diplomacy.active_deals.get_mut(&id).unwrap().status = ti4_model::DealStatus::Active;

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
            state.diplomacy.history.first().map(|deal| deal.status),
            Some(ti4_model::DealStatus::Fulfilled),
            "the promised use happened, so the deal is kept"
        );
    }

    /// Ssruu's printed text is every other faction agent's text "even if that agent is
    /// exhausted". A Ssruu holder who has infantry and no resources can therefore pay for
    /// research when Doctor Sucaban is already spent -- and only Ssruu is exhausted, never the
    /// agent being copied.
    #[test]
    fn ssruu_copies_sucaban_even_when_the_agent_is_exhausted() {
        let content = ContentStore::embedded();
        let sources = ti4_model::content_types::DEFAULT;
        let (researcher, owner) = (PlayerId::new("a"), PlayerId::new("b"));
        let mut state = game(&["a", "b"]);
        let system = SystemId::new(crate::fixtures::plain_systems(1)[0].clone());
        state.board.entry(system.clone()).or_default();
        crate::fixtures::put(&mut state, &system, "infantry", &researcher, 4);
        if let Some(seat) = state.player_mut(&researcher) {
            seat.trade_goods = 0;
            seat.leaders.insert(
                ti4_model::id::LeaderId::new("yssarilagent"),
                ti4_model::state::LeaderStatus::Readied,
            );
        }
        if let Some(seat) = state.player_mut(&owner) {
            seat.leaders.insert(
                ti4_model::id::LeaderId::new("jolnaragent"),
                ti4_model::state::LeaderStatus::Exhausted,
            );
        }
        let before = state.player(&researcher).unwrap().technologies.len();

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
            "the copy opens the same window the agent does"
        );
        assert_eq!(
            state.system_state(&system).units_of(&researcher).len(),
            0,
            "the researcher pays with their own infantry, not the source's"
        );
        assert_eq!(
            state
                .player(&researcher)
                .unwrap()
                .leaders
                .get(&ti4_model::id::LeaderId::new("yssarilagent")),
            Some(&ti4_model::state::LeaderStatus::Exhausted),
            "the copy is what was spent"
        );
        assert_eq!(
            state
                .player(&owner)
                .unwrap()
                .leaders
                .get(&ti4_model::id::LeaderId::new("jolnaragent")),
            Some(&ti4_model::state::LeaderStatus::Exhausted),
            "the copied agent was already exhausted and is left alone"
        );
    }

    /// Simultaneous rights follow the action-phase timing order. Here the borrower is active, so
    /// their copy is offered before the native owner even though native was inserted first.
    #[test]
    fn the_active_borrowers_copy_precedes_native_sucaban() {
        let content = ContentStore::embedded();
        let sources = ti4_model::content_types::DEFAULT;
        let (researcher, owner) = (PlayerId::new("a"), PlayerId::new("b"));
        let mut state = game(&["a", "b"]);
        let system = SystemId::new(crate::fixtures::plain_systems(1)[0].clone());
        state.board.entry(system.clone()).or_default();
        crate::fixtures::put(&mut state, &system, "infantry", &researcher, 4);
        if let Some(seat) = state.player_mut(&researcher) {
            seat.trade_goods = 0;
            seat.leaders.insert(
                ti4_model::id::LeaderId::new("yssarilagent"),
                ti4_model::state::LeaderStatus::Readied,
            );
        }
        if let Some(seat) = state.player_mut(&owner) {
            seat.leaders.insert(
                ti4_model::id::LeaderId::new("jolnaragent"),
                ti4_model::state::LeaderStatus::Readied,
            );
        }
        state.active = Some(researcher.clone());
        let (captured, seen) = crate::choice::Capturing::new(Box::new(crate::choice::FirstOption));
        let mut table = Table::with_default(Box::new(captured));
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

        let prompts: Vec<String> = seen
            .borrow()
            .iter()
            .filter(|choice| choice.prompt.contains("Sucaban") || choice.prompt.contains("Ssruu"))
            .map(|choice| choice.prompt.clone())
            .collect();
        assert_eq!(
            prompts.first().map(String::as_str),
            Some("Ssruu: copy b's Doctor Sucaban to trade infantry for research"),
            "the active borrower's right is first"
        );
        assert_eq!(prompts.len(), 1, "an accepted copy closes the native offer");
        assert_eq!(
            state
                .player(&researcher)
                .unwrap()
                .leaders
                .get(&ti4_model::id::LeaderId::new("yssarilagent")),
            Some(&ti4_model::state::LeaderStatus::Exhausted),
            "Ssruu is the card spent"
        );
        assert_eq!(
            state
                .player(&owner)
                .unwrap()
                .leaders
                .get(&ti4_model::id::LeaderId::new("jolnaragent")),
            Some(&ti4_model::state::LeaderStatus::Readied),
            "the exact source card is unchanged"
        );
    }

    /// Declining the agent does not close the copy: the two rights are independent, and the
    /// same table can be asked again in the same transaction.
    #[test]
    fn a_declined_agent_leaves_the_copy_available() {
        let content = ContentStore::embedded();
        let sources = ti4_model::content_types::DEFAULT;
        let (researcher, owner) = (PlayerId::new("a"), PlayerId::new("b"));
        let mut state = game(&["a", "b"]);
        let system = SystemId::new(crate::fixtures::plain_systems(1)[0].clone());
        state.board.entry(system.clone()).or_default();
        crate::fixtures::put(&mut state, &system, "infantry", &researcher, 4);
        if let Some(seat) = state.player_mut(&researcher) {
            seat.trade_goods = 0;
            seat.leaders.insert(
                ti4_model::id::LeaderId::new("yssarilagent"),
                ti4_model::state::LeaderStatus::Readied,
            );
        }
        if let Some(seat) = state.player_mut(&owner) {
            seat.leaders.insert(
                ti4_model::id::LeaderId::new("jolnaragent"),
                ti4_model::state::LeaderStatus::Readied,
            );
        }
        state.active = Some(owner.clone());
        let before = state.player(&researcher).unwrap().technologies.len();
        // The owner says no; everything after that falls back to the first option.
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new(vec![
            "decline".to_owned(),
        ])));
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

        let native = table.log.records.first().expect("native offer was first");
        assert_eq!(
            native.prompt,
            "Doctor Sucaban: exhaust to let a trade infantry for research"
        );
        assert_eq!(
            native
                .context
                .as_ref()
                .map(|context| context.subtype.as_str()),
            Some("doctor_sucaban_exhaust"),
            "the native decision identity stays stable"
        );

        assert_eq!(
            state.player(&researcher).unwrap().technologies.len(),
            before + 1,
            "the research still happens, through the copy"
        );
        assert_eq!(
            state
                .player(&owner)
                .unwrap()
                .leaders
                .get(&ti4_model::id::LeaderId::new("jolnaragent")),
            Some(&ti4_model::state::LeaderStatus::Readied),
            "a declined agent is not spent"
        );
        assert_eq!(
            state
                .player(&researcher)
                .unwrap()
                .leaders
                .get(&ti4_model::id::LeaderId::new("yssarilagent")),
            Some(&ti4_model::state::LeaderStatus::Exhausted),
            "the copy is"
        );
    }

    /// There is no window without something to trade: an offer to a seat with no infantry would
    /// be a prompt that can only be declined.
    #[test]
    fn sucaban_is_not_offered_without_infantry() {
        let content = ContentStore::embedded();
        let sources = ti4_model::content_types::DEFAULT;
        let (researcher, owner) = (PlayerId::new("a"), PlayerId::new("b"));
        let mut state = game(&["a", "b"]);
        if let Some(seat) = state.player_mut(&researcher) {
            seat.trade_goods = 0;
            seat.leaders.insert(
                ti4_model::id::LeaderId::new("yssarilagent"),
                ti4_model::state::LeaderStatus::Readied,
            );
        }
        if let Some(seat) = state.player_mut(&owner) {
            seat.leaders.insert(
                ti4_model::id::LeaderId::new("jolnaragent"),
                ti4_model::state::LeaderStatus::Exhausted,
            );
        }
        let before = state.player(&researcher).unwrap().technologies.len();
        let (captured, seen) =
            crate::choice::Capturing::new(Box::new(crate::choice::AlwaysDecline));
        let mut table = Table::with_default(Box::new(captured));
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

        assert!(
            !seen
                .borrow()
                .iter()
                .any(|choice| choice.prompt.contains("Sucaban") || choice.prompt.contains("Ssruu")),
            "nothing was asked, because there was nothing to trade"
        );
        assert_eq!(
            state.player(&researcher).unwrap().technologies.len(),
            before,
            "and no research happened"
        );
        assert_eq!(
            state
                .player(&researcher)
                .unwrap()
                .leaders
                .get(&ti4_model::id::LeaderId::new("yssarilagent")),
            Some(&ti4_model::state::LeaderStatus::Readied),
            "an unopened window does not spend the card"
        );
    }

    /// A copied attempt is one transaction: an invalid answer after one infantry was removed
    /// restores state and the decision log, without rewinding the scripted input. The same table
    /// can then retry successfully, and the borrowed-use hook runs exactly once on that success.
    #[test]
    fn a_failed_borrowed_payment_rolls_back_and_the_same_table_retries() {
        let content = ContentStore::embedded();
        let sources = ti4_model::content_types::DEFAULT;
        let (researcher, owner) = (PlayerId::new("a"), PlayerId::new("b"));
        let mut state = game(&["a", "b"]);
        let system = SystemId::new(crate::fixtures::plain_systems(1)[0].clone());
        state.board.entry(system.clone()).or_default();
        crate::fixtures::put(&mut state, &system, "infantry", &researcher, 4);
        if let Some(seat) = state.player_mut(&researcher) {
            seat.trade_goods = 0;
            seat.leaders.insert(
                ti4_model::id::LeaderId::new("yssarilagent"),
                ti4_model::state::LeaderStatus::Readied,
            );
        }
        if let Some(seat) = state.player_mut(&owner) {
            seat.leaders.insert(
                ti4_model::id::LeaderId::new("jolnaragent"),
                ti4_model::state::LeaderStatus::Exhausted,
            );
        }
        let before = state.clone();
        let before_tech = state.player(&researcher).unwrap().technologies.len();
        let site = format!("{system}:");
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([
            "yes".to_owned(),
            site,
            "not-an-offered-infantry".to_owned(),
            "yes".to_owned(),
        ])));
        let log_before = table.log.clone();
        let hook = crate::factions::hooks_cards::CardHooks {
            borrowed_agent_used_state: Some(|state, who, source| {
                let key = format!("test:sucaban-borrowed:{who}");
                let count = state
                    .faction_marks
                    .get(&key)
                    .and_then(|value| value.rsplit_once('|'))
                    .and_then(|(_, count)| count.parse::<usize>().ok())
                    .unwrap_or(0)
                    + 1;
                state.faction_marks.insert(key, format!("{source}|{count}"));
            }),
            ..crate::factions::hooks_cards::CardHooks::NONE
        };
        crate::factions::hooks_cards::with_test_hooks(hook, || {
            assert!(
                secondary(
                    &mut state,
                    content,
                    sources,
                    None,
                    &mut table,
                    &researcher,
                    &card("Technology"),
                )
                .is_err(),
                "the second infantry answer was not offered"
            );
            assert_eq!(state, before, "the copied attempt is state-atomic");
            assert_eq!(
                table.log, log_before,
                "the failed attempt leaves no replay records"
            );

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
        });
        assert_eq!(
            state.player(&researcher).unwrap().technologies.len(),
            before_tech + 1,
            "the retry completes the paid research"
        );
        assert_eq!(state.system_state(&system).units_of(&researcher).len(), 0);
        assert_eq!(
            state
                .faction_marks
                .get(&format!("test:sucaban-borrowed:{researcher}"))
                .map(String::as_str),
            Some("jolnaragent|1"),
            "the state-only borrowed hook runs once, after success only"
        );
    }

    /// The Technology primary's paid second half is the other live producer of this window, and
    /// it uses the same seam.
    #[test]
    fn the_technology_primary_also_offers_the_copy() {
        let content = ContentStore::embedded();
        let sources = ti4_model::content_types::DEFAULT;
        let researcher = PlayerId::new("a");
        let mut state = game(&["a", "b"]);
        let system = SystemId::new(crate::fixtures::plain_systems(1)[0].clone());
        state.board.entry(system.clone()).or_default();
        crate::fixtures::put(&mut state, &system, "infantry", &researcher, 6);
        if let Some(seat) = state.player_mut(&researcher) {
            seat.trade_goods = 0;
            seat.leaders.insert(
                ti4_model::id::LeaderId::new("yssarilagent"),
                ti4_model::state::LeaderStatus::Readied,
            );
        }
        if let Some(seat) = state.player_mut(&PlayerId::new("b")) {
            seat.leaders.insert(
                ti4_model::id::LeaderId::new("jolnaragent"),
                ti4_model::state::LeaderStatus::Exhausted,
            );
        }
        let before = state.player(&researcher).unwrap().technologies.len();
        let mut table = Table::with_default(Box::new(crate::choice::FirstOption));
        primary(
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
            before + 2,
            "the primary's free half and its paid half both resolve here"
        );
        assert_eq!(
            state
                .player(&researcher)
                .unwrap()
                .leaders
                .get(&ti4_model::id::LeaderId::new("yssarilagent")),
            Some(&ti4_model::state::LeaderStatus::Exhausted),
            "and the copy is what paid for it"
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
        let production_limit =
            crate::production::capacity(&state, ContentStore::embedded(), POK, &player, &system);
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
        assert!(
            i64::try_from(after - before).unwrap_or(i64::MAX) <= production_limit,
            "Warfare's secondary is one use of PRODUCTION and cannot reset its limit between purchases"
        );
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

    #[test]
    fn leadership_carries_planet_overpayment_across_command_tokens() {
        // 52.3 is one "spend any amount of influence" transaction. Arcturus (4) pays for the
        // first token and leaves one influence toward the second; Arinam (2) then reaches six.
        // Treating the tokens as isolated bills discarded that one and either stopped early or
        // demanded a third influence, charging seven printed influence for two tokens.
        let content = ContentStore::embedded();
        let mut state = game(&["a"]);
        let actor = PlayerId::new("a");
        for (system, planet) in [("53", "arcturus"), ("37", "arinam")] {
            state
                .system_mut(&SystemId::new(system))
                .set_control(PlanetId::new(planet), actor.clone());
        }
        let before = state.player(&actor).unwrap().strategic_tokens;
        let (recorder, seen) = SpeakerRecording::new(&[
            "yes",
            "exhaust|arcturus",
            "strategic_tokens",
            "yes",
            "strategic_tokens",
        ]);
        let mut table = Table::with_default(Box::new(recorder));

        buy_tokens_with_influence(&mut state, content, POK, None, &mut table, &actor).unwrap();

        let prompts: Vec<String> = seen
            .borrow()
            .iter()
            .map(|(prompt, _)| prompt.clone())
            .collect();
        assert!(
            prompts
                .iter()
                .any(|prompt| prompt == "spend 2 more influence for a command token"),
            "the retained fourth influence reduces the next instalment: {prompts:?}"
        );
        assert!(state.exhausted_planets.contains(&PlanetId::new("arcturus")));
        assert!(state.exhausted_planets.contains(&PlanetId::new("arinam")));
        assert_eq!(
            state.player(&actor).unwrap().strategic_tokens,
            before + 2,
            "exactly six influence buys two command tokens"
        );
    }
}

#[cfg(test)]
mod research_carries_its_price {
    use super::*;

    /// Each research option carries what it costs, as a NUMBER in the payload.
    ///
    /// The three ways to research have three different prices -- the Technology primary's first is
    /// free, its second is six resources, the secondary is a command token plus four -- and a bare
    /// list of technology names asks a policy to judge whether one is worth buying while hiding the
    /// price. A numeric payload entry becomes `payload-number:cost`, which the feature projection
    /// turns into a real input; a label would be read by nobody.
    #[test]
    fn every_research_option_states_its_cost() {
        let content = ContentStore::embedded();
        let id = TechnologyId::new("cv2");

        let free = research_option(content, &id, 0, 0, 3);
        let second = research_option(content, &id, TECHNOLOGY_PRIMARY_SECOND_COST, 0, 3);
        let secondary = research_option(content, &id, TECHNOLOGY_SECONDARY_COST, 1, 3);

        for (option, resources, tokens) in [
            (&free, 0, 0),
            (&second, TECHNOLOGY_PRIMARY_SECOND_COST, 0),
            (&secondary, TECHNOLOGY_SECONDARY_COST, 1),
        ] {
            assert_eq!(
                option
                    .payload
                    .get("cost")
                    .and_then(serde_json::Value::as_i64),
                Some(resources),
                "the resource price is on the option"
            );
            assert_eq!(
                option
                    .payload
                    .get("cost_tokens")
                    .and_then(serde_json::Value::as_i64),
                Some(tokens),
                "the token price is on the option"
            );
            assert_eq!(option.kind, RESEARCH_KIND);
            assert_eq!(
                option.id,
                id.to_string(),
                "the id still names the technology"
            );
        }

        assert_ne!(
            free.payload.get("cost"),
            second.payload.get("cost"),
            "the free research and the six-resource one must not look alike"
        );
    }
}

#[cfg(test)]
mod bf_f3_tests {
    use super::*;
    use ti4_model::content_types::POK;

    #[test]
    fn trade_stages_its_gain_only_when_a_module_seat_could_react() {
        let content = ContentStore::embedded();
        let trade = card_id(content);
        let mut plain = crate::fixtures::game(&["a", "b"]);
        let mut table = Table::new();
        primary(
            &mut plain,
            content,
            POK,
            None,
            &mut table,
            &PlayerId::new("a"),
            &trade,
        )
        .unwrap();
        assert_eq!(plain.player(&PlayerId::new("a")).unwrap().trade_goods, 3);
        assert_eq!(crate::supply::staged_events(&plain), 0);

        let mut state = crate::fixtures::seated_game(&[("a", "mentak"), ("b", "sol")], POK);
        let before = state.player(&PlayerId::new("a")).unwrap().trade_goods;
        primary(
            &mut state,
            content,
            POK,
            None,
            &mut table,
            &PlayerId::new("a"),
            &trade,
        )
        .unwrap();
        assert_eq!(
            state.player(&PlayerId::new("a")).unwrap().trade_goods,
            before + 3
        );
        assert_eq!(
            crate::supply::staged_event_types(&state),
            ["TRADE_GOODS_GAINED"]
        );
    }

    fn card_id(content: &ContentStore) -> String {
        content
            .records(ti4_model::content_types::ContentType::StrategyCards)
            .iter()
            .find(|record| {
                record.text("name") == Some("Trade")
                    && record.text("id").is_some_and(|id| id.starts_with("pok"))
            })
            .and_then(|record| record.text("id"))
            .map(ToOwned::to_owned)
            .expect("a Trade card")
    }
}
