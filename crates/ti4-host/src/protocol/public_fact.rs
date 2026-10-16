//! The typed public fact of a resolved decision ([`PublicFact`]), for the log of a client.
//!
//! The same rule as the `detail` line of an event: a fact is built only from the option the
//! engine offered and the seat chose, and from public state. It carries ids only (seats, units,
//! systems, planets, cards, technologies); the client names them through its catalogue. What a
//! fact cannot derive verifiably it leaves out. Never in a fact: a secret objective before it is
//! scored, an action card before it is played (reaction offers, a hand, a discard), the cards or
//! notes of a transaction, an agenda that was looked at, a hidden roll.

use serde::{Deserialize, Serialize};
use ti4_engine::choice::{ChoiceOption, DecisionRecord};
use ti4_engine::decision_context::{DecisionContext, DecisionSource, DecisionTarget};
use ti4_model::id::{ObjectiveId, PlayerId};
use ti4_model::state::GameState;

/// What one resolved decision did, in public ids.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum PublicFact {
    /// The action of a turn: the choice of the action menu, or the second action of a turn.
    ActionStarted {
        action: TurnAction,
    },
    /// The seat ended its turn.
    TurnEnded,
    /// A transaction or a diplomatic contact opened with `with` (it takes no action).
    TransactionOpened {
        with: PlayerId,
    },
    /// A proposal to `to`. What it offers is not public.
    TransactionProposed {
        to: PlayerId,
    },
    /// The seat ended the negotiation with `with` without a further proposal.
    NegotiationEnded {
        with: PlayerId,
    },
    /// The answer of the seat to the proposal of `proposer`.
    TransactionAnswered {
        proposer: PlayerId,
        answer: TransactionAnswer,
    },
    Activated {
        system: String,
    },
    Moved {
        unit: String,
        from: String,
        to: String,
    },
    /// A unit picked up for transport: in `system`, from `planet` or from space.
    Loaded {
        unit: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        system: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        planet: Option<String>,
    },
    /// Units produced in `system`. `planet` is where they are placed when the build said so;
    /// otherwise a [`PublicFact::UnitPlaced`] follows.
    Produced {
        unit: String,
        count: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        system: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        planet: Option<String>,
    },
    /// Where a produced unit was placed, when production asked.
    UnitPlaced {
        unit: String,
        system: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        planet: Option<String>,
    },
    /// One step of a payment.
    Paid {
        with: PaidWith,
        spend: Spend,
    },
    StructurePlaced {
        unit: String,
        system: String,
        planet: String,
    },
    /// A unit the seat chose to lose.
    Casualty {
        seat: PlayerId,
        unit: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        system: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        planet: Option<String>,
        damaged: bool,
    },
    Sustained {
        seat: PlayerId,
        unit: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        system: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        planet: Option<String>,
    },
    /// The seat announced a retreat (`retreat`), or stayed to fight.
    RetreatAnnounced {
        seat: PlayerId,
        system: String,
        retreat: bool,
    },
    Retreated {
        seat: PlayerId,
        from: String,
        to: String,
    },
    Landed {
        unit: String,
        planet: String,
        damaged: bool,
    },
    Researched {
        tech: String,
    },
    StrategyPicked {
        card: String,
    },
    /// A vote for `outcome`; `None` is an abstention.
    Voted {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        outcome: Option<String>,
    },
    /// The speaker broke a tie for `outcome`.
    TieBroken {
        outcome: String,
    },
    PlanetExhaustedForVote {
        planet: String,
    },
    /// An action card the engine announced as played.
    CardPlayed {
        card: String,
    },
    /// An objective scored; a secret is named only once it is scored.
    ObjectiveScored {
        objective: String,
        secret: bool,
    },
    TokensGained {
        pool: TokenPool,
        count: u32,
    },
    /// The answer of a seat to the secondary of `card` (an id): it followed or declined. What a
    /// follow does is told by the facts after it (a research, a structure, a production...); a
    /// secondary with no question (Trade, Politics, Imperial) is this fact alone. Leadership's
    /// follow is its first purchase: [`PublicFact::Paid`] and [`PublicFact::TokensGained`]
    /// follow.
    SecondaryFollowed {
        card: String,
        followed: bool,
    },
    /// The seat buys one more command token for influence (Leadership). The token is the
    /// [`PublicFact::TokensGained`] after the payment, so a count of tokens counts that fact, not
    /// this one. The stop of the purchase has no fact.
    TokenBoughtWithInfluence,
    /// The system of Diplomacy's primary.
    DiplomacyChosen {
        system: String,
    },
    /// The new speaker (Politics).
    SpeakerChosen {
        seat: PlayerId,
    },
    /// The commodities of `seat` replenished (Trade's primary). The end of the replenishing has
    /// no fact.
    CommoditiesReplenished {
        seat: PlayerId,
    },
    PlanetReadied {
        planet: String,
    },
    /// The command sheet after a redistribution (the status phase, Warfare).
    TokensRedistributed {
        tactic: u32,
        fleet: u32,
        strategy: u32,
    },
    /// The command token taken back from `system` (Warfare's primary).
    TokenRecalled {
        system: String,
    },
    /// The system of a tactical action without a command token (Thunder's Edge Warfare).
    FreeTactical {
        system: String,
    },
    /// A round of ground combat on `planet`; its dice are the rolls of the action.
    GroundCombatRound {
        seat: PlayerId,
        planet: String,
    },
    /// The seat started a further ground combat on `planet` (coexistence).
    GroundCombatStarted {
        seat: PlayerId,
        planet: String,
    },
    /// The seat did not sustain damage against a hit.
    SustainDeclined {
        seat: PlayerId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        system: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        planet: Option<String>,
    },
    /// Bombardment hits on `planet` go to the units of `target` (coexistence).
    BombardmentTargeted {
        target: PlayerId,
        planet: String,
    },
    /// The seat removed the custodians token in `system`.
    CustodiansRemoved {
        system: String,
    },
    /// A unit the seat removed over its fleet supply or capacity in `system`.
    UnitRemoved {
        unit: String,
        system: String,
    },
}

/// The action a turn took.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum TurnAction {
    Tactical,
    /// The strategic action of `card`; `None` when the table no longer says which card it was.
    Strategic {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        card: Option<String>,
    },
    Component {
        source: ComponentSource,
    },
    Pass,
}

