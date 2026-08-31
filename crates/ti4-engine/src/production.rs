//! Production and payment (LRR 68, 75, 34, 47).
//!
//! Ported from the oracle's `engine/production.py`: `spendable_planets`, `available`, `pay`,
//! `producers`, `capacity`, `structure_allowed`, `placements`, `buildable_for` and `resolve`.
//!
//! Choices are asked inline through a [`Table`], matching `combat.rs` and `invasion.rs`.

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_content::units::{UnitType, catalogue};
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlanetId, PlayerId, SystemId, TechnologyId, UnitTypeId};
use ti4_model::state::GameState;
use ti4_model::units::Unit;

use crate::choice::{Choice, ChoiceOption, IllegalChoice, Observed, Resolving, Table, Window};

/// The two things a planet card can be exhausted for (LRR 75.2, 47).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spend {
    Resources,
    Influence,
}

const fn spend_name(kind: Spend) -> &'static str {
    match kind {
        Spend::Resources => "resources",
        Spend::Influence => "influence",
    }
}

/// The choice kind for exhausting a planet to pay.
pub const PAY_KIND: &str = "pay";
/// The choice kind for producing one unit.
pub const PRODUCE_KIND: &str = "produce";
/// The choice kind for placing a produced unit.
pub const PLACE_KIND: &str = "place";
/// The id standing for a system's space area.
pub const SPACE: &str = "space";

/// What may be produced at all. Structures arrive through Construction, not PRODUCTION.
pub const BUILDABLE: [&str; 9] = [
    "fighter",
    "infantry",
    "carrier",
    "cruiser",
    "destroyer",
    "dreadnought",
    "mech",
    "flagship",
    "warsun",
];

/// A war sun cannot be produced without the technology that unlocks it (67.x).
pub const UNLOCKED_BY: [(&str, &str); 1] = [("warsun", "ws")];

/// How many of a structure one planet may hold.
///
/// A planet takes one space dock and two PDS. (The second PDS needs Space Dock II in the base
/// game, which is not modelled — the cap is what matters, and it is a cap either way.)
#[must_use]
pub fn structure_limit(base_type: &str) -> Option<usize> {
    match base_type {
        "spacedock" => Some(1),
        "pds" => Some(2),
        _ => None,
    }
}

/// A planet's printed resources or influence.
#[must_use]
pub fn planet_value(
    content: &ContentStore,
    sources: SourceSet,
    planet: &PlanetId,
    kind: Spend,
) -> i64 {
    // A point lookup, not the whole catalogue: this is called once per payment face, inside
    // `payment_options`' per-planet affordability guard, which is itself O(planets squared).
    ti4_content::galaxy::planet(content, planet.as_str(), sources).map_or(0, |record| match kind {
        Spend::Resources => record.resources(),
        Spend::Influence => record.influence(),
    })
}

/// A planet's resources or influence **as they now stand**, printed value plus attachments.
///
/// [`planet_value`] stays the printed number, because most callers want the card as dealt. Three
/// laws attach to a planet and change what it is worth in play — Core Mining, Senate Sanctuary and
/// Terraforming Initiative — so anything asking "what can this planet pay" must ask here instead.
#[must_use]
pub fn planet_value_now(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    planet: &PlanetId,
    kind: Spend,
) -> i64 {
    planet_value(content, sources, planet, kind)
        + crate::laws::planet_value_bonus(state, planet, kind)
        // Nano-Forge attaches to a planet and adds two of each, the same shape as the three
        // attachment laws, so it belongs on the same path rather than a second one.
        + crate::relics::nanoforge_bonus(state, planet)
}

/// Controlled planets whose cards are still readied (LRR 34, 75.2).
#[must_use]
pub fn spendable_planets(state: &GameState, player: &PlayerId) -> Vec<PlanetId> {
    state
        .controlled_planets(player)
        .into_iter()
        .map(|(_, planet)| planet.clone())
        .filter(|planet| !state.exhausted_planets.contains(planet))
        .collect()
}

/// What one trade good is worth as payment: two with the `mc` technology, else one.
///
/// Oracle parity (`engine/production.py`): the multiplier applies in `available()` and in
/// every step of the payment loop.
#[must_use]
pub fn trade_good_worth(state: &GameState, player: &PlayerId) -> i64 {
    if state
        .player(player)
        .is_some_and(|seat| seat.technologies.contains(&TechnologyId::new("mc")))
    {
        2
    } else {
        1
    }
}

/// War Machine: the faces of budget this step gains, per copy played in this activation.
///
/// Each card says "apply +4 to the total PRODUCTION value of your units and reduce the
/// combined cost of the produced units by 1". The engine keeps one budget for a production
/// step — produced units are paid out of the same faces the units' PRODUCTION value provides
/// — so the +4 and the -1 land as five faces on it. The marker is keyed to the activation
/// that played the card, and production happens once per tactical action, which is what
/// keeps a later step from spending a bonus it never earned.
#[must_use]
pub fn war_machine_bonus(state: &GameState, player: &PlayerId) -> i64 {
    state.player(player).map_or(0, |seat| {
        5
            * i64::try_from(
                seat.war_machine_use
                    .iter()
                    .filter(|seq| **seq == state.activation_seq)
                    .count(),
            )
            .unwrap_or(i64::MAX)
    })
}

/// Spendable resources or influence, counting trade goods (LRR 75.3, 47.3).
#[must_use]
pub fn available(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    kind: Spend,
) -> i64 {
    // Oracle parity (engine/production.py `available`): a planet counts for its *largest*
    // face — Xxcha's Archon's Gift alternate included — never the sum of both.
    let from_planets: i64 = spendable_planets(state, player)
        .iter()
        .map(|planet| max_face_value(state, content, sources, player, planet, kind))
        .sum();
    // The Triad is "readied and spent as if it were a planet card", so it adds a face here rather
    // than needing a payment path of its own. It is not in `spendable_planets` because it is not a
    // planet and must not appear anywhere planets are counted.
    let from_triad = crate::relics::triad_value(state, player).unwrap_or(0);
    let goods = state.player(player).map_or(0, |seat| {
        i64::from(seat.trade_goods) * trade_good_worth(state, player)
    });
    // War Machine's "reduce the combined cost of the produced units by 1" is spent from the
    // same budget as its "+4 to the total PRODUCTION value", so it joins the faces here as
    // well. Only resources: the card touches production, not influence bills such as the
    // Custodians' removal fee.
    let war_machine = if kind == Spend::Resources {
        war_machine_bonus(state, player)
    } else {
        0
    };
    from_planets + from_triad + goods + war_machine
}

/// The faces by which this planet can pay one bill.
///
/// Oracle parity (`engine/production.py` `_planet_payment_values`): the printed value is kept
/// only when it has positive worth, and Xxcha's *Archon's Gift* adds the other kind's printed
/// value as an alternate face — never both at once. The planet exhausts exactly either way.
#[must_use]
fn payment_faces(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    planet: &PlanetId,
    kind: Spend,
) -> Vec<(Spend, i64)> {
    // `planet_value_now`, not the printed value: Core Mining, Senate Sanctuary and Terraforming
    // Initiative attach to a planet card and change what it can pay. Both faces are computed here,
    // so this one substitution reaches every spending path.
    // Xxekir Grom pays a planet's resources and influence together, as both. Folded into the
    // ordinary face rather than added as an alternate: the combined value *is* what the planet is
    // worth to this player, for either kind of bill.
    let ordinary = if crate::leaders::combines_planet_values(state, player) {
        planet_value_now(state, content, sources, planet, Spend::Resources)
            + planet_value_now(state, content, sources, planet, Spend::Influence)
    } else {
        planet_value_now(state, content, sources, planet, kind)
    };
    let mut faces = if ordinary > 0 {
        vec![(kind, ordinary)]
    } else {
        Vec::new()
    };
    let Some(seat) = state.player(player) else {
        return faces;
    };
    // Freelancers: "You may spend influence as if it were resources to produce this unit." The
    // same shape as Archon's Gift below -- a second face on the planet card -- so it is the same
    // code, and a caller cannot honour one and forget the other.
    let freelancers =
        kind == Spend::Resources && state.influence_pays_for_units.contains(player);
    let archons_gift = seat
        .breakthrough
        .as_ref()
        .is_some_and(|held| held.as_str() == "xxchabt");
    if !freelancers && !archons_gift {
        return faces;
    }
    let alternate_kind = match kind {
        Spend::Resources => Spend::Influence,
        Spend::Influence => Spend::Resources,
    };
    let alternate = planet_value_now(state, content, sources, planet, alternate_kind);
    if alternate > 0 && alternate != ordinary {
        faces.push((alternate_kind, alternate));
    }
    faces
}

/// The largest face value this planet can pay a bill of `kind` with — what the oracle's
/// affordability guard credits to it when deciding whether some other face would strand the rest.
#[must_use]
fn max_face_value(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    planet: &PlanetId,
    kind: Spend,
) -> i64 {
    payment_faces(state, content, sources, player, planet, kind)
        .into_iter()
        .map(|(_, worth)| worth)
        .max()
        .unwrap_or(0)
}

/// The payment options for one step of a bill.
///
/// Oracle parity (`engine/production.py` `pay()`): spendable planets first — every face that,
/// taken now, would not strand the rest of the bill (the guard covers *all* faces, including
/// *Archon's Gift* alternates) — then trade goods, which are never guarded. `paid` and `cost`
/// describe the whole bill; a window paying only its remaining amount passes `(0, owed)`, which is
/// arithmetically identical.
#[must_use]
fn payment_options(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    kind: Spend,
    paid: i64,
    cost: i64,
) -> Vec<ChoiceOption> {
    let spendable = spendable_planets(state, player);
    // Goods capacity under the guard uses the `mc`-multiplied worth (engine/production.py pay()).
    let goods_capacity = state.player(player).map_or(0, |seat| {
        i64::from(seat.trade_goods) * trade_good_worth(state, player)
    });
    let mut options: Vec<ChoiceOption> = Vec::new();
    for planet in &spendable {
        // What would remain after this face pays: the goods and every other planet's best face.
        let remaining_after = spendable
            .iter()
            .filter(|other| *other != planet)
            .map(|other| max_face_value(state, content, sources, player, other, kind))
            .sum::<i64>()
            + goods_capacity;
        for (source, worth) in payment_faces(state, content, sources, player, planet, kind) {
            // Do not offer a face that would make the rest of this mandatory bill unpayable.
            if paid + worth + remaining_after < cost {
                continue;
            }
            let id = if source == kind {
                format!("exhaust|{planet}")
            } else {
                // Cross-source *Archon's Gift* face: the id carries its source kind.
                format!("exhaust|{planet}|{}", spend_name(source))
            };
            let mut label = format!("exhaust {planet} for {worth} {}", spend_name(kind));
            if source != kind {
                label.push_str(" using its ");
                label.push_str(spend_name(source));
            }
            options.push(
                ChoiceOption::labelled(id, PAY_KIND, label)
                    .with("worth", worth)
                    .with("owed", cost - paid)
                    .with("kind", spend_name(kind))
                    .with("source", spend_name(source)),
            );
        }
    }
    let goods_held = state
        .player(player)
        .map_or(0, |seat| i64::from(seat.trade_goods));
    if goods_held > 0 {
        options.push(
            ChoiceOption::labelled("trade_good", PAY_KIND, "spend a trade good")
                .with("worth", trade_good_worth(state, player))
                .with("owed", cost - paid)
                .with("kind", spend_name(kind)),
        );
    }
    options
}

