//! Exploration and relic fragments (LRR 35, and 35.9 for fragments).
//!
//! Ported from the oracle's `engine/exploration.py`: `trait_of`, `draw`, `explore`, `_resolve`,
//! `_gain_fragment` and `_attach`, plus the fragment half of `engine/relics.py`.

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{PlanetId, PlayerId, RelicId, SystemId};
use ti4_model::state::GameState;

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

/// The deck a planet explores into, or `None` if it cannot be explored (35.2b).
#[must_use]
pub fn trait_of(content: &ContentStore, sources: SourceSet, planet: &PlanetId) -> Option<String> {
    let catalogue = ti4_content::galaxy::all_planets(content, sources);
    let record = catalogue.get(planet.as_str())?;
    // A planet may carry more than one trait (Thunder's Edge has six such); it explores into the
    // first of them that is an exploration deck.
    record.planet_types().into_iter().find_map(|kind| {
        let trait_name = kind.to_ascii_uppercase();
        EXPLORATION_TRAITS
            .iter()
            .find(|known| **known == trait_name && **known != FRONTIER)
            .map(|known| (*known).to_owned())
    })
}

/// Draw the top card of one exploration deck.
pub fn draw(state: &mut GameState, deck: &str) -> Option<String> {
    let cards = state.exploration_decks.get_mut(deck)?;
    if cards.is_empty() {
        return None;
    }
    Some(cards.remove(0))
}

/// How a card resolves, from the corpus.
#[must_use]
pub fn resolution(content: &ContentStore, card: &str) -> Option<String> {
    content
        .get(ContentType::Explores, card)
        .and_then(|record| record.text("resolution"))
        .map(ToOwned::to_owned)
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
    if let Some(seat) = state.player_mut(player) {
        seat.commodities = (seat.commodities + count).min(limit);
    }
}

/// Turn up to `most` commodities into trade goods, or all of them when `most` is `None`.
///
/// 21.5 only converts commodities when they *change hands*; these cards say so explicitly, which
/// is why this is written here rather than reached for anywhere a commodity is spent.
fn convert_commodities(state: &mut GameState, player: &PlayerId, most: Option<i32>) {
    if let Some(seat) = state.player_mut(player) {
        let moved = most.map_or(seat.commodities, |cap| seat.commodities.min(cap));
        seat.commodities -= moved;
        seat.trade_goods += moved;
    }
}

/// Ask this player one question with the given options.
fn ask(
    ctx: &mut crate::choice::Resolving<'_>,
    state: &GameState,
    player: &PlayerId,
    prompt: &str,
    options: &[(&str, &str)],
) -> Option<String> {
    let choice = crate::choice::Choice::new(
        player.clone(),
        prompt,
        options
            .iter()
            .map(|(id, label)| crate::choice::ChoiceOption::labelled(*id, "explore", *label))
            .collect(),
    );
    ctx.ask_seeing(state, &choice).ok().map(|answer| answer.id)
}

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
    let faction = state
        .player(player)
        .map(|seat| seat.faction.to_string())
        .unwrap_or_default();
    // A faction's own version first — a Sol infantry is not the generic one — then the plain
    // unit when the seat has no faction record. A base type with neither is not placed at all,
    // which is right for a mech: a factionless seat has no mech to place.
    let generic = ti4_content::units::catalogue(content, sources)
        .get(base_type)
        .map(|unit| unit.id().to_owned());
    let Some(id) = ti4_content::units::faction_unit(content, &faction, base_type, sources)
        .map(|unit| unit.id().to_owned())
        .or(generic)
    else {
        return false;
    };
    let type_id = ti4_model::id::UnitTypeId::new(id);
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
        .push(ti4_model::units::Unit::new(type_id, player.clone()));
    true
}

/// "If you have at least 1 mech on this planet, or if you remove 1 infantry from this planet."
///
/// A mech pays by being there; infantry pays by dying. A player with neither cannot resolve the
/// card at all, which is the card working rather than a gap.
fn pay_with_mech_or_infantry(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    planet: &PlanetId,
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
    if let Some(held) = state.system_mut(&system).planet_units.get_mut(planet) {
        held.remove(index);
    }
    true
}

