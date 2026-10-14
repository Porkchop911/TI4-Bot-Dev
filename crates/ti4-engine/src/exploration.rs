//! Exploration and relic fragments (LRR 35, and 35.9 for fragments).
//!
//! Ported from the oracle's `engine/exploration.py`: `trait_of`, `draw`, `explore`, `_resolve`,
//! `_gain_fragment` and `_attach`, plus the fragment half of `engine/relics.py`.

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{PlanetId, PlayerId, RelicId, SystemId, TechnologyId};
use ti4_model::state::GameState;

use crate::decision_context::{DecisionContext, DecisionSource};
use crate::deck::EXPLORATION_TRAITS;

/// The frontier deck, which needs no planet (35.5).
pub const FRONTIER: &str = "FRONTIER";

/// How many fragments of one trait buy a relic (35.9).
pub const FRAGMENTS_PER_RELIC: i32 = 3;

/// What resolving one exploration card did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Explored {
    /// A relic fragment, kept faceup until purged (35.9).
    Fragment { trait_name: String },
    /// An attachment on the planet.
    Attached { card: String },
    /// Drawn, but this engine has no handler for it. Announced, never silently dropped.
    Unresolved { card: String },
    /// An instant card this engine resolved.
    Resolved { card: String },
    /// An attachment drawn from the frontier, which has no planet to attach to.
    Discarded { card: String },
}

/// Every exploration deck a planet could be explored into, in the planet's own printed trait
/// order (35.2b).
///
/// Some Thunder's Edge planets (Lazul Rex, Lesab, and others) print two traits. `galaxy::Planet`
/// already warns about this shape at its own `planet_type`: "a dual-trait planet answers to both
/// of its traits, and picking the first would drop one of them." [`trait_of`] used to do exactly
/// that -- `find_map` over the planet's traits stopped at the first one with a deck, so Lazul Rex
/// (INDUSTRIAL, CULTURAL) always explored INDUSTRIAL and its CULTURAL half could never be
/// reached. This returns every matching deck so a caller with a decider can ask which.
#[must_use]
pub fn traits_of(content: &ContentStore, sources: SourceSet, planet: &PlanetId) -> Vec<String> {
    let catalogue = ti4_content::galaxy::all_planets(content, sources);
    let Some(record) = catalogue.get(planet.as_str()) else {
        return Vec::new();
    };
    record
        .planet_types()
        .into_iter()
        .filter_map(|kind| {
            let trait_name = kind.to_ascii_uppercase();
            EXPLORATION_TRAITS
                .iter()
                .find(|known| **known == trait_name && **known != FRONTIER)
                .map(|known| (*known).to_owned())
        })
        .collect()
}

/// The deck a planet explores into, or `None` if it cannot be explored (35.2b).
///
/// For a planet with more than one trait this reports only the first — callers that can ask a
/// player which deck to use should call [`traits_of`] (or [`choose_deck`]) instead; keeping this
/// unchanged preserves every existing single-trait caller.
#[must_use]
pub fn trait_of(content: &ContentStore, sources: SourceSet, planet: &PlanetId) -> Option<String> {
    traits_of(content, sources, planet).into_iter().next()
}

/// The deck this player explores `planet` into, asking when more than one trait qualifies.
///
/// A dual-trait planet is the player's choice, not the engine's: without asking, the planet
/// would always explore into whichever trait happens to sort first among its own printed traits,
/// silently dropping the deck a player might have preferred.
pub fn choose_deck(
    ctx: &mut crate::choice::Resolving<'_>,
    state: &GameState,
    player: &PlayerId,
    planet: &PlanetId,
) -> Option<String> {
    let mut traits = crate::planets::traits_now(state, ctx.content, ctx.sources, planet);
    match traits.len() {
        0 => None,
        1 => traits.pop(),
        _ => {
            let options: Vec<(&str, &str)> = traits
                .iter()
                .map(|trait_name| (trait_name.as_str(), trait_name.as_str()))
                .collect();
            ask(
                ctx,
                state,
                player,
                "exploration",
                "choose which deck to explore this planet into",
                &options,
            )
            .or_else(|| traits.into_iter().next())
        }
    }
}

/// Draw the top card of one exploration deck.
pub fn draw(state: &mut GameState, deck: &str) -> Option<String> {
    let cards = state.exploration_decks.get_mut(deck)?;
    ti4_model::deck_reserve::take_top(
        &mut state.deck_reserve,
        &format!("exploration:{deck}"),
        cards,
        None,
    )
}

/// How a card resolves, from the corpus.
#[must_use]
pub fn resolution(content: &ContentStore, card: &str) -> Option<String> {
    content
        .get(ContentType::Explores, card)
        .and_then(|record| record.text("resolution"))
        .map(ToOwned::to_owned)
}

/// The attachment an `Attach` explore card puts on `planet`.
///
/// The card's `attachmentId`, or the card id when the content gives none. A research facility is
/// two attachments: on a planet that already has a technology specialty it is the `...stat` record,
/// +1 resource and +1 influence; otherwise it is the record that gives the planet that specialty.
#[must_use]
pub fn attachment_for(content: &ContentStore, card: &str, planet: &PlanetId) -> String {
    let id = content
        .get(ContentType::Explores, card)
        .and_then(|record| record.text("attachmentId"))
        .unwrap_or(card)
        .to_owned();
    let stat = format!("{id}stat");
    if content.get(ContentType::Attachments, &stat).is_none() {
        return id;
    }
    let has_specialty =
        ti4_content::galaxy::planet(content, planet.as_str(), ti4_model::content_types::FULL)
            .is_some_and(|record| !record.tech_specialties().is_empty());
    if has_specialty { stat } else { id }
}

/// This player's commodity value, from their faction (21.1).
fn commodity_limit(state: &GameState, content: &ContentStore, player: &PlayerId) -> i32 {
    state.player(player).map_or(0, |seat| {
        ti4_content::factions::get(content, seat.faction.as_str())
            .map_or(0, |faction| faction.commodities())
    })
}

/// Gain up to `count` commodities, never past the faction's value (21.2).
fn gain_commodities(state: &mut GameState, content: &ContentStore, player: &PlayerId, count: i32) {
    let limit = commodity_limit(state, content, player);
    let mut gained = 0;
    if let Some(seat) = state.player_mut(player) {
        let after = (seat.commodities + count).min(limit);
        gained = (after - seat.commodities).max(0);
        seat.commodities = after;
    }
    crate::supply::note_commodities_gained(state, player, gained);
}

/// Turn up to `most` commodities into trade goods, or all of them when `most` is `None`.
///
/// 21.5 only converts commodities when they *change hands*; these cards say so explicitly, which
/// is why this is written here rather than reached for anywhere a commodity is spent.
fn convert_commodities(
    state: &mut GameState,
    ctx: &mut crate::choice::Resolving<'_>,
    player: &PlayerId,
    most: Option<i32>,
) {
    let moved = state.player_mut(player).map_or(0, |seat| {
        let moved = most.map_or(seat.commodities, |cap| seat.commodities.min(cap));
        seat.commodities -= moved;
        moved
    });
    crate::supply::gain_trade_goods_announced(state, ctx, player, moved, "exploration");
}

/// Ask this player one question with the given options.
fn ask(
    ctx: &mut crate::choice::Resolving<'_>,
    state: &GameState,
    player: &PlayerId,
    card: &str,
    prompt: &str,
    options: &[(&str, &str)],
) -> Option<String> {
    ask_decorated(ctx, state, player, card, prompt, options, |choice| choice)
}

/// [`ask`] with display-only details added to the choice (an offer card). The options, their
/// order and the context are the same, so records and replays are unaffected.
fn ask_decorated(
    ctx: &mut crate::choice::Resolving<'_>,
    state: &GameState,
    player: &PlayerId,
    card: &str,
    prompt: &str,
    options: &[(&str, &str)],
    decorate: impl FnOnce(crate::choice::Choice) -> crate::choice::Choice,
) -> Option<String> {
    let choice = crate::choice::Choice::new(
        player.clone(),
        prompt,
        options
            .iter()
            .map(|(id, label)| crate::choice::ChoiceOption::labelled(*id, "explore", *label))
            .collect(),
    )
    .contextualized(DecisionContext::new(
        player.clone(),
        DecisionSource::Content(card.to_owned()),
        format!("{card}_choose_reward"),
        state.phase,
        state.round,
    ));
    let mut choice = decorate(choice);
    if COMMODITY_REWARDS.contains(&card) {
        // Display only: what the seat holds now, so "gain 2" or "convert" reads against it.
        choice = choice.detailed(
            "commodities",
            crate::strategy_cards::commodity_standing(state, ctx.content, player),
        );
    }
    ctx.ask_seeing(state, &choice).ok().map(|answer| answer.id)
}

/// Exploration cards whose reward choice gains, replenishes, spends or converts commodities.
const COMMODITY_REWARDS: [&str; 4] = [
    "abandoned_warehouses",
    "merchant_station",
    "functioning_base",
    "local_fabricators",
];

/// The system a planet sits in, according to the board.
fn system_of(state: &GameState, planet: &PlanetId) -> Option<ti4_model::id::SystemId> {
    state
        .board
        .iter()
        .find(|(_, board)| {
            board.planet_units.contains_key(planet) || board.planet_control.contains_key(planet)
        })
        .map(|(id, _)| id.clone())
}

/// Place one unit of a base type on a planet.
fn place_on_planet(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    planet: &PlanetId,
    base_type: &str,
) -> bool {
    let Some(system) = system_of(state, planet) else {
        return false;
    };
    // The seat's own form of the unit: its upgrade, else its faction's version, else the plain
    // one. A base type with none is not placed (a factionless seat has no mech).
    let Some(type_id) =
        crate::action_cards::placed_unit_id(state, content, sources, player, base_type)
    else {
        return false;
    };
    // 31.4, through the one door: a mech is plastic and may have none left, while infantry is
    // cardboard and always passes.
    if crate::supply::allowed(state, content, sources, player, &type_id, 1) == 0 {
        return false;
    }
    state
        .system_mut(&system)
        .planet_units
        .entry(planet.clone())
        .or_default()
        .push(ti4_model::units::Unit::new(type_id.clone(), player.clone()));
    crate::supply::stage_naaz_mech_placed(state, player, &system, &type_id);
    true
}

/// "If you have at least 1 mech on this planet, or if you remove 1 infantry from this planet."
///
/// A mech satisfies the condition merely by being there. Infantry satisfies it only when the
/// player explicitly chooses to remove one; declining the sacrifice leaves the reward unresolved.
/// A player with neither cannot resolve the card at all.
fn pay_with_mech_or_infantry(
    ctx: &mut crate::choice::Resolving<'_>,
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    planet: &PlanetId,
    card: &str,
    reward: &str,
) -> bool {
    let Some(system) = system_of(state, planet) else {
        return false;
    };
    let types = ti4_content::units::catalogue(content, sources);
    let units = state
        .system_state(&system)
        .planet_units
        .get(planet)
        .cloned()
        .unwrap_or_default();

    if units.iter().any(|unit| {
        &unit.owner == player
            && types
                .get(unit.type_id.as_str())
                .is_some_and(|kind| kind.base_type() == "mech")
    }) {
        return true;
    }

    let infantry = units.iter().position(|unit| {
        &unit.owner == player
            && types
                .get(unit.type_id.as_str())
                .is_some_and(|kind| kind.base_type() == "infantry")
    });
    let Some(index) = infantry else {
        return false;
    };
    // Display only: the card as printed, what is lost and exactly what is gained.
    let record = content.get(ContentType::Explores, card);
    let name = record
        .and_then(|r| r.text("name"))
        .unwrap_or(card)
        .to_owned();
    let tag = format!(
        "{} exploration card",
        record
            .and_then(|r| r.text("type"))
            .unwrap_or("")
            .to_ascii_lowercase()
    );
    let printed = record.and_then(|r| r.text("text")).map(ToOwned::to_owned);
    let system_name = system.to_string();
    let chosen = ask_decorated(
        ctx,
        state,
        player,
        card,
        "remove 1 infantry from this planet to resolve the exploration card",
        &[
            ("remove_infantry", "remove 1 infantry and gain the reward"),
            ("decline", "do not remove an infantry"),
        ],
        |choice| {
            choice.offered(
                crate::choice::offer_card(
                    &name,
                    &tag,
                    Some("on exploring this planet"),
                    printed.as_deref(),
                ),
                vec![
                    crate::choice::offer_fact_planet("Planet", planet.as_str(), &system_name),
                    crate::choice::offer_fact("You lose", "1 infantry on this planet"),
                    crate::choice::offer_fact("You gain", reward),
                ],
                &[
                    (
                        "remove_infantry",
                        crate::choice::offer_caption(
                            &format!("Remove 1 infantry and {reward}"),
                            Some("the infantry is returned to your reinforcements"),
                        ),
                    ),
                    (
                        "decline",
                        crate::choice::offer_caption(
                            "Keep the infantry",
                            Some("the card has no effect"),
                        ),
                    ),
                ],
            )
        },
    );
    if chosen.as_deref() != Some("remove_infantry") {
        return false;
    }
    if let Some(held) = state.system_mut(&system).planet_units.get_mut(planet) {
        held.remove(index);
    }
    true
}