/// Apply the chosen payment option; returns its value against the bill.
///
/// `None` means the id was never offered (defensive — validated tables cannot produce it). As in
/// the oracle, the face's recorded worth is what applies and a recompute is only the fallback;
/// trade goods count double with the `mc` technology. Overpayment is lost by the caller.
fn apply_payment_option(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    kind: Spend,
    answer: &ChoiceOption,
) -> Option<i64> {
    if answer.id == "trade_good" {
        let goods = state
            .player(player)
            .map_or(0, |seat| i64::from(seat.trade_goods));
        if goods <= 0 {
            return None;
        }
        let worth = trade_good_worth(state, player);
        let seat = state.player_mut(player)?;
        seat.trade_goods -= 1;
        return Some(worth);
    }
    let rest = answer.id.strip_prefix("exhaust|")?;
    // The face's source kind rides on the payload (oracle parity key). A cross-source *Archon's
    // Gift* face appends it to the id as well — and only then. Planet ids may themselves contain
    // a '|' (`{system}|{name}`) or be single-segment, so never reparse them blindly.
    let (planet_str, source) = match (
        answer
            .payload
            .get("source")
            .and_then(serde_json::Value::as_str),
        rest.rfind('|'),
    ) {
        // Cross-source face: the id is `{planet}|{source kind}` and only then may the last
        // segment be stripped — planet ids themselves may contain '|' or be single-segment.
        (Some(name), Some(pos)) if name != spend_name(kind) => {
            debug_assert_eq!(&rest[pos + 1..], name);
            (
                &rest[..pos],
                if name == "influence" {
                    Spend::Influence
                } else {
                    Spend::Resources
                },
            )
        }
        _ => (rest, kind), // ordinary face: the id is exactly the planet id
    };
    let planet = PlanetId::new(planet_str.to_owned());
    let worth = answer
        .payload
        .get("worth")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or_else(|| planet_value(content, sources, &planet, source));
    state.exhaust_planet(planet);
    Some(worth)
}

/// Spend resources or influence, the player choosing what to exhaust.
///
/// A planet card is exhausted for one or the other, **never both** (34.3, 75.2), and a trade
/// good stands in for either (75.3, 47.3). Returns `false` without spending anything if the
/// cost cannot be met.
///
/// # Errors
/// [`IllegalChoice`] when a decider answers with something not offered.
pub fn pay(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut Table,
    player: &PlayerId,
    cost: i64,
    kind: Spend,
) -> Result<bool, IllegalChoice> {
    pay_with_observation(state, content, sources, table, player, cost, kind, None)
}

/// Spend with the public board observation available to a learned decider.
///
/// # Errors
/// Returns [`IllegalChoice`] if the decider selects an option that was not offered.
pub fn pay_seeing(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    cost: i64,
    kind: Spend,
) -> Result<bool, IllegalChoice> {
    pay_with_observation(state, content, sources, table, player, cost, kind, galaxy)
}

#[allow(
    clippy::too_many_arguments,
    reason = "payment needs the rules position plus an optional learned-policy observation"
)]
fn pay_with_observation(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut Table,
    player: &PlayerId,
    cost: i64,
    kind: Spend,
    galaxy: Option<&Galaxy>,
) -> Result<bool, IllegalChoice> {
    if cost <= 0 {
        return Ok(true);
    }
    if available(state, content, sources, player, kind) < cost {
        return Ok(false);
    }

    let mut paid = 0;
    while paid < cost {
        // Oracle parity (engine/production.py pay()): spendable planets first — every face that,
        // taken now, would not strand the rest of the bill — then trade goods, never guarded.
        let options = payment_options(state, content, sources, player, kind, paid, cost);
        if options.is_empty() {
            return Ok(false);
        }

        // The oracle takes a lone option without asking; only real choices reach a decider.
        let answer = if options.len() == 1 {
            options[0].clone()
        } else {
            // Oracle wording: each iteration names the remaining debt and its kind
            // (`pay {cost - paid} more {kind}` in engine/production.py).
            let choice = Choice::new(
                player.clone(),
                format!("pay {} more {}", cost - paid, spend_name(kind)),
                options,
            );
            table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?
        };

        match apply_payment_option(state, content, sources, player, kind, &answer) {
            Some(worth) => paid += worth,
            None => return Ok(false), // an id no offered option carries (unreachable after validate)
        }
    }
    Ok(true)
}

fn sling_relay_candidates(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> BTreeMap<SystemId, Vec<(String, i64)>> {
    let types = catalogue(content, sources);
    let affordable = available(state, content, sources, player, Spend::Resources);
    let mut candidates = BTreeMap::new();
    for (system, board) in &state.board {
        let dock_planets: Vec<&ti4_model::id::PlanetId> = board
            .planet_units
            .iter()
            .filter(|(_, units)| {
                units.iter().any(|unit| {
                    unit.owner == *player
                        && types
                            .get(unit.type_id.as_str())
                            .is_some_and(|kind| kind.base_type() == "spacedock")
                })
            })
            .map(|(planet, _)| planet)
            .collect();
        let has_dock = !dock_planets.is_empty();
        // Coexistence rule 4: "A coexisting structure is always blockaded, regardless of what
        // ships, if any, are in the system." A dock the player built while coexisting produces
        // nothing even in a system they otherwise hold uncontested.
        let coexisting_dock = dock_planets.iter().any(|planet| {
            board
                .coexisting
                .get(*planet)
                .is_some_and(|others| others.contains(player))
        });
        let blockaded = coexisting_dock
            || board.units.iter().any(|unit| {
                unit.owner != *player
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(ti4_content::units::UnitType::is_ship)
            });
        if !has_dock || blockaded {
            continue;
        }
        let ships: Vec<(String, i64)> = buildable_for(state, content, sources, player)
            .into_iter()
            .filter_map(|id| {
                let kind = types.get(id.as_str())?;
                let cost = price_of(kind).0;
                (kind.is_ship()
                    && cost <= affordable
                    && crate::supply::allowed(
                        state,
                        content,
                        sources,
                        player,
                        &UnitTypeId::new(id.clone()),
                        1,
                    ) > 0)
                    .then_some((id, cost))
            })
            .collect();
        if !ships.is_empty() {
            candidates.insert(system.clone(), ships);
        }
    }
    candidates
}

/// Whether Sling Relay can currently produce an affordable ship at an unblocked dock.
#[must_use]
pub fn can_sling_relay(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> bool {
    !sling_relay_candidates(state, content, sources, player).is_empty()
}

/// Produce Sling Relay's one ship without consuming a dock's PRODUCTION value.
///
/// # Errors
/// Returns [`IllegalChoice`] if the decider selects an unoffered system, ship, or payment.
pub fn sling_relay(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
) -> Result<bool, IllegalChoice> {
    let candidates = sling_relay_candidates(state, content, sources, player);
    let Some(mut system) = candidates.keys().next().cloned() else {
        return Ok(false);
    };
    if candidates.len() > 1 {
        let choice = Choice::new(
            player.clone(),
            "Sling Relay: produce in which dock system",
            candidates
                .keys()
                .map(|candidate| {
                    ChoiceOption::labelled(
                        candidate.to_string(),
                        PLACE_KIND,
                        format!("produce in {candidate}"),
                    )
                    .with("sling_relay", true)
                    .with("system", candidate.to_string())
                })
                .collect(),
        );
        system = SystemId::new(
            table
                .ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?
                .id,
        );
    }
    let ships = &candidates[&system];
    let chosen = if ships.len() == 1 {
        ships[0].clone()
    } else {
        let choice = Choice::new(
            player.clone(),
            "Sling Relay: produce one ship",
            ships
                .iter()
                .map(|(id, cost)| {
                    ChoiceOption::labelled(
                        format!("build|{id}|1"),
                        PRODUCE_KIND,
                        format!("produce 1x {id} for {cost}"),
                    )
                    .with("unit", id.clone())
                    .with("count", 1)
                    .with("cost", *cost)
                    .with("sling_relay", true)
                    .with("system", system.to_string())
                })
                .collect(),
        );
        let answer = table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
        let id = answer
            .id
            .strip_prefix("build|")
            .and_then(|rest| rest.strip_suffix("|1"))
            .unwrap_or_default();
        ships
            .iter()
            .find(|(candidate, _)| candidate == id)
            .cloned()
            .unwrap_or_else(|| ships[0].clone())
    };
    if !pay_seeing(
        state,
        content,
        sources,
        galaxy,
        table,
        player,
        chosen.1,
        Spend::Resources,
    )? {
        return Ok(false);
    }
    state
        .system_mut(&system)
        .units
        .push(Unit::new(UnitTypeId::new(chosen.0), player.clone()));
    Ok(true)
}

/// Integrated Economy: after gaining a planet, produce units there up to its resource value.
///
/// This is not a use of a unit's `PRODUCTION` ability.  It therefore has no production-capacity
/// limit or production-only discount; the conquered planet's resource value is the allowance and
/// ordinary printed unit prices spend it down.  The player still pays those prices normally.
///
/// # Errors
/// Returns [`IllegalChoice`] if a build, payment, or mandatory fleet-limit choice is invalid.
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "the triggered producer needs the full observed rules position"
)]
pub fn integrated_economy(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) -> Result<bool, IllegalChoice> {
    let mut budget = planet_value(content, sources, planet, Spend::Resources);
    if budget <= 0 {
        return Ok(false);
    }
    let types = catalogue(content, sources);
    let mut built = false;

    while budget > 0 {
        let affordable = available(state, content, sources, player, Spend::Resources);
        let mut options = Vec::new();
        for id in buildable_for(state, content, sources, player) {
            let Some(kind) = types.get(id.as_str()) else {
                continue;
            };
            let (cost, pair) = price_of_under(Some(state), kind);
            if cost <= 0 || cost > budget || cost > affordable {
                continue;
            }
            let made = crate::supply::allowed(
                state,
                content,
                sources,
                player,
                &UnitTypeId::new(&id),
                pair,
            );
            if made == 0 {
                continue;
            }
            if crate::fleet::counts_against_supply(kind) {
                let mut projected = state.clone();
                projected
                    .system_mut(system)
                    .units
                    .push(Unit::new(UnitTypeId::new(&id), player.clone()));
                if crate::fleet::over_supply(&projected, content, sources, player, system) > 0 {
                    continue;
                }
            }
            options.push(
                ChoiceOption::labelled(
                    format!("build|{id}|{made}"),
                    PRODUCE_KIND,
                    format!("Integrated Economy: produce {made}x {id} for {cost}"),
                )
                .with("unit", id.clone())
                .with("count", i64::try_from(made).unwrap_or(1))
                .with("cost", cost)
                .with("system", system.to_string())
                .with("planet", planet.to_string())
                .with("integrated_economy", true),
            );
        }
        if options.is_empty() {
            break;
        }
        options.push(ChoiceOption::labelled(
            "done_producing",
            crate::choice::DECLINE_KIND,
            "finish production",
        ));
        let choice = Choice::new(
            player.clone(),
            format!("Integrated Economy on {planet} ({budget} cost left)"),
            options,
        );
        let answer = table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
        if answer.is_decline() {
            break;
        }
        let mut parts = answer.id.split('|');
        let (Some("build"), Some(id), Some(made)) = (
            parts.next(),
            parts.next(),
            parts.next().and_then(|value| value.parse::<usize>().ok()),
        ) else {
            break;
        };
        let Some(kind) = types.get(id) else {
            break;
        };
        let cost = price_of(kind).0;
        if cost <= 0
            || cost > budget
            || !pay_seeing(
                state,
                content,
                sources,
                galaxy,
                table,
                player,
                cost,
                Spend::Resources,
            )?
        {
            break;
        }
        let where_to = if kind.is_ship() || kind.is_fighter() {
            SPACE
        } else {
            planet.as_str()
        };
        let made =
            crate::supply::allowed(state, content, sources, player, &UnitTypeId::new(id), made);
        for _ in 0..made {
            let unit = Unit::new(UnitTypeId::new(id), player.clone());
            if where_to == SPACE {
                state.system_mut(system).units.push(unit);
            } else {
                state
                    .system_mut(system)
                    .planet_units
                    .entry(planet.clone())
                    .or_default()
                    .push(unit);
            }
        }
        built |= made > 0;
        budget -= cost;
    }

    crate::fleet::enforce_seeing(state, content, sources, galaxy, table, player, system)?;
    Ok(built)
}