/// Component actions from exploration cards held faceup in the play area.
///
/// Only the two Enigmatic Device cards print one. Offered on the same 22.3 terms as a relic action:
/// withheld when its six resources cannot be paid, because an action that cannot fully resolve is
/// never offered.
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
    seat.exploration_cards
        .iter()
        .filter(|card| matches!(card.as_str(), "ed1" | "ed2"))
        .filter(|_| {
            crate::production::available(
                state,
                content,
                sources,
                player,
                crate::production::Spend::Resources,
            ) >= ENIGMATIC_DEVICE_COST
        })
        .map(|card| {
            crate::choice::ChoiceOption::labelled(
                format!("{PLAY_AREA_PREFIX}{card}"),
                crate::relics::ACTION_KIND,
                "spend 6 resources to research a technology".to_owned(),
            )
        })
        .collect()
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
    let Some(card) = option.id.strip_prefix(PLAY_AREA_PREFIX) else {
        return false;
    };
    if !state
        .player(player)
        .is_some_and(|seat| seat.exploration_cards.iter().any(|held| held == card))
    {
        return false;
    }
    // Paid before the question, as the relic is: 22.3 does not offer an action that cannot fully
    // resolve, and asking which technology before knowing it can be paid for spends a decision on
    // nothing.
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
    crate::relics::grant_chosen_technology(state, content, sources, table, galaxy, player, None)
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
                state, ctx, player, &system,
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
                "Abandoned Warehouses",
                &[
                    ("gain", "gain 2 commodities"),
                    ("convert", "convert up to 2 commodities to trade goods"),
                ],
            );
            if chosen.as_deref() == Some("convert") {
                convert_commodities(state, player, Some(2));
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
                "Merchant Station",
                &[
                    ("replenish", "replenish commodities"),
                    ("convert", "convert commodities to trade goods"),
                ],
            );
            if chosen.as_deref() == Some("convert") {
                convert_commodities(state, player, None);
            } else {
                let limit = commodity_limit(state, content, player);
                gain_commodities(state, content, player, limit);
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
            let chosen = ask(ctx, state, player, "Functioning Base", &options);
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
            if goods_held >= 1 {
                options.push(("spend_tg", "spend 1 trade good to place a mech"));
            }
            if commodities_held >= 1 {
                options.push(("spend_com", "spend 1 commodity to place a mech"));
            }
            let chosen = ask(ctx, state, player, "Local Fabricators", &options);
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
            if pay_with_mech_or_infantry(state, content, sources, player, planet)
                && let Some(seat) = state.player_mut(player)
            {
                seat.trade_goods += 1;
            }
            return true;
        }
        "exp1" | "exp2" | "exp3" => {
            let Some(planet) = planet else {
                return true;
            };
            if pay_with_mech_or_infantry(state, content, sources, player, planet) {
                state.exhausted_planets.remove(planet);
            }
            return true;
        }
        "vfs1" | "vfs2" | "vfs3" => {
            let Some(planet) = planet else {
                return true;
            };
            if pay_with_mech_or_infantry(state, content, sources, player, planet) {
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
    if let Some(seat) = state.player_mut(player) {
        seat.trade_goods += goods;
    }
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
        .system_ids()
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
    explore_with(state, ctx, player, FRONTIER, None)
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
    let content = ctx.content;
    let card = draw(state, deck)?;
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
            state
                .planet_attachments
                .entry(planet.clone())
                .or_default()
                .push(card.clone());
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

    let relic = state.relic_deck.first().cloned()?;
    state.relic_deck.remove(0);

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

    /// An Enigmatic Device explored goes to the play area and is offered as an action there.
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
        assert_eq!(offered.len(), 1, "and a seat that can pay is");

        let mut table = crate::choice::Table::with_default(Box::new(crate::choice::FirstOption));
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
            &offered[0],
        ));
        assert_eq!(
            state.player(&player()).unwrap().technologies.len(),
            before + 1,
            "a technology arrived"
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
        resolve_card(&mut state, &player(), Some(&planet), "cm1", &[]);
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
    fn an_expedition_readies_the_planet_it_was_found_on() {
        let (mut state, planet) = holder(0, 0);
        let system = crate::fixtures::a_placed_planet().0;
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &player(), 1);
        state.exhausted_planets.insert(planet.clone());

        resolve_card(&mut state, &player(), Some(&planet), "exp1", &[]);

        assert!(!state.exhausted_planets.contains(&planet));
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