/// Every technology Enigmatic Device could grant right now: not already held, and -- 90.11 --
/// belonging to this player's own faction if it belongs to any.
///
/// Mirrors `relics::grant_chosen_technology`'s own filter (colour narrowing aside, which neither
/// exploration card uses): Enigmatic Device deliberately does not route through `can_research`,
/// because its price is already the price and a prerequisite check would charge for one twice.
/// Sorted, not corpus order, for the same reason every other option list here is (F-M08-019-1).
fn enigmatic_device_technologies(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<TechnologyId> {
    let held: std::collections::BTreeSet<String> = state
        .player(player)
        .map(|seat| {
            seat.technologies
                .iter()
                .map(|alias| alias.as_str().to_owned())
                .collect()
        })
        .unwrap_or_default();
    let faction = state.player(player).map(|seat| seat.faction.to_string());
    let mut technologies: Vec<TechnologyId> = content
        .from_sources(ContentType::Technologies, sources)
        .filter(|record| {
            record
                .text("faction")
                .is_none_or(|owner| faction.as_deref().is_some_and(|mine| mine == owner))
        })
        .filter_map(|record| record.text("alias"))
        .filter(|alias| !held.contains(*alias))
        .map(TechnologyId::new)
        .collect();
    technologies.sort();
    technologies
}

/// Component actions from exploration cards held faceup in the play area.
///
/// Only the two Enigmatic Device cards print one. Offered on the same 22.3 terms as a relic
/// action: withheld when its six resources cannot be paid, because an action that cannot fully
/// resolve is never offered.
///
/// One option per card. The technology is chosen in a second, named decision
/// ([`perform_action`]) *before* the resources are spent, so the price is still not paid blind.
/// An earlier version listed one option per (card, technology) pair, which put every unheld
/// technology -- 36 of them in a long game -- into the component action menu.
#[must_use]
pub fn available_actions(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<crate::choice::ChoiceOption> {
    let Some(seat) = state.player(player) else {
        return Vec::new();
    };
    if crate::production::available(
        state,
        content,
        sources,
        player,
        crate::production::Spend::Resources,
    ) < ENIGMATIC_DEVICE_COST
        || enigmatic_device_technologies(state, content, sources, player).is_empty()
    {
        return Vec::new();
    }
    if state.legacy_device_ids {
        let technologies = enigmatic_device_technologies(state, content, sources, player);
        return seat
            .exploration_cards
            .iter()
            .filter(|card| matches!(card.as_str(), "ed1" | "ed2"))
            .flat_map(|card| {
                technologies.iter().map(move |tech| {
                    crate::choice::ChoiceOption::labelled(
                        format!("{PLAY_AREA_PREFIX}{card}:{tech}"),
                        crate::relics::ACTION_KIND,
                        format!(
                            "spend 6 resources to research {}",
                            crate::technology::name(content, tech)
                        ),
                    )
                    .with("technology", tech.to_string())
                })
            })
            .collect();
    }
    seat.exploration_cards
        .iter()
        .filter(|card| matches!(card.as_str(), "ed1" | "ed2"))
        .map(|card| {
            crate::choice::ChoiceOption::labelled(
                format!("{PLAY_AREA_PREFIX}{card}"),
                crate::relics::ACTION_KIND,
                "Enigmatic Device: spend 6 resources to research a technology",
            )
        })
        .collect()
}

/// Ask which technology an Enigmatic Device researches, by name, before anything is paid.
#[allow(clippy::too_many_arguments, reason = "the arguments of a decision site")]
fn ask_device_technology(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut crate::choice::Table,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    player: &PlayerId,
    card: &str,
    technologies: &[TechnologyId],
) -> Option<TechnologyId> {
    let choice = crate::choice::Choice::new(
        player.clone(),
        "research which technology with the Enigmatic Device",
        technologies
            .iter()
            .map(|tech| {
                crate::choice::ChoiceOption::labelled(
                    tech.to_string(),
                    "technology",
                    format!("research {}", crate::technology::name(content, tech)),
                )
            })
            .collect(),
    )
    .contextualized(DecisionContext::new(
        player.clone(),
        DecisionSource::Content(card.to_owned()),
        "enigmatic_device_choose_technology",
        state.phase,
        state.round,
    ));
    let Ok(answer) = table.ask_seeing(
        &choice,
        &crate::choice::Observed::new(state, content, sources, galaxy),
    ) else {
        return None;
    };
    Some(TechnologyId::new(answer.id))
}

/// Resolve a component action offered by [`available_actions`].
pub fn perform_action(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut crate::choice::Table,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    player: &PlayerId,
    option: &crate::choice::ChoiceOption,
) -> bool {
    let Some(rest) = option.id.strip_prefix(PLAY_AREA_PREFIX) else {
        return false;
    };
    // The old per-technology id names its technology already: no follow-up question, so the
    // decisions recorded under it replay one for one.
    let (card, legacy_technology) = match rest.split_once(':') {
        Some((card, tech)) => (card, Some(tech)),
        None => (rest, None),
    };
    if !state
        .player(player)
        .is_some_and(|seat| seat.exploration_cards.iter().any(|held| held == card))
    {
        return false;
    }
    // Which technology is asked before the price is paid: the cost is already known to be
    // payable (`available_actions`), and a decider is never made to choose after the resources
    // are gone.
    let technologies = enigmatic_device_technologies(state, content, sources, player);
    if technologies.is_empty() {
        return false;
    }
    let technology = match legacy_technology {
        Some(tech) => TechnologyId::new(tech),
        None => {
            let Some(chosen) =
                ask_device_technology(state, content, sources, table, galaxy, player, card, &technologies)
            else {
                return false;
            };
            chosen
        }
    };
    if !crate::production::pay(
        state,
        content,
        sources,
        table,
        player,
        ENIGMATIC_DEVICE_COST,
        crate::production::Spend::Resources,
    )
    .unwrap_or(false)
    {
        return false;
    }
    if let Some(seat) = state.player_mut(player)
        && let Some(at) = seat.exploration_cards.iter().position(|held| held == card)
    {
        seat.exploration_cards.remove(at); // purged
    }
    crate::technology::gain(state, content, sources, player, &technology);
    true
}

/// Whether recorded decisions use the old per-technology Enigmatic Device ids (`play_area:ed2:ws`),
/// in an answer or among the offered options. If so, marks the state so the replay offers them
/// again and the recorded offers and hashes reproduce. A new game never contains one.
pub fn mark_legacy_device_ids(state: &mut GameState, records: &[crate::choice::DecisionRecord]) {
    let legacy = |id: &String| {
        id.strip_prefix(PLAY_AREA_PREFIX)
            .is_some_and(|rest| rest.contains(':'))
    };
    if records
        .iter()
        .any(|r| legacy(&r.chosen) || r.offered.iter().any(legacy))
    {
        state.legacy_device_ids = true;
    }
}

/// The Enigmatic Device's price, on the relic and on both exploration cards.
const ENIGMATIC_DEVICE_COST: i64 = 6;

/// Marks a play-area exploration card's action apart from every other option id.
const PLAY_AREA_PREFIX: &str = "play_area:";

/// Flip the ion storm after a move that used it.
///
/// > At the end of the "Move Ships" or "Retreat" substep of a tactical action during which 1 or
/// > more of your ships use the ion storm wormhole, flip the ion storm token to its opposing side.
///
/// "Use the wormhole" means the move crossed it, which is true when the storm's system is one of
/// the two ends of a wormhole hop -- the move's origin or its destination. Returns whether it
/// flipped.
pub fn flip_ion_storm(
    state: &mut GameState,
    from: &ti4_model::id::SystemId,
    to: &ti4_model::id::SystemId,
) -> bool {
    let Some((system, face)) = state.ion_storm.as_ref() else {
        return false;
    };
    if system != from && system != to {
        return false;
    }
    let flipped = if face == "ALPHA" { "BETA" } else { "ALPHA" };
    state.ion_storm = Some((system.clone(), flipped.to_owned()));
    true
}

/// Exploration cards this engine resolves.
///
/// The rest are drawn, announced [`Explored::Unresolved`], and do nothing — the registry design
/// used throughout. `unimplemented` reports them.
#[must_use]
pub fn registered_cards() -> Vec<&'static str> {
    vec![
        "aw1", "aw2", "aw3", "aw4", "cm1", "cm2", "cm3", "dv1", "dv2", "dw", "ed1", "ed2", "ent",
        "exp1", "exp2", "exp3", "fb1", "fb2", "fb3", "fb4", "frln1", "frln2", "frln3", "gamma",
        "gw", "ion", "kel1", "kel2", "lc1", "lc2", "lf1", "lf2", "lf3", "lf4", "majent", "minent",
        "mirage", "mo1", "mo2", "mo3", "ms1", "ms2", "vfs1", "vfs2", "vfs3",
    ]
}

/// Cards the engine draws but cannot resolve.
#[must_use]
pub fn unimplemented(content: &ContentStore, sources: SourceSet) -> Vec<String> {
    let known = registered_cards();
    content
        .from_sources(ContentType::Explores, sources)
        .filter(|record| !matches!(record.text("resolution"), Some("Fragment" | "Attach")))
        .filter_map(|record| record.text("id").or_else(|| record.text("alias")))
        .filter(|id| !known.contains(id))
        .map(ToOwned::to_owned)
        .collect()
}

/// Resolve an instant card this engine knows.
///
/// Returns `false` for a card with no handler, so the caller announces it unresolved rather
/// than reporting a card that did nothing as having worked.
#[allow(
    clippy::too_many_lines,
    reason = "one arm per card: the list is the point, and splitting it hides the set"
)]
fn resolve_instant(
    state: &mut GameState,
    ctx: &mut crate::choice::Resolving<'_>,
    player: &PlayerId,
    planet: Option<&PlanetId>,
    card: &str,
) -> bool {
    let (content, sources) = (ctx.content, ctx.sources);
    // Command tokens go to the strategy pool. LRR 52.4 lets the player choose, and this does
    // not ask — recorded as a simplification rather than passed off as the rule.
    let (tokens, goods) = match card {
        "minent" => (1, 1),
        "ent" => (1, 2),
        "majent" => (1, 3),
        "kel1" | "kel2" => (2, 0),
        "gw" | "gamma" => {
            // Gamma Wormhole and Gamma Relay both read "place a gamma wormhole token in this
            // system, then purge this card". `wormhole_tokens` already exists for the Creuss
            // tokens and is keyed by kind, which is what makes two cards placing the same token
            // one line rather than two.
            //
            // A gamma wormhole is not alpha or beta, so neither wormhole law touches it: Enforced
            // Travel Ban and Wormhole Reconstruction both name those two.
            let Some(system) = state.active_system.clone() else {
                return false;
            };
            state.wormhole_tokens.insert("GAMMA".to_owned(), system);
            return true;
        }
        "ion" => {
            // "Place the ion storm token in this system with either side faceup. Then, place this
            // card in the common play area."
            //
            // The token is a wormhole showing alpha or beta, and which side is up is the player's
            // choice. The flip clause -- after ships move through it -- is handled by
            // `flip_ion_storm`, called from the tactical action.
            let Some(here) = planet else {
                return false;
            };
            let Some(system) = ti4_content::galaxy::planet(content, here.as_str(), sources)
                .and_then(|record| record.system_id())
                .map(ti4_model::id::SystemId::new)
            else {
                return false;
            };
            let face = ask(
                ctx,
                state,
                player,
                "ion_storm",
                "Ion Storm: which side faceup",
                &[("ALPHA", "alpha wormhole"), ("BETA", "beta wormhole")],
            )
            .unwrap_or_else(|| "ALPHA".to_owned());
            state.ion_storm = Some((system, face));
            return true;
        }
        "ed1" | "ed2" => {
            // "Place this card faceup in your play area. ACTION: You may spend 6 resource and purge
            // this card to research 1 technology."
            //
            // The exploring is over once the card is placed; the ACTION comes later, from
            // `available_actions`. Same text as the Enigmatic Device relic, and the same helper
            // resolves both -- but it is not a relic, so it lives in its own place rather than
            // borrowing the relic list and picking up every relic effect on the way.
            if let Some(seat) = state.player_mut(player) {
                seat.exploration_cards.push(card.to_owned());
            }
            return true;
        }
        "mirage" => {
            // "Place the Mirage planet token in this system. Gain the Mirage planet card and ready
            // it. Then, purge this card."
            //
            // The system comes from the planet being explored, which is the only "this system"
            // there is: exploration always happens on a planet, and Mirage joins it on its tile.
            let Some(here) = planet else {
                return false; // 22.3: no planet explored means no system to place it in
            };
            let Some(system) = ti4_content::galaxy::planet(content, here.as_str(), sources)
                .and_then(|record| record.system_id())
                .map(ti4_model::id::SystemId::new)
            else {
                return false;
            };
            return crate::planets::place(state, &system, &PlanetId::new("mirage"), player);
        }
        "frln1" | "frln2" | "frln3" => {
            // Freelancers: "You may produce 1 unit in this system. You may spend influence as if it
            // were resources to produce this unit."
            //
            // One unit, not one purchase, and the influence clause is a substitution at the paying
            // site rather than a different kind of bill -- `Spend::Influence` already exists, so
            // what this needs is for the production window to accept it.
            let Some(here) = planet else {
                return false;
            };
            let Some(system) = ti4_content::galaxy::planet(content, here.as_str(), sources)
                .and_then(|record| record.system_id())
                .map(ti4_model::id::SystemId::new)
            else {
                return false;
            };
            return crate::production::produce_one_paying_with_influence(
                state, ctx, player, &system, card,
            );
        }
        "dw" => {
            // Draw 1 relic, through `relics::gain` — a relic can be worth a point the moment it
            // arrives, and taking it off the deck here scored nobody the Shard.
            crate::relics::gain(state, player);
            return true; // an empty deck gives nothing, which is not a failure
        }
        "aw1" | "aw2" | "aw3" | "aw4" => {
            let chosen = ask(
                ctx,
                state,
                player,
                "abandoned_warehouses",
                "Abandoned Warehouses",
                &[
                    ("gain", "gain 2 commodities"),
                    ("convert", "convert up to 2 commodities to trade goods"),
                ],
            );
            if chosen.as_deref() == Some("convert") {
                convert_commodities(state, ctx, player, Some(2));
            } else {
                gain_commodities(state, content, player, 2);
            }
            return true;
        }
        "ms1" | "ms2" => {
            let chosen = ask(
                ctx,
                state,
                player,
                "merchant_station",
                "Merchant Station",
                &[
                    ("replenish", "replenish commodities"),
                    ("convert", "convert commodities to trade goods"),
                ],
            );
            if chosen.as_deref() == Some("convert") {
                convert_commodities(state, ctx, player, None);
            } else {
                let limit = commodity_limit(state, content, player);
                gain_commodities(state, content, player, limit);
                // A replenish like any other: Trade Agreement and Stillness of Stars read it.
                crate::promissory::trade_agreement_on_replenish(state, player);
            }
            return true;
        }
        "fb1" | "fb2" | "fb3" | "fb4" => {
            let (goods_held, commodities_held) = state
                .player(player)
                .map_or((0, 0), |seat| (seat.trade_goods, seat.commodities));
            let mut options = vec![("gain", "gain 1 commodity")];
            if goods_held >= 1 {
                options.push(("spend_tg", "spend 1 trade good to draw an action card"));
            }
            if commodities_held >= 1 {
                options.push(("spend_com", "spend 1 commodity to draw an action card"));
            }
            let chosen = ask(
                ctx,
                state,
                player,
                "functioning_base",
                "Functioning Base",
                &options,
            );
            match chosen.as_deref() {
                Some("spend_tg" | "spend_com") => {
                    if let Some(seat) = state.player_mut(player) {
                        if chosen.as_deref() == Some("spend_tg") {
                            seat.trade_goods -= 1;
                        } else {
                            seat.commodities -= 1;
                        }
                    }
                    let _ = crate::action_cards::draw(state, content, ctx.table, player, 1);
                }
                _ => gain_commodities(state, content, player, 1),
            }
            return true;
        }
        "lf1" | "lf2" | "lf3" | "lf4" => {
            let Some(planet) = planet else {
                return true; // no planet, so nothing to build on
            };
            let (goods_held, commodities_held) = state
                .player(player)
                .map_or((0, 0), |seat| (seat.trade_goods, seat.commodities));
            let mut options = vec![("gain", "gain 1 commodity")];
            let forbidden = crate::factions::hooks_economy::effect_placement_forbidden(
                state,
                content,
                sources,
                player,
                &ti4_model::id::UnitTypeId::new("mech"),
            );
            if goods_held >= 1 && !forbidden {
                options.push(("spend_tg", "spend 1 trade good to place a mech"));
            }
            if commodities_held >= 1 && !forbidden {
                options.push(("spend_com", "spend 1 commodity to place a mech"));
            }
            let chosen = ask(
                ctx,
                state,
                player,
                "local_fabricators",
                "Local Fabricators",
                &options,
            );
            match chosen.as_deref() {
                Some("spend_tg" | "spend_com") => {
                    if !place_on_planet(state, content, sources, player, planet, "mech") {
                        return true; // no mech to place, and nothing was charged for it
                    }
                    if let Some(seat) = state.player_mut(player) {
                        if chosen.as_deref() == Some("spend_tg") {
                            seat.trade_goods -= 1;
                        } else {
                            seat.commodities -= 1;
                        }
                    }
                }
                _ => gain_commodities(state, content, player, 1),
            }
            return true;
        }
        "mo1" | "mo2" | "mo3" => {
            let Some(planet) = planet else {
                return true;
            };
            let chosen = ask(
                ctx,
                state,
                player,
                "mercenary_outfit",
                "Mercenary Outfit",
                &[("place", "place 1 infantry"), ("decline", "place nothing")],
            );
            if chosen.as_deref() == Some("place") {
                place_on_planet(state, content, sources, player, planet, "infantry");
            }
            return true;
        }
        "cm1" | "cm2" | "cm3" => {
            let Some(planet) = planet else {
                return true;
            };
            if pay_with_mech_or_infantry(
                ctx,
                state,
                content,
                sources,
                player,
                planet,
                card,
                "gain 1 trade good",
            ) {
                crate::supply::gain_trade_goods_announced(state, ctx, player, 1, "exploration");
            }
            return true;
        }
        "exp1" | "exp2" | "exp3" => {
            let Some(planet) = planet else {
                return true;
            };
            if pay_with_mech_or_infantry(
                ctx,
                state,
                content,
                sources,
                player,
                planet,
                card,
                "ready this planet",
            ) {
                state.exhausted_planets.remove(planet);
            }
            return true;
        }
        "vfs1" | "vfs2" | "vfs3" => {
            let Some(planet) = planet else {
                return true;
            };
            if pay_with_mech_or_infantry(
                ctx,
                state,
                content,
                sources,
                player,
                planet,
                card,
                "gain 1 command token",
            ) {
                state.gain_token(player, ti4_model::state::TokenPool::Strategic, 1);
            }
            return true;
        }
        "dv1" | "dv2" => {
            let _ = crate::secrets::draw(state, content, ctx.table, player);
            return true;
        }
        "lc1" | "lc2" => {
            let _ = crate::action_cards::draw(state, content, ctx.table, player, 2);
            return true;
        }
        _ => return false,
    };
    state.gain_token(player, ti4_model::state::TokenPool::Strategic, tokens);
    crate::supply::gain_trade_goods_announced(state, ctx, player, goods, "exploration");
    true
}