/// Produce exactly one unit in a chosen system, outside a normal use of `PRODUCTION`.
///
/// Used by free technology windows such as Chaos Mapping.  The unit is paid for normally, but
/// neither a production allowance nor the two-for-one fighter/infantry count applies.
///
/// # Errors
/// Returns [`IllegalChoice`] for an invalid unit, placement, or payment answer.
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "single-unit production needs the complete observed rules position"
)]
pub fn produce_one(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    system: &SystemId,
) -> Result<bool, IllegalChoice> {
    let types = catalogue(content, sources);
    let affordable = available(state, content, sources, player, Spend::Resources);
    let candidates: Vec<(String, i64)> = buildable_for(state, content, sources, player)
        .into_iter()
        .filter_map(|id| {
            let kind = types.get(id.as_str())?;
            let cost = price_of(kind).0;
            let within_fleet_supply = if crate::fleet::counts_against_supply(kind) {
                let mut projected = state.clone();
                projected
                    .system_mut(system)
                    .units
                    .push(Unit::new(UnitTypeId::new(&id), player.clone()));
                crate::fleet::over_supply(&projected, content, sources, player, system) == 0
            } else {
                true
            };
            (cost <= affordable
                && !placements(state, content, sources, player, system, kind).is_empty()
                && within_fleet_supply
                && crate::supply::allowed(
                    state,
                    content,
                    sources,
                    player,
                    &UnitTypeId::new(&id),
                    1,
                ) > 0)
                .then_some((id, cost))
        })
        .collect();
    if candidates.is_empty() {
        return Ok(false);
    }
    let choice = Choice::new(
        player.clone(),
        format!("produce one unit in {system}"),
        candidates
            .iter()
            .map(|(id, cost)| {
                ChoiceOption::labelled(
                    format!("build|{id}|1"),
                    PRODUCE_KIND,
                    format!("produce 1x {id} for {cost}"),
                )
                .with("unit", id.clone())
                .with("count", 1)
                .with("cost", *cost)
                .with("system", system.to_string())
            })
            .collect(),
    );
    let answer = table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
    let Some((id, cost)) = candidates
        .iter()
        .find(|(id, _)| answer.id == format!("build|{id}|1"))
        .cloned()
    else {
        return Ok(false);
    };
    if !pay_seeing(
        state,
        content,
        sources,
        galaxy,
        table,
        player,
        cost,
        Spend::Resources,
    )? {
        return Ok(false);
    }
    let Some(kind) = types.get(id.as_str()).copied() else {
        return Ok(false);
    };
    let spots = placements(state, content, sources, player, system, &kind);
    let Some(mut where_to) = spots.first().cloned() else {
        return Ok(false);
    };
    if spots.len() > 1 {
        let choice = Choice::new(
            player.clone(),
            format!("place the {id}"),
            spots
                .iter()
                .map(|spot| {
                    ChoiceOption::labelled(
                        format!("place|{spot}"),
                        PLACE_KIND,
                        format!("place on {spot}"),
                    )
                    .with("unit", id.clone())
                    .with("system", system.to_string())
                })
                .collect(),
        );
        table
            .ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?
            .id
            .strip_prefix("place|")
            .unwrap_or(SPACE)
            .clone_into(&mut where_to);
    }
    let unit = Unit::new(UnitTypeId::new(id), player.clone());
    if where_to == SPACE {
        state.system_mut(system).units.push(unit);
    } else {
        state
            .system_mut(system)
            .planet_units
            .entry(PlanetId::new(where_to))
            .or_default()
            .push(unit);
    }
    crate::fleet::enforce_seeing(state, content, sources, galaxy, table, player, system)?;
    Ok(true)
}

/// What one production step costs, and how many units it yields.
///
/// 68.2: a unit whose printed cost is below one — a fighter or an infantry — is produced
/// **two at a time** for that one resource. Charging `ceil` and yielding one would make the
/// two commonest units in the game cost double what the rules ask, which is not a rounding
/// detail: it is most of an early fleet.
#[must_use]
/// Freelancers: produce one unit here, with influence spending as if it were resources.
///
/// Wraps [`produce_one`] rather than duplicating it -- the card changes only what may pay, and the
/// substitution is a face on the planet card, which `payment_faces` already knows how to add. The
/// permission is cleared on every path out, including the failing ones: it is scoped to this one
/// production, and a permission left behind would quietly apply to the next.
pub fn produce_one_paying_with_influence(
    state: &mut GameState,
    ctx: &mut crate::choice::Resolving<'_>,
    player: &PlayerId,
    system: &SystemId,
) -> bool {
    state.influence_pays_for_units.insert(player.clone());
    let made = produce_one(
        state,
        ctx.content,
        ctx.sources,
        None,
        ctx.table,
        player,
        system,
    );
    state.influence_pays_for_units.remove(player);
    made.unwrap_or(false)
}

pub fn price_of(kind: &UnitType<'_>) -> (i64, usize) {
    price_of_under(None, kind)
}

/// The same, under whatever laws are in play.
///
/// Regulated Conscription: "When a player produces units, they produce only 1 fighter and infantry
/// for its cost instead of 2." It halves the yield rather than doubling the price, which is a
/// different card: the cost stays one resource.
///
/// `price_of` remains the printed rule for callers that mean the printed rule. Passing the state is
/// how a caller says it means *now*.
#[must_use]
pub fn price_of_under(state: Option<&GameState>, kind: &UnitType<'_>) -> (i64, usize) {
    let printed = kind.cost();
    if printed > 0.0 && printed < 1.0 {
        let yielded = if state.is_some_and(crate::laws::single_unit_production) {
            1
        } else {
            2
        };
        return (1, yielded);
    }
    // Costs are small printed integers; anything that is not finite is treated as free rather
    // than wrapping to a nonsense charge.
    // Costs are small printed integers. Counting up rather than casting keeps this free of
    // float-to-int truncation entirely, and a cost the corpus never prints simply stops at the
    // cap rather than wrapping.
    let rounded = printed.ceil().max(0.0);
    let mut charge = 0_i64;
    while f64::from(u32::try_from(charge).unwrap_or(u32::MAX)) < rounded && charge < 64 {
        charge += 1;
    }
    (charge, 1)
}