/// The component that a component action uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ComponentSource {
    Technology {
        tech: String,
    },
    Leader {
        leader: String,
    },
    /// An action card from the hand or the salvage yard. Which card is a [`PublicFact::CardPlayed`]
    /// once the engine announces it; the menu option does not say.
    ActionCard,
    Relic {
        relic: String,
    },
    /// Three relic fragments of `fragment` (a planet trait, or "frontier") purged for a relic.
    RelicFragments {
        fragment: String,
    },
    /// A card of the play area (an exploration card).
    Exploration {
        card: String,
    },
    /// A faction ability, a faction promissory note or a faction card: the id after "faction|".
    Faction {
        ability: String,
    },
    /// A Thunder's Edge expedition.
    Expedition,
    /// A payment owed on a deal.
    DealPayment,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum TransactionAnswer {
    Accept,
    Refuse,
    Counter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum PaidWith {
    Planet { planet: String },
    TradeGood,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Spend {
    Resources,
    Influence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum TokenPool {
    Tactic,
    Fleet,
    Strategy,
}

fn text<'a>(option: &'a ChoiceOption, key: &str) -> Option<&'a str> {
    option.payload.get(key).and_then(serde_json::Value::as_str)
}

fn flag(option: &ChoiceOption, key: &str) -> bool {
    option.payload.get(key).and_then(serde_json::Value::as_bool) == Some(true)
}

fn count(option: &ChoiceOption) -> Option<u32> {
    option
        .payload
        .get("count")
        .and_then(serde_json::Value::as_u64)
        .and_then(|count| u32::try_from(count).ok())
}

/// The system and the planet of a decision about a place.
fn place(context: &DecisionContext) -> (Option<String>, Option<String>) {
    match context.target.as_ref() {
        Some(DecisionTarget::System(system) | DecisionTarget::Unit { system, .. }) => {
            (Some(system.to_string()), None)
        }
        Some(DecisionTarget::Planet { system, planet }) => {
            (Some(system.to_string()), Some(planet.to_string()))
        }
        _ => None.unzip(),
    }
}

/// The seat that a decision is about (the partner of a transaction).
fn about_seat(context: &DecisionContext) -> Option<PlayerId> {
    match context.target.as_ref()? {
        DecisionTarget::Player(seat) => Some(seat.clone()),
        _ => None,
    }
}

/// The system and the planet of a production spot: "space", a planet, or "system@spot".
fn spot(producing: &str, spot: &str) -> (String, Option<String>) {
    let (system, at) =
        ti4_engine::production::placement_target(&ti4_model::id::SystemId::new(producing), spot);
    (
        system.to_string(),
        (at != ti4_engine::production::SPACE && !at.is_empty()).then(|| at.to_owned()),
    )
}

/// The strategy card of a bare "strategic" option: the only card of the seat, or else its only
/// card that is not yet used (the action is still under way when its decision is published).
fn sole_strategy_card(state: &GameState, seat: &PlayerId) -> Option<String> {
    let player = state.player(seat)?;
    if let [only] = player.strategy_cards.as_slice() {
        return Some(only.to_string());
    }
    match player.unused_strategy_cards().as_slice() {
        [only] => Some(only.to_string()),
        _ => None,
    }
}

/// The component of a component action, from the id of its menu option.
fn component_source(id: &str) -> ComponentSource {
    let segment = |rest: &str| rest.split('|').next().unwrap_or_default().to_owned();
    if let Some(tech) = id.strip_prefix("component|tech|") {
        ComponentSource::Technology {
            tech: segment(tech),
        }
    } else if let Some(leader) = id.strip_prefix("component|leader|") {
        ComponentSource::Leader {
            leader: segment(leader),
        }
    } else if id.starts_with("action_card|") {
        // The id names a position in the hand, never the card.
        ComponentSource::ActionCard
    } else if let Some(relic) = id.strip_prefix("relic|") {
        ComponentSource::Relic {
            relic: segment(relic),
        }
    } else if let Some(fragment) = id.strip_prefix("purge|") {
        ComponentSource::RelicFragments {
            fragment: segment(fragment),
        }
    } else if let Some(card) = id.strip_prefix("play_area:") {
        ComponentSource::Exploration {
            card: card.split(':').next().unwrap_or_default().to_owned(),
        }
    } else if let Some(ability) = id.strip_prefix("faction|") {
        ComponentSource::Faction {
            ability: ability.to_owned(),
        }
    } else if id.starts_with("component|expedition|") {
        ComponentSource::Expedition
    } else if id.starts_with("component|diplomacy-payment|") {
        ComponentSource::DealPayment
    } else {
        ComponentSource::Other
    }
}

/// The id of the card of a follower's secondary question.
fn secondary_card(context: &DecisionContext) -> Option<&str> {
    match &context.source {
        DecisionSource::StrategyCard {
            card,
            secondary: true,
        } => Some(card.as_str()),
        _ => None,
    }
}

/// The seat of the faction that an option names by its id (Politics' speaker, Trade's
/// replenish).
fn seat_of(state: &GameState, faction: &str) -> Option<PlayerId> {
    ti4_engine::promissory::seat_of(state, faction)
}

/// The fact of a choice of a strategy card's primary or secondary, or of the status phase.
///
/// The closing steps (the end of a purchase or of a replenishing, a declined structure, research
/// or Imperial score) have no fact: they end a part whose facts came before.
fn strategy_fact(
    context: &DecisionContext,
    option: &ChoiceOption,
    state: &GameState,
) -> Option<PublicFact> {
    let id = option.id.as_str();
    Some(match (context.subtype.as_str(), option.kind.as_str()) {
        (ti4_engine::strategy::FOLLOW_SUBTYPE, _) => {
            let card = secondary_card(context)?;
            ti4_engine::strategy_cards::card_name(ti4_content::ContentStore::embedded(), card)?;
            PublicFact::SecondaryFollowed {
                card: card.to_owned(),
                followed: !(option.is_decline() || id == "no"),
            }
        }
        ("buy_token_with_influence", ti4_engine::strategy::STRATEGY_KIND) => {
            match (secondary_card(context), id) {
                // The window's question: "yes" buys the first token, Enervate's option takes the
                // primary instead; both follow.
                (Some(card), _) => {
                    ti4_engine::strategy_cards::card_name(
                        ti4_content::ContentStore::embedded(),
                        card,
                    )?;
                    PublicFact::SecondaryFollowed {
                        card: card.to_owned(),
                        followed: !(option.is_decline() || id == "no"),
                    }
                }
                (None, "yes") => PublicFact::TokenBoughtWithInfluence,
                _ => return None,
            }
        }
        ("diplomacy_choose_system", "system") => PublicFact::DiplomacyChosen {
            system: id.to_owned(),
        },
        ("politics_choose_speaker", "speaker") => PublicFact::SpeakerChosen {
            seat: seat_of(state, id)?,
        },
        ("trade_choose_replenish", "replenish") => PublicFact::CommoditiesReplenished {
            seat: seat_of(state, id)?,
        },
        ("ready_planet", "ready") => PublicFact::PlanetReadied {
            planet: id.to_owned(),
        },
        (subtype, ti4_engine::tokens::REDISTRIBUTE_KIND)
            if subtype.ends_with("_redistribute_tokens") =>
        {
            let mut parts = id.split('|').map(str::parse::<u32>);
            let (Some(Ok(tactic)), Some(Ok(fleet)), Some(Ok(strategy)), None) =
                (parts.next(), parts.next(), parts.next(), parts.next())
            else {
                return None;
            };
            PublicFact::TokensRedistributed {
                tactic,
                fleet,
                strategy,
            }
        }
        ("warfare_recall_token", "recall") => PublicFact::TokenRecalled {
            system: id.to_owned(),
        },
        ("warfare_free_tactical", "activate") => PublicFact::FreeTactical {
            system: id.to_owned(),
        },
        (ti4_engine::fleet::REMOVE_EXCESS_SUBTYPE, ti4_engine::fleet::REMOVE_KIND) => {
            // The label of the option is `remove <unit type>`, and the units on the board are
            // public; a label that names no unit type gives no fact.
            let unit = option.label.strip_prefix("remove ")?;
            ti4_content::units::unit_type(
                ti4_content::ContentStore::embedded(),
                unit,
                ti4_model::content_types::DEFAULT,
            )?;
            let (system, _) = place(context);
            PublicFact::UnitRemoved {
                unit: unit.to_owned(),
                system: system?,
            }
        }
        _ => return None,
    })
}

/// The fact of a choice of the action menu (or of the second action at the end of a turn).
fn turn_fact(state: &GameState, seat: &PlayerId, option: &ChoiceOption) -> Option<PublicFact> {
    let id = option.id.as_str();
    if id == ti4_engine::game::END_TURN_ID {
        return Some(PublicFact::TurnEnded);
    }
    if let Some(with) = ti4_engine::transactions::opens_with(state, option)
        .or_else(|| ti4_engine::diplomacy::candidates::contact_target(state, option))
    {
        return Some(PublicFact::TransactionOpened { with });
    }
    let action = match id {
        ti4_engine::game::TACTICAL_ACTION_ID => TurnAction::Tactical,
        "pass" => TurnAction::Pass,
        ti4_engine::strategy::STRATEGIC_ACTION_ID => TurnAction::Strategic {
            card: sole_strategy_card(state, seat),
        },
        id => match id.strip_prefix("strategic|") {
            Some(card) => TurnAction::Strategic {
                card: Some(card.to_owned()),
            },
            None => TurnAction::Component {
                source: component_source(id),
            },
        },
    };
    Some(PublicFact::ActionStarted { action })
}

/// The fact of a combat or invasion choice.
///
/// The closing steps (done committing, a declined further ground combat) have no fact: they end
/// a part whose facts came before.
fn combat_fact(
    context: &DecisionContext,
    option: &ChoiceOption,
    seat: &PlayerId,
    state: &GameState,
) -> Option<PublicFact> {
    let (system, planet) = place(context);
    let unit = || text(option, "unit").map(str::to_owned);
    Some(match (context.subtype.as_str(), option.kind.as_str()) {
        ("assign_casualty" | "assign_ground_casualty", "casualty" | "ground_casualty")
            if option.id.starts_with("destroy|") =>
        {
            PublicFact::Casualty {
                seat: seat.clone(),
                unit: unit()?,
                system,
                planet,
                damaged: flag(option, "damaged"),
            }
        }
        ("sustain_damage", "sustain") if option.id.starts_with("sustain|") => {
            PublicFact::Sustained {
                seat: seat.clone(),
                unit: unit()?,
                system,
                planet,
            }
        }
        ("announce_retreat", "retreat") if matches!(option.id.as_str(), "retreat" | "stay") => {
            PublicFact::RetreatAnnounced {
                seat: seat.clone(),
                system: system?,
                retreat: option.id == "retreat",
            }
        }
        ("retreat_to", "retreat_to") => {
            let to = text(option, "system").filter(|to| *to == option.id)?;
            PublicFact::Retreated {
                seat: seat.clone(),
                from: system?,
                to: to.to_owned(),
            }
        }
        ("sustain_damage", "decline") if option.id == "decline" => PublicFact::SustainDeclined {
            seat: seat.clone(),
            system,
            planet,
        },
        ("fight_ground_combat_round", "ground_casualty") if option.id == "fight" => {
            PublicFact::GroundCombatRound {
                seat: seat.clone(),
                planet: planet?,
            }
        }
        ("start_next_ground_combat", "ground_casualty") if option.id.starts_with("fight|") => {
            PublicFact::GroundCombatStarted {
                seat: seat.clone(),
                planet: planet?,
            }
        }
        ("bombardment_target", "bombardment_target") => PublicFact::BombardmentTargeted {
            target: state
                .player(&PlayerId::new(&option.id))
                .map(|target| target.id.clone())?,
            planet: planet?,
        },
        ("remove_custodians", "custodians") if option.id == "yes" => {
            PublicFact::CustodiansRemoved { system: system? }
        }
        ("commit_ground_forces", "commit") if option.id.starts_with("commit|") => {
            PublicFact::Landed {
                unit: unit()?,
                planet: text(option, "planet")?.to_owned(),
                damaged: flag(option, "damaged"),
            }
        }
        _ => return None,
    })
}

/// The fact of a choice that names its outcome in the id or an allowlisted payload field.
///
/// The closing steps (done moving, loading, producing, voting) have no fact: they end a part
/// whose facts came before.
#[allow(
    clippy::too_many_lines,
    reason = "one arm per kind of public decision, read as a table"
)]
fn choice_fact(
    context: &DecisionContext,
    option: &ChoiceOption,
    seat: &PlayerId,
    destination: Option<&str>,
    state: &GameState,
) -> Option<PublicFact> {
    let id = option.id.as_str();
    Some(match (context.subtype.as_str(), option.kind.as_str()) {
        ("draft_strategy_card", "strategy_card") => {
            ti4_engine::strategy_cards::card_name(ti4_content::ContentStore::embedded(), id)?;
            PublicFact::StrategyPicked {
                card: id.to_owned(),
            }
        }
        ("activate_system", "activate") => PublicFact::Activated {
            system: id.to_owned(),
        },
        ("movement_step", "move") => PublicFact::Moved {
            unit: text(option, "unit")?.to_owned(),
            from: text(option, "origin")?.to_owned(),
            to: destination?.to_owned(),
        },
        ("load_cargo", "load") => PublicFact::Loaded {
            unit: text(option, "unit")?.to_owned(),
            system: text(option, "pickup_system").map(str::to_owned),
            planet: text(option, "source").map(str::to_owned),
        },
        ("produce_unit", "produce") => {
            let system = text(option, "system");
            let (system, planet) = match (system, text(option, "destination")) {
                (Some(system), Some(destination)) => {
                    let (system, planet) = spot(system, destination);
                    (Some(system), planet)
                }
                (system, _) => (system.map(str::to_owned), None),
            };
            PublicFact::Produced {
                unit: text(option, "unit")?.to_owned(),
                count: count(option)?,
                system,
                planet,
            }
        }
        ("place_unit", "place") => {
            let (system, planet) = spot(text(option, "system")?, text(option, "destination")?);
            PublicFact::UnitPlaced {
                unit: text(option, "unit")?.to_owned(),
                system,
                planet,
            }
        }
        ("pay_resources" | "pay_influence", "pay") => {
            let spend = if context.subtype == "pay_resources" {
                Spend::Resources
            } else {
                Spend::Influence
            };
            let with = if id == "trade_good" {
                PaidWith::TradeGood
            } else {
                PaidWith::Planet {
                    planet: id
                        .strip_prefix("exhaust|")
                        .filter(|planet| !planet.is_empty())?
                        .to_owned(),
                }
            };
            PublicFact::Paid { with, spend }
        }
        ("place_structure", "build") => {
            let mut parts = id.split('|');
            let (Some(unit), Some(system), Some(planet), None) =
                (parts.next(), parts.next(), parts.next(), parts.next())
            else {
                return None;
            };
            PublicFact::StructurePlaced {
                unit: unit.to_owned(),
                system: system.to_owned(),
                planet: planet.to_owned(),
            }
        }
        ("research_technology", "research") => PublicFact::Researched {
            tech: id.to_owned(),
        },
        ("cast_vote", "vote") => PublicFact::Voted {
            outcome: Some(id.to_owned()),
        },
        ("cast_vote", _) if option.is_decline() => PublicFact::Voted { outcome: None },
        ("vote_tiebreak", "tiebreak") => PublicFact::TieBroken {
            outcome: id.to_owned(),
        },
        ("vote_exhaust_planet", "vote_planet") => PublicFact::PlanetExhaustedForVote {
            planet: id.strip_suffix("|resources").unwrap_or(id).to_owned(),
        },
        ("gain_command_token", "pool") => PublicFact::TokensGained {
            pool: match id {
                "tactic_tokens" => TokenPool::Tactic,
                "fleet_tokens" => TokenPool::Fleet,
                "strategic_tokens" => TokenPool::Strategy,
                _ => return None,
            },
            count: 1,
        },
        ("imperial_score_objective", "objective") => PublicFact::ObjectiveScored {
            objective: id.to_owned(),
            secret: false,
        },
        ("answer_transaction", _) => PublicFact::TransactionAnswered {
            proposer: about_seat(context)?,
            answer: match id {
                "accept" => TransactionAnswer::Accept,
                "refuse" => TransactionAnswer::Refuse,
                "counter" => TransactionAnswer::Counter,
                _ => return None,
            },
        },
        ("propose_transaction", _) if option.is_decline() => PublicFact::NegotiationEnded {
            with: about_seat(context)?,
        },
        // The offer itself (its cards, its notes) is not public; that one was made is.
        ("propose_transaction", _) => PublicFact::TransactionProposed {
            to: about_seat(context)?,
        },
        _ => {
            return strategy_fact(context, option, state)
                .or_else(|| combat_fact(context, option, seat, state));
        }
    })
}