/// Systems with nothing to land on, which is where a frontier token goes (35.5).
///
/// Deep space and planetless anomalies both qualify: what matters is that there is nothing to
/// land on, which is what makes the token the only reason to go there at all.
#[must_use]
pub fn frontier_systems(
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &Galaxy,
) -> Vec<SystemId> {
    let mut systems: Vec<SystemId> = galaxy
        .system_ids_holding_things()
        .into_iter()
        .filter(|id| {
            ti4_content::galaxy::system(content, id, sources).is_none_or(|record| {
                // Space stations rule 14: "If a system tile contains a space station, but no
                // planets, then a frontier token will be placed in that system during game setup."
                // A station is listed in `planets`, so a station-only tile read as "has planets"
                // and got no token. The Watchtower (117) is the tile this affects.
                record
                    .planets()
                    .into_iter()
                    .all(|planet| ti4_content::galaxy::is_space_station(content, planet, sources))
            })
        })
        .map(SystemId::new)
        .collect();
    systems.sort();
    systems
}

/// Put a frontier token on every planetless system (35.5). Returns how many were placed.
///
/// Nothing in this engine placed one: `frontier_tokens` was declared, initialised empty, and never
/// written, so the twenty-card frontier deck was unreachable and no frontier fragment could ever
/// be gained. The oracle places them at game start (`engine/game.py`), and this mirrors that.
pub fn place_frontier_tokens(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &Galaxy,
) -> usize {
    let systems = frontier_systems(content, sources, galaxy);
    let count = systems.len();
    state.frontier_tokens = systems.into_iter().collect();
    count
}

/// Explore a frontier token: remove it and resolve one card from the frontier deck (35.5).
///
/// The caller decides that exploration is legal — LRR 35 allows it only for a player who owns
/// the Dark Energy Tap technology or another game effect — so this function checks only the
/// mechanical preconditions: the token is in the system, and the player has a ship in it.
///
/// The token is removed whether or not the card does anything, so it is consumed before the card
/// resolves — a card that fails to resolve must not leave the token for the same fleet to trip
/// over again next turn.
pub fn explore_frontier(
    state: &mut GameState,
    ctx: &mut crate::choice::Resolving<'_>,
    player: &PlayerId,
    system: &SystemId,
) -> Option<Explored> {
    if !state.frontier_tokens.contains(system) {
        return None;
    }
    let has_ship = state
        .system_state(system)
        .units
        .iter()
        .any(|unit| &unit.owner == player);
    if !has_ship {
        return None;
    }
    state.frontier_tokens.remove(system);
    let logged = state.exploration_log.len();
    let outcome = explore_with(state, ctx, player, FRONTIER, None);
    if let Some(record) = state.exploration_log.get_mut(logged) {
        record.system = Some(system.to_string());
    }
    outcome
}

/// Explore a planet, resolving one card (35.2).
///
/// `planet` is `None` for a frontier draw, which is the point of the frontier deck: those cards
/// are resolved without a planet.
pub fn explore(
    state: &mut GameState,
    content: &ContentStore,
    player: &PlayerId,
    deck: &str,
    planet: Option<&PlanetId>,
) -> Option<Explored> {
    let mut table = crate::choice::Table::new();
    let mut dice = crate::dice::Dice::new();
    let mut rng = crate::rng::GameRng::new(0);
    let mut ctx = crate::choice::Resolving {
        content,
        sources: ti4_model::content_types::POK,
        dice: &mut dice,
        rng: &mut rng,
        table: &mut table,
        timing: None,
    };
    explore_with(state, &mut ctx, player, deck, planet)
}

/// Explore a planet with the table that is answering this action (35.2).
///
/// Most instant cards read "you may", so resolving one is a decision. Taking the first option
/// on the player's behalf is what the plain [`explore`] does, and it is only right when nobody
/// is seated — a driver with a table should pass it.
pub fn explore_with(
    state: &mut GameState,
    ctx: &mut crate::choice::Resolving<'_>,
    player: &PlayerId,
    deck: &str,
    planet: Option<&PlanetId>,
) -> Option<Explored> {
    let logged = state.exploration_log.len();
    let before = Holdings::of(state, player, planet);
    let wormholes_before = state.wormhole_tokens.clone();
    let ion_before = state.ion_storm.clone();
    let outcome = explore_drawn(state, ctx, player, deck, planet)?;
    // UI-01: the record `explore_drawn` opened gets its outcome now that it is known.
    let text = match &outcome {
        Explored::Fragment { .. } => "fragment".to_owned(),
        Explored::Attached { .. } => format!(
            "attached:{}",
            planet
                .and_then(|planet| state.planet_attachments.get(planet))
                .and_then(|attached| attached.last().cloned())
                .unwrap_or_default()
        ),
        Explored::Unresolved { .. } => "unresolved".to_owned(),
        Explored::Resolved { .. } => "resolved".to_owned(),
        Explored::Discarded { .. } => "discarded".to_owned(),
    };
    let after = Holdings::of(state, player, planet);
    let result = before.describe_change(&after, planet, &outcome);
    let gains = before.gains(&after);
    let mut placed = Vec::new();
    for (kind, system) in &state.wormhole_tokens {
        if wormholes_before.get(kind) != Some(system) {
            placed.push(format!("wormhole:{kind}:{system}"));
        }
    }
    if state.ion_storm != ion_before
        && let Some((system, face)) = &state.ion_storm
    {
        placed.push(format!("ion_storm:{system}:{face}"));
    }
    if let Explored::Fragment { trait_name } = &outcome {
        placed.push(format!("fragment:{}", trait_name.to_ascii_lowercase()));
    }
    if after.play_area > before.play_area
        && let Some(card) = state.player(player).and_then(|seat| seat.exploration_cards.last())
    {
        placed.push(format!("play_area:{card}"));
    }
    if let (Explored::Attached { .. }, Some(planet)) = (&outcome, planet)
        && let Some(id) = text.strip_prefix("attached:")
    {
        placed.push(format!("attachment:{id}:{planet}"));
    }
    if let Some(record) = state.exploration_log.get_mut(logged) {
        record.outcome = text;
        record.result = result;
        record.gains = gains;
        record.placed = placed;
    }
    if let Some(explored) = planet {
        // Pre-Fab Arcologies et al. run first (they change the state), then the typed event.
        crate::factions::hooks_economy::explored(state, ctx.content, ctx.sources, player, explored);
        let mut payload = std::collections::BTreeMap::new();
        payload.insert("player".to_owned(), player.to_string().into());
        payload.insert("planet".to_owned(), explored.to_string().into());
        payload.insert("deck".to_owned(), deck.into());
        payload.insert(
            "card".to_owned(),
            state
                .exploration_log
                .get(logged)
                .map(|record| record.card.clone())
                .unwrap_or_default()
                .into(),
        );
        crate::factions::hooks_economy::emit(ctx, state, "PLANET_EXPLORED", payload);
    }
    Some(outcome)
}