/// The player's units with Production here, paired with the planet they sit on.
#[must_use]
pub fn producers(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> Vec<(Unit, Option<PlanetId>)> {
    let types = catalogue(content, sources);
    let board = state.system_state(system);
    let produces = |unit: &Unit| {
        types
            .get(unit.type_id.as_str())
            .is_some_and(UnitType::has_production)
    };

    let mut found: Vec<(Unit, Option<PlanetId>)> = board
        .units_of(player)
        .into_iter()
        .filter(|unit| produces(unit))
        .map(|unit| (unit.clone(), None))
        .collect();
    for (planet, units) in &board.planet_units {
        found.extend(
            units
                .iter()
                .filter(|unit| &unit.owner == player && produces(unit))
                .map(|unit| (unit.clone(), Some(planet.clone()))),
        );
    }
    found
}

/// 68.1a: the production values of all the player's producing units here, combined.
///
/// A space dock's value depends on the resources of the planet it sits on, which is why the
/// planet travels with the unit rather than the value being read from the unit alone. War
/// Machines add to the total, so they ride along here as well.
#[must_use]
pub fn capacity(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> i64 {
    let types = catalogue(content, sources);
    producers(state, content, sources, player, system)
        .into_iter()
        .filter_map(|(unit, planet)| {
            let kind = types.get(unit.type_id.as_str())?;
            let resources = planet.map_or(0, |planet| {
                planet_value(content, sources, &planet, Spend::Resources)
            });
            Some(kind.production(resources))
        })
        .sum::<i64>()
        + war_machine_bonus(state, player)
}

/// How many of this structure the player already has on that planet.
#[must_use]
pub fn structures_on(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    planet: &PlanetId,
    base_type: &str,
) -> usize {
    let types = catalogue(content, sources);
    state
        .board
        .values()
        .filter_map(|system| system.planet_units.get(planet))
        .flatten()
        .filter(|unit| &unit.owner == player)
        .filter(|unit| {
            types
                .get(unit.type_id.as_str())
                .is_some_and(|kind| kind.base_type() == base_type)
        })
        .count()
}

/// 79.2: whether another of this structure may be built on that planet.
#[must_use]
pub fn structure_allowed(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    planet: &PlanetId,
    base_type: &str,
) -> bool {
    // Homeland Defense Act removes the PDS cap outright rather than raising it.
    if crate::laws::structure_cap_lifted(state, base_type) {
        return true;
    }
    // Demilitarized Zone: nothing may be placed on the elected planet at all.
    if crate::laws::planet_is_demilitarized(state, planet) {
        return false;
    }
    structure_limit(base_type)
        .is_none_or(|cap| structures_on(state, content, sources, player, planet, base_type) < cap)
}

/// Where a produced unit may go. [`SPACE`] denotes the space area.
#[must_use]
pub fn placements(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    kind: &UnitType<'_>,
) -> Vec<String> {
    if kind.is_ship() {
        // 68.10: "A player cannot produce ships in a system that contains other players' ships."
        // 68.10a keeps ground forces available, which is why this sits on the ship branch rather
        // than at the top: a blockaded space dock still makes infantry.
        //
        // The blockade was checked in the bot-facing "what could I build here" helper and nowhere
        // in the path that actually produces, so a blockaded dock built ships in play while the
        // helper said it could not.
        let types = catalogue(content, sources);
        let blockaded = state.system_state(system).units.iter().any(|unit| {
            unit.owner != *player
                && types
                    .get(unit.type_id.as_str())
                    .is_some_and(ti4_content::units::UnitType::is_ship)
        });
        if blockaded {
            return Vec::new();
        }
        return vec![SPACE.to_owned()]; // 68.2
    }
    // Entropic scars rule 2: PRODUCTION is a unit ability, so a space dock inside a scar produces
    // nothing. Rule 2.2 covers the Space Dock II text that defines X for its Production ability --
    // that text has no effect because the ability it modifies is gone.
    if !crate::entropic_scars::abilities_usable(content, sources, system, None) {
        return Vec::new();
    }
    let made = producers(state, content, sources, player, system);
    let mut spots: Vec<String> = made
        .iter()
        .filter_map(|(_, planet)| planet.clone())
        // Holy Planet of Ixth: units on the elected planet cannot use PRODUCTION.
        // Demilitarized Zone: nothing may be produced on the elected planet.
        .filter(|planet| {
            !crate::laws::production_forbidden_on(state, planet)
                && !crate::laws::planet_is_demilitarized(state, planet)
        })
        // Space stations rule 5. Defensive: with structures barred from stations a station cannot
        // hold a producer in the first place, so this should be unreachable -- but `placements` is
        // the last gate before a unit is placed, and the rule belongs at the gate too.
        .filter(|planet| !ti4_content::galaxy::is_space_station(content, planet.as_str(), sources))
        .filter(|planet| {
            structure_allowed(state, content, sources, player, planet, kind.base_type())
        })
        .map(|planet| planet.to_string())
        .collect(); // 68.3, 79.2
    if made.iter().any(|(_, planet)| planet.is_none()) {
        spots.push(SPACE.to_owned()); // 68.4
    }
    spots.dedup();
    spots
}

/// What this player can produce.
///
/// A war sun needs its technology; nothing else is gated. Faction-specific hulls are not
/// resolved — see the evidence for what that costs.
#[must_use]
pub fn buildable_for(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<String> {
    let owned = state.player(player).map(|seat| seat.technologies.clone());
    let faction = state
        .player(player)
        .map(|seat| seat.faction.to_string())
        .unwrap_or_default();
    let mut out = Vec::new();
    for base in BUILDABLE {
        if let Some((_, gate)) = UNLOCKED_BY.iter().find(|(unit, _)| *unit == base) {
            let has = owned.as_ref().is_some_and(|held| {
                held.iter()
                    .any(|tech| tech.as_str() == *gate || tech.as_str().ends_with(*gate))
            });
            if !has {
                continue;
            }
        }
        if let Some(own) = ti4_content::units::faction_unit(content, &faction, base, sources) {
            out.push(own.id().to_owned());
        } else if !matches!(base, "mech" | "flagship") {
            out.push(base.to_owned());
        }
    }
    out
}

/// What one production step did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProductionReport {
    /// Units produced, with where they were placed.
    pub produced: Vec<(UnitTypeId, String)>,
    /// Production capacity that went unused.
    pub unused_capacity: i64,
}

/// Where an open production step has reached.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Stage {
    /// Choosing what to build, or stopping.
    Choosing,
    /// Paying for the unit just chosen.
    Paying {
        id: String,
        owed: i64,
        made: usize,
    },
    /// Placing it.
    Placing {
        id: String,
        made: usize,
    },
    Done,
}

/// LRR 68: produce units in the active system, up to capacity, paying for each.
///
/// A [`Window`], so the driver can step it one decision at a time.
#[derive(Debug, Clone)]
pub struct ProductionWindow {
    player: PlayerId,
    system: SystemId,
    remaining: i64,
    stage: Stage,
    report: ProductionReport,
    /// Whether the end-of-use effects have fired, so they fire once per use rather than once per
    /// path that reaches `Done`.
    settled: bool,
    /// Production limit that capacity ships have opened for small units this use (Sol's Bellum
    /// Gloriosum). Spent by fighters and ground forces, and never below zero.
    free_capacity: i64,
    /// Resource value paid but not yet consumed, held across the unit selections of *this* use.
    ///
    /// 68.1: one use of PRODUCTION has one combined cost. This window collects selections one at a
    /// time, which is a good interface and a bad bill -- exhausting a two-resource planet for a
    /// one-resource batch used to throw the other resource away, and the next batch in the same
    /// use then demanded a second planet. The credit is what makes incremental selection pay the
    /// same as choosing the whole build up front.
    ///
    /// It never leaves the window, so it cannot reach another use of PRODUCTION: `new` starts it
    /// at zero and nothing else constructs one.
    credit: i64,
}

impl ProductionWindow {
    /// Open production for one player in one system.
    #[must_use]
    pub fn new(
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
        player: &PlayerId,
        system: &SystemId,
    ) -> Self {
        let remaining = capacity(state, content, sources, player, system);
        Self {
            player: player.clone(),
            system: system.clone(),
            remaining,
            stage: if remaining > 0 {
                Stage::Choosing
            } else {
                Stage::Done
            },
            report: ProductionReport::default(),
            settled: false,
            free_capacity: 0,
            credit: 0,
        }
    }

    /// What was produced.
    #[must_use]
    pub fn into_report(mut self) -> ProductionReport {
        self.report.unused_capacity = self.remaining.max(0);
        self.report
    }

    /// Re-settle the budget once the step's reaction window has resolved.
    ///
    /// War Machine is played "when 1 or more of your units use PRODUCTION" — the driver opens
    /// that window before the first choice is built, so any faces it adds must land in
    /// `remaining` before the first offer. Re-deriving from [`capacity`] also re-opens a step
    /// that would otherwise have been done: a player whose units total zero can still produce
    /// with the machine's +4. Calling it mid-payment or mid-placement would rewrite a budget
    /// the step has already spent against, so those stages decline it.
    pub fn refresh(&mut self, state: &GameState, content: &ContentStore, sources: SourceSet) {
        if matches!(self.stage, Stage::Paying { .. } | Stage::Placing { .. }) {
            return;
        }
        self.remaining = capacity(state, content, sources, &self.player, &self.system);
        self.stage = if self.remaining > 0 { Stage::Choosing } else { Stage::Done };
    }

    /// Draw down the credit against a cost, returning what is still owed.
    fn spend_credit(&mut self, cost: i64) -> i64 {
        let used = self.credit.min(cost);
        self.credit -= used;
        cost - used
    }

    /// Options for what to build now: affordable, placeable, one per unit type.
    fn build_options(
        &self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
    ) -> Vec<ChoiceOption> {
        let types = catalogue(content, sources);
        let mut options = Vec::new();
        for id in buildable_for(state, content, sources, &self.player) {
            let Some(kind) = types.get(id.as_str()) else {
                continue;
            };
            let (cost, pair) = price_of_under(Some(state), kind);
            // Credit already paid counts towards affordability, or a build the player has in fact
            // paid for would be withheld as unaffordable.
            if cost
                > available(state, content, sources, &self.player, Spend::Resources) + self.credit
            {
                continue;
            }
            if placements(state, content, sources, &self.player, &self.system, kind).is_empty() {
                continue;
            }
            // 31.4: a unit with no plastic left in the box is not offered. Offering it would let
            // a player spend resources on something that cannot be placed.
            if crate::supply::allowed(
                state,
                content,
                sources,
                &self.player,
                &UnitTypeId::new(id.clone()),
                1,
            ) == 0
            {
                continue;
            }
            let made = pair.min(usize::try_from(self.remaining).unwrap_or(0));
            if made == 0 {
                continue;
            }
            options.push(
                ChoiceOption::labelled(
                    format!("build|{id}|{made}"),
                    PRODUCE_KIND,
                    format!("produce {made}x {id} for {cost}"),
                )
                .with("cost", cost)
                .with("count", i64::try_from(made).unwrap_or(1))
                .with("unit", id.clone())
                .with("system", self.system.to_string()),
            );
            if pair > 1 && made > 1 {
                options.push(
                    ChoiceOption::labelled(
                        format!("build|{id}|1"),
                        PRODUCE_KIND,
                        format!("produce 1x {id} for {cost}"),
                    )
                    .with("cost", cost)
                    .with("count", 1)
                    .with("unit", id.clone())
                    .with("system", self.system.to_string()),
                );
            }
        }
        options
    }

    /// Put `made` copies of a unit into a placement, up to what the box still holds.
    ///
    /// 31.4 is applied here rather than only at the offer, because a two-for-one may be offered
    /// with one model left: producing two fighters is fine, producing two carriers when one
    /// remains is not.
    fn place(
        &mut self,
        state: &mut GameState,
        content: &ContentStore,
        sources: SourceSet,
        id: &str,
        where_to: &str,
        made: usize,
    ) {
        let made = crate::supply::allowed(
            state,
            content,
            sources,
            &self.player,
            &UnitTypeId::new(id),
            made,
        );
        for _ in 0..made {
            let unit = Unit::new(UnitTypeId::new(id), self.player.clone());
            if where_to == SPACE {
                state.system_mut(&self.system).units.push(unit);
            } else {
                state
                    .system_mut(&self.system)
                    .planet_units
                    .entry(PlanetId::new(where_to))
                    .or_default()
                    .push(unit);
            }
            self.report
                .produced
                .push((UnitTypeId::new(id), where_to.to_owned()));
        }
        // Bellum Gloriosum: a capacity ship opens an allowance that fighters and ground forces
        // spend instead of the production limit. Opened after the ship is placed and spent by
        // later purchases, which is the order the card describes -- the ship comes first.
        let kind = UnitTypeId::new(id);
        let count = i64::try_from(made).unwrap_or(i64::MAX);
        self.free_capacity += crate::breakthroughs::free_capacity_granted(
            state,
            content,
            sources,
            &self.player,
            &kind,
        ) * i64::from(made > 0);

        // 68.1a limits the number of units produced, not the number of purchases.  A
        // two-infantry purchase consumes two points of production capacity.
        let free = if crate::breakthroughs::spends_free_capacity(content, sources, &kind) {
            count.min(self.free_capacity)
        } else {
            0
        };
        self.free_capacity -= free;
        self.remaining -= count - free;
    }
}

impl Window for ProductionWindow {
    fn pending_choice(
        &self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
    ) -> Option<Choice> {
        match &self.stage {
            Stage::Done => None,
            Stage::Choosing => {
                if self.remaining <= 0 {
                    return None;
                }
                let mut options = self.build_options(state, content, sources);
                if options.is_empty() {
                    return None;
                }
                options.push(ChoiceOption::labelled(
                    "done_producing",
                    "decline",
                    "produce nothing further",
                ));
                Some(Choice::new(
                    self.player.clone(),
                    format!("produce in {} ({} left)", self.system, self.remaining),
                    options,
                ))
            }
            Stage::Paying { owed, .. } => {
                // Same face set and affordability guard as the free `pay` function (engine/
                // production.py pay()); a lone option settles in `settle`, never asked.
                let options = payment_options(
                    state,
                    content,
                    sources,
                    &self.player,
                    Spend::Resources,
                    0,
                    *owed,
                );
                if options.is_empty() {
                    return None; // unreachable under the affordability gate (see settle)
                }
                Some(Choice::new(
                    self.player.clone(),
                    format!("pay {owed} more resources"),
                    options,
                ))
            }
            Stage::Placing { id, .. } => {
                let types = catalogue(content, sources);
                let kind = types.get(id.as_str())?;
                let spots = placements(state, content, sources, &self.player, &self.system, kind);
                if spots.len() < 2 {
                    return None; // settled without a question
                }
                Some(Choice::new(
                    self.player.clone(),
                    format!("place the {id}"),
                    spots
                        .iter()
                        .map(|spot| {
                            ChoiceOption::labelled(
                                format!("place|{spot}"),
                                PLACE_KIND,
                                format!("place on {spot}"),
                            )
                            .with("system", self.system.to_string())
                            .with("unit", id.clone())
                        })
                        .collect(),
                ))
            }
        }
    }

    fn resolve(
        &mut self,
        state: &mut GameState,
        ctx: &mut Resolving<'_>,
        answer: ChoiceOption,
    ) -> Result<(), IllegalChoice> {
        let (content, sources) = (ctx.content, ctx.sources);
        let Some(choice) = self.pending_choice(state, content, sources) else {
            return Ok(());
        };
        let option = crate::choice::validate(&choice, answer)?;

        match self.stage.clone() {
            Stage::Done => {}
            Stage::Choosing => {
                if option.is_decline() {
                    self.stage = Stage::Done;
                } else if let Some(rest) = option.id.strip_prefix("build|") {
                    let mut parts = rest.split('|');
                    let Some(id) = parts.next() else {
                        return Ok(());
                    };
                    let made = parts
                        .next()
                        .and_then(|value| value.parse::<usize>().ok())
                        .unwrap_or(1);
                    let types = catalogue(content, sources);
                    let cost = types.get(id).map_or(0, |kind| price_of(kind).0);
                    // A purchase the credit covers outright goes straight to placing. Entering the
                    // paying stage owing nothing would ask `payment_options` for a bill of zero,
                    // which has no options, and the stage would abort with the unit unplaced.
                    let owed = self.spend_credit(cost);
                    self.stage = if owed > 0 {
                        Stage::Paying {
                            id: id.to_owned(),
                            owed,
                            made,
                        }
                    } else {
                        Stage::Placing {
                            id: id.to_owned(),
                            made,
                        }
                    };
                }
            }
            Stage::Paying { id, owed, made } => {
                // Paid before placed: a unit that could not be afforded must not reach the
                // board even for an instant, or an ability reacting to placement sees
                // something never bought.
                // The face's recorded worth is what applies — trade goods count double with the
                // `mc` technology (engine/production.py pay()).
                let Some(worth) = apply_payment_option(
                    state,
                    content,
                    sources,
                    &self.player,
                    Spend::Resources,
                    &option,
                ) else {
                    self.stage = Stage::Done; // unreachable for validated answers: abort
                    return Ok(());
                };
                let owed = owed - worth;
                self.stage = if owed > 0 {
                    Stage::Paying { id, owed, made }
                } else {
                    // Overpayment is kept for the rest of this use rather than discarded: one use
                    // of PRODUCTION is one bill, however many selections it was collected in.
                    self.credit += -owed;
                    Stage::Placing { id, made }
                };
            }
            Stage::Placing { id, made } => {
                let where_to = option.id.strip_prefix("place|").unwrap_or(SPACE).to_owned();
                self.place(state, content, sources, &id, &where_to, made);
                self.stage = Stage::Choosing;
            }
        }
        self.settle(state, content, sources);
        // After `settle`, not before: the loop inside it can be what reaches `Done`, and a check
        // ahead of it would miss exactly the uses that ended without another question.
        //
        // Auto-Factories reads the whole use of PRODUCTION, so it fires once where the use ends
        // rather than at each placement -- three ships pay once, not three times. Several paths
        // reach `Done`, so this is a flag rather than a call at each of them.
        if matches!(self.stage, Stage::Done) && !self.settled {
            self.settled = true;
            let (who, made) = (self.player.clone(), self.report.produced.clone());
            crate::breakthroughs::on_production_finished(state, content, sources, &who, &made);
            // Prophecy of Ixth: using PRODUCTION discards the law unless two or more fighters were
            // produced. Read over the whole use for the same reason Auto-Factories is.
            let types = ti4_content::units::catalogue(content, sources);
            let fighters = made
                .iter()
                .filter(|(kind, _)| {
                    types
                        .get(kind.as_str())
                        .is_some_and(ti4_content::units::UnitType::is_fighter)
                })
                .count();
            crate::laws::prophecy_after_production(state, &who, fighters);
        }
        Ok(())
    }
}

impl ProductionWindow {
    /// Advance past any stage that has nothing left to ask.
    fn settle(&mut self, state: &mut GameState, content: &ContentStore, sources: SourceSet) {
        loop {
            match self.stage.clone() {
                Stage::Paying { id, owed, made } => {
                    // Oracle pay(): a lone option is taken without asking. Settle such degenerate
                    // steps here so the question never reaches a decider (and the trace gains no
                    // degenerate decision).
                    let options = payment_options(
                        state,
                        content,
                        sources,
                        &self.player,
                        Spend::Resources,
                        0,
                        owed,
                    );
                    match options.as_slice() {
                        [] => {
                            // Unreachable under the affordability gate (available >= cost is
                            // checked before a build option is offered).
                            self.stage = Stage::Done;
                            return;
                        }
                        [only] => {
                            let Some(worth) = apply_payment_option(
                                state,
                                content,
                                sources,
                                &self.player,
                                Spend::Resources,
                                only,
                            ) else {
                                self.stage = Stage::Done; // unreachable for offered options: abort
                                return;
                            };
                            if owed > worth {
                                self.stage = Stage::Paying {
                                    id,
                                    owed: owed - worth,
                                    made,
                                };
                                continue; // the next step may itself be degenerate
                            }
                            self.credit += worth - owed;
                            self.stage = Stage::Placing { id, made };
                        }
                        _ => return,
                    }
                }
                Stage::Placing { id, made } => {
                    let types = catalogue(content, sources);
                    let Some(kind) = types.get(id.as_str()).copied() else {
                        self.stage = Stage::Done;
                        return;
                    };
                    let spots =
                        placements(state, content, sources, &self.player, &self.system, &kind);
                    // Exactly one legal placement is not a decision.
                    match spots.as_slice() {
                        [only] => {
                            let only = only.clone();
                            self.place(state, content, sources, &id, &only, made);
                            self.stage = Stage::Choosing;
                        }
                        [] => {
                            self.stage = Stage::Done;
                            return;
                        }
                        _ => return,
                    }
                }
                Stage::Choosing => {
                    if self.remaining <= 0 || self.build_options(state, content, sources).is_empty()
                    {
                        self.stage = Stage::Done;
                    }
                    return;
                }
                // A finished window has nothing left to settle: leaving the loop is mandatory,
                // because `resolve` settles after every answer and a decline ends production.
                Stage::Done => break,
            }
        }
    }
}

/// Run production to the end against a table.
///
/// # Errors
/// [`IllegalChoice`] when a decider answers with something not offered.
pub fn resolve(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    system: &SystemId,
) -> Result<ProductionReport, IllegalChoice> {
    let mut window = ProductionWindow::new(state, content, sources, player, system);
    // Production rolls nothing, so these are never drawn from. Kept explicit rather than
    // hidden behind an Option: if a future rule does roll here, it must be handed the game's
    // generator instead of finding a convenient throwaway already in scope.
    let mut dice = crate::dice::Dice::new();
    let mut rng = crate::rng::GameRng::new(0);
    let mut ctx = Resolving {
        content,
        sources,
        dice: &mut dice,
        rng: &mut rng,
        table,
        timing: None,
    };
    while let Some(choice) = window.pending_choice(state, content, sources) {
        let answer = ctx
            .table
            .ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
        window.resolve(state, &mut ctx, answer)?;
    }
    Ok(window.into_report())
}

#[cfg(test)]
mod tests {

    /// 68.10: no producing *ships* in a system that contains another player's ships.
    ///
    /// 68.10a keeps ground forces available, which is why the guard belongs on the ship branch of
    /// `placements` rather than on the system: a blockaded space dock still makes infantry.
    #[test]
    fn a_blockaded_system_produces_ground_forces_but_no_ships() {
        let content = ti4_content::ContentStore::embedded();
        let sources = ti4_model::content_types::DEFAULT;
        let (mine, theirs) = (PlayerId::new("a"), PlayerId::new("b"));
        let mut state = crate::fixtures::game(&["a", "b"]);

        let (system, planet) = crate::fixtures::a_placed_planet();
        state.board.entry(system.clone()).or_default();
        if let Some(here) = state.board.get_mut(&system) {
            here.set_control(planet.clone(), mine.clone());
            here.planet_units.entry(planet.clone()).or_default().push(
                ti4_model::units::Unit::new(UnitTypeId::new("spacedock"), mine.clone()),
            );
        }
        let types = catalogue(content, sources);
        let cruiser = types.get("cruiser").copied().expect("a cruiser");
        let infantry = types.get("infantry").copied().expect("an infantry");

        assert!(
            !placements(&state, content, sources, &mine, &system, &cruiser).is_empty(),
            "uncontested, the dock builds ships"
        );

        crate::fixtures::put(&mut state, &system, "destroyer", &theirs, 1);
        assert!(
            placements(&state, content, sources, &mine, &system, &cruiser).is_empty(),
            "an enemy ship blockades ship production"
        );
        assert!(
            !placements(&state, content, sources, &mine, &system, &infantry).is_empty(),
            "but ground forces are still produced (68.10a)"
        );
    }

    /// Four infantry in one use of PRODUCTION cost two resources, from one two-resource planet.
    ///
    /// The acceptance case from `plans/BUG_2026-08-29_PRODUCTION_COMBINED_PAYMENT.md`. Selected as
    /// two batches of two, which is the shape that used to throw the planet's second resource away
    /// and then demand a second payment source for a bill that was already covered.
    #[test]
    fn one_production_use_is_one_bill_across_batches() {
        let content = ti4_content::ContentStore::embedded();
        let sources = ti4_model::content_types::DEFAULT;
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);

        // A two-resource planet and nothing else: no trade goods, so a second payment source
        // simply does not exist and the old behaviour cannot hide behind one.
        let (system, planet) = ti4_content::galaxy::all_planets(content, sources)
            .iter()
            .find(|(_, record)| {
                record.system_id().is_some()
                    && !record.is_placed_during_play()
                    && record.resources() == 2
            })
            .map(|(id, record)| {
                (
                    ti4_model::id::SystemId::new(record.system_id().unwrap_or_default()),
                    PlanetId::new(*id),
                )
            })
            .expect("the corpus has a two-resource planet");
        state.board.entry(system.clone()).or_default();
        if let Some(here) = state.board.get_mut(&system) {
            here.set_control(planet.clone(), player.clone());
            here.planet_units.entry(planet.clone()).or_default().push(
                ti4_model::units::Unit::new(UnitTypeId::new("spacedock"), player.clone()),
            );
        }
        if let Some(seat) = state.player_mut(&player) {
            seat.trade_goods = 0;
        }

        let mut window = ProductionWindow::new(&state, content, sources, &player, &system);
        let mut table = Table::with_default(Box::new(crate::choice::FirstOption));
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(1);
        let mut inner = Table::new();
        let mut ctx = crate::choice::Resolving {
            content,
            sources,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut inner,
            timing: None,
        };
        let mut steps = 0;
        let mut infantry = 0;
        while let Some(choice) = window.pending_choice(&state, content, sources) {
            // Buy infantry whenever they are offered, and stop once four are on order.
            let wanted = choice
                .options
                .iter()
                .find(|option| infantry < 4 && option.id.contains("infantry"))
                .cloned();
            let answer = match wanted {
                Some(option) => {
                    infantry += 2; // infantry are bought two to a purchase (68.2)
                    option
                }
                None => table.ask(&choice).expect("an answer"),
            };
            window.resolve(&mut state, &mut ctx, answer).expect("resolves");
            steps += 1;
            assert!(steps < 200, "production must terminate");
        }

        let report = window.into_report();
        let built = report
            .produced
            .iter()
            .filter(|(kind, _)| kind.as_str().contains("infantry"))
            .count();
        assert_eq!(built, 4, "four infantry were produced");
        assert!(
            state.exhausted_planets.contains(&planet),
            "the planet paid"
        );
        assert_eq!(
            state.player(&player).unwrap().trade_goods,
            0,
            "and nothing else was needed: one two-resource planet covers all four"
        );
    }

    /// Xxekir Grom makes a planet pay its resources and influence together, as either kind.
    ///
    /// Asserted through `available`, not through `payment_faces`: the point of folding it into the
    /// face is that every spending path sees it, and `available` is one of the paths that would
    /// have missed a fix applied at the card.
    #[test]
    fn xxekir_grom_pays_resources_and_influence_together() {
        let content = ti4_content::ContentStore::embedded();
        let sources = ti4_model::content_types::DEFAULT;
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);

        // A planet worth something in both, so the combination is visible.
        let (system, planet) = ti4_content::galaxy::all_planets(content, sources)
            .iter()
            .find(|(_, record)| {
                record.system_id().is_some()
                    && !record.is_placed_during_play()
                    && record.resources() > 0
                    && record.influence() > 0
            })
            .map(|(id, record)| {
                (
                    ti4_model::id::SystemId::new(record.system_id().unwrap_or_default()),
                    PlanetId::new(*id),
                )
            })
            .expect("the corpus has a planet worth both");
        let worth = ti4_content::galaxy::planet(content, planet.as_str(), sources)
            .map(|record| (record.resources(), record.influence()))
            .expect("the planet is in the corpus");

        state.board.entry(system.clone()).or_default();
        if let Some(here) = state.board.get_mut(&system) {
            here.set_control(planet.clone(), player.clone());
        }

        let before = available(&state, content, sources, &player, Spend::Resources);
        assert_eq!(before, worth.0, "ordinarily the planet pays its resources");

        if let Some(seat) = state.player_mut(&player) {
            seat.leaders.insert(
                ti4_model::id::LeaderId::new("xxchahero"),
                ti4_model::state::LeaderStatus::Unlocked,
            );
        }
        let after = available(&state, content, sources, &player, Spend::Resources);
        assert_eq!(
            after,
            worth.0 + worth.1,
            "with the hero it pays both, against a resource bill"
        );
        assert_eq!(
            available(&state, content, sources, &player, Spend::Influence),
            worth.0 + worth.1,
            "and the same against an influence bill"
        );
    }
    use ti4_model::content_types::POK;

    use std::{cell::RefCell, rc::Rc};

    use super::*;
    use crate::fixtures::{a_placed_planet, game, put, put_on_planet};

    fn player() -> PlayerId {
        PlayerId::new("a")
    }

    struct PaymentKindChecking(&'static str);

    impl crate::choice::Decider for PaymentKindChecking {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            assert!(
                choice.options.iter().all(|option| {
                    option
                        .payload
                        .get("kind")
                        .and_then(serde_json::Value::as_str)
                        == Some(self.0)
                }),
                "every payment option carries the oracle payload key `kind`"
            );
            Ok(choice.options[0].clone())
        }
    }

    fn seated() -> (GameState, SystemId, PlanetId) {
        let state = game(&["a", "b"]);
        let (system, planet) = a_placed_planet();
        (state, system, planet)
    }

    #[test]
    fn the_mc_technology_doubles_trade_good_payment_value() {
        // Oracle `engine/production.py`: while "mc" is owned, one trade good stands for two in
        // both `available()` and each payment step.
        struct WorthChecking;
        impl crate::choice::Decider for WorthChecking {
            fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
                assert_eq!(choice.prompt, "pay 2 more influence");
                let good = choice
                    .options
                    .iter()
                    .find(|option| option.id == "trade_good")
                    .unwrap();
                assert_eq!(
                    good.payload
                        .get("worth")
                        .and_then(serde_json::Value::as_i64),
                    Some(2)
                );
                Ok(choice.option("trade_good").cloned().unwrap())
            }
        }

        let mut state = game(&["a", "b"]);
        let player = PlayerId::new("a");
        {
            let seat = state.player_mut(&player).unwrap();
            seat.trade_goods = 1;
            seat.technologies.insert(TechnologyId::new("mc"));
        }
        assert_eq!(
            available(
                &state,
                ContentStore::embedded(),
                POK,
                &player,
                Spend::Influence
            ),
            2
        );

        let mut table = Table::with_default(Box::new(WorthChecking));
        assert!(
            pay_seeing(
                &mut state,
                ContentStore::embedded(),
                POK,
                None,
                &mut table,
                &player,
                2,
                Spend::Influence
            )
            .unwrap()
        );
        assert_eq!(state.player(&player).unwrap().trade_goods, 0);
    }

    #[test]
    fn only_readied_controlled_planets_can_be_spent() {
        // 34, 75.2.
        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        assert_eq!(spendable_planets(&state, &player()), vec![planet.clone()]);

        state.exhaust_planet(planet);
        assert!(spendable_planets(&state, &player()).is_empty());
    }

    #[test]
    fn trade_goods_count_towards_what_can_be_afforded() {
        // 75.3, 47.3.
        let (mut state, _, _) = seated();
        state.player_mut(&player()).unwrap().trade_goods = 3;

        assert_eq!(
            available(
                &state,
                ContentStore::embedded(),
                POK,
                &player(),
                Spend::Resources
            ),
            3
        );
    }

    #[test]
    fn payment_options_label_the_resource_or_influence_they_spend() {
        for (kind, name) in [
            (Spend::Resources, "resources"),
            (Spend::Influence, "influence"),
        ] {
            let (mut state, _, _) = seated();
            state.player_mut(&player()).unwrap().trade_goods = 1;
            let mut table = Table::new();
            table.seat(player(), Box::new(PaymentKindChecking(name)));

            assert!(
                pay(
                    &mut state,
                    ContentStore::embedded(),
                    POK,
                    &mut table,
                    &player(),
                    1,
                    kind,
                )
                .unwrap()
            );
        }
    }

    type RecordedPayment = (String, Vec<String>);

    struct PaymentPromptRecording(Rc<RefCell<Vec<RecordedPayment>>>);

    impl crate::choice::Decider for PaymentPromptRecording {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            let labels = choice
                .options
                .iter()
                .map(|option| option.label.clone())
                .collect();
            self.0.borrow_mut().push((choice.prompt.clone(), labels));
            Ok(choice.options[0].clone())
        }
    }

    #[test]
    fn the_payment_prompt_names_the_remaining_debt_and_its_kind() {
        // Oracle payment loop (engine/production.py): each iteration asks `pay {cost - paid}
        // more {kind}` and a planet's option label names the kind being spent.
        for (kind, name) in [
            (Spend::Resources, "resources"),
            (Spend::Influence, "influence"),
        ] {
            let (mut state, system, planet) = seated();
            state
                .system_mut(&system)
                .set_control(planet.clone(), player());
            let worth = planet_value(ContentStore::embedded(), POK, &planet, kind);
            state.player_mut(&player()).unwrap().trade_goods = 1;

            let cost = if worth > 0 { worth + 1 } else { 1 };
            let recorded = Rc::new(RefCell::new(Vec::new()));
            let mut table = Table::new();
            table.seat(player(), Box::new(PaymentPromptRecording(recorded.clone())));

            assert!(
                pay(
                    &mut state,
                    ContentStore::embedded(),
                    POK,
                    &mut table,
                    &player(),
                    cost,
                    kind,
                )
                .unwrap()
            );

            let seen = recorded.borrow();
            if worth > 0 {
                // One real question: the planet face against the trade good (oracle pay()). The
                // final unit is a lone option and is taken without asking (P1-g f5).
                assert_eq!(seen.len(), 1, "only the genuine choice is asked");
                assert_eq!(seen[0].0, format!("pay {cost} more {name}"));
                assert_eq!(
                    seen[0].1,
                    vec![
                        format!("exhaust {planet} for {worth} {name}"),
                        "spend a trade good".to_owned(),
                    ]
                );
                assert_eq!(state.player(&player()).unwrap().trade_goods, 0);
            } else {
                // Oracle parity (P1-g): zero-worth faces are never offered, so the lone trade
                // good is taken without any question at all.
                assert!(seen.is_empty(), "the lone payment option settles silently");
                assert_eq!(state.player(&player()).unwrap().trade_goods, 0);
            }
        }
    }

    #[test]
    fn the_production_window_payment_prompt_names_the_remaining_debt_and_its_kind() {
        // Same oracle wording for the window's paying stage (resources only).
        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        state.player_mut(&player()).unwrap().trade_goods = 1;
        let mut window =
            ProductionWindow::new(&state, ContentStore::embedded(), POK, &player(), &system);
        window.stage = Stage::Paying {
            id: "cruiser".to_owned(),
            owed: 3,
            made: 1,
        };

        let choice = window
            .pending_choice(&state, ContentStore::embedded(), POK)
            .expect("a payment question is pending");
        assert_eq!(choice.prompt, "pay 3 more resources");
        for option in &choice.options {
            if option.id.starts_with("exhaust|") {
                let worth = planet_value(ContentStore::embedded(), POK, &planet, Spend::Resources);
                assert_eq!(
                    option.label,
                    format!("exhaust {planet} for {worth} resources")
                );
            } else {
                assert_eq!(
                    (option.id.as_str(), option.label.as_str()),
                    ("trade_good", "spend a trade good")
                );
            }
        }
    }

    #[test]
    fn a_lone_payment_option_is_taken_without_a_question() {
        // Oracle pay(): exactly one legal option is taken without asking — the table never sees
        // it. One controlled planet and no trade goods, so the whole bill is that single face.
        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        let worth = planet_value(ContentStore::embedded(), POK, &planet, Spend::Resources);
        assert!(worth > 0, "the fixture planet has a resource face");

        let recorded = Rc::new(RefCell::new(Vec::new()));
        let mut table = Table::new();
        table.seat(player(), Box::new(PaymentPromptRecording(recorded.clone())));
        assert!(
            pay(
                &mut state,
                ContentStore::embedded(),
                POK,
                &mut table,
                &player(),
                worth,
                Spend::Resources,
            )
            .unwrap()
        );

        assert!(
            recorded.borrow().is_empty(),
            "a lone option is never a question"
        );
        assert!(
            spendable_planets(&state, &player()).is_empty(),
            "the planet paid and exhausted"
        );
    }

    #[test]
    fn zero_worth_faces_are_never_offered() {
        // Oracle _planet_payment_values keeps only positive faces; Rust used to offer
        // "exhaust X for 0" (recorded F7) — a decision the oracle never makes. With one trade
        // good left, payment settles by itself and the planet stays unspent.
        let store = ContentStore::embedded();
        let (planet_id, system_id) = ti4_content::galaxy::all_planets(store, POK)
            .iter()
            .find(|(id, p)| {
                p.system_id().is_some()
                    && planet_value(store, POK, &PlanetId::new(**id), Spend::Resources) == 0
            })
            .map(|(id, p)| (PlanetId::new(*id), SystemId::new(p.system_id().unwrap())))
            .expect("the corpus has a resource-less placed planet");
        let mut state = game(&["a", "b"]);
        state
            .system_mut(&system_id)
            .set_control(planet_id.clone(), player());
        state.player_mut(&player()).unwrap().trade_goods = 1;

        let recorded = Rc::new(RefCell::new(Vec::new()));
        let mut table = Table::new();
        table.seat(player(), Box::new(PaymentPromptRecording(recorded.clone())));
        assert!(
            pay(
                &mut state,
                store,
                POK,
                &mut table,
                &player(),
                1,
                Spend::Resources
            )
            .unwrap()
        );

        assert!(
            recorded.borrow().is_empty(),
            "the lone trade good is taken without asking"
        );
        assert_eq!(state.player(&player()).unwrap().trade_goods, 0);
        assert_eq!(
            spendable_planets(&state, &player()),
            vec![planet_id],
            "a zero face spends nothing"
        );
    }

    /// One recorded question: each offered option's id with its payload flattened to sorted
    /// `key=value` strings.
    type RecordedOption = (String, Vec<String>);

    /// Records every option of each payment question, then takes the first — planet faces
    /// always precede trade goods in `payment_options`.
    struct PaymentPayloadRecording(Rc<RefCell<Vec<Vec<RecordedOption>>>>);

    impl crate::choice::Decider for PaymentPayloadRecording {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            let mut rows = Vec::new();
            for option in &choice.options {
                assert_eq!(
                    option
                        .payload
                        .get("kind")
                        .and_then(serde_json::Value::as_str),
                    Some("resources"),
                    "every face names the kind of bill it pays"
                );
                // Every payload value as `key=value`, sorted by key (payload is a BTreeMap).
                let mut pairs: std::collections::BTreeMap<String, String> = BTreeMap::new();
                for (key, value) in &option.payload {
                    match value {
                        serde_json::Value::Number(number) => {
                            pairs.insert(key.clone(), number.to_string());
                        }
                        serde_json::Value::String(text) => {
                            pairs.insert(key.clone(), text.clone());
                        }
                        _ => {}
                    }
                }
                let flat: Vec<String> = pairs
                    .into_iter()
                    .map(|(key, value)| format!("{key}={value}"))
                    .collect();
                rows.push((option.id.clone(), flat));
            }
            self.0.borrow_mut().push(rows);
            Ok(choice.options[0].clone())
        }
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one scenario walks every oracle rule of the alternate face"
    )]
    fn archons_gift_offers_and_guards_the_alternate_face() {
        // Oracle _planet_payment_values + pay(): Xxcha's Archon's Gift adds the other printed
        // value as a second face of the same planet, and no face is offered that would strand
        // the rest of the bill. The cross-source face carries its source in id, label and payload.
        let store = ContentStore::embedded();
        let (planet_id, system_id) = ti4_content::galaxy::all_planets(store, POK)
            .iter()
            .find(|(id, p)| {
                let planet = PlanetId::new(**id);
                let res = planet_value(store, POK, &planet, Spend::Resources);
                let inf = planet_value(store, POK, &planet, Spend::Influence);
                p.system_id().is_some() && res > 0 && inf > 0 && res != inf
            })
            .map(|(id, p)| (PlanetId::new(*id), SystemId::new(p.system_id().unwrap())))
            .expect("the corpus has a two-faced planet with distinct values");
        let res = planet_value(store, POK, &planet_id, Spend::Resources);
        let inf = planet_value(store, POK, &planet_id, Spend::Influence);

        let mut state = game(&["a", "b"]);
        state
            .system_mut(&system_id)
            .set_control(planet_id.clone(), player());
        state.player_mut(&player()).unwrap().breakthrough =
            Some(ti4_model::id::BreakthroughId::new("xxchabt"));

        // Cost one more than the larger face: exactly one trade good must cover the difference,
        // so only the larger face survives the affordability guard.
        let cost = if res > inf { res + 1 } else { inf + 1 };
        state.player_mut(&player()).unwrap().trade_goods = 1;
        let probe = state.clone();

        let recorded = Rc::new(RefCell::new(Vec::new()));
        let mut table = Table::new();
        table.seat(
            player(),
            Box::new(PaymentPayloadRecording(recorded.clone())),
        );
        assert!(
            pay(
                &mut state,
                store,
                POK,
                &mut table,
                &player(),
                cost,
                Spend::Resources
            )
            .unwrap()
        );

        // One real question (the guarded face plus the trade good); the final unit is auto-picked.
        let rows = recorded.borrow();
        assert_eq!(
            rows.len(),
            1,
            "only the first step of the bill is a question"
        );
        if res > inf {
            assert_eq!(
                rows[0],
                vec![
                    (
                        format!("exhaust|{planet_id}"),
                        vec![
                            format!("kind=resources"),
                            format!("owed={cost}"),
                            "source=resources".to_owned(),
                            format!("worth={res}"),
                        ],
                    ),
                    (
                        "trade_good".to_owned(),
                        vec![
                            "kind=resources".to_owned(),
                            format!("owed={cost}"),
                            "worth=1".to_owned()
                        ],
                    )
                ]
            );
        } else {
            assert_eq!(
                rows[0],
                vec![
                    (
                        format!("exhaust|{planet_id}|influence"),
                        vec![
                            "kind=resources".to_owned(),
                            format!("owed={cost}"),
                            "source=influence".to_owned(),
                            format!("worth={inf}"),
                        ],
                    ),
                    (
                        "trade_good".to_owned(),
                        vec![
                            "kind=resources".to_owned(),
                            format!("owed={cost}"),
                            "worth=1".to_owned()
                        ],
                    )
                ]
            );
        }
        // Label parity for the offered face (engine/production.py pay()): the cross-source
        // *Archon's Gift* face names its source.
        let faces = payment_options(&probe, store, POK, &player(), Spend::Resources, 0, cost);
        if res > inf {
            assert!(
                faces.iter().any(|face| {
                    face.label == format!("exhaust {planet_id} for {res} resources")
                })
            );
            assert!(!faces.iter().any(|face| face.id.contains("|influence")));
        } else {
            let cross = faces
                .iter()
                .find(|face| face.id == format!("exhaust|{planet_id}|influence"))
                .expect("the alternate face is offered");
            assert_eq!(
                cross.label,
                format!("exhaust {planet_id} for {inf} resources using its influence")
            );
        }
        assert_eq!(state.player(&player()).unwrap().trade_goods, 0);
        assert!(
            spendable_planets(&state, &player()).is_empty(),
            "one face exhausted the planet, as in the oracle"
        );
    }

    #[test]
    fn a_window_paying_stage_settles_a_lone_option_without_asking() {
        // Oracle pay(): the window's paying stage shares exactly that option set, so with one
        // trade good and only resource-less planets controlled (the old F7 offered them) there is
        // precisely one way to pay — `settle` takes it and no question reaches a decider.
        let store = ContentStore::embedded();
        let (planet_id, system_id) = ti4_content::galaxy::all_planets(store, POK)
            .iter()
            .find(|(id, p)| {
                p.system_id().is_some()
                    && planet_value(store, POK, &PlanetId::new(**id), Spend::Resources) == 0
            })
            .map(|(id, p)| (PlanetId::new(*id), SystemId::new(p.system_id().unwrap())))
            .expect("the corpus has a resource-less placed planet");
        let mut state = game(&["a", "b"]);
        state
            .system_mut(&system_id)
            .set_control(planet_id.clone(), player());
        state.player_mut(&player()).unwrap().trade_goods = 1;

        let mut window = ProductionWindow::new(&state, store, POK, &player(), &system_id);
        window.stage = Stage::Paying {
            id: "fighter".to_owned(),
            owed: 1,
            made: 1,
        };
        window.remaining = 0;
        window.settle(&mut state, store, POK);

        assert!(matches!(window.stage, Stage::Done));
        assert_eq!(state.player(&player()).unwrap().trade_goods, 0);
        assert_eq!(
            spendable_planets(&state, &player()),
            vec![planet_id],
            "the zero-worth planet was never offered"
        );
    }

    #[test]
    fn paying_exhausts_the_planet_it_used() {
        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        let worth = planet_value(ContentStore::embedded(), POK, &planet, Spend::Resources);
        assert!(worth > 0, "the fixture planet is worth something");
        let mut table = Table::new();

        let paid = pay(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &player(),
            worth,
            Spend::Resources,
        )
        .unwrap();

        assert!(paid);
        assert!(state.exhausted_planets.contains(&planet));
    }

    #[test]
    fn an_unaffordable_cost_spends_nothing() {
        let (mut state, _, _) = seated();
        let before = state.clone();
        let mut table = Table::new();

        let paid = pay(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &player(),
            99,
            Spend::Resources,
        )
        .unwrap();

        assert!(!paid);
        assert!(state.identical(&before), "nothing was exhausted or spent");
    }

    #[test]
    fn a_planet_pays_for_one_thing_or_the_other_never_both() {
        // 34.3: exhausting for influence leaves nothing to give for resources.
        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        let mut table = Table::new();

        let influence = planet_value(ContentStore::embedded(), POK, &planet, Spend::Influence);
        if influence > 0 {
            pay(
                &mut state,
                ContentStore::embedded(),
                POK,
                &mut table,
                &player(),
                influence,
                Spend::Influence,
            )
            .unwrap();
            assert_eq!(
                available(
                    &state,
                    ContentStore::embedded(),
                    POK,
                    &player(),
                    Spend::Resources
                ),
                0,
                "the card is exhausted, so it gives nothing further"
            );
        }
    }

    #[test]
    fn a_space_dock_produces_and_a_cruiser_does_not() {
        let (mut state, system, planet) = seated();
        put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);
        put(&mut state, &system, "cruiser", &player(), 2);

        let made = producers(&state, ContentStore::embedded(), POK, &player(), &system);
        assert_eq!(made.len(), 1);
        assert_eq!(made[0].1, Some(planet));
    }

    #[test]
    fn a_unit_with_no_plastic_left_is_not_offered() {
        // 31.4, where it actually binds. A batch of random games never builds enough to reach a
        // cap, so a test that only watches games pass would hold whether or not the rule exists —
        // and this one did, until it was written against the offer instead.
        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);
        state.player_mut(&player()).unwrap().trade_goods = 50;

        let carriers_offered = |state: &GameState| {
            crate::choice::Window::pending_choice(
                &ProductionWindow::new(state, ContentStore::embedded(), POK, &player(), &system),
                state,
                ContentStore::embedded(),
                POK,
            )
            .map_or(0, |choice| {
                choice
                    .options
                    .iter()
                    .filter(|option| option.id.contains("carrier"))
                    .count()
            })
        };

        assert!(carriers_offered(&state) > 0, "a carrier can be built");

        put(&mut state, &system, "carrier", &player(), 4);
        assert_eq!(
            carriers_offered(&state),
            0,
            "four carriers are every carrier in the box"
        );
    }

    #[test]
    fn a_docks_capacity_follows_the_planet_it_sits_on() {
        // 68.1a: a space dock's production value is read from its planet's resources, which is
        // why the planet travels with the unit rather than the value coming from the unit.
        let (mut state, system, planet) = seated();
        put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);

        let resources = planet_value(ContentStore::embedded(), POK, &planet, Spend::Resources);
        let got = capacity(&state, ContentStore::embedded(), POK, &player(), &system);

        assert!(got > 0);
        assert!(
            got >= resources,
            "a dock is worth its planet's resources plus two"
        );
    }

    #[test]
    fn ships_go_to_space_and_structures_to_a_planet() {
        // 68.2 and 68.3.
        let (mut state, system, planet) = seated();
        put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);
        let types = catalogue(ContentStore::embedded(), POK);

        let ship = types.get("cruiser").unwrap();
        assert_eq!(
            placements(
                &state,
                ContentStore::embedded(),
                POK,
                &player(),
                &system,
                ship
            ),
            vec![SPACE.to_owned()]
        );

        let structure = types.get("pds").unwrap();
        assert!(
            placements(
                &state,
                ContentStore::embedded(),
                POK,
                &player(),
                &system,
                structure
            )
            .contains(&planet.to_string())
        );
    }

    #[test]
    fn one_planet_takes_only_one_space_dock() {
        // 79.2.
        let (mut state, system, planet) = seated();
        assert!(structure_allowed(
            &state,
            ContentStore::embedded(),
            POK,
            &player(),
            &planet,
            "spacedock"
        ));

        put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);
        assert!(
            !structure_allowed(
                &state,
                ContentStore::embedded(),
                POK,
                &player(),
                &planet,
                "spacedock"
            ),
            "a second dock has nowhere to go"
        );
    }

    #[test]
    fn a_fighter_costs_one_and_arrives_in_pairs() {
        // 68.2: printed cost below one means two units for one resource. Charging ceil and
        // yielding one would make the two commonest units cost double.
        let types = catalogue(ContentStore::embedded(), POK);
        let fighter = types.get("fighter").unwrap();
        assert!(fighter.cost() < 1.0, "the corpus prices it below one");
        assert_eq!(price_of(fighter), (1, 2));

        let cruiser = types.get("cruiser").unwrap();
        assert_eq!(price_of(cruiser).1, 1, "a full-cost unit comes singly");
    }

    #[test]
    fn a_war_sun_needs_its_technology() {
        // 67.x.
        let (mut state, _, _) = seated();
        assert!(
            !buildable_for(&state, ContentStore::embedded(), POK, &player())
                .contains(&"warsun".to_owned())
        );

        state
            .player_mut(&player())
            .unwrap()
            .technologies
            .insert(ti4_model::id::TechnologyId::new("ws"));
        assert!(
            buildable_for(&state, ContentStore::embedded(), POK, &player())
                .contains(&"warsun".to_owned())
        );
    }

    #[test]
    fn normal_production_uses_faction_units_and_never_builds_structures() {
        let mut state = game(&["a"]);
        state.player_mut(&player()).unwrap().faction = ti4_model::id::FactionId::new("hacan");

        let buildable = buildable_for(&state, ContentStore::embedded(), POK, &player());

        assert!(buildable.contains(&"hacan_mech".to_owned()));
        assert!(!buildable.contains(&"mech".to_owned()));
        assert!(!buildable.contains(&"pds".to_owned()));
        assert!(!buildable.contains(&"spacedock".to_owned()));
    }

    #[test]
    fn production_can_be_stepped_one_decision_at_a_time() {
        // The point of the Window trait: a caller can inspect the game between decisions,
        // which the inline version made impossible.
        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);
        state.player_mut(&player()).unwrap().trade_goods = 10;

        let mut window =
            ProductionWindow::new(&state, ContentStore::embedded(), POK, &player(), &system);
        let mut table = Table::new();
        let mut decisions = 0;

        while let Some(choice) = window.pending_choice(&state, ContentStore::embedded(), POK) {
            // Between every pair of decisions the game is a whole, inspectable state.
            let snapshot = state.clone();
            assert!(snapshot.identical(&state));

            let answer = table.ask(&choice).unwrap();
            let mut dice = crate::dice::Dice::new();
            let mut rng = crate::rng::GameRng::new(0);
            let mut inner = Table::new();
            let mut ctx = Resolving {
                content: ContentStore::embedded(),
                sources: POK,
                dice: &mut dice,
                rng: &mut rng,
                table: &mut inner,
                timing: None,
            };
            window.resolve(&mut state, &mut ctx, answer).unwrap();
            decisions += 1;
            assert!(decisions < 50, "production should terminate");
        }

        assert!(decisions > 1, "more than one decision was owed");
        assert!(!window.into_report().produced.is_empty());
    }

    #[test]
    fn a_declined_production_window_settles_without_spinning() {
        // P1-g regression guard: settle's `Stage::Done` arm must leave the loop. A decline ends
        // production and resolve settles afterwards, so on the falling-through version of that
        // arm this test never returns instead of finishing in one step.
        struct Decline;
        impl crate::choice::Decider for Decline {
            fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
                Ok(choice.option("done_producing").cloned().unwrap())
            }
        }

        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);
        state.player_mut(&player()).unwrap().trade_goods = 10;

        let mut window =
            ProductionWindow::new(&state, ContentStore::embedded(), POK, &player(), &system);
        assert!(
            window
                .pending_choice(&state, ContentStore::embedded(), POK)
                .is_some()
        );

        let mut table = Table::new();
        table.seat(player(), Box::new(Decline));
        let choice = window
            .pending_choice(&state, ContentStore::embedded(), POK)
            .unwrap();
        let answer = table.ask(&choice).unwrap();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut ctx = Resolving {
            content: ContentStore::embedded(),
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };

        window.resolve(&mut state, &mut ctx, answer).unwrap();

        assert!(matches!(window.stage, Stage::Done));
    }

    #[test]
    fn a_window_that_is_finished_owes_no_choice() {
        let (state, system, _) = seated();
        let window =
            ProductionWindow::new(&state, ContentStore::embedded(), POK, &player(), &system);
        // No producer, so nothing is owed and nothing is produced.
        assert!(
            window
                .pending_choice(&state, ContentStore::embedded(), POK)
                .is_none()
        );
        assert!(window.into_report().produced.is_empty());
    }

    #[test]
    fn a_system_with_no_producer_produces_nothing() {
        let (mut state, system, _) = seated();
        put(&mut state, &system, "cruiser", &player(), 3);
        let mut table = Table::new();

        let report = resolve(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            &player(),
            &system,
        )
        .unwrap();

        assert!(report.produced.is_empty());
    }

    #[test]
    fn production_places_units_and_charges_for_them() {
        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);
        state.player_mut(&player()).unwrap().trade_goods = 10;
        let before_goods = state.player(&player()).unwrap().trade_goods;
        let mut table = Table::new();

        let report = resolve(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            &player(),
            &system,
        )
        .unwrap();

        assert!(!report.produced.is_empty(), "the dock built something");
        let spent = before_goods > state.player(&player()).unwrap().trade_goods
            || !state.exhausted_planets.is_empty();
        assert!(spent, "and it was paid for");
    }
}