/// The fact of a question that has no context: Sling Relay's ship, whose option says its unit and
/// its system. Its question of the system has no fact (the production names it). The engine
/// leaves these questions without a context, so that the features of the learned policy stay
/// what they were trained on.
fn uncontexted_fact(option: &ChoiceOption) -> Option<PublicFact> {
    (option.kind == ti4_engine::production::PRODUCE_KIND
        && flag(option, "sling_relay")
        && option.id.starts_with("build|"))
    .then(|| {
        Some(PublicFact::Produced {
            unit: text(option, "unit")?.to_owned(),
            count: count(option)?,
            system: Some(text(option, "system")?.to_owned()),
            planet: None,
        })
    })
    .flatten()
}

/// The public fact of `record`, from the option the engine offered for it and public state.
///
/// `offered` must be the option the engine offered with the id the record chose; anything else
/// gives no fact. `destination` is the active system of a movement.
#[must_use]
pub fn decision_fact(
    record: &DecisionRecord,
    offered: Option<&ChoiceOption>,
    destination: Option<&str>,
    state: &GameState,
) -> Option<PublicFact> {
    let option = offered.filter(|option| option.id == record.chosen)?;
    let seat = &record.player;
    let subtype = record
        .context
        .as_ref()
        .map(|context| context.subtype.as_str());
    if record.prompt == "action phase" || matches!(subtype, Some("action_menu" | "end_turn")) {
        return turn_fact(state, seat, option);
    }
    let Some(context) = record.context.as_ref() else {
        return uncontexted_fact(option);
    };
    if matches!(
        context.subtype.as_str(),
        "score_objective" | "score_secret_objective"
    ) {
        // A scored secret is revealed; an offered but unscored one is not.
        return (option.kind == "score"
            && state
                .scored_objectives
                .get(seat)
                .is_some_and(|scored| scored.contains(&ObjectiveId::new(&option.id))))
        .then(|| PublicFact::ObjectiveScored {
            objective: option.id.clone(),
            secret: context.subtype == "score_secret_objective"
                || ti4_content::secret_objectives::group_of(&option.id).is_some(),
        });
    }
    if super::server::played_card_detail(context, option, seat).is_some() {
        // Only a card the engine announced as played; selecting an offer is not enough.
        let alias = text(option, "card")?;
        return state
            .action_card_plays
            .iter()
            .any(|(player, card)| player == seat && card.as_str() == alias)
            .then(|| PublicFact::CardPlayed {
                card: alias.to_owned(),
            });
    }
    choice_fact(context, option, seat, destination, state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::server::{
        EventVisibility, GameEvent, GameEventKind, verified_decision_facts, verified_event_facts,
    };
    use crate::protocol::status::ViewerRole;
    use std::collections::BTreeMap;
    use ti4_engine::decision_context::DecisionSource;
    use ti4_model::id::{ActionCardId, FactionId, PlanetId, StrategyCardId, SystemId};
    use ti4_model::state::Phase;

    fn p(id: &str) -> PlayerId {
        PlayerId::new(id)
    }

    fn state() -> GameState {
        let mut state = GameState::new(&[p("p1"), p("p2")], &[], BTreeMap::new(), None, 42);
        state.player_mut(&p("p1")).unwrap().faction = FactionId::new("sol");
        state.player_mut(&p("p2")).unwrap().faction = FactionId::new("hacan");
        state
    }

    fn record(subtype: &str, option: &ChoiceOption) -> DecisionRecord {
        DecisionRecord {
            player: p("p1"),
            prompt: "test".into(),
            chosen: option.id.clone(),
            offered: vec![option.id.clone()],
            context: Some(DecisionContext::new(
                p("p1"),
                DecisionSource::Rule("test".into()),
                subtype,
                Phase::Action,
                1,
            )),
        }
    }

    fn about(mut record: DecisionRecord, target: DecisionTarget) -> DecisionRecord {
        record.context.as_mut().unwrap().target = Some(target);
        record
    }

    fn fact(
        record: &DecisionRecord,
        option: &ChoiceOption,
        state: &GameState,
    ) -> Option<PublicFact> {
        decision_fact(record, Some(option), Some("22"), state)
    }

    fn menu(id: &str, kind: &str) -> (DecisionRecord, ChoiceOption) {
        let option = ChoiceOption::labelled(id, kind, "play Sabotage");
        let record = DecisionRecord {
            prompt: "action phase".into(),
            ..record("action_menu", &option)
        };
        (record, option)
    }

    #[test]
    fn the_action_of_a_turn_names_its_kind_and_its_component() {
        let mut state = state();
        state.player_mut(&p("p1")).unwrap().strategy_cards =
            vec![StrategyCardId::new("pok6warfare")];
        let started = |action| Some(PublicFact::ActionStarted { action });
        let cases = [
            ("tactical", "action", started(TurnAction::Tactical)),
            ("pass", "action", started(TurnAction::Pass)),
            (
                "strategic",
                "action",
                started(TurnAction::Strategic {
                    card: Some("pok6warfare".into()),
                }),
            ),
            (
                "strategic|pok2diplomacy",
                "action",
                started(TurnAction::Strategic {
                    card: Some("pok2diplomacy".into()),
                }),
            ),
            (
                "component|tech|sr",
                "component",
                started(TurnAction::Component {
                    source: ComponentSource::Technology { tech: "sr".into() },
                }),
            ),
            (
                "component|leader|hacanagent",
                "component",
                started(TurnAction::Component {
                    source: ComponentSource::Leader {
                        leader: "hacanagent".into(),
                    },
                }),
            ),
            (
                "relic|stellarconverter",
                "component",
                started(TurnAction::Component {
                    source: ComponentSource::Relic {
                        relic: "stellarconverter".into(),
                    },
                }),
            ),
            (
                "faction|orbital_drop",
                "component",
                started(TurnAction::Component {
                    source: ComponentSource::Faction {
                        ability: "orbital_drop".into(),
                    },
                }),
            ),
            (
                "component|trade|hacan",
                "open_transaction",
                Some(PublicFact::TransactionOpened { with: p("p2") }),
            ),
            (
                "component|diplomacy|seat|1",
                "open_diplomacy",
                Some(PublicFact::TransactionOpened { with: p("p2") }),
            ),
        ];
        for (id, kind, expected) in cases {
            let (record, option) = menu(id, kind);
            assert_eq!(fact(&record, &option, &state), expected, "{id}");
            assert_eq!(decision_fact(&record, None, None, &state), None, "{id}");
        }
        let end = ChoiceOption::labelled("end_turn", "end_turn", "end your turn");
        assert_eq!(
            fact(&record("end_turn", &end), &end, &state),
            Some(PublicFact::TurnEnded)
        );
    }

    #[test]
    fn the_line_of_an_action_selection_is_its_action_not_the_menu() {
        let mut state = state();
        state.player_mut(&p("p1")).unwrap().strategy_cards =
            vec![StrategyCardId::new("pok6warfare")];
        for (id, line) in [
            ("tactical", "p1 began a tactical action"),
            ("strategic", "p1 played Warfare"),
            ("component|trade|hacan", "p1 opened a transaction with p2"),
        ] {
            // The live hosts record the menu with its context, not with the bare prompt.
            let option = ChoiceOption::new(id, "action");
            let selection = record("action_menu", &option);
            assert_eq!(
                verified_decision_facts(&selection, Some(&option), None, &state)
                    .0
                    .as_deref(),
                Some(line)
            );
        }
        let end = ChoiceOption::new("end_turn", "end_turn");
        assert_eq!(
            verified_decision_facts(&record("end_turn", &end), Some(&end), None, &state)
                .0
                .as_deref(),
            Some("p1 ended their turn")
        );
    }

    #[test]
    fn a_component_action_card_is_never_named_by_its_menu_option() {
        let state = state();
        let (record, option) = menu("action_card|3", "component");
        let fact = fact(&record, &option, &state).unwrap();
        assert_eq!(
            fact,
            PublicFact::ActionStarted {
                action: TurnAction::Component {
                    source: ComponentSource::ActionCard
                }
            }
        );
        let wire = serde_json::to_string(&fact).unwrap();
        assert!(!wire.contains("Sabotage") && !wire.contains('3'), "{wire}");
    }

    #[test]
    fn a_bare_strategic_action_with_two_used_cards_does_not_guess_the_card() {
        let mut state = state();
        let seat = state.player_mut(&p("p1")).unwrap();
        seat.strategy_cards = vec![
            StrategyCardId::new("pok1leadership"),
            StrategyCardId::new("pok6warfare"),
        ];
        seat.exhausted_strategy_cards = seat.strategy_cards.iter().cloned().collect();
        let (record, option) = menu("strategic", "action");
        assert_eq!(
            fact(&record, &option, &state),
            Some(PublicFact::ActionStarted {
                action: TurnAction::Strategic { card: None }
            })
        );
        state
            .player_mut(&p("p1"))
            .unwrap()
            .exhausted_strategy_cards
            .remove(&StrategyCardId::new("pok6warfare"));
        assert_eq!(
            fact(&record, &option, &state),
            Some(PublicFact::ActionStarted {
                action: TurnAction::Strategic {
                    card: Some("pok6warfare".into())
                }
            })
        );
    }

    #[test]
    fn board_and_table_decisions_have_their_fact() {
        let state = state();
        let system = DecisionTarget::System(SystemId::new("22"));
        let planet = DecisionTarget::Planet {
            system: SystemId::new("22"),
            planet: PlanetId::new("jord"),
        };
        let cases: Vec<(&str, ChoiceOption, Option<DecisionTarget>, PublicFact)> = vec![
            (
                "draft_strategy_card",
                ChoiceOption::new("pok1leadership", "strategy_card"),
                None,
                PublicFact::StrategyPicked {
                    card: "pok1leadership".into(),
                },
            ),
            (
                "activate_system",
                ChoiceOption::new("22", "activate"),
                None,
                PublicFact::Activated {
                    system: "22".into(),
                },
            ),
            (
                "movement_step",
                ChoiceOption::new("move|16|fighter", "move")
                    .with("origin", "16")
                    .with("unit", "fighter"),
                None,
                PublicFact::Moved {
                    unit: "fighter".into(),
                    from: "16".into(),
                    to: "22".into(),
                },
            ),
            (
                "load_cargo",
                ChoiceOption::new("load|0", "load")
                    .with("unit", "infantry")
                    .with("source", "jord")
                    .with("pickup_system", "16"),
                None,
                PublicFact::Loaded {
                    unit: "infantry".into(),
                    system: Some("16".into()),
                    planet: Some("jord".into()),
                },
            ),
            (
                "produce_unit",
                ChoiceOption::new("build|fighter|2", "produce")
                    .with("unit", "fighter")
                    .with("count", 2)
                    .with("system", "22")
                    .with("destination", "space"),
                None,
                PublicFact::Produced {
                    unit: "fighter".into(),
                    count: 2,
                    system: Some("22".into()),
                    planet: None,
                },
            ),
            (
                "place_unit",
                ChoiceOption::new("place|jord", "place")
                    .with("unit", "infantry")
                    .with("system", "22")
                    .with("destination", "jord"),
                None,
                PublicFact::UnitPlaced {
                    unit: "infantry".into(),
                    system: "22".into(),
                    planet: Some("jord".into()),
                },
            ),
            (
                "pay_resources",
                ChoiceOption::new("exhaust|jord", "pay"),
                None,
                PublicFact::Paid {
                    with: PaidWith::Planet {
                        planet: "jord".into(),
                    },
                    spend: Spend::Resources,
                },
            ),
            (
                "pay_influence",
                ChoiceOption::new("trade_good", "pay"),
                None,
                PublicFact::Paid {
                    with: PaidWith::TradeGood,
                    spend: Spend::Influence,
                },
            ),
            (
                "place_structure",
                ChoiceOption::new("pds|22|jord", "build"),
                None,
                PublicFact::StructurePlaced {
                    unit: "pds".into(),
                    system: "22".into(),
                    planet: "jord".into(),
                },
            ),
            (
                "assign_casualty",
                ChoiceOption::new("destroy|4", "casualty")
                    .with("unit", "dreadnought")
                    .with("damaged", true),
                Some(system.clone()),
                PublicFact::Casualty {
                    seat: p("p1"),
                    unit: "dreadnought".into(),
                    system: Some("22".into()),
                    planet: None,
                    damaged: true,
                },
            ),
            (
                "assign_ground_casualty",
                ChoiceOption::new("destroy|1", "ground_casualty").with("unit", "infantry"),
                Some(planet.clone()),
                PublicFact::Casualty {
                    seat: p("p1"),
                    unit: "infantry".into(),
                    system: Some("22".into()),
                    planet: Some("jord".into()),
                    damaged: false,
                },
            ),
            (
                "sustain_damage",
                ChoiceOption::new("sustain|2", "sustain").with("unit", "dreadnought"),
                Some(system.clone()),
                PublicFact::Sustained {
                    seat: p("p1"),
                    unit: "dreadnought".into(),
                    system: Some("22".into()),
                    planet: None,
                },
            ),
            (
                "announce_retreat",
                ChoiceOption::new("retreat", "retreat"),
                Some(system.clone()),
                PublicFact::RetreatAnnounced {
                    seat: p("p1"),
                    system: "22".into(),
                    retreat: true,
                },
            ),
            (
                "retreat_to",
                ChoiceOption::new("16", "retreat_to").with("system", "16"),
                Some(system.clone()),
                PublicFact::Retreated {
                    seat: p("p1"),
                    from: "22".into(),
                    to: "16".into(),
                },
            ),
            (
                "commit_ground_forces",
                ChoiceOption::new("commit|0|jord", "commit")
                    .with("unit", "mech")
                    .with("planet", "jord"),
                Some(system),
                PublicFact::Landed {
                    unit: "mech".into(),
                    planet: "jord".into(),
                    damaged: false,
                },
            ),
            (
                "research_technology",
                ChoiceOption::new("amd", "research"),
                None,
                PublicFact::Researched { tech: "amd".into() },
            ),
            (
                "cast_vote",
                ChoiceOption::new("for", "vote"),
                None,
                PublicFact::Voted {
                    outcome: Some("for".into()),
                },
            ),
            (
                "cast_vote",
                ChoiceOption::decline(),
                None,
                PublicFact::Voted { outcome: None },
            ),
            (
                "vote_exhaust_planet",
                ChoiceOption::new("jord|resources", "vote_planet"),
                None,
                PublicFact::PlanetExhaustedForVote {
                    planet: "jord".into(),
                },
            ),
            (
                "gain_command_token",
                ChoiceOption::new("fleet_tokens", "pool"),
                None,
                PublicFact::TokensGained {
                    pool: TokenPool::Fleet,
                    count: 1,
                },
            ),
            (
                "answer_transaction",
                ChoiceOption::new("accept", "answer"),
                Some(DecisionTarget::Player(p("p2"))),
                PublicFact::TransactionAnswered {
                    proposer: p("p2"),
                    answer: TransactionAnswer::Accept,
                },
            ),
            (
                "propose_transaction",
                ChoiceOption::decline(),
                Some(DecisionTarget::Player(p("p2"))),
                PublicFact::NegotiationEnded { with: p("p2") },
            ),
        ];
        for (subtype, option, target, expected) in cases {
            let mut decision = record(subtype, &option);
            decision.context.as_mut().unwrap().target = target;
            assert_eq!(
                fact(&decision, &option, &state),
                Some(expected),
                "{subtype}"
            );
            assert_eq!(
                decision_fact(&decision, None, None, &state),
                None,
                "{subtype}"
            );
            let other = ChoiceOption::new("not-chosen", option.kind.as_str());
            assert_eq!(fact(&decision, &other, &state), None, "{subtype}");
        }
    }

    fn secondary(mut record: DecisionRecord, card: &str) -> DecisionRecord {
        record.context.as_mut().unwrap().source = DecisionSource::StrategyCard {
            card: card.into(),
            secondary: true,
        };
        record
    }

    #[test]
    fn a_follow_names_its_card_and_whether_the_seat_followed() {
        let state = state();
        let follow = |subtype: &str, id: &str, card: &str| {
            let option = ChoiceOption::new(id, "strategy");
            fact(&secondary(record(subtype, &option), card), &option, &state)
        };
        let followed = |card: &str, followed| {
            Some(PublicFact::SecondaryFollowed {
                card: card.into(),
                followed,
            })
        };
        let subtype = ti4_engine::strategy::FOLLOW_SUBTYPE;
        assert_eq!(
            follow(subtype, "yes", "pok6warfare"),
            followed("pok6warfare", true)
        );
        assert_eq!(
            follow(subtype, "no", "pok5trade"),
            followed("pok5trade", false)
        );
        assert_eq!(
            follow(subtype, "follow|waived|0|winnu", "pok7technology"),
            followed("pok7technology", true)
        );
        let decline = ChoiceOption::decline();
        assert_eq!(
            fact(
                &secondary(record(subtype, &decline), "pok8imperial"),
                &decline,
                &state
            ),
            followed("pok8imperial", false)
        );
        // Leadership's follow is its first purchase; a later purchase is no follow.
        let buy = "buy_token_with_influence";
        assert_eq!(
            follow(buy, "yes", "pok1leadership"),
            followed("pok1leadership", true)
        );
        assert_eq!(
            follow(buy, "no", "pok1leadership"),
            followed("pok1leadership", false)
        );
        let yes = ChoiceOption::new("yes", "strategy");
        assert_eq!(
            fact(&record(buy, &yes), &yes, &state),
            Some(PublicFact::TokenBoughtWithInfluence)
        );
        let no = ChoiceOption::new("no", "strategy");
        assert_eq!(fact(&record(buy, &no), &no, &state), None);
        // A card that is not in the catalogue, or a question without the card, gives no fact.
        assert_eq!(follow(subtype, "yes", "nocard"), None);
        assert_eq!(fact(&record(subtype, &yes), &yes, &state), None);
    }

    #[test]
    fn primaries_and_the_status_phase_have_their_fact() {
        let state = state();
        let cases = [
            (
                "diplomacy_choose_system",
                ChoiceOption::new("22", "system"),
                PublicFact::DiplomacyChosen {
                    system: "22".into(),
                },
            ),
            (
                "politics_choose_speaker",
                ChoiceOption::new("hacan", "speaker"),
                PublicFact::SpeakerChosen { seat: p("p2") },
            ),
            (
                "trade_choose_replenish",
                ChoiceOption::new("hacan", "replenish"),
                PublicFact::CommoditiesReplenished { seat: p("p2") },
            ),
            (
                "ready_planet",
                ChoiceOption::new("jord", "ready"),
                PublicFact::PlanetReadied {
                    planet: "jord".into(),
                },
            ),
            (
                "status_redistribute_tokens",
                ChoiceOption::new("3|4|2", "redistribute"),
                PublicFact::TokensRedistributed {
                    tactic: 3,
                    fleet: 4,
                    strategy: 2,
                },
            ),
            (
                "warfare_redistribute_tokens",
                ChoiceOption::new("0|2|1", "redistribute"),
                PublicFact::TokensRedistributed {
                    tactic: 0,
                    fleet: 2,
                    strategy: 1,
                },
            ),
            (
                "warfare_recall_token",
                ChoiceOption::new("22", "recall"),
                PublicFact::TokenRecalled {
                    system: "22".into(),
                },
            ),
            (
                "warfare_free_tactical",
                ChoiceOption::new("22", "activate"),
                PublicFact::FreeTactical {
                    system: "22".into(),
                },
            ),
        ];
        for (subtype, option, expected) in cases {
            let decision = record(subtype, &option);
            assert_eq!(
                fact(&decision, &option, &state),
                Some(expected),
                "{subtype}"
            );
            let other = ChoiceOption::new("not-chosen", option.kind.as_str());
            assert_eq!(fact(&decision, &other, &state), None, "{subtype}");
        }
        // A faction that is not at the table (a seat id is no faction), or a sheet that is not three counts, gives no fact.
        for (subtype, option) in [
            (
                "politics_choose_speaker",
                ChoiceOption::new("p9", "speaker"),
            ),
            (
                "trade_choose_replenish",
                ChoiceOption::new("p9", "replenish"),
            ),
            (
                "status_redistribute_tokens",
                ChoiceOption::new("3|4", "redistribute"),
            ),
            ("trade_choose_replenish", ChoiceOption::decline()),
            ("research_technology", ChoiceOption::decline()),
        ] {
            assert_eq!(
                fact(&record(subtype, &option), &option, &state),
                None,
                "{subtype} {}",
                option.id
            );
        }
    }

    #[test]
    fn invasion_steps_and_a_removed_unit_have_their_fact() {
        let state = state();
        let system = DecisionTarget::System(SystemId::new("22"));
        let planet = DecisionTarget::Planet {
            system: SystemId::new("22"),
            planet: PlanetId::new("jord"),
        };
        let cases = [
            (
                "sustain_damage",
                ChoiceOption::decline(),
                system.clone(),
                PublicFact::SustainDeclined {
                    seat: p("p1"),
                    system: Some("22".into()),
                    planet: None,
                },
            ),
            (
                "fight_ground_combat_round",
                ChoiceOption::new("fight", "ground_casualty"),
                planet.clone(),
                PublicFact::GroundCombatRound {
                    seat: p("p1"),
                    planet: "jord".into(),
                },
            ),
            (
                "start_next_ground_combat",
                ChoiceOption::new("fight|p2", "ground_casualty"),
                planet.clone(),
                PublicFact::GroundCombatStarted {
                    seat: p("p1"),
                    planet: "jord".into(),
                },
            ),
            (
                "bombardment_target",
                ChoiceOption::new("p2", "bombardment_target"),
                planet.clone(),
                PublicFact::BombardmentTargeted {
                    target: p("p2"),
                    planet: "jord".into(),
                },
            ),
            (
                "remove_custodians",
                ChoiceOption::new("yes", "custodians"),
                system.clone(),
                PublicFact::CustodiansRemoved {
                    system: "22".into(),
                },
            ),
            (
                ti4_engine::fleet::REMOVE_EXCESS_SUBTYPE,
                ChoiceOption::labelled("remove|1", "remove", "remove cruiser"),
                system.clone(),
                PublicFact::UnitRemoved {
                    unit: "cruiser".into(),
                    system: "22".into(),
                },
            ),
        ];
        for (subtype, option, target, expected) in cases {
            let decision = about(record(subtype, &option), target);
            assert_eq!(
                fact(&decision, &option, &state),
                Some(expected),
                "{subtype}"
            );
        }
        // A label that names no unit, a target that is no seat, or a closing step: no fact.
        for (subtype, option, target) in [
            (
                ti4_engine::fleet::REMOVE_EXCESS_SUBTYPE,
                ChoiceOption::labelled("remove|1", "remove", "remove a secret"),
                system.clone(),
            ),
            (
                "bombardment_target",
                ChoiceOption::new("p9", "bombardment_target"),
                planet.clone(),
            ),
            ("start_next_ground_combat", ChoiceOption::decline(), planet),
            (
                "commit_ground_forces",
                ChoiceOption::new("done_committing", "decline"),
                system,
            ),
        ] {
            let decision = about(record(subtype, &option), target);
            assert_eq!(fact(&decision, &option, &state), None, "{subtype}");
        }
    }

    #[test]
    fn a_sling_relay_ship_is_a_production_without_a_context() {
        let state = state();
        let ship = ChoiceOption::new("build|cruiser|1", "produce")
            .with("unit", "cruiser")
            .with("count", 1)
            .with("sling_relay", true)
            .with("system", "22");
        let mut decision = record("produce_unit", &ship);
        decision.context = None;
        assert_eq!(
            fact(&decision, &ship, &state),
            Some(PublicFact::Produced {
                unit: "cruiser".into(),
                count: 1,
                system: Some("22".into()),
                planet: None,
            })
        );
        // Any other question without a context has no fact.
        let follow = ChoiceOption::new("yes", "strategy");
        let mut bare = record("x", &follow);
        bare.context = None;
        assert_eq!(fact(&bare, &follow, &state), None);
    }

    #[test]
    fn a_proposal_names_its_partner_and_nothing_it_offers() {
        let offer = ChoiceOption::labelled(
            "pnhacan_tradeagreement:hacan:2",
            "offer",
            "give Trade Agreement",
        )
        .with("card", "secret_card");
        let decision = about(
            record("propose_transaction", &offer),
            DecisionTarget::Player(p("p2")),
        );
        let fact = fact(&decision, &offer, &state()).unwrap();
        assert_eq!(fact, PublicFact::TransactionProposed { to: p("p2") });
        let wire = serde_json::to_string(&fact).unwrap();
        assert!(
            !wire.contains("trade") && !wire.contains("secret_card"),
            "{wire}"
        );
    }

    #[test]
    fn a_secret_objective_is_a_fact_only_once_it_is_scored() {
        let mut state = state();
        let score = ChoiceOption::new("otf", "score");
        for subtype in ["score_objective", "score_secret_objective"] {
            let decision = record(subtype, &score);
            assert_eq!(fact(&decision, &score, &state), None, "{subtype}");
        }
        state.record_score(&p("p1"), ObjectiveId::new("otf"));
        for subtype in ["score_objective", "score_secret_objective"] {
            let decision = record(subtype, &score);
            assert_eq!(
                fact(&decision, &score, &state),
                Some(PublicFact::ObjectiveScored {
                    objective: "otf".into(),
                    secret: true,
                }),
                "{subtype}"
            );
        }
        // Another seat's score does not reveal what this seat chose.
        let other = record("score_secret_objective", &ChoiceOption::new("mlp", "score"));
        state.record_score(&p("p2"), ObjectiveId::new("mlp"));
        assert_eq!(
            fact(&other, &ChoiceOption::new("mlp", "score"), &state),
            None
        );
    }

    #[test]
    fn hands_discards_and_looked_at_agendas_have_no_fact() {
        let state = state();
        for (subtype, id) in [
            ("politics_place_agenda", "top"),
            ("discard_over_hand_limit", "0"),
            ("expedition_discard_action_card", "0"),
            ("return_over_secret_hand_limit", "hidden_objective"),
            ("legendary_galactic_council", "hidden_objective"),
            ("play_card", "hidden_card"),
        ] {
            let option = ChoiceOption::new(id, "private").with("card", "hidden_card");
            assert_eq!(
                fact(&record(subtype, &option), &option, &state),
                None,
                "{subtype}"
            );
        }
    }

    #[test]
    fn a_reaction_card_is_a_fact_only_when_the_engine_announced_it() {
        let mut state = state();
        let card = ChoiceOption::labelled("sh1", "action_card", "play Shields Holding")
            .with("card", "sh1")
            .with("card_name", "Shields Holding");
        let decision = record("play_reaction_when_HITS_TO_ASSIGN", &card);
        assert_eq!(fact(&decision, &card, &state), None);
        let declined = ChoiceOption::decline();
        assert_eq!(
            fact(
                &record("play_reaction_when_HITS_TO_ASSIGN", &declined),
                &declined,
                &state
            ),
            None
        );
        state
            .action_card_plays
            .push((p("p1"), ActionCardId::new("sh1")));
        assert_eq!(
            fact(&decision, &card, &state),
            Some(PublicFact::CardPlayed { card: "sh1".into() })
        );
        // A forged name is not the card.
        let forged = card.clone().with("card_name", "Direct Hit");
        assert_eq!(fact(&decision, &forged, &state), None);
    }

    #[test]
    fn the_fact_reaches_every_viewer_and_the_secret_detail_only_its_seat() {
        let mut state = state();
        let score = ChoiceOption::new("otf", "score");
        let decision = record("score_objective", &score);
        let event = |facts: crate::protocol::server::DecisionFacts| GameEvent {
            id: "e".into(),
            timestamp: String::new(),
            version: Some(1),
            visibility: EventVisibility::Public,
            event: GameEventKind::DecisionResolved,
            decision_count: Some(1),
            batch_id: None,
            batch_start_cursor: None,
            batch_end_cursor: None,
            action_id: None,
            action_start_cursor: None,
            actor: None,
            round: None,
            phase: None,
            action_type: None,
            action_actor: None,
            stage: None,
            detail: facts.detail,
            movement: facts.movement,
            fact: facts.fact,
            seat_detail: facts.seat_detail,
            private_detail: None,
            roll: None,
        };
        // Selected, not scored: only the seat sees it, in its private line.
        let before = event(verified_event_facts(&decision, Some(&score), None, &state));
        for viewer in [ViewerRole::Player(p("p2")), ViewerRole::Spectator] {
            let wire = serde_json::to_string(&before.for_viewer(&viewer).unwrap()).unwrap();
            assert!(!wire.contains("otf"), "{wire}");
            assert!(!wire.contains("\"fact\""), "{wire}");
        }
        state.record_score(&p("p1"), ObjectiveId::new("otf"));
        let after = event(verified_event_facts(&decision, Some(&score), None, &state));
        for viewer in [
            ViewerRole::Player(p("p1")),
            ViewerRole::Player(p("p2")),
            ViewerRole::Spectator,
        ] {
            let projected = after.for_viewer(&viewer).unwrap();
            assert_eq!(projected.fact, after.fact);
            let json = serde_json::to_value(&projected).unwrap();
            assert_eq!(
                json["fact"],
                serde_json::json!({"kind": "objective_scored", "objective": "otf", "secret": true})
            );
        }
    }

    #[test]
    fn a_fact_round_trips_in_its_wire_shape() {
        let fact = PublicFact::ActionStarted {
            action: TurnAction::Component {
                source: ComponentSource::Technology { tech: "sr".into() },
            },
        };
        let json = serde_json::to_value(&fact).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "kind": "action_started",
                "action": {"type": "component", "source": {"type": "technology", "tech": "sr"}}
            })
        );
        assert_eq!(serde_json::from_value::<PublicFact>(json).unwrap(), fact);
        let paid = serde_json::to_value(PublicFact::Paid {
            with: PaidWith::TradeGood,
            spend: Spend::Resources,
        })
        .unwrap();
        assert_eq!(
            paid,
            serde_json::json!({"kind": "paid", "with": {"type": "trade_good"}, "spend": "resources"})
        );
    }
}