/// What a seat holds that an exploration card can change; compared before and after a card to say
/// what it did. Display only.
struct Holdings {
    trade_goods: i32,
    commodities: i32,
    command_tokens: i32,
    action_cards: usize,
    secrets: usize,
    relics: usize,
    technologies: usize,
    play_area: usize,
    own_infantry: usize,
    own_mechs: usize,
    exhausted: bool,
}

impl Holdings {
    fn of(state: &GameState, player: &PlayerId, planet: Option<&PlanetId>) -> Self {
        let seat = state.player(player);
        let (mut own_infantry, mut own_mechs) = (0, 0);
        if let Some(planet) = planet
            && let Some(system) = system_of(state, planet)
            && let Some(units) = state.system_state(&system).planet_units.get(planet)
        {
            for unit in units.iter().filter(|unit| &unit.owner == player) {
                let id = unit.type_id.as_str();
                if id.contains("infantry") {
                    own_infantry += 1;
                } else if id.contains("mech") {
                    own_mechs += 1;
                }
            }
        }
        Self {
            trade_goods: seat.map_or(0, |seat| seat.trade_goods),
            commodities: seat.map_or(0, |seat| seat.commodities),
            command_tokens: seat.map_or(0, |seat| {
                seat.tactic_tokens + seat.fleet_tokens + seat.strategic_tokens
            }),
            action_cards: seat.map_or(0, |seat| seat.action_cards.len()),
            secrets: seat.map_or(0, |seat| seat.secret_objectives.len()),
            relics: seat.map_or(0, |seat| seat.relics.len()),
            technologies: seat.map_or(0, |seat| seat.technologies.len()),
            play_area: seat.map_or(0, |seat| seat.exploration_cards.len()),
            own_infantry,
            own_mechs,
            exhausted: planet.is_some_and(|planet| state.exhausted_planets.contains(planet)),
        }
    }

    /// The signed change of each holding, for the exploration record's `gains`.
    fn gains(&self, after: &Self) -> std::collections::BTreeMap<String, i32> {
        let to_i32 = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);
        [
            ("trade_goods", after.trade_goods - self.trade_goods),
            ("commodities", after.commodities - self.commodities),
            ("command_tokens", after.command_tokens - self.command_tokens),
            ("action_cards", to_i32(after.action_cards) - to_i32(self.action_cards)),
            ("secret_objectives", to_i32(after.secrets) - to_i32(self.secrets)),
            ("relics", to_i32(after.relics) - to_i32(self.relics)),
            ("technologies", to_i32(after.technologies) - to_i32(self.technologies)),
        ]
        .into_iter()
        .filter(|(_, change)| *change != 0)
        .map(|(key, change)| (key.to_owned(), change))
        .collect()
    }

    /// "gained 1 trade good, removed 1 infantry" -- empty when nothing the seat holds changed.
    fn describe_change(
        &self,
        after: &Self,
        planet: Option<&PlanetId>,
        outcome: &Explored,
    ) -> String {
        fn plural(n: i64, one: &str, many: &str) -> String {
            format!("{n} {}", if n == 1 { one } else { many })
        }
        let mut parts: Vec<String> = Vec::new();
        let delta = |a: i64, b: i64| b - a;
        let goods = delta(i64::from(self.trade_goods), i64::from(after.trade_goods));
        if goods > 0 {
            parts.push(format!(
                "gained {}",
                plural(goods, "trade good", "trade goods")
            ));
        } else if goods < 0 {
            parts.push(format!(
                "spent {}",
                plural(-goods, "trade good", "trade goods")
            ));
        }
        let commodities = delta(i64::from(self.commodities), i64::from(after.commodities));
        if commodities > 0 {
            parts.push(format!(
                "gained {}",
                plural(commodities, "commodity", "commodities")
            ));
        } else if commodities < 0 && goods <= 0 {
            // Converting commodities into trade goods is reported by the goods gained.
            parts.push(format!(
                "spent {}",
                plural(-commodities, "commodity", "commodities")
            ));
        }
        let tokens = delta(
            i64::from(self.command_tokens),
            i64::from(after.command_tokens),
        );
        if tokens > 0 {
            parts.push(format!(
                "gained {}",
                plural(tokens, "command token", "command tokens")
            ));
        }
        let cards = delta(self.action_cards as i64, after.action_cards as i64);
        if cards > 0 {
            parts.push(format!(
                "drew {}",
                plural(cards, "action card", "action cards")
            ));
        }
        if after.secrets > self.secrets {
            parts.push("drew a secret objective".to_owned());
        }
        if after.relics > self.relics {
            parts.push("gained a relic".to_owned());
        }
        if after.technologies > self.technologies {
            parts.push("gained a technology".to_owned());
        }
        if after.play_area > self.play_area {
            // Enigmatic Device: kept faceup, and its ACTION is offered from there.
            parts.push("placed it faceup in the play area (ACTION: spend 6 resources to research a technology)".to_owned());
        }
        let infantry = delta(self.own_infantry as i64, after.own_infantry as i64);
        if infantry < 0 {
            parts.push(format!(
                "removed {} from the planet",
                plural(-infantry, "infantry", "infantry")
            ));
        } else if infantry > 0 {
            parts.push(format!(
                "placed {} on the planet",
                plural(infantry, "infantry", "infantry")
            ));
        }
        if after.own_mechs > self.own_mechs {
            parts.push("placed a mech on the planet".to_owned());
        }
        if self.exhausted && !after.exhausted && planet.is_some() {
            parts.push("readied the planet".to_owned());
        }
        match outcome {
            Explored::Fragment { trait_name } => parts.insert(
                0,
                format!("took a {} relic fragment", trait_name.to_ascii_lowercase()),
            ),
            Explored::Unresolved { .. } => parts.insert(0, "could not be resolved".to_owned()),
            Explored::Discarded { .. } => {
                parts.insert(0, "discarded (nothing to attach to)".to_owned());
            }
            Explored::Attached { .. } | Explored::Resolved { .. } => {}
        }
        parts.join(", ")
    }
}

fn explore_drawn(
    state: &mut GameState,
    ctx: &mut crate::choice::Resolving<'_>,
    player: &PlayerId,
    deck: &str,
    planet: Option<&PlanetId>,
) -> Option<Explored> {
    let content = ctx.content;
    // The deck is already ordered. A draw need not touch the RNG, but its
    // identity and every reward it causes are still unknown to a preview.
    ctx.rng.observe_hidden_information();
    let mut card = draw(state, deck)?;
    // Naaz Distant Suns: draw 1 additional card, resolve 1 of the two, discard the other (a
    // discarded fragment is not gained). The engine keeps no discard pile; the card leaves the deck.
    if let Some(here) = planet
        && crate::factions::hooks_economy::explore_extra_draw(
            state,
            content,
            ctx.sources,
            player,
            here,
        )
        && let Some(second) = draw(state, deck)
    {
        let label = |id: &str| format!("resolve {id}");
        let (first_label, second_label) = (label(&card), label(&second));
        let picked = ask(
            ctx,
            state,
            player,
            &card,
            "Distant Suns: which exploration card to resolve",
            &[
                (card.as_str(), &first_label),
                (second.as_str(), &second_label),
            ],
        );
        if picked.as_deref() == Some(second.as_str()) {
            card = second;
        }
    }
    state
        .exploration_log
        .push(ti4_model::state::ExplorationRecord {
            round: state.round,
            player: player.clone(),
            deck: deck.to_owned(),
            card: card.clone(),
            planet: planet.cloned(),
            outcome: String::new(),
            system: None,
            result: String::new(),
            gains: Default::default(),
            placed: Vec::new(),
        });
    let kind = resolution(content, &card).unwrap_or_default();

    let outcome = match kind.as_str() {
        "Fragment" => {
            // 35.9: fragments stay faceup in the play area until purged for a relic. A
            // frontier fragment needs no planet, which is most of why the frontier deck is
            // worth drawing at all.
            let trait_name = content
                .get(ContentType::Explores, &card)
                .and_then(|record| record.text("type"))
                .unwrap_or(deck)
                .to_ascii_uppercase();
            gain_fragment(state, player, &trait_name);
            Explored::Fragment { trait_name }
        }
        "Attach" => {
            let Some(planet) = planet else {
                // Discarded rather than silently applied to nothing, and said out loud so a
                // count of unresolved cards stays honest.
                return Some(Explored::Discarded { card });
            };
            // Stored by *attachment* id, the key every reader uses (`tombofemphidia` for the
            // Crown's check, the attachment record for the planet's value). The explore card id
            // (`toe`, `ds`) named nothing outside this deck.
            let attachment = attachment_for(content, &card, planet);
            state
                .planet_attachments
                .entry(planet.clone())
                .or_default()
                .push(attachment);
            Explored::Attached { card }
        }
        // Instant and token cards need per-card handlers. Those this engine has are resolved;
        // the rest are announced rather than dropped, so an unimplemented card is visible as a
        // gap instead of passing for one that did nothing on purpose.
        _ => {
            if resolve_instant(state, ctx, player, planet, &card) {
                Explored::Resolved { card }
            } else {
                Explored::Unresolved { card }
            }
        }
    };
    Some(outcome)
}

/// Add one relic fragment of a trait to a player's play area.
pub fn gain_fragment(state: &mut GameState, player: &PlayerId, trait_name: &str) {
    if let Some(seat) = state.player_mut(player) {
        *seat
            .relic_fragments
            .entry(trait_name.to_ascii_uppercase())
            .or_insert(0) += 1;
    }
}

/// Traits this player could purge three of for a relic (35.9).
///
/// Frontier fragments substitute for any trait, so they are counted towards every other one
/// rather than forming a pile of their own that can never be cashed.
#[must_use]
pub fn purgeable(state: &GameState, player: &PlayerId) -> Vec<String> {
    let Some(seat) = state.player(player) else {
        return Vec::new();
    };
    let frontier = seat.relic_fragments.get(FRONTIER).copied().unwrap_or(0);
    seat.relic_fragments
        .iter()
        .filter(|(trait_name, _)| trait_name.as_str() != FRONTIER)
        .filter(|(_, held)| **held + frontier >= FRAGMENTS_PER_RELIC)
        .map(|(trait_name, _)| trait_name.clone())
        .collect()
}

/// Purge three fragments of a trait and draw a relic (35.9).
///
/// Frontier fragments make up any shortfall, and are spent only after the matching ones — a
/// wildcard spent first would be a wildcard wasted.
pub fn purge_for_relic(
    state: &mut GameState,
    player: &PlayerId,
    trait_name: &str,
) -> Option<RelicId> {
    let trait_name = trait_name.to_ascii_uppercase();
    let seat = state.player(player)?;
    let matching = seat.relic_fragments.get(&trait_name).copied().unwrap_or(0);
    let frontier = seat.relic_fragments.get(FRONTIER).copied().unwrap_or(0);
    if matching + frontier < FRAGMENTS_PER_RELIC {
        return None;
    }
    let from_matching = matching.min(FRAGMENTS_PER_RELIC);
    let from_frontier = FRAGMENTS_PER_RELIC - from_matching;

    let relic = ti4_model::deck_reserve::take_top(
        &mut state.deck_reserve,
        "relic",
        &mut state.relic_deck,
        Some(player.as_str()),
    )?;

    let seat = state.player_mut(player)?;
    *seat.relic_fragments.entry(trait_name).or_insert(0) -= from_matching;
    if from_frontier > 0 {
        *seat.relic_fragments.entry(FRONTIER.to_owned()).or_insert(0) -= from_frontier;
    }
    seat.relics.push(relic.clone());
    Some(relic)
}

#[cfg(test)]
mod tests {
    use ti4_model::content_types::POK;

    /// Gamma Wormhole and Gamma Relay both place the gamma token in the explored system.
    ///
    /// Driven through `resolve_instant`, which is what the draw path calls, so a handler that
    /// existed but was never reached would fail here.
    #[test]
    fn the_gamma_cards_place_their_token_in_the_active_system() {
        for card in ["gw", "gamma"] {
            let mut state = crate::fixtures::game(&["a"]);
            let system = ti4_model::id::SystemId::new("19");
            state.active_system = Some(system.clone());

            let content = ti4_content::ContentStore::embedded();
            let mut dice = crate::dice::Dice::new();
            let mut rng = crate::rng::GameRng::new(3);
            let mut table = crate::choice::Table::new();
            let mut ctx = crate::choice::Resolving {
                content,
                sources: ti4_model::content_types::DEFAULT,
                dice: &mut dice,
                rng: &mut rng,
                table: &mut table,
                timing: None,
            };

            assert!(
                resolve_instant(&mut state, &mut ctx, &PlayerId::new("a"), None, card),
                "{card} must resolve"
            );
            assert_eq!(
                state.wormhole_tokens.get("GAMMA"),
                Some(&system),
                "{card} places the gamma token where it was explored"
            );
        }
    }

    /// With no active system there is nowhere to put it, and the card says so rather than
    /// silently succeeding.
    #[test]
    fn a_gamma_card_with_nowhere_to_place_reports_unresolved() {
        let mut state = crate::fixtures::game(&["a"]);
        state.active_system = None;
        let content = ti4_content::ContentStore::embedded();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(3);
        let mut table = crate::choice::Table::new();
        let mut ctx = crate::choice::Resolving {
            content,
            sources: ti4_model::content_types::DEFAULT,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };
        assert!(!resolve_instant(
            &mut state,
            &mut ctx,
            &PlayerId::new("a"),
            None,
            "gw"
        ));
        assert!(state.wormhole_tokens.is_empty());
    }

    /// OBS-003h slice 2: an exploration decision identifies the card effect, rather than asking
    /// the policy to recover it from its display prompt.
    #[test]
    fn obs003h_exploration_choice_carries_card_context() {
        let (system, planet) = crate::fixtures::a_placed_planet();
        let player = PlayerId::new("a");
        let mut state = game(&["a"]);
        state.board.entry(system).or_default();
        let (decider, seen) = crate::choice::Capturing::new(Box::new(crate::choice::FirstOption));
        let mut table = crate::choice::Table::with_default(Box::new(decider));
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(3);
        let mut ctx = crate::choice::Resolving {
            content: ContentStore::embedded(),
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };

        assert!(resolve_instant(
            &mut state,
            &mut ctx,
            &player,
            Some(&planet),
            "ion"
        ));

        let context = seen.borrow()[0].context.clone().expect("typed context");
        assert_eq!(
            context.source,
            DecisionSource::Content("ion_storm".to_owned())
        );
        assert_eq!(context.subtype, "ion_storm_choose_reward");
    }

    use ti4_model::content_types::DEFAULT as ALL_SOURCES;

    /// Space stations rule 14: a tile with a station but no planets still takes a frontier token.
    ///
    /// The Watchtower (117) is the only such tile in the corpus. Before this it read as "has a
    /// planet" because the station is listed in the tile's `planets` array, so the token was never
    /// placed and the frontier deck was unreachable from that system.
    #[test]
    fn a_station_only_system_takes_a_frontier_token() {
        let content = ti4_content::ContentStore::embedded();
        let ids = ["117", "18", "19", "20", "21", "22", "23"];
        let galaxy =
            ti4_content::galaxy::Galaxy::build(content, &ids, ALL_SOURCES, 1).expect("a valid map");

        let systems = frontier_systems(content, ALL_SOURCES, &galaxy);
        assert!(
            systems.contains(&SystemId::new("117")),
            "117 holds only a space station, so rule 14 puts a frontier token there: {systems:?}"
        );
    }

    use super::*;
    use crate::fixtures::game;

    fn player() -> PlayerId {
        PlayerId::new("a")
    }

    #[test]
    fn a_planet_explores_into_its_own_trait_deck() {
        // 35.2b: a planet with no trait cannot be explored at all.
        let catalogue = ti4_content::galaxy::all_planets(ContentStore::embedded(), POK);
        let mut traited = 0;
        let mut untraited = 0;
        for (id, record) in &catalogue {
            let found = trait_of(ContentStore::embedded(), POK, &PlanetId::new(*id));
            match record.planet_type() {
                Some(kind) if EXPLORATION_TRAITS.contains(&kind.to_ascii_uppercase().as_str()) => {
                    assert_eq!(found.as_deref(), Some(kind.to_ascii_uppercase().as_str()));
                    traited += 1;
                }
                _ => {
                    assert_eq!(found, None);
                    untraited += 1;
                }
            }
        }
        assert!(traited > 0 && untraited > 0, "the corpus has both");
    }

    /// `traits_of` reports every deck a dual-trait planet could explore into, in the planet's own
    /// printed order -- not just the first one, which is what `trait_of` still gives callers who
    /// have no decider to ask.
    #[test]
    fn a_dual_trait_planet_reports_both_of_its_decks() {
        let content = ContentStore::embedded();
        let planet = PlanetId::new("lazulrex");
        assert_eq!(
            traits_of(content, ti4_model::content_types::FULL, &planet),
            vec!["INDUSTRIAL".to_owned(), "CULTURAL".to_owned()]
        );
        assert_eq!(
            trait_of(content, ti4_model::content_types::FULL, &planet).as_deref(),
            Some("INDUSTRIAL"),
            "the single-deck lookup still reports only the first, for callers with no decider"
        );
    }

    /// When more than one deck qualifies, `choose_deck` asks and honors the answer rather than
    /// silently taking the planet's first printed trait.
    #[test]
    fn choose_deck_asks_when_a_planet_has_more_than_one_trait() {
        let planet = PlanetId::new("lazulrex");
        let state = game(&["a"]);
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut table =
            crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new([
                "CULTURAL",
            ])));
        let mut ctx = crate::choice::Resolving {
            content: ContentStore::embedded(),
            sources: ti4_model::content_types::FULL,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };

        assert_eq!(
            choose_deck(&mut ctx, &state, &player(), &planet).as_deref(),
            Some("CULTURAL"),
            "the answer is honored, not the printed order"
        );
    }

    #[test]
    fn drawing_takes_from_the_top_and_empties() {
        let mut state = game(&["a"]);
        state
            .exploration_decks
            .insert("CULTURAL".to_owned(), vec!["one".into(), "two".into()]);

        assert_eq!(draw(&mut state, "CULTURAL").as_deref(), Some("one"));
        assert_eq!(draw(&mut state, "CULTURAL").as_deref(), Some("two"));
        assert_eq!(draw(&mut state, "CULTURAL"), None);
    }

    #[test]
    fn an_unknown_card_is_announced_rather_than_dropped() {
        // An unresolved card must be visible as a gap, not silently discarded.
        let mut state = game(&["a"]);
        state
            .exploration_decks
            .insert("CULTURAL".to_owned(), vec!["not_a_card".into()]);

        let outcome = explore(
            &mut state,
            ContentStore::embedded(),
            &player(),
            "CULTURAL",
            None,
        );
        assert!(matches!(outcome, Some(Explored::Unresolved { .. })));
    }

    #[test]
    fn an_attachment_from_the_frontier_is_discarded_not_applied_to_nothing() {
        let attach = ContentStore::embedded()
            .records(ContentType::Explores)
            .iter()
            .find(|record| record.text("resolution") == Some("Attach"))
            .and_then(|record| record.text("id").or_else(|| record.text("alias")))
            .map(ToOwned::to_owned);
        let Some(attach) = attach else {
            return;
        };

        let mut state = game(&["a"]);
        state
            .exploration_decks
            .insert(FRONTIER.to_owned(), vec![attach]);

        let outcome = explore(
            &mut state,
            ContentStore::embedded(),
            &player(),
            FRONTIER,
            None,
        );
        assert!(matches!(outcome, Some(Explored::Discarded { .. })));
    }

    /// A controlled, printed planet with (or without) a technology specialty, for attachment tests.
    fn a_planet(with_specialty: bool) -> (PlanetId, i64, i64) {
        let planets = ti4_content::galaxy::all_planets(ContentStore::embedded(), POK);
        let (id, record) = planets
            .iter()
            .filter(|(_, record)| !record.is_placed_during_play())
            .find(|(_, record)| record.tech_specialties().is_empty() != with_specialty)
            .expect("the corpus has planets of both kinds");
        (PlanetId::new(*id), record.resources(), record.influence())
    }

    fn value(state: &GameState, planet: &PlanetId, kind: crate::production::Spend) -> i64 {
        crate::production::planet_value_now(state, ContentStore::embedded(), POK, planet, kind)
    }

    /// An explored attachment is stored by its attachment id and changes what the planet is worth:
    /// to pay with, and to vote with. Before, the explore card id (`ds`) was stored, which no
    /// reader recognised, and no reader added an exploration attachment's modifiers at all.
    #[test]
    fn an_explored_attachment_is_stored_by_its_id_and_changes_the_planets_value() {
        use crate::production::Spend;
        let (planet, resources, influence) = a_planet(false);
        let mut state = game(&["a"]);
        state
            .exploration_decks
            .insert("CULTURAL".to_owned(), vec!["ds".to_owned()]);
        let outcome = explore(
            &mut state,
            ContentStore::embedded(),
            &player(),
            "CULTURAL",
            Some(&planet),
        );
        assert!(matches!(outcome, Some(Explored::Attached { .. })));
        assert_eq!(
            state.planet_attachments.get(&planet),
            Some(&vec!["dysonsphere".to_owned()])
        );
        // UI-01: the draw is logged with what came of it.
        let logged = state.exploration_log.last().expect("the draw is logged");
        assert_eq!(
            (
                logged.card.as_str(),
                logged.outcome.as_str(),
                logged.planet.as_ref()
            ),
            ("ds", "attached:dysonsphere", Some(&planet))
        );
        // Dyson Sphere: +2 resources, +1 influence.
        assert_eq!(value(&state, &planet, Spend::Resources), resources + 2);
        assert_eq!(value(&state, &planet, Spend::Influence), influence + 1);
        assert_eq!(
            crate::vote::influence_of(&state, ContentStore::embedded(), POK, &planet),
            influence + 1,
            "an attachment's influence votes"
        );
    }

    /// A research facility is +1/+1 on a planet that already has a specialty, and otherwise gives
    /// the planet the specialty and no value.
    #[test]
    fn a_research_facility_depends_on_the_planets_specialty() {
        use crate::production::Spend;
        for (with_specialty, stored, bonus) in [(true, "bioticstat", 1), (false, "biotic", 0)] {
            let (planet, resources, influence) = a_planet(with_specialty);
            assert_eq!(
                attachment_for(ContentStore::embedded(), "biotic", &planet),
                stored
            );
            let mut state = game(&["a"]);
            state
                .planet_attachments
                .insert(planet.clone(), vec![stored.to_owned()]);
            assert_eq!(value(&state, &planet, Spend::Resources), resources + bonus);
            assert_eq!(value(&state, &planet, Spend::Influence), influence + bonus);
        }
    }

    /// Nano-Forge is no longer a special case, and must still be worth exactly +2/+2, not +4/+4.
    #[test]
    fn nanoforge_counts_once() {
        use crate::production::Spend;
        let (planet, resources, influence) = a_planet(false);
        let mut state = game(&["a"]);
        state
            .planet_attachments
            .insert(planet.clone(), vec!["nanoforge".to_owned()]);
        assert_eq!(value(&state, &planet, Spend::Resources), resources + 2);
        assert_eq!(value(&state, &planet, Spend::Influence), influence + 2);
    }

    #[test]
    fn an_instant_card_pays_what_it_says() {
        // Minor, ordinary and major Entities differ only by the trade goods, so a handler that
        // confused them would be invisible without checking the numbers.
        for (card, tokens, goods) in [("minent", 1, 1), ("ent", 1, 2), ("majent", 1, 3)] {
            let mut state = game(&["a"]);
            state
                .exploration_decks
                .insert("CULTURAL".to_owned(), vec![card.to_owned()]);
            let before = state.player(&player()).unwrap().clone();

            let outcome = explore(
                &mut state,
                ContentStore::embedded(),
                &player(),
                "CULTURAL",
                None,
            );

            assert!(matches!(outcome, Some(Explored::Resolved { .. })), "{card}");
            let after = state.player(&player()).unwrap();
            assert_eq!(
                after.trade_goods,
                before.trade_goods + goods,
                "{card} goods"
            );
            assert_eq!(
                after.total_tokens(),
                before.total_tokens() + tokens,
                "{card} tokens"
            );
        }
    }

    #[test]
    fn a_derelict_vessel_draws_a_relic() {
        let mut state = game(&["a"]);
        state.relic_deck = vec![RelicId::new("some_relic")];
        state
            .exploration_decks
            .insert(FRONTIER.to_owned(), vec!["dw".to_owned()]);

        explore(
            &mut state,
            ContentStore::embedded(),
            &player(),
            FRONTIER,
            None,
        );

        assert_eq!(state.player(&player()).unwrap().relics.len(), 1);
        assert!(state.relic_deck.is_empty());
    }

    #[test]
    fn an_empty_relic_deck_is_not_a_failure() {
        // The card still resolved; there was simply nothing to take.
        let mut state = game(&["a"]);
        state.relic_deck.clear();
        state
            .exploration_decks
            .insert(FRONTIER.to_owned(), vec!["dw".to_owned()]);

        let outcome = explore(
            &mut state,
            ContentStore::embedded(),
            &player(),
            FRONTIER,
            None,
        );

        assert!(matches!(outcome, Some(Explored::Resolved { .. })));
        assert!(state.player(&player()).unwrap().relics.is_empty());
    }

    /// Mirage joins the system it was explored in, controlled and readied.
    #[test]
    fn mirage_is_placed_and_gained() {
        let (mut state, planet) = holder(0, 0);
        let system = ti4_content::galaxy::planet(ContentStore::embedded(), planet.as_str(), POK)
            .and_then(|record| record.system_id())
            .map(ti4_model::id::SystemId::new)
            .expect("the explored planet sits on a tile");
        let mirage = PlanetId::new("mirage");

        resolve_card(&mut state, &player(), Some(&planet), "mirage", &[]);

        assert_eq!(
            state.placed_planets.get(&mirage),
            Some(&system),
            "the token went into the explored system"
        );
        assert!(
            crate::planets::in_system(&state, ContentStore::embedded(), POK, &system)
                .contains(&mirage),
            "and the system now has it"
        );
        assert!(
            state
                .controlled_planets(&player())
                .into_iter()
                .any(|(_, held)| *held == mirage),
            "and the card was gained"
        );
        assert!(
            !state.exhausted_planets.contains(&mirage),
            "readied, as the card says"
        );
    }

    /// A gamma token makes its system adjacent to the other gamma systems.
    ///
    /// The reason this test exists: `wormhole_tokens` was written by three separate effects and
    /// read by nothing, so every gamma token this engine placed connected precisely nothing. The
    /// assertion is about *adjacency*, not about the map being written to, because the map being
    /// written to was already true and was not enough.
    #[test]
    fn a_gamma_token_actually_connects_its_system() {
        let hub = crate::fixtures::plain_hub();
        let (a, b) = (hub.outer[0].clone(), hub.outer[2].clone());
        let mut galaxy = hub.galaxy;
        assert!(
            !galaxy.are_adjacent(&a, &b),
            "two ring seats are not adjacent to begin with"
        );

        let mut state = crate::fixtures::game(&["a"]);
        state
            .wormhole_tokens
            .insert("GAMMA".to_owned(), ti4_model::id::SystemId::new(&a));
        state
            .wormhole_tokens
            .insert("GAMMA2".to_owned(), ti4_model::id::SystemId::new(&b));
        // Both entries are gamma tokens; the map is keyed by kind, so the second needs its own key.
        state.wormhole_tokens.remove("GAMMA2");
        state.ion_storm = Some((ti4_model::id::SystemId::new(&b), "GAMMA".to_owned()));

        crate::laws::apply_to_galaxy(&state, &mut galaxy);
        assert!(
            galaxy.are_adjacent(&a, &b),
            "a gamma token at each end links them"
        );
    }

    /// The ion storm flips when a move uses it, and not when a move ignores it.
    #[test]
    fn the_ion_storm_flips_only_for_moves_that_touch_it() {
        let mut state = crate::fixtures::game(&["a"]);
        let (here, elsewhere, far) = (
            ti4_model::id::SystemId::new("18"),
            ti4_model::id::SystemId::new("19"),
            ti4_model::id::SystemId::new("20"),
        );
        state.ion_storm = Some((here.clone(), "ALPHA".to_owned()));

        assert!(!flip_ion_storm(&mut state, &elsewhere, &far), "not touched");
        assert_eq!(
            state.ion_storm.as_ref().map(|(_, face)| face.as_str()),
            Some("ALPHA")
        );

        assert!(
            flip_ion_storm(&mut state, &elsewhere, &here),
            "arrived at it"
        );
        assert_eq!(
            state.ion_storm.as_ref().map(|(_, face)| face.as_str()),
            Some("BETA"),
            "and it shows the other side"
        );
        assert!(flip_ion_storm(&mut state, &here, &far), "left from it");
        assert_eq!(
            state.ion_storm.as_ref().map(|(_, face)| face.as_str()),
            Some("ALPHA"),
            "and back again"
        );
    }

    /// An Enigmatic Device explored goes to the play area and is offered as one component action,
    /// not one per technology; the technology is a follow-up decision.
    #[test]
    fn an_enigmatic_device_becomes_a_component_action() {
        let (mut state, planet) = holder(0, 0);
        resolve_card(&mut state, &player(), Some(&planet), "ed1", &[]);
        assert_eq!(
            state.player(&player()).unwrap().exploration_cards,
            vec!["ed1".to_owned()],
            "the card is faceup in the play area"
        );

        // 22.3: not offered while its six resources cannot be paid.
        assert!(
            available_actions(&state, ContentStore::embedded(), POK, &player()).is_empty(),
            "a seat that cannot pay is not offered it"
        );
        if let Some(seat) = state.player_mut(&player()) {
            seat.trade_goods = 6;
        }
        let offered = available_actions(&state, ContentStore::embedded(), POK, &player());
        assert_eq!(
            offered.len(),
            1,
            "one component action for the card, not one per unheld technology: {}",
            offered.len()
        );
        let chosen = &offered[0];
        assert_eq!(chosen.id, "play_area:ed1");
        let wanted = enigmatic_device_technologies(&state, ContentStore::embedded(), POK, &player())
            .into_iter()
            .next_back()
            .expect("a technology to research")
            .to_string();

        // The technology is asked once, by name, before anything is paid.
        let mut table = crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new([wanted.clone()])));
        let before = state.player(&player()).unwrap().technologies.len();
        // Measured as spending power, not as trade goods: the seat also controls a planet, so a
        // correct payment draws on both and a trade-good count alone would read as underpaying.
        let purse = crate::production::available(
            &state,
            ContentStore::embedded(),
            POK,
            &player(),
            crate::production::Spend::Resources,
        );
        assert!(perform_action(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            None,
            &player(),
            chosen,
        ));
        assert_eq!(
            state.player(&player()).unwrap().technologies.len(),
            before + 1,
            "a technology arrived"
        );
        assert!(
            state
                .player(&player())
                .unwrap()
                .technologies
                .contains(&TechnologyId::new(&wanted)),
            "and it is the exact technology the chosen option named"
        );
        assert_eq!(
            crate::production::available(
                &state,
                ContentStore::embedded(),
                POK,
                &player(),
                crate::production::Spend::Resources,
            ),
            purse - 6,
            "six resources were spent"
        );
        assert!(
            state
                .player(&player())
                .unwrap()
                .exploration_cards
                .is_empty(),
            "and the card is purged, so it cannot be used twice"
        );
    }

    /// Every exploration card in the corpus is resolved.
    ///
    /// This used to assert the opposite -- `!missing.is_empty()`, "most instants are still
    /// unresolved" -- which was true when written and expired the moment the last one landed. The
    /// second assertion is what stops the first from passing vacuously: `unimplemented` returning
    /// an empty list because it found no cards at all would be a broken query, not full coverage.
    #[test]
    fn every_exploration_card_is_resolved() {
        let missing = unimplemented(ContentStore::embedded(), POK);
        assert!(missing.is_empty(), "unresolved: {missing:?}");

        let instants = ContentStore::embedded()
            .records(ContentType::Explores)
            .iter()
            .filter(|record| record.in_sources(POK))
            .filter(|record| !matches!(record.text("resolution"), Some("Fragment" | "Attach")))
            .count();
        assert!(
            instants > 30,
            "the corpus really does have instants to cover ({instants})"
        );
    }

    #[test]
    fn every_registered_card_is_a_real_one() {
        for card in registered_cards() {
            assert!(
                ContentStore::embedded()
                    .get(ContentType::Explores, card)
                    .is_some(),
                "{card} is not an exploration card the corpus knows"
            );
        }
    }

    /// Explore one named card with a scripted answer, and give back the state it left.
    fn resolve_card(
        state: &mut GameState,
        player: &PlayerId,
        planet: Option<&PlanetId>,
        card: &str,
        answers: &[&str],
    ) -> Option<Explored> {
        let mut table = crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new(
            answers.iter().map(|answer| (*answer).to_owned()),
        )));
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut ctx = crate::choice::Resolving {
            content: ContentStore::embedded(),
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };
        state
            .exploration_decks
            .insert("CULTURAL".to_owned(), vec![card.to_owned()]);
        explore_with(state, &mut ctx, player, "CULTURAL", planet)
    }

    /// A player holding a planet, with a real faction and the seat's economy set.
    ///
    /// The faction matters: commodity value comes from it, and a seat with none can hold no
    /// commodities at all, which would make every gain in these tests a silent no-op.
    fn holder(goods: i32, commodities: i32) -> (GameState, PlanetId) {
        let mut state = game(&["a"]);
        let (system, planet) = crate::fixtures::a_placed_planet();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        let seat = state.player_mut(&player()).unwrap();
        seat.faction = ti4_model::id::FactionId::new("sol");
        seat.trade_goods = goods;
        seat.commodities = commodities;
        (state, planet)
    }

    #[test]
    fn abandoned_warehouses_converts_or_gains_but_not_both() {
        let (mut state, planet) = holder(0, 2);
        resolve_card(&mut state, &player(), Some(&planet), "aw1", &["convert"]);
        let seat = state.player(&player()).unwrap();
        assert_eq!(
            seat.trade_goods, 2,
            "two commodities became two trade goods"
        );
        assert_eq!(seat.commodities, 0);

        let (mut state, planet) = holder(0, 0);
        resolve_card(&mut state, &player(), Some(&planet), "aw1", &["gain"]);
        let seat = state.player(&player()).unwrap();
        assert_eq!(
            seat.commodities, 2,
            "gained as commodities, not trade goods"
        );
        assert_eq!(seat.trade_goods, 0);
    }

    #[test]
    fn a_commodity_reward_states_the_seats_commodities_for_display() {
        let (mut state, planet) = holder(3, 1);
        let limit = commodity_limit(&state, ContentStore::embedded(), &player());
        let (decider, seen) =
            crate::choice::Capturing::new(Box::new(crate::choice::Scripted::new([
                "gain".to_owned()
            ])));
        let mut table = crate::choice::Table::with_default(Box::new(decider));
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut ctx = crate::choice::Resolving {
            content: ContentStore::embedded(),
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };
        state
            .exploration_decks
            .insert("CULTURAL".to_owned(), vec!["ms1".to_owned()]);
        explore_with(&mut state, &mut ctx, &player(), "CULTURAL", Some(&planet));

        let seen = seen.borrow();
        let ask = seen
            .iter()
            .find(|choice| {
                choice.context.as_ref().map(|c| c.subtype.as_str())
                    == Some("merchant_station_choose_reward")
            })
            .expect("the Merchant Station question");
        assert_eq!(ask.details["commodities"]["have"], 1);
        assert_eq!(ask.details["commodities"]["max"], limit);
        assert_eq!(ask.details["commodities"]["trade_goods"], 3);
        assert_eq!(
            ask.ids(),
            vec!["replenish", "convert"],
            "the options are unchanged"
        );
    }

    #[test]
    fn commodities_never_pass_the_factions_value() {
        // 21.2. A card that says "gain 2" gains what the seat can hold, and a faction with a
        // value of 2 does not end up with 4.
        let (mut state, planet) = holder(0, 0);
        let limit = commodity_limit(&state, ContentStore::embedded(), &player());
        state.player_mut(&player()).unwrap().commodities = limit;

        resolve_card(&mut state, &player(), Some(&planet), "aw1", &["gain"]);

        assert_eq!(
            state.player(&player()).unwrap().commodities,
            limit,
            "already full, so nothing was gained"
        );
    }

    #[test]
    fn a_functioning_base_charges_for_the_card_it_draws() {
        let (mut state, planet) = holder(1, 0);
        state.action_card_deck = vec![ti4_model::id::ActionCardId::new("some_card")];

        resolve_card(&mut state, &player(), Some(&planet), "fb1", &["spend_tg"]);

        let seat = state.player(&player()).unwrap();
        assert_eq!(seat.trade_goods, 0, "the trade good was spent");
        assert_eq!(seat.action_cards.len(), 1, "and a card was drawn");
    }

    #[test]
    fn a_functioning_base_cannot_spend_what_it_does_not_have() {
        // The option is not offered, so a scripted answer naming it falls through to the
        // default rather than drawing a free card.
        let (mut state, planet) = holder(0, 0);
        state.action_card_deck = vec![ti4_model::id::ActionCardId::new("some_card")];

        resolve_card(&mut state, &player(), Some(&planet), "fb1", &[]);

        let seat = state.player(&player()).unwrap();
        assert_eq!(seat.trade_goods, 0);
        assert!(seat.action_cards.is_empty(), "nothing was drawn on credit");
        assert!(seat.commodities > 0, "the gain was taken instead");
    }

    #[test]
    fn a_core_mine_is_paid_for_with_a_mech_or_an_infantry() {
        // "If you have at least 1 mech on this planet, or if you remove 1 infantry from this
        // planet" — a player with neither gains nothing, which is the card working.
        let (mut state, planet) = holder(0, 0);
        resolve_card(
            &mut state,
            &player(),
            Some(&planet),
            "cm1",
            &["remove_infantry"],
        );
        assert_eq!(
            state.player(&player()).unwrap().trade_goods,
            0,
            "nothing on the planet, so nothing was mined"
        );

        let system = crate::fixtures::a_placed_planet().0;
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &player(), 1);
        resolve_card(&mut state, &player(), Some(&planet), "cm1", &[]);

        assert_eq!(state.player(&player()).unwrap().trade_goods, 1);
        assert!(
            state
                .system_state(&system)
                .planet_units
                .get(&planet)
                .is_none_or(Vec::is_empty),
            "the infantry paid for it"
        );
    }

    #[test]
    fn the_infantry_sacrifice_shows_its_reward_and_the_log_says_what_happened() {
        for (card, reward, result) in [
            (
                "cm1",
                "gain 1 trade good",
                "gained 1 trade good, removed 1 infantry from the planet",
            ),
            (
                "vfs1",
                "gain 1 command token",
                "gained 1 command token, removed 1 infantry from the planet",
            ),
            (
                "exp1",
                "ready this planet",
                "removed 1 infantry from the planet, readied the planet",
            ),
        ] {
            let (mut state, planet) = holder(0, 0);
            let system = crate::fixtures::a_placed_planet().0;
            crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &player(), 1);
            state.exhausted_planets.insert(planet.clone());
            let (decider, seen) =
                crate::choice::Capturing::new(Box::new(crate::choice::Scripted::new([
                    "remove_infantry".to_owned(),
                ])));
            let mut table = crate::choice::Table::with_default(Box::new(decider));
            let mut dice = crate::dice::Dice::new();
            let mut rng = crate::rng::GameRng::new(0);
            let mut ctx = crate::choice::Resolving {
                content: ContentStore::embedded(),
                sources: POK,
                dice: &mut dice,
                rng: &mut rng,
                table: &mut table,
                timing: None,
            };
            state
                .exploration_decks
                .insert("HAZARDOUS".to_owned(), vec![card.to_owned()]);
            explore_with(&mut state, &mut ctx, &player(), "HAZARDOUS", Some(&planet));

            let asked = seen.borrow();
            let details = &asked[0].details;
            assert_eq!(details["kind"], "offer", "{card}");
            let facts = details["facts"].to_string();
            assert!(facts.contains(reward), "{card}: {facts}");
            assert!(facts.contains("1 infantry"), "{card}: {facts}");
            assert!(
                details["card"]["text"]
                    .as_str()
                    .is_some_and(|t| t.contains("remove 1 infantry"))
            );
            assert_eq!(state.exploration_log[0].result, result, "{card}");
        }
    }

    #[test]
    fn a_declined_sacrifice_logs_no_result() {
        let (mut state, planet) = holder(0, 0);
        let system = crate::fixtures::a_placed_planet().0;
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &player(), 1);
        resolve_card(&mut state, &player(), Some(&planet), "cm1", &["decline"]);
        assert_eq!(state.exploration_log[0].result, "");
    }

    /// A game recorded with `play_area:<card>:<tech>` keeps those ids on replay, and the old id
    /// resolves in one step (no follow-up question), so recorded decisions line up one for one.
    #[test]
    fn legacy_per_technology_device_ids_still_replay() {
        let (mut state, planet) = holder(0, 0);
        resolve_card(&mut state, &player(), Some(&planet), "ed2", &[]);
        if let Some(seat) = state.player_mut(&player()) {
            seat.trade_goods = 6;
        }
        let record = |chosen: &str, offered: &[&str]| crate::choice::DecisionRecord {
            player: player(),
            prompt: "action phase".to_owned(),
            chosen: chosen.to_owned(),
            offered: offered.iter().map(|id| (*id).to_owned()).collect(),
            context: None,
        };
        mark_legacy_device_ids(&mut state, &[record("tactical", &["tactical", "play_area:ed2"])]);
        assert!(!state.legacy_device_ids, "new-style ids never mark a game");
        mark_legacy_device_ids(
            &mut state,
            &[record("tactical", &["tactical", "play_area:ed2:gd"])],
        );
        assert!(state.legacy_device_ids);

        let offered = available_actions(&state, ContentStore::embedded(), POK, &player());
        assert!(offered.len() > 1 && offered.iter().all(|o| o.id.starts_with("play_area:ed2:")));
        let chosen = offered.iter().find(|o| o.id == "play_area:ed2:gd").unwrap();
        let mut table = crate::choice::Table::with_default(Box::new(crate::choice::AlwaysDecline));
        assert!(perform_action(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            None,
            &player(),
            chosen,
        ));
        assert!(state.player(&player()).unwrap().technologies.contains(&TechnologyId::new("gd")));
        assert!(
            table.log.records.iter().all(|r| !r.prompt.contains("which technology")),
            "no follow-up technology question was asked"
        );
    }

    /// The record carries structured data a viewer can draw: signed holdings changes, and what was
    /// put on the map.
    #[test]
    fn an_exploration_record_publishes_gains_and_map_placements() {
        let (mut state, planet) = holder(0, 0);
        resolve_card(&mut state, &player(), Some(&planet), "aw1", &[]);
        assert_eq!(state.exploration_log[0].gains.get("commodities"), Some(&2));
        assert!(state.exploration_log[0].placed.is_empty());

        let system = crate::fixtures::a_placed_planet().0;
        state.active_system = Some(system.clone());
        resolve_card(&mut state, &player(), Some(&planet), "gw", &[]);
        assert_eq!(
            state.exploration_log[1].placed,
            vec![format!("wormhole:GAMMA:{system}")]
        );
    }

    #[test]
    fn an_explored_enigmatic_device_says_where_it_went() {
        // The log line the toast shows: "nothing happened" would hide that a card was kept.
        let (mut state, planet) = holder(0, 0);
        resolve_card(&mut state, &player(), Some(&planet), "ed2", &[]);
        let result = &state.exploration_log[0].result;
        assert!(result.contains("play area"), "{result}");
        assert_eq!(state.exploration_log[0].placed, vec!["play_area:ed2".to_owned()]);
    }

    #[test]
    fn an_expedition_readies_the_planet_it_was_found_on() {
        let (mut state, planet) = holder(0, 0);
        let system = crate::fixtures::a_placed_planet().0;
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &player(), 1);
        state.exhausted_planets.insert(planet.clone());

        resolve_card(
            &mut state,
            &player(),
            Some(&planet),
            "exp1",
            &["remove_infantry"],
        );

        assert!(!state.exhausted_planets.contains(&planet));
    }

    #[test]
    fn hazardous_exploration_never_removes_an_infantry_without_consent() {
        for card in ["cm1", "exp1", "vfs1"] {
            let (mut state, planet) = holder(0, 0);
            let system = crate::fixtures::a_placed_planet().0;
            crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &player(), 1);
            state.exhausted_planets.insert(planet.clone());
            let before_tokens = state.player(&player()).unwrap().strategic_tokens;

            resolve_card(&mut state, &player(), Some(&planet), card, &["decline"]);

            let units = state
                .system_state(&system)
                .planet_units
                .get(&planet)
                .map_or(0, Vec::len);
            assert_eq!(units, 1, "{card} preserved the declined infantry");
            assert_eq!(state.player(&player()).unwrap().trade_goods, 0);
            assert_eq!(
                state.player(&player()).unwrap().strategic_tokens,
                before_tokens
            );
            assert!(state.exhausted_planets.contains(&planet));
        }
    }

    #[test]
    fn a_lost_crew_draws_two_action_cards() {
        let (mut state, planet) = holder(0, 0);
        state.action_card_deck = (0..2)
            .map(|n| ti4_model::id::ActionCardId::new(format!("card{n}")))
            .collect();

        resolve_card(&mut state, &player(), Some(&planet), "lc1", &[]);

        assert_eq!(state.player(&player()).unwrap().action_cards.len(), 2);
    }

    #[test]
    fn a_derelict_vessel_draws_a_secret_objective() {
        let (mut state, planet) = holder(0, 0);
        state
            .player_mut(&player())
            .unwrap()
            .secret_objectives
            .clear();
        state.secret_deck = vec![ti4_model::id::SecretObjectiveId::new("some_secret")];

        resolve_card(&mut state, &player(), Some(&planet), "dv1", &[]);

        assert_eq!(state.player(&player()).unwrap().secret_objectives.len(), 1);
    }

    /// Explore Freelancers with a scripted answer, recording every question it asked.
    fn freelancers(answers: &[&str]) -> (GameState, Vec<crate::choice::Choice>) {
        let (mut state, planet) = holder(5, 0);
        let (capturing, seen) = crate::choice::Capturing::new(Box::new(
            crate::choice::Scripted::new(answers.iter().map(|answer| (*answer).to_owned())),
        ));
        let mut table = crate::choice::Table::with_default(Box::new(capturing));
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut ctx = crate::choice::Resolving {
            content: ContentStore::embedded(),
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };
        state
            .exploration_decks
            .insert("CULTURAL".to_owned(), vec!["frln1".to_owned()]);
        explore_with(&mut state, &mut ctx, &player(), "CULTURAL", Some(&planet));
        let seen = seen.borrow().clone();
        (state, seen)
    }

    fn units_of_player(state: &GameState) -> usize {
        let system = crate::fixtures::a_placed_planet().0;
        let board = state.system_state(&system);
        board.units_of(&player()).len()
            + board
                .planet_units
                .values()
                .map(|units| units.iter().filter(|unit| unit.owner == player()).count())
                .sum::<usize>()
    }

    /// Freelancers ("You may produce 1 unit in this system"): the unit question names the card,
    /// is typed as an optional single-unit production about the system, and ends with a decline.
    #[test]
    fn freelancers_asks_a_named_optional_unit_question() {
        let (_, seen) = freelancers(&["decline"]);
        let ask = seen
            .iter()
            .find(|choice| choice.options.iter().any(|o| o.kind == "produce"))
            .expect("Freelancers asked which unit to produce");
        assert!(
            ask.prompt.starts_with("Freelancers: produce one unit in "),
            "{}",
            ask.prompt
        );
        let context = ask.context.as_ref().expect("the question is typed");
        assert_eq!(context.subtype, crate::production::PRODUCE_ONE_SUBTYPE);
        assert!(context.optional);
        assert_eq!(
            context.source,
            crate::decision_context::DecisionSource::Content("frln1".to_owned())
        );
        let last = ask.options.last().expect("options");
        assert!(last.is_decline(), "the decline comes last: {:?}", ask.ids());
        assert!(
            ask.options[..ask.options.len() - 1]
                .iter()
                .all(|o| o.id.starts_with("build|") && o.id.ends_with("|1")),
            "the unit options keep their ids: {:?}",
            ask.ids()
        );
    }

    /// A unit the player cannot get here is named with its reason beside the question, never as an
    /// option: `details.not_offered` is display-only and leaves the option ids alone.
    #[test]
    fn freelancers_says_why_a_unit_is_not_offered() {
        let (_, seen) = freelancers(&["decline"]);
        let ask = seen
            .iter()
            .find(|choice| choice.options.iter().any(|o| o.kind == "produce"))
            .expect("Freelancers asked which unit to produce");
        let missing = ask
            .details
            .get("not_offered")
            .and_then(|value| value.as_array())
            .expect("some unit is out of reach, so the reasons are listed");
        for entry in missing {
            let unit = entry["unit"].as_str().expect("unit id");
            assert!(entry["reason"].as_str().is_some_and(|r| !r.is_empty()));
            assert!(
                !ask.options.iter().any(|o| o.id == format!("build|{unit}|1")),
                "{unit} is both offered and listed as not offered"
            );
        }
    }

    #[test]
    fn freelancers_may_be_declined() {
        let before = holder(5, 0).0;
        let (state, seen) = freelancers(&["decline"]);
        assert_eq!(
            units_of_player(&state),
            units_of_player(&before),
            "nothing built"
        );
        assert_eq!(
            state.player(&player()).unwrap().trade_goods,
            5,
            "nothing paid"
        );
        assert!(
            !seen
                .iter()
                .any(|c| c.options.iter().any(|o| o.kind == "pay")),
            "declining asks for no payment"
        );
        assert!(
            state.influence_pays_for_units.is_empty(),
            "the influence permission does not outlive the card"
        );
    }

    #[test]
    fn freelancers_still_builds_when_a_unit_is_chosen() {
        let before = holder(5, 0).0;
        // A ship: a fighter in a system without capacity would be removed again.
        let (state, seen) = freelancers(&["build|destroyer|1"]);
        assert_eq!(
            units_of_player(&state),
            units_of_player(&before) + 1,
            "one unit: {:?}",
            seen.iter().map(|c| c.prompt.clone()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_mercenary_outfit_may_be_declined() {
        let (mut state, planet) = holder(0, 0);
        let system = crate::fixtures::a_placed_planet().0;

        resolve_card(&mut state, &player(), Some(&planet), "mo1", &["decline"]);
        assert!(
            state
                .system_state(&system)
                .planet_units
                .get(&planet)
                .is_none_or(Vec::is_empty),
            "declining places nothing"
        );

        resolve_card(&mut state, &player(), Some(&planet), "mo1", &["place"]);
        assert_eq!(
            state
                .system_state(&system)
                .planet_units
                .get(&planet)
                .map_or(0, Vec::len),
            1,
            "one infantry, from the player's own faction"
        );
    }

    #[test]
    fn the_cards_left_unresolved_are_the_ones_that_need_more_engine() {
        // Nine remain, and each needs machinery this engine does not have: a held card with its
        // own ACTION, production with a payer, or a token the galaxy cannot carry.
        let missing = unimplemented(ContentStore::embedded(), POK);
        for card in registered_cards() {
            assert!(!missing.contains(&card.to_owned()), "{card} is registered");
        }
        assert!(
            missing.iter().all(|card| card.starts_with("ed")
                || card.starts_with("frln")
                || matches!(card.as_str(), "gamma" | "gw" | "ion" | "mirage")),
            "an unexpected card is unresolved: {missing:?}"
        );
    }

    #[test]
    fn three_matching_fragments_buy_a_relic() {
        // 35.9.
        let mut state = game(&["a"]);
        state.relic_deck = vec![RelicId::new("some_relic")];
        for _ in 0..3 {
            gain_fragment(&mut state, &player(), "CULTURAL");
        }

        assert_eq!(purgeable(&state, &player()), vec!["CULTURAL".to_owned()]);
        let relic = purge_for_relic(&mut state, &player(), "CULTURAL");

        assert_eq!(relic, Some(RelicId::new("some_relic")));
        let seat = state.player(&player()).unwrap();
        assert_eq!(seat.relic_fragments.get("CULTURAL"), Some(&0));
        assert_eq!(seat.relics.len(), 1);
        assert!(state.relic_deck.is_empty());
    }

    #[test]
    fn two_fragments_are_not_enough() {
        let mut state = game(&["a"]);
        state.relic_deck = vec![RelicId::new("some_relic")];
        for _ in 0..2 {
            gain_fragment(&mut state, &player(), "CULTURAL");
        }

        assert!(purgeable(&state, &player()).is_empty());
        assert_eq!(purge_for_relic(&mut state, &player(), "CULTURAL"), None);
        assert_eq!(state.relic_deck.len(), 1, "the deck was not touched");
    }

    #[test]
    fn a_frontier_fragment_substitutes_for_any_trait() {
        // 35.9. A wildcard that could not be cashed would be a pile that only grows.
        let mut state = game(&["a"]);
        state.relic_deck = vec![RelicId::new("some_relic")];
        gain_fragment(&mut state, &player(), "HAZARDOUS");
        gain_fragment(&mut state, &player(), "HAZARDOUS");
        gain_fragment(&mut state, &player(), FRONTIER);

        assert_eq!(purgeable(&state, &player()), vec!["HAZARDOUS".to_owned()]);
        assert!(purge_for_relic(&mut state, &player(), "HAZARDOUS").is_some());

        let seat = state.player(&player()).unwrap();
        assert_eq!(seat.relic_fragments.get("HAZARDOUS"), Some(&0));
        assert_eq!(
            seat.relic_fragments.get(FRONTIER),
            Some(&0),
            "the wildcard made up the shortfall"
        );
    }

    #[test]
    fn matching_fragments_are_spent_before_wildcards() {
        // A frontier fragment spent while a matching one was available is a wildcard wasted.
        let mut state = game(&["a"]);
        state.relic_deck = vec![RelicId::new("r1")];
        for _ in 0..3 {
            gain_fragment(&mut state, &player(), "INDUSTRIAL");
        }
        gain_fragment(&mut state, &player(), FRONTIER);

        purge_for_relic(&mut state, &player(), "INDUSTRIAL").unwrap();

        let seat = state.player(&player()).unwrap();
        assert_eq!(
            seat.relic_fragments.get(FRONTIER),
            Some(&1),
            "the wildcard was kept"
        );
    }

    #[test]
    fn no_relic_deck_means_no_relic() {
        let mut state = game(&["a"]);
        state.relic_deck.clear();
        for _ in 0..3 {
            gain_fragment(&mut state, &player(), "CULTURAL");
        }
        assert_eq!(purge_for_relic(&mut state, &player(), "CULTURAL"), None);
        assert_eq!(
            state
                .player(&player())
                .unwrap()
                .relic_fragments
                .get("CULTURAL"),
            Some(&3),
            "nothing was spent"
        );
    }
}

#[cfg(test)]
mod bf_f3_tests {
    use super::*;
    use crate::choice::{Resolving, Scripted, Table};
    use crate::factions::hooks_economy::{EconomyHooks, with_test_hooks};
    use crate::fixtures::game;
    use ti4_model::content_types::POK;

    fn explore_scripted(
        state: &mut GameState,
        planet: &PlanetId,
        answers: &[&str],
    ) -> Option<Explored> {
        let mut table = Table::with_default(Box::new(Scripted::new(answers.iter().copied())));
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(1);
        let mut ctx = Resolving {
            content: ContentStore::embedded(),
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };
        explore_with(
            state,
            &mut ctx,
            &PlayerId::new("a"),
            "CULTURAL",
            Some(planet),
        )
    }

    fn deck_game() -> (GameState, PlanetId) {
        let mut state = game(&["a"]);
        state.exploration_decks.insert(
            "CULTURAL".to_owned(),
            vec!["minent".to_owned(), "majent".to_owned(), "ent".to_owned()],
        );
        (state, crate::fixtures::a_placed_planet().1)
    }

    #[test]
    fn without_the_hook_one_card_is_drawn() {
        let (mut state, planet) = deck_game();
        let before = state.player(&PlayerId::new("a")).unwrap().trade_goods;
        explore_scripted(&mut state, &planet, &[]);
        assert_eq!(state.exploration_decks["CULTURAL"].len(), 2);
        assert_eq!(
            state.player(&PlayerId::new("a")).unwrap().trade_goods,
            before + 1,
            "minent gives 1 trade good"
        );
    }

    #[test]
    fn an_extra_draw_lets_the_player_pick_and_discards_the_other() {
        let hook = EconomyHooks {
            explore_extra_draw: Some(|_, _, _, _, _| true),
            ..EconomyHooks::NONE
        };
        let (mut state, planet) = deck_game();
        let before = state.player(&PlayerId::new("a")).unwrap().trade_goods;
        with_test_hooks(hook, || explore_scripted(&mut state, &planet, &["majent"]));
        assert_eq!(
            state.player(&PlayerId::new("a")).unwrap().trade_goods,
            before + 3,
            "the chosen second card (majent: 3 goods) resolved, the first did not"
        );
        assert_eq!(state.exploration_decks["CULTURAL"], ["ent"]);
        assert_eq!(state.exploration_log.len(), 1);
        assert_eq!(state.exploration_log[0].card, "majent");
    }

    #[test]
    fn an_extra_draw_with_a_lone_card_resolves_it() {
        let hook = EconomyHooks {
            explore_extra_draw: Some(|_, _, _, _, _| true),
            ..EconomyHooks::NONE
        };
        let (mut state, planet) = deck_game();
        state
            .exploration_decks
            .insert("CULTURAL".to_owned(), vec!["minent".to_owned()]);
        let outcome = with_test_hooks(hook, || explore_scripted(&mut state, &planet, &[]));
        assert!(matches!(outcome, Some(Explored::Resolved { .. })));
    }

    #[test]
    fn the_explored_callback_runs_once_after_a_planet_is_explored() {
        let hook = EconomyHooks {
            explored: Some(|state, _, _, _, planet| {
                state.exhausted_planets.remove(planet);
            }),
            ..EconomyHooks::NONE
        };
        let (mut state, planet) = deck_game();
        state.exhausted_planets.insert(planet.clone());
        explore_scripted(&mut state, &planet, &[]);
        assert!(
            state.exhausted_planets.contains(&planet),
            "no hook, no change"
        );
        with_test_hooks(hook, || explore_scripted(&mut state, &planet, &[]));
        assert!(!state.exhausted_planets.contains(&planet));
    }

    #[test]
    fn a_frontier_draw_calls_no_explored_callback() {
        let hook = EconomyHooks {
            explored: Some(|state, _, _, _, _| {
                state
                    .faction_marks
                    .insert("called".to_owned(), String::new());
            }),
            ..EconomyHooks::NONE
        };
        let (mut state, _) = deck_game();
        state
            .exploration_decks
            .insert("FRONTIER".to_owned(), vec!["minent".to_owned()]);
        let mut table = Table::new();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(1);
        let mut ctx = Resolving {
            content: ContentStore::embedded(),
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };
        with_test_hooks(hook, || {
            explore_with(&mut state, &mut ctx, &PlayerId::new("a"), "FRONTIER", None)
        });
        assert!(!state.faction_marks.contains_key("called"));
    }

    #[test]
    fn planet_explored_is_announced_with_a_timing_handle() {
        let (mut state, planet) = deck_game();
        let mut resolver = crate::timing::Resolver::new(
            vec![PlayerId::new("a")],
            Some(PlayerId::new("a")),
            Table::default(),
        );
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = seen.clone();
        resolver.register([crate::timing::Ability::new(
            "test:listener",
            PlayerId::new("a"),
            "PLANET_EXPLORED",
            crate::timing::Relation::After,
            std::sync::Arc::new(move |event, _| {
                sink.lock().unwrap().push(event.payload.clone());
                Ok(())
            }),
        )]);
        let mut sequence = crate::event::EventSequence::new();
        let mut table = Table::new();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(1);
        let mut ctx = Resolving {
            content: ContentStore::embedded(),
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: Some(crate::choice::TimingHandle {
                resolver: &mut resolver,
                sequence: &mut sequence,
                galaxy: None,
            }),
        };
        explore_with(
            &mut state,
            &mut ctx,
            &PlayerId::new("a"),
            "CULTURAL",
            Some(&planet),
        );
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0]["planet"], planet.to_string());
        assert_eq!(seen[0]["card"], "minent");
    }

    #[test]
    fn an_exploration_gain_is_announced_as_trade_goods_gained() {
        let (mut state, planet) = deck_game();
        let mut resolver = crate::timing::Resolver::new(
            vec![PlayerId::new("a")],
            Some(PlayerId::new("a")),
            Table::default(),
        );
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = seen.clone();
        resolver.register([crate::timing::Ability::new(
            "test:listener",
            PlayerId::new("a"),
            "TRADE_GOODS_GAINED",
            crate::timing::Relation::After,
            std::sync::Arc::new(move |event, _| {
                sink.lock().unwrap().push(event.payload.clone());
                Ok(())
            }),
        )]);
        let mut sequence = crate::event::EventSequence::new();
        let mut table = Table::new();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(1);
        let mut ctx = Resolving {
            content: ContentStore::embedded(),
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: Some(crate::choice::TimingHandle {
                resolver: &mut resolver,
                sequence: &mut sequence,
                galaxy: None,
            }),
        };
        explore_with(
            &mut state,
            &mut ctx,
            &PlayerId::new("a"),
            "CULTURAL",
            Some(&planet),
        );
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0]["amount"], 1);
        assert_eq!(seen[0]["source"], "exploration");
    }
    #[test]
    fn local_fabricators_with_a_maximum_offers_only_the_commodity_reward() {
        let content = ContentStore::embedded();
        let sources = ti4_model::content_types::DEFAULT;
        let mut state = crate::fixtures::seated_game(&[("a", "naaz"), ("b", "sol")], sources);
        let a = PlayerId::new("a");
        let (system, planet) = crate::fixtures::a_placed_planet();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "naaz_voltron", &a, 1);
        state.player_mut(&a).unwrap().trade_goods = 2;
        state.player_mut(&a).unwrap().commodities = 1;
        let board = state.board.clone();
        let (decider, seen) = crate::choice::Capturing::new(Box::new(crate::choice::FirstOption));
        let mut table = crate::choice::Table::with_default(Box::new(decider));
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut ctx = crate::choice::Resolving {
            content,
            sources,
            table: &mut table,
            dice: &mut dice,
            rng: &mut rng,
            timing: None,
        };
        assert!(resolve_instant(
            &mut state,
            &mut ctx,
            &a,
            Some(&planet),
            "lf1"
        ));
        assert_eq!(state.board, board);
        assert_eq!(state.player(&a).unwrap().trade_goods, 2);
        assert_eq!(state.player(&a).unwrap().commodities, 2);
        let seen = seen.borrow();
        assert_eq!(seen.len(), 1);
        assert_eq!(
            seen[0]
                .options
                .iter()
                .map(|o| o.id.as_str())
                .collect::<Vec<_>>(),
            vec!["gain"]
        );
    }
}
