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
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{PlanetId, PlayerId, SystemId, UnitTypeId};
use ti4_model::state::GameState;
use ti4_model::units::Unit;

use crate::choice::{Choice, ChoiceOption, IllegalChoice, Observed, Resolving, Table, Window};
use crate::decision_context::{
    ConstraintKind, DecisionContext, DecisionSource, DecisionTarget, OutstandingConstraint,
};
use crate::fleet::{Arrival, Standing};
use crate::preview::{Delta, Preview, Quantity};

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

const fn spend_constraint(kind: Spend) -> ConstraintKind {
    match kind {
        Spend::Resources => ConstraintKind::Resources,
        Spend::Influence => ConstraintKind::Influence,
    }
}

const fn spend_quantity(kind: Spend) -> Quantity {
    match kind {
        Spend::Resources => Quantity::Resources,
        Spend::Influence => Quantity::Influence,
    }
}

fn payment_context(
    state: &GameState,
    player: &PlayerId,
    kind: Spend,
    amount: i64,
    paid: i64,
) -> DecisionContext {
    DecisionContext::new(
        player.clone(),
        DecisionSource::Rule("34.3/75.2/75.3".to_owned()),
        format!("pay_{}", spend_name(kind)),
        state.phase,
        state.round,
    )
    .owing(OutstandingConstraint::new(
        spend_constraint(kind),
        amount,
        paid,
    ))
}

/// The choice kind for exhausting a planet to pay.
pub const PAY_KIND: &str = "pay";
/// The choice kind for producing one unit.
pub const PRODUCE_KIND: &str = "produce";
/// The choice kind for placing a produced unit.
pub const PLACE_KIND: &str = "place";
/// The id standing for a system's space area.
pub const SPACE: &str = "space";

/// Separates the system from the spot in a placement outside the producing system
/// (`"<system>@<planet or space>"`), offered by `EconomyHooks::production_destinations`.
pub const REMOTE_SEPARATOR: char = '@';

/// A placement spot split into the system it is in and the spot there: `(system, spot)` for a
/// remote spot, `(producing, spot)` otherwise.
#[must_use]
pub fn placement_target<'s>(producing: &SystemId, spot: &'s str) -> (SystemId, &'s str) {
    match spot.split_once(REMOTE_SEPARATOR) {
        Some((system, at)) => (SystemId::new(system), at),
        None => (producing.clone(), spot),
    }
}

/// What a placement leaves of the two limits it can spend (LRR 37, 16).
///
/// Headroom is signed and free capacity is not. Production places units first and the limits are
/// enforced afterwards, so a negative headroom is a position a seat can genuinely reach and its
/// magnitude is what will be taken off the board; free capacity has no such reading, because what
/// does not fit is excess rather than negative room.
fn limit_deltas(before: &Standing, after: &Standing) -> [Delta; 2] {
    [
        Delta::new(
            Quantity::FleetSupplyHeadroom,
            before.fleet_headroom(),
            after.fleet_headroom(),
        ),
        Delta::new(
            Quantity::CapacityFree,
            before.capacity_free(),
            after.capacity_free(),
        ),
    ]
}

/// The exact fleet and transport aftermath of one placement, as option payload facts.
///
/// Separate from [`limit_deltas`] because a preview states the change while these state the exact
/// pre-enforcement violations the placement reaches.
///
/// Enforcement removes fleet-supply candidates first, and that removal can change the capacity
/// answer. It also asks the owner which unit to remove. A sum of the independent excesses is
/// therefore not a truthful prediction of units that will leave the board; these facts deliberately
/// name the violations rather than inventing such a prediction.
fn placement_facts(option: ChoiceOption, before: &Standing, after: &Standing) -> ChoiceOption {
    option
        .with("capacity_used", after.consumed - before.consumed)
        .with("fleet_headroom_after", after.fleet_headroom())
        .with("capacity_free_after", after.capacity_free())
        .with("fleet_excess_after", after.fleet_excess())
        .with("capacity_excess_after", after.capacity_excess)
}

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
        + attachment_bonus(state, content, planet, kind)
        + match kind {
            // The Last Bastion's Helios dock raises the resource value of its own planet.
            Spend::Resources => crate::factions::bastion_units::resource_bonus(state, planet),
            Spend::Influence => 0,
        }
}

/// The faction mark holding the [`GameState::production_seq`] a value swap was declared in.
const SWAP_SEQ_MARK: &str = "production:value_swap_seq";

/// Record that, for the current use of PRODUCTION, `planet`'s resource and influence values are
/// swapped (Winnu Hegemonic Trade Policy). Replaces any earlier swap. The swap is scoped to the
/// use it was declared in: it is read only while [`GameState::production_seq`] is unchanged, and
/// it is cleared when the production window ends ([`end_value_swap`]).
///
/// Hegemonic Trade Policy: "Exhaust this card when 1 or more of your units use PRODUCTION; swap
/// the resource and influence values of 1 planet you control during that use of Production."
/// Call it from the module's `PRODUCTION_USED` window (after `production_seq` was advanced for
/// the use, before the window's `refresh`), having exhausted the card and chosen the planet. The
/// module's `planet_spend_value` hook then returns [`swapped_value`] for that planet.
pub fn begin_value_swap(state: &mut GameState, planet: &PlanetId) {
    state.production_value_swapped_planet = Some(planet.clone());
    state
        .faction_marks
        .insert(SWAP_SEQ_MARK.to_owned(), state.production_seq.to_string());
}

/// End the current value swap, if any (the use of PRODUCTION is over).
pub fn end_value_swap(state: &mut GameState) {
    state.production_value_swapped_planet = None;
    state.faction_marks.remove(SWAP_SEQ_MARK);
}

/// The planet whose values are swapped in the use of PRODUCTION now in progress, if any.
#[must_use]
pub fn swapped_planet(state: &GameState) -> Option<&PlanetId> {
    let planet = state.production_value_swapped_planet.as_ref()?;
    let declared = state.faction_marks.get(SWAP_SEQ_MARK)?;
    (*declared == state.production_seq.to_string()).then_some(planet)
}

/// What `planet` reads as `kind` for this player right now if it is the planet whose values are
/// swapped: the *other* kind's value (attachments and laws included, as [`planet_value_now`]). `None`
/// when `planet` is not the swapped planet, so a module's `planet_spend_value` hook can write
/// `swapped_value(..).unwrap_or(value)`. It does not check who controls the planet or holds the
/// card: the module that began the swap does.
#[must_use]
pub fn swapped_value(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    planet: &PlanetId,
    kind: Spend,
) -> Option<i64> {
    if swapped_planet(state) != Some(planet) {
        return None;
    }
    let other = match kind {
        Spend::Resources => Spend::Influence,
        Spend::Influence => Spend::Resources,
    };
    Some(planet_value_now(state, content, sources, planet, other))
}

/// What the attachments on a planet add to it (LRR 35.8): each attachment record's
/// `resourcesModifier` / `influenceModifier`, summed.
///
/// Exploration attachments (Dyson Sphere, Mining World, a research facility on a planet that
/// already has a specialty, ...) and Nano-Forge all land in `planet_attachments` by attachment id,
/// so one reading of the content covers them. Nano-Forge used to be a special case here; it is now
/// just the `nanoforge` record's +2/+2. An id the content does not know adds nothing.
#[must_use]
pub fn attachment_bonus(
    state: &GameState,
    content: &ContentStore,
    planet: &PlanetId,
    kind: Spend,
) -> i64 {
    let key = match kind {
        Spend::Resources => "resourcesModifier",
        Spend::Influence => "influenceModifier",
    };
    state.planet_attachments.get(planet).map_or(0, |attached| {
        attached
            .iter()
            .filter_map(|id| content.get(ContentType::Attachments, id))
            .filter_map(|record| record.int(key))
            .sum()
    })
}

/// Controlled planets whose cards are still readied (LRR 34, 75.2).
#[must_use]
pub fn spendable_planets(state: &GameState, player: &PlayerId) -> Vec<PlanetId> {
    state
        .controlled_planets(player)
        .into_iter()
        .map(|(_, planet)| planet.clone())
        .filter(|planet| !state.exhausted_planets.contains(planet))
        // The Deepwrought's ocean cards are planet cards off the map (1 resource, 1 influence).
        .chain(crate::factions::deepwrought::spendable_oceans(
            state, player,
        ))
        .collect()
}

/// What one trade good is worth as payment: two with the `mc` technology, else one.
///
/// Oracle parity (`engine/production.py`): the multiplier applies in `available()` and in
/// every step of the payment loop.
#[must_use]
pub fn trade_good_worth(state: &GameState, player: &PlayerId) -> i64 {
    // Mentak `mc` lives in `factions/mentak.rs` on the hook below; nothing is hard-coded here.
    let base = 1;
    crate::factions::hooks_economy::trade_good_worth(state, player, base)
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
        5 * i64::try_from(
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
    }) + crate::factions::keleres::spendable_commodities(state, player)
        * trade_good_worth(state, player);
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
    //
    // `planet_spend_value` is the faction hook for a value that changes what the planet pays for
    // this player (Winnu Hegemonic Trade Policy swaps resources and influence). Applied per face,
    // to the value that face reads, so the swap reaches the alternate face below as well.
    let face = |kind: Spend| {
        crate::factions::hooks_economy::planet_spend_value(
            state,
            content,
            player,
            planet,
            kind,
            planet_value_now(state, content, sources, planet, kind),
        )
    };
    let ordinary = if crate::leaders::combines_planet_values(state, player) {
        face(Spend::Resources) + face(Spend::Influence)
    } else {
        face(kind)
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
    let freelancers = kind == Spend::Resources && state.influence_pays_for_units.contains(player);
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
    let alternate = face(alternate_kind);
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
    let pool_before = available(state, content, sources, player, kind);
    let goods_before = state
        .player(player)
        .map_or(0, |seat| i64::from(seat.trade_goods));
    // Goods capacity under the guard uses the `mc`-multiplied worth (engine/production.py pay()).
    // Xander Alexin Victori III (Keleres agent): commodities the player may spend as trade goods.
    let commodities = crate::factions::keleres::spendable_commodities(state, player);
    let goods_capacity = state.player(player).map_or(0, |seat| {
        i64::from(seat.trade_goods) * trade_good_worth(state, player)
    }) + commodities * trade_good_worth(state, player);
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
            // The spendable pool is the best legal face of every ready card. Exhausting this
            // planet removes that whole best face even when the actor deliberately selects a
            // smaller Archon's Gift face. The debt, separately, falls by `worth`.
            let removed_from_pool = max_face_value(state, content, sources, player, planet, kind);
            options.push(
                ChoiceOption::labelled(id, PAY_KIND, label)
                    .with("worth", worth)
                    .with("owed", cost - paid)
                    .with("kind", spend_name(kind))
                    .with("source", spend_name(source))
                    .previewed(Preview::certain(vec![
                        Delta::new(
                            spend_quantity(kind),
                            pool_before,
                            pool_before - removed_from_pool,
                        ),
                        Delta::new(Quantity::TradeGoods, goods_before, goods_before),
                    ])),
            );
        }
    }
    let goods_held = state
        .player(player)
        .map_or(0, |seat| i64::from(seat.trade_goods));
    if goods_held > 0 {
        let worth = trade_good_worth(state, player);
        options.push(
            ChoiceOption::labelled("trade_good", PAY_KIND, "spend a trade good")
                .with("worth", worth)
                .with("owed", cost - paid)
                .with("kind", spend_name(kind))
                .previewed(Preview::certain(vec![
                    Delta::new(spend_quantity(kind), pool_before, pool_before - worth),
                    Delta::new(Quantity::TradeGoods, goods_before, goods_before - 1),
                ])),
        );
    }
    if commodities > 0 {
        let worth = trade_good_worth(state, player);
        options.push(
            ChoiceOption::labelled("commodity", PAY_KIND, "spend a commodity as a trade good")
                .with("worth", worth)
                .with("owed", cost - paid)
                .with("kind", spend_name(kind))
                .previewed(Preview::certain(vec![Delta::new(
                    spend_quantity(kind),
                    pool_before,
                    pool_before - worth,
                )])),
        );
    }
    options
}

/// The ready planets that pay `kind` in their own currency, each with the worth of that face,
/// exactly as the payment loop would list them for an unbounded bill. Cross-source faces
/// (Archon's Gift) are left out.
pub fn native_payment_planets(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    kind: Spend,
) -> Vec<(PlanetId, i64)> {
    spendable_planets(state, player)
        .into_iter()
        .filter_map(|planet| {
            payment_faces(state, content, sources, player, &planet, kind)
                .into_iter()
                .find(|(source, _)| *source == kind)
                .map(|(_, worth)| (planet, worth))
        })
        .collect()
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
    if answer.id == "commodity" {
        // Keleres agent: a commodity spent as if it were a trade good.
        if crate::factions::keleres::spendable_commodities(state, player) <= 0 {
            return None;
        }
        let worth = trade_good_worth(state, player);
        let seat = state.player_mut(player)?;
        seat.commodities -= 1;
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
        .unwrap_or_else(|| planet_value_now(state, content, sources, &planet, source));
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

/// Spend as one instalment of a larger bill, retaining any overpayment for its next instalment.
///
/// Leadership is a single "spend any amount of influence" transaction that awards one command
/// token per three influence. A four-influence planet therefore pays the first three and leaves
/// one toward the next token; treating each token as a separate bill incorrectly charges seven
/// printed influence for two tokens instead of six.
///
/// The credit belongs to the caller and must not escape the enclosing transaction.
#[allow(
    clippy::too_many_arguments,
    reason = "payment needs the rules position, observation, and transaction-local credit"
)]
pub(crate) fn pay_seeing_with_credit(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    cost: i64,
    kind: Spend,
    credit: &mut i64,
) -> Result<bool, IllegalChoice> {
    pay_offering_agent(
        state, content, sources, table, player, cost, kind, credit, galaxy,
    )
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
    let mut credit = 0;
    pay_offering_agent(
        state,
        content,
        sources,
        table,
        player,
        cost,
        kind,
        &mut credit,
        galaxy,
    )
}

/// The shared payment window every `pay*` variant goes through.
///
/// Xander Alexin Victori III (Keleres agent) is offered once here, to the holder, when the payer's
/// commodities would make the bill payable; its permission ends with the payment. A payment made
/// inside a larger window the agent was already used for (Leadership's purchase loop) does not offer
/// it again.
#[allow(
    clippy::too_many_arguments,
    reason = "payment needs the rules position, observation, and transaction-local credit"
)]
fn pay_offering_agent(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut Table,
    player: &PlayerId,
    cost: i64,
    kind: Spend,
    credit: &mut i64,
    galaxy: Option<&Galaxy>,
) -> Result<bool, IllegalChoice> {
    let owed = cost - (*credit).min(cost.max(0));
    let opened = owed > 0
        && crate::factions::keleres::with_agent_granted(state, player, |granted| {
            available(granted, content, sources, player, kind) >= owed
        })
        .unwrap_or(false)
        && crate::factions::keleres::offer_agent(state, content, sources, galaxy, table, player)?;
    let paid = pay_with_observation_credit(
        state, content, sources, table, player, cost, kind, credit, galaxy,
    );
    if opened {
        crate::factions::keleres::close_agent_window(state, player);
    }
    paid
}

#[allow(
    clippy::too_many_arguments,
    reason = "payment needs the rules position, observation, and transaction-local credit"
)]
fn pay_with_observation_credit(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut Table,
    player: &PlayerId,
    cost: i64,
    kind: Spend,
    credit: &mut i64,
    galaxy: Option<&Galaxy>,
) -> Result<bool, IllegalChoice> {
    let used_credit = (*credit).min(cost.max(0));
    let owed = cost - used_credit;
    if owed <= 0 {
        *credit -= used_credit;
        return Ok(true);
    }
    if available(state, content, sources, player, kind) < owed {
        return Ok(false);
    }

    let mut paid = 0;
    while paid < owed {
        // Oracle parity (engine/production.py pay()): spendable planets first — every face that,
        // taken now, would not strand the rest of the bill — then trade goods, never guarded.
        let options = payment_options(state, content, sources, player, kind, paid, owed);
        if options.is_empty() {
            return Ok(false);
        }

        // The oracle takes a lone option without asking; only real choices reach a decider.
        let answer = {
            // Oracle wording: each iteration names the remaining debt and its kind
            // (`pay {cost - paid} more {kind}` in engine/production.py).
            let choice = Choice::new(
                player.clone(),
                format!("pay {} more {}", owed - paid, spend_name(kind)),
                options,
            )
            .contextualized(payment_context(
                state,
                player,
                kind,
                cost,
                used_credit + paid,
            ));
            if let Some(only) = table.auto_resolve(&choice, "it was the only way left to pay") {
                only
            } else {
                table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?
            }
        };

        match apply_payment_option(state, content, sources, player, kind, &answer) {
            Some(worth) => paid += worth,
            None => return Ok(false), // an id no offered option carries (unreachable after validate)
        }
    }
    *credit = *credit - used_credit + paid - owed;
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
        // A space dock in the space area (Saar's Floating Factory) is a space dock too.
        let has_dock = !dock_planets.is_empty()
            || board.units.iter().any(|unit| {
                unit.owner == *player
                    && types.get(unit.type_id.as_str()).is_some_and(|kind| {
                        kind.base_type() == "spacedock" && kind.is_space_only_structure()
                    })
            });
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
    let mut budget = planet_value_now(state, content, sources, planet, Spend::Resources);
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
        if made > 0 {
            crate::supply::stage_naaz_mech_placed(state, player, system, &UnitTypeId::new(id));
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
    produce_one_from(state, content, sources, galaxy, table, player, system, None)
}

/// The effect a single-unit production comes from, for the prompt and the decision context:
/// `(content alias, printed name)`, e.g. `("frln1", "Freelancers")`.
type ProduceOneSource<'a> = Option<(&'a str, &'a str)>;

/// The context subtype of the "which unit" question a named single-unit production asks.
pub const PRODUCE_ONE_SUBTYPE: &str = "produce_one_unit";

/// [`produce_one`], naming the effect that grants it.
///
/// Every single-unit production is a "may" (Chaos Mapping, Minister of Industry, Freelancers), so
/// the unit question always ends with a decline: answering it produces nothing and pays nothing.
/// It is appended last, so the ids and positions of the unit options do not move.
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "single-unit production needs the complete observed rules position"
)]
fn produce_one_from(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    system: &SystemId,
    source: ProduceOneSource<'_>,
) -> Result<bool, IllegalChoice> {
    let types = catalogue(content, sources);
    let affordable = available(state, content, sources, player, Spend::Resources);
    // Units the player could build in general but not here, with the first thing in the way. Shown
    // beside the question (`details.not_offered`) so a short list is not a mystery; it never
    // changes the options or their ids.
    let mut not_offered: Vec<serde_json::Value> = Vec::new();
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
            let blocked = if cost > affordable {
                Some(format!("costs {cost}, you can pay {affordable}"))
            } else if placements(state, content, sources, player, system, kind).is_empty() {
                Some("no place to put it in this system (ground forces need a planet you control)".to_owned())
            } else if !within_fleet_supply {
                Some("the fleet supply in this system is full".to_owned())
            } else if crate::supply::allowed(state, content, sources, player, &UnitTypeId::new(&id), 1)
                == 0
            {
                Some("none left in your reinforcements".to_owned())
            } else {
                None
            };
            match blocked {
                None => Some((id, cost)),
                Some(reason) => {
                    not_offered.push(serde_json::json!({ "unit": id, "reason": reason }));
                    None
                }
            }
        })
        .collect();
    if candidates.is_empty() {
        return Ok(false);
    }
    let mut options: Vec<ChoiceOption> = candidates
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
        .collect();
    options.push(ChoiceOption::decline());
    let prompt = match source {
        Some((_, name)) => format!("{name}: produce one unit in {system}"),
        None => format!("produce one unit in {system}"),
    };
    let mut choice = Choice::new(player.clone(), prompt, options);
    if !not_offered.is_empty() {
        choice = choice.detailed("not_offered", serde_json::Value::Array(not_offered));
    }
    if let Some((alias, _)) = source {
        choice = choice.contextualized(
            DecisionContext::new(
                player.clone(),
                DecisionSource::Content(alias.to_owned()),
                PRODUCE_ONE_SUBTYPE,
                state.phase,
                state.round,
            )
            .optional(true)
            .about(DecisionTarget::System(system.clone())),
        );
    }
    let answer = table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
    if answer.is_decline() {
        return Ok(false);
    }
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
    let unit = Unit::new(UnitTypeId::new(&id), player.clone());
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
    crate::supply::stage_naaz_mech_placed(state, player, system, &UnitTypeId::new(id));
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
    card: &str,
) -> bool {
    state.influence_pays_for_units.insert(player.clone());
    let made = produce_one_from(
        state,
        ctx.content,
        ctx.sources,
        None,
        ctx.table,
        player,
        system,
        Some((card, "Freelancers")),
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
        // Miniaturization: a Ral Nel structure in the space area cannot use PRODUCTION.
        .filter(|unit| {
            produces(unit)
                && !crate::factions::ralnel::silenced_in_space(state, content, sources, unit)
        })
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

/// The player's mobile production structures in `system`: space-only structures with PRODUCTION
/// (`isSpaceOnly` in the content), sitting in the space area. Saar's Floating Factory.
///
/// Already included in [`producers`] (with no planet, so its printed flat value applies) and
/// therefore in [`capacity`] and every production window; this names them for the callers that
/// treat them specially (blockade, movement). Empty unless a faction content flags a unit so.
///
/// Floating Factory I: "This unit is placed in the space area instead of on a planet. This unit
/// can move and retreat as if it were a ship. If this unit is blockaded, it is destroyed."
#[must_use]
pub fn mobile_docks(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> Vec<Unit> {
    let types = catalogue(content, sources);
    state
        .board
        .get(system)
        .map(|board| {
            board
                .units
                .iter()
                .filter(|unit| &unit.owner == player)
                .filter(|unit| {
                    types
                        .get(unit.type_id.as_str())
                        .is_some_and(|kind| kind.is_space_only_structure() && kind.has_production())
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// Destroy every mobile dock in `system` that is blockaded: an enemy ship is present and the
/// dock's owner has no ship there. Returns the docks destroyed. Their plastic goes back to the
/// owner's reinforcements (the board no longer holds it), and nothing else changes; the caller
/// announces any destruction event it wants.
///
/// Floating Factory I: "If this unit is blockaded, it is destroyed." Blockade here is the
/// space-dock blockade of the LRR (enemy ships and none of your own); that is a different test
/// from the 68.10 production ban above, which applies whatever ships the producer has. The
/// caller decides *when* to look (a moved-into system, the end of a combat); the Floating Factory
/// does not carry a printed timing, so this is a rules question recorded in the evidence file.
/// Neutral units' ships count as enemy ships.
pub fn destroy_blockaded_mobile_docks(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
) -> Vec<Unit> {
    let types = catalogue(content, sources);
    let Some(board) = state.board.get(system) else {
        return Vec::new();
    };
    let is_ship = |unit: &Unit| {
        types
            .get(unit.type_id.as_str())
            .is_some_and(UnitType::is_ship)
    };
    let owners: std::collections::BTreeSet<PlayerId> = board
        .units
        .iter()
        .filter(|unit| {
            types
                .get(unit.type_id.as_str())
                .is_some_and(|kind| kind.is_space_only_structure() && kind.has_production())
        })
        .map(|unit| unit.owner.clone())
        .collect();
    let doomed: Vec<Unit> = owners
        .into_iter()
        .filter(|owner| {
            let enemy_ship = board
                .units
                .iter()
                .any(|unit| &unit.owner != owner && is_ship(unit));
            let own_ship = board
                .units
                .iter()
                .any(|unit| &unit.owner == owner && is_ship(unit));
            enemy_ship && !own_ship
        })
        .flat_map(|owner| {
            board
                .units
                .iter()
                .filter(|unit| {
                    unit.owner == owner
                        && types.get(unit.type_id.as_str()).is_some_and(|kind| {
                            kind.is_space_only_structure() && kind.has_production()
                        })
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .collect();
    if !doomed.is_empty() {
        state.destroy_units(system, &doomed);
    }
    doomed
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
                // The hook sees the planet as this player's units read it: Hegemonic Trade
                // Policy's swap turns a PRODUCTION-by-resources dock into PRODUCTION-by-influence.
                crate::factions::hooks_economy::planet_spend_value(
                    state,
                    content,
                    player,
                    &planet,
                    Spend::Resources,
                    planet_value_now(state, content, sources, &planet, Spend::Resources),
                )
            });
            Some(kind.production(resources))
        })
        .sum::<i64>()
        + war_machine_bonus(state, player)
        + module_production(state, content, sources, player, system)
}

/// PRODUCTION modules grant `system` as if from a unit (`EconomyHooks::extra_production` for the
/// space area, `extra_production_planet` for each planet holding any of the player's units there).
/// Zero with no such hook. Muaat Magmus Reactor, Creuss Particle Synthesis, Argent Hololattice.
#[must_use]
pub fn module_production(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> i64 {
    crate::factions::hooks_economy::extra_production(state, content, sources, player, system).max(0)
        + module_planet_producers(state, content, sources, player, system)
            .into_iter()
            .map(|(_, value)| value)
            .sum::<i64>()
}

/// The planets of `system` a module makes a producer for `player`, with each one's PRODUCTION.
fn module_planet_producers(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> Vec<(PlanetId, i64)> {
    // Planets holding units, and planets the module may value without any (Keleres, Custodian's
    // Favour: Mecatol Rex gains PRODUCTION 3 while its controller holds Custodia Vigilia, units or
    // not). Each hook decides for itself and answers zero for a planet that is not its own.
    let here = state.system_state(system);
    let planets: std::collections::BTreeSet<PlanetId> = here
        .planet_units
        .keys()
        .chain(here.planet_control.keys())
        .cloned()
        .collect();
    planets
        .into_iter()
        .filter_map(|planet| {
            let value = crate::factions::hooks_economy::extra_production_planet(
                state, content, sources, player, system, &planet,
            );
            (value > 0).then_some((planet, value))
        })
        .collect()
}

/// The PRODUCTION value of the player's producers in `system` that a faction module bars from
/// producing `kind` (`cannot_produce`); zero with no such hook. Read the same way as [`capacity`]
/// (planet resources through `planet_spend_value`), without War Machine.
#[must_use]
pub fn barred_capacity(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    kind: &UnitType<'_>,
) -> i64 {
    if !crate::factions::hooks_economy::has_cannot_produce() {
        return 0;
    }
    let types = catalogue(content, sources);
    producers(state, content, sources, player, system)
        .into_iter()
        .filter_map(|(unit, planet)| {
            let producer = types.get(unit.type_id.as_str())?;
            if !crate::factions::hooks_economy::cannot_produce(
                state,
                content,
                player,
                kind.base_type(),
                producer.base_type(),
            ) {
                return None;
            }
            let resources = planet.map_or(0, |planet| {
                crate::factions::hooks_economy::planet_spend_value(
                    state,
                    content,
                    player,
                    &planet,
                    Spend::Resources,
                    planet_value_now(state, content, sources, &planet, Spend::Resources),
                )
            });
            Some(producer.production(resources))
        })
        .sum()
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
    // Quietus: PRODUCTION is a unit ability, and a unit in an active breach beside another player's
    // Quietus has lost every unit ability, so its space dock produces nothing.
    if crate::factions::crimson::abilities_lost(state, player, system) {
        return Vec::new();
    }
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
    // A module may bar a producer from making this unit (Arborec Mitosis: space docks cannot
    // produce infantry). Filtered before anything reads the producers, so a planet whose only
    // producer is barred is not a spot at all.
    let types = catalogue(content, sources);
    let made: Vec<(Unit, Option<PlanetId>)> = producers(state, content, sources, player, system)
        .into_iter()
        .filter(|(unit, _)| {
            let producer = types
                .get(unit.type_id.as_str())
                .map_or(unit.type_id.as_str(), |found| found.base_type());
            !crate::factions::hooks_economy::cannot_produce(
                state,
                content,
                player,
                kind.base_type(),
                producer,
            )
        })
        .collect();
    let module_planets: Vec<PlanetId> =
        module_planet_producers(state, content, sources, player, system)
            .into_iter()
            .map(|(planet, _)| planet)
            .collect();
    let mut spots: Vec<String> = made
        .iter()
        .filter_map(|(_, planet)| planet.clone())
        .chain(module_planets)
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
    if made.iter().any(|(_, planet)| planet.is_none())
        || crate::factions::hooks_economy::extra_production(state, content, sources, player, system)
            > 0
    {
        spots.push(SPACE.to_owned()); // 68.4
    }
    let mut seen = std::collections::BTreeSet::new();
    spots.retain(|spot| seen.insert(spot.clone()));
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
            // A faction whose sheet prints its own unit of this type (the Muaat's Prototype War
            // Sun) builds it from the start; its upgrade (`pws2`) does not end in the gate.
            let native =
                ti4_content::units::faction_unit(content, &faction, base, sources).is_some();
            if !has && !native {
                continue;
            }
        }
        // 90.7/90.8: a researched unit upgrade replaces the unit it covers, so what a player
        // builds is their upgraded version when they own one. Without this, Cruiser II was
        // researched and every cruiser still cost 2, moved 2 and carried nothing.
        let held: Vec<String> = owned
            .as_ref()
            .map(|techs| techs.iter().map(|tech| tech.as_str().to_owned()).collect())
            .unwrap_or_default();
        let chosen = if let Some(better) =
            ti4_content::units::unlocked_upgrade(content, sources, base, &faction, &held)
        {
            Some(better.id().to_owned())
        } else if let Some(own) = ti4_content::units::faction_unit(content, &faction, base, sources)
        {
            Some(own.id().to_owned())
        } else if !matches!(base, "mech" | "flagship") {
            Some(base.to_owned())
        } else {
            None
        };
        // A module may name another form for this base type (Mentak Corsair's acquisition).
        let overridden = crate::factions::hooks_strategy::unit_form_override(
            state,
            content,
            sources,
            player,
            base,
            chosen.as_deref().unwrap_or_default(),
        );
        if let Some(form) = overridden {
            out.push(form.as_str().to_owned());
        } else if let Some(id) = chosen {
            out.push(id);
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
        /// Full unit bill before production-use credit.
        cost: i64,
        /// Credit and payment faces already committed to this bill.
        paid: i64,
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
    /// Full PRODUCTION limit for this use, after pre-production reactions have settled.
    limit: i64,
    remaining: i64,
    stage: Stage,
    report: ProductionReport,
    /// Whether the end-of-use effects have fired, so they fire once per use rather than once per
    /// path that reaches `Done`.
    settled: bool,
    /// Production limit that capacity ships have opened for small units this use (Sol's Bellum
    /// Gloriosum). Spent by fighters and ground forces, and never below zero.
    free_capacity: i64,
    /// Cabal commander: at most two fighters or infantry avoid this use's production limit.
    small_unit_exemptions: i64,
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
    /// Resource-cost discount still available against this use's combined bill (Sarween Tools,
    /// AI Development Algorithm). Distinct from `credit`: this reduces what a build costs before
    /// any payment is collected, rather than pre-paying it.
    ///
    /// Read from [`GameState::production_discount_remaining`] in [`Self::refresh`] rather than
    /// computed here, because both sources are decided by [`crate::technology::production_used`]
    /// before this window's first choice — Sarween Tools automatically, AI Development Algorithm
    /// by an ask this window has no table to make. `new` starts it at zero so a caller that never
    /// calls `refresh` (every test that opens a window directly) sees no discount it never asked
    /// for.
    discount_remaining: i64,
    /// Harrugh Gefhara: every build this use costs nothing, once the leader's `ACTION` has been
    /// paid by purging it.
    ///
    /// True only when the marker in [`GameState::free_production_use`] names *this* production's
    /// sequence number, so a marker left over from a leader used earlier for a since-finished
    /// production cannot make a later, unrelated one free.
    free_this_use: bool,
    /// Produced by an ability rather than by a unit's PRODUCTION (see [`Self::for_ability`]).
    ability: bool,
    /// A fixed production limit that replaces the system's printed capacity (ability production
    /// only). `None` reads [`capacity`].
    fixed_limit: Option<i64>,
    /// Highest printed cost of one unit this use may produce (Muaat Umbat: "4 or less"). `None`
    /// is no cap. Applied in [`Self::build_options`].
    max_unit_cost: Option<i64>,
    /// The only unit type this use may produce (Nekro `nekroc4y`: "a ship of the same type").
    /// `None` is any. Applied in [`Self::build_options`].
    only_unit: Option<String>,
    /// Whether the current placement batch has opened its pre-placement ground-force window.
    placement_timing_done: bool,
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
        Self::open(state, content, sources, player, system, None, false)
    }

    /// Open production driven by an *ability* rather than by the PRODUCTION of units in the
    /// system (Arborec flagship "produce up to 5 units in this system", commander "produce 1
    /// unit in that system", hero "produce any number of units in any number of systems that
    /// contain 1 or more of your ground forces").
    ///
    /// The same window, so it reuses the whole production flow unchanged: unit offers,
    /// affordability, payment faces and credit, the plastic cap (31.4), fleet and structure
    /// limits, the blockade rule (68.10) and the one-bill-per-use accounting. What differs:
    ///
    /// * `limit` is the number of units the ability allows (`Some(5)`), replacing the system's
    ///   PRODUCTION total; `None` uses the printed capacity of units in the system.
    /// * Ground forces may be placed on any planet in the system the player controls (not only
    ///   on a planet holding a producer), since the ability, not a unit, produces them.
    ///   Structures still obey 79.2 and Demilitarized Zone. This reading is recorded as a rules
    ///   question in `plans/evidence/BF-00b-economy.md`.
    /// * The `UNITS_PRODUCED` event reports `source: "ability"`.
    ///
    /// It does **not** fire `PRODUCTION_USED` or the tactical-action extras (Sarween Tools,
    /// AI Development Algorithm, War Machine, Harrugh Gefhara): whether an ability's production is
    /// "a use of PRODUCTION" is a rules question the caller decides by emitting those first.
    #[must_use]
    pub fn for_ability(
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
        player: &PlayerId,
        system: &SystemId,
        limit: Option<i64>,
    ) -> Self {
        Self::open(state, content, sources, player, system, limit, true)
    }

    fn open(
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
        player: &PlayerId,
        system: &SystemId,
        fixed_limit: Option<i64>,
        ability: bool,
    ) -> Self {
        let remaining = fixed_limit
            .unwrap_or_else(|| capacity(state, content, sources, player, system))
            .max(0);
        Self {
            player: player.clone(),
            system: system.clone(),
            limit: remaining,
            remaining,
            stage: if remaining > 0 {
                Stage::Choosing
            } else {
                Stage::Done
            },
            report: ProductionReport::default(),
            settled: false,
            free_capacity: 0,
            small_unit_exemptions: if crate::promissory::has_commander_ability(
                state,
                player,
                "cabalcommander",
            ) {
                2
            } else {
                0
            },
            credit: 0,
            // Faction cost reductions (Particle Synthesis, Hololattice) are part of the
            // combined-bill discount pool.
            discount_remaining: crate::factions::hooks_economy::production_cost_reduction(
                state, player, system,
            ),
            free_this_use: false,
            ability,
            fixed_limit,
            max_unit_cost: None,
            only_unit: None,
            placement_timing_done: false,
        }
    }

    /// Only units whose printed cost is at most `max` may be offered (BF-F3; Muaat Umbat "each have a
    /// cost of 4 or less"). Builder for [`Self::for_ability`].
    #[must_use]
    pub fn with_max_unit_cost(mut self, max: Option<i64>) -> Self {
        self.max_unit_cost = max;
        self
    }

    /// Only this unit type may be offered (Nekro `nekroc4y`). Builder for [`Self::for_ability`].
    #[must_use]
    pub fn with_only_unit(mut self, unit: Option<String>) -> Self {
        self.only_unit = unit;
        self
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
        self.limit = self
            .fixed_limit
            .unwrap_or_else(|| capacity(state, content, sources, &self.player, &self.system))
            .max(0);
        self.remaining = self.limit;
        self.small_unit_exemptions =
            if crate::promissory::has_commander_ability(state, &self.player, "cabalcommander") {
                2
            } else {
                0
            };
        self.discount_remaining = i64::from(state.production_discount_remaining)
            + crate::factions::hooks_economy::production_cost_reduction(
                state,
                &self.player,
                &self.system,
            );
        self.free_this_use = state
            .player(&self.player)
            .is_some_and(|seat| seat.free_production_use == Some(state.production_seq));
        self.stage = if self.remaining > 0 {
            Stage::Choosing
        } else {
            Stage::Done
        };
    }

    /// Where this window may place `kind`: [`placements`], except that ability production puts
    /// ground forces on any planet in the system the player controls.
    fn spots(
        &self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
        kind: UnitType<'_>,
    ) -> Vec<String> {
        let mut spots = self.local_spots(state, content, sources, kind);
        // A module may let the unit go elsewhere, but only a unit this system can produce at all
        // (68.10: a blockaded dock makes no ships, wherever they would be placed).
        if !spots.is_empty() {
            spots.extend(
                crate::factions::hooks_economy::production_destinations(
                    state,
                    content,
                    sources,
                    &self.player,
                    &self.system,
                    kind.base_type(),
                )
                .into_iter()
                .map(|(system, planet)| {
                    let at = planet.map_or_else(|| SPACE.to_owned(), |planet| planet.to_string());
                    format!("{system}{REMOTE_SEPARATOR}{at}")
                }),
            );
            if kind.is_ship() {
                // The Last Bastion's Icon: "place those ships in a system that contains 1 of your
                // command tokens, ...". Shapes the ship spots over the whole use.
                crate::factions::bastion::adjust_ship_spots(
                    state,
                    content,
                    sources,
                    &self.player,
                    &self.system,
                    &self.report.produced,
                    &mut spots,
                );
                // The Crimson hero: "You may place any of those ships onto this card."
                crate::factions::crimson_cards::offer_card_spot(state, &self.player, &mut spots);
            }
        }
        spots
    }

    /// [`Self::spots`] in the producing system only.
    fn local_spots(
        &self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
        kind: UnitType<'_>,
    ) -> Vec<String> {
        if !self.ability || kind.is_ship() {
            return placements(state, content, sources, &self.player, &self.system, &kind);
        }
        if !crate::entropic_scars::abilities_usable(content, sources, &self.system, None) {
            return Vec::new();
        }
        let mut spots: Vec<String> = state
            .controlled_planets(&self.player)
            .into_iter()
            .filter(|(system, _)| **system == self.system)
            .map(|(_, planet)| planet.clone())
            .filter(|planet| !crate::laws::planet_is_demilitarized(state, planet))
            .filter(|planet| {
                !ti4_content::galaxy::is_space_station(content, planet.as_str(), sources)
            })
            .filter(|planet| {
                structure_allowed(
                    state,
                    content,
                    sources,
                    &self.player,
                    planet,
                    kind.base_type(),
                )
            })
            .map(|planet| planet.to_string())
            .collect();
        spots.dedup();
        spots
    }

    /// Draw down the credit against a cost, returning what is still owed.
    fn spend_credit(&mut self, cost: i64) -> i64 {
        let used = self.credit.min(cost);
        self.credit -= used;
        cost - used
    }

    fn context(&self, state: &GameState) -> DecisionContext {
        DecisionContext::new(
            self.player.clone(),
            DecisionSource::Rule("68".to_owned()),
            "produce_unit",
            state.phase,
            state.round,
        )
        .about(DecisionTarget::System(self.system.clone()))
        .owing(OutstandingConstraint::new(
            ConstraintKind::ProductionCapacity,
            self.limit,
            (self.limit - self.remaining).max(0),
        ))
    }

    /// The production-limit and Bellum allowance after placing this batch.
    ///
    /// This is analytic: it mirrors the two independently meaningful arithmetic operations in
    /// [`Self::place`] without constructing a state or placing a unit. The unit's chosen location
    /// does not affect either quantity, so it is truthful before the later placement choice.
    fn post_build_constraints(
        &self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
        id: &str,
        made: usize,
    ) -> (i64, i64) {
        let count = i64::try_from(made).unwrap_or(i64::MAX);
        let kind = UnitTypeId::new(id);
        let granted = crate::breakthroughs::free_capacity_granted(
            state,
            content,
            sources,
            &self.player,
            &kind,
        ) * i64::from(made > 0);
        let free = if crate::breakthroughs::spends_free_capacity(content, sources, &kind) {
            count.min(self.free_capacity + granted)
        } else {
            0
        };
        let exempt = if matches!(kind.as_str(), "fighter" | "infantry")
            || catalogue(content, sources)
                .get(id)
                .is_some_and(|unit| matches!(unit.base_type(), "fighter" | "infantry"))
        {
            (count - free).min(self.small_unit_exemptions)
        } else {
            0
        };
        (
            self.remaining - (count - free - exempt),
            self.free_capacity + granted - free + self.small_unit_exemptions - exempt,
        )
    }

    /// Fleet supply and transport as they would stand with `count` copies of `kind` at `where_to`.
    ///
    /// Analytic. It asks [`crate::fleet::standing`] the question the end-of-turn enforcement will
    /// ask, with the unit counted as already there, and builds no state and places nothing. The
    /// two limits are answered together because Fighter II couples them: a ground force put in a
    /// space area can push fighters out of capacity and onto the fleet pool.
    fn standing_after(
        &self,
        types: &BTreeMap<&str, UnitType<'_>>,
        state: &GameState,
        content: &ContentStore,
        kind: UnitType<'_>,
        where_to: &str,
        count: i64,
    ) -> Standing {
        let (target, spot) = placement_target(&self.system, where_to);
        crate::fleet::standing_using(
            types,
            state,
            content,
            &self.player,
            &target,
            Some(Arrival {
                kind,
                count,
                in_space: spot == SPACE,
            }),
        )
    }

    /// Where to put what was just produced, and what each destination would leave behind.
    ///
    /// Returns nothing when there is one destination or none: the placement settles without a
    /// question, and its consequence was already stated on the build option that chose the unit.
    fn placement_choice(
        &self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
        id: &str,
        made: usize,
    ) -> Option<Choice> {
        let types = catalogue(content, sources);
        let kind = types.get(id)?;
        let spots = self.spots(state, content, sources, *kind);
        if spots.len() < 2 {
            return None; // settled without a question
        }
        let placed = i64::try_from(self.will_place(state, content, sources, id, made)).unwrap_or(0);
        let before =
            crate::fleet::standing_using(&types, state, content, &self.player, &self.system, None);
        Some(
            Choice::new(
                self.player.clone(),
                format!("place the {id}"),
                spots
                    .iter()
                    .map(|spot| {
                        let after =
                            self.standing_after(&types, state, content, *kind, spot, placed);
                        // A spot in another system is measured against that system's position.
                        let (target, _) = placement_target(&self.system, spot);
                        let remote_before = (target != self.system).then(|| {
                            crate::fleet::standing_using(
                                &types,
                                state,
                                content,
                                &self.player,
                                &target,
                                None,
                            )
                        });
                        let before = remote_before.as_ref().unwrap_or(&before);
                        placement_facts(
                            ChoiceOption::labelled(
                                format!("place|{spot}"),
                                PLACE_KIND,
                                format!("place on {spot}"),
                            )
                            .with("system", self.system.to_string())
                            .with("unit", id.to_owned())
                            .with("destination", spot.clone())
                            .with("count", i64::try_from(made).unwrap_or(1))
                            .with("placed", placed),
                            before,
                            &after,
                        )
                        .previewed(Preview::certain(limit_deltas(before, &after).to_vec()))
                    })
                    .collect(),
            )
            .contextualized(self.placement_context(state, &before)),
        )
    }

    /// Why a destination is being asked for, and what the two limits stand at while it is asked.
    ///
    /// Both constraints are reported as the position rather than as a bill: `amount` is the limit
    /// the system offers and `paid` is what is already spent against it, so `remaining` is the room
    /// a further unit could take. A seat already over a limit reports no room, which is true; how
    /// far over it is belongs to the per-option preview, where it differs by destination.
    fn placement_context(&self, state: &GameState, standing: &Standing) -> DecisionContext {
        DecisionContext::new(
            self.player.clone(),
            DecisionSource::Rule("68".to_owned()),
            "place_unit",
            state.phase,
            state.round,
        )
        .about(DecisionTarget::System(self.system.clone()))
        .owing(OutstandingConstraint::new(
            ConstraintKind::FleetSupply,
            standing.fleet_limit,
            standing.fleet_charged,
        ))
        .owing(OutstandingConstraint::new(
            ConstraintKind::TransportCapacity,
            standing.transport,
            standing.consumed,
        ))
    }

    /// What one build's printed cost becomes after this use's discount (Sarween Tools, AI
    /// Development Algorithm, Harrugh Gefhara).
    ///
    /// Returns `(effective_cost, discount_used)`. Read-only: spending the discount for real is
    /// [`Self::spend_discount`], called only for the option actually chosen. A preview built from
    /// this must therefore treat the discount as available to every offered option alike, exactly
    /// as [`Self::credit`] already does -- two build options shown in the same choice cannot both
    /// spend the same single point of discount, so each preview states what taking *that one*
    /// option would cost, not what taking all of them together would.
    fn discounted(&self, printed: i64) -> (i64, i64) {
        if self.free_this_use {
            return (0, printed);
        }
        let used = self.discount_remaining.min(printed);
        (printed - used, used)
    }

    /// A borrowed Nomad commander waives flagship resources independently of other discounts.
    fn discounted_unit(&self, state: &GameState, kind: UnitType<'_>, printed: i64) -> (i64, i64) {
        if kind.base_type() == "flagship"
            && crate::promissory::has_commander_ability(state, &self.player, "nomadcommander")
        {
            (0, 0)
        } else {
            self.discounted(printed)
        }
    }

    /// Spend the discount actually used by the option that was chosen.
    fn spend_discount(&mut self, used: i64) {
        if !self.free_this_use {
            self.discount_remaining -= used;
        }
    }

    /// The units that will actually arrive, which is not always the batch that was bought.
    ///
    /// 31.4 is applied when the unit is placed, so a preview that assumed the whole batch would
    /// state a consequence the box cannot deliver.
    fn will_place(
        &self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
        id: &str,
        made: usize,
    ) -> usize {
        crate::supply::allowed(
            state,
            content,
            sources,
            &self.player,
            &UnitTypeId::new(id),
            made,
        )
    }

    /// Options for what to build now: affordable, placeable, one per unit type.
    #[allow(
        clippy::too_many_lines,
        reason = "one linear filter-and-price pass per buildable unit"
    )]
    fn build_options(
        &self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
    ) -> Vec<ChoiceOption> {
        let types = catalogue(content, sources);
        // The position before any of these options is taken, read once for all of them.
        let before =
            crate::fleet::standing_using(&types, state, content, &self.player, &self.system, None);
        let spendable_resources =
            available(state, content, sources, &self.player, Spend::Resources);
        let mut options = Vec::new();
        for id in buildable_for(state, content, sources, &self.player) {
            let Some(kind) = types.get(id.as_str()) else {
                continue;
            };
            if self.only_unit.as_ref().is_some_and(|only| only != &id) {
                continue;
            }
            if self
                .max_unit_cost
                .is_some_and(|max| kind.cost() > f64::from(i32::try_from(max).unwrap_or(i32::MAX)))
            {
                continue;
            }
            // Ability production has no unit producer, but blanket bans still apply. A hook
            // specific to space docks (Mitosis) must not bar production granted by an ability.
            if self.ability
                && crate::factions::hooks_economy::cannot_produce(
                    state,
                    content,
                    &self.player,
                    kind.base_type(),
                    "ability",
                )
            {
                continue;
            }
            let (printed, pair) = price_of_under(Some(state), kind);
            let (cost, discount_used) = self.discounted_unit(state, *kind, printed);
            // Cabal Amalgamation: "When you produce a unit: You may return 1 captured unit of that
            // type to produce that unit without spending resources." The exchange is settled here,
            // before affordability is asked, because a unit bought by returning a captured model is
            // not bought with resources: a Cabal with nothing left to spend still has the option the
            // card gives it. The hook is pure -- it asks nothing and changes nothing.
            let exchange = crate::factions::hooks_economy::production_unit_exchange(
                state,
                content,
                sources,
                &self.player,
                &self.system,
                &id,
            );
            // Credit already paid counts towards affordability, or a build the player has in fact
            // paid for would be withheld as unaffordable. Affordability is judged on the
            // discounted bill: a unit Sarween Tools or Harrugh Gefhara brings within reach must
            // not be withheld for a price nobody would actually charge. A unit bought with a
            // captured model is not withheld at all, because no resource is being asked for it.
            let affordable = cost
                <= available(state, content, sources, &self.player, Spend::Resources) + self.credit;
            if !affordable && !exchange {
                continue;
            }
            let spots = self.spots(state, content, sources, *kind);
            if spots.is_empty() {
                continue;
            }
            // Producers barred from this unit (Mitosis: docks and infantry) do not lend it their
            // PRODUCTION. The most this unit can use is what is left of the total, but never more
            // than the total less the barred producers less what this use has already produced
            // of the same unit (other purchases are assumed to have drawn on the barred share
            // first, the most permissive reading).
            let barred = if self.ability {
                0
            } else {
                barred_capacity(state, content, sources, &self.player, &self.system, kind)
            };
            let usable = if barred == 0 {
                self.remaining
            } else {
                let types = catalogue(content, sources);
                let same_kind = self
                    .report
                    .produced
                    .iter()
                    .filter(|(produced, _)| {
                        types
                            .get(produced.as_str())
                            .is_some_and(|made| made.base_type() == kind.base_type())
                    })
                    .count();
                self.remaining
                    .min(self.limit - barred - i64::try_from(same_kind).unwrap_or(i64::MAX))
            };
            let extra = if matches!(kind.base_type(), "fighter" | "infantry") {
                self.small_unit_exemptions
            } else {
                0
            };
            let made = pair.min(usize::try_from(usable + extra).unwrap_or(0));
            if made == 0 {
                continue;
            }
            // 31.4, asked once for two answers: the batch the box can actually supply, and whether
            // the unit may be offered at all. Offering a unit with no plastic left would let a
            // player spend resources on something that cannot be placed. Every quantity below is
            // stated about the units that will arrive, not the batch that was bought -- and this
            // walks the whole board for the count, which is why it is not asked twice.
            let placed = self.will_place(state, content, sources, &id, made);
            if placed == 0 {
                continue;
            }
            let (remaining_after, free_capacity_after) =
                self.post_build_constraints(state, content, sources, &id, placed);
            // The paid build is offered only when it can actually be paid for. An unaffordable
            // unit reaches this point only through the exchange below, which charges nothing.
            if affordable {
                let credit_used = self.credit.min(cost);
                let production_spent = self.remaining - remaining_after;
                let mut deltas = vec![
                    Delta::new(
                        Quantity::ProductionRemaining,
                        self.remaining,
                        remaining_after,
                    ),
                    Delta::new(
                        Quantity::ProductionFreeCapacity,
                        self.free_capacity + self.small_unit_exemptions,
                        free_capacity_after,
                    ),
                ];
                let mut option = ChoiceOption::labelled(
                    format!("build|{id}|{made}"),
                    PRODUCE_KIND,
                    format!("produce {made}x {id} for {cost}"),
                )
                .with("cost", cost)
                .with("printed_cost", printed)
                .with("discount", discount_used)
                .with("count", i64::try_from(made).unwrap_or(1))
                .with("placed", i64::try_from(placed).unwrap_or(1))
                .with("yield", i64::try_from(pair).unwrap_or(1))
                .with("credit", self.credit)
                .with("available_resources", spendable_resources)
                .with("free_this_use", self.free_this_use)
                .with("credit_used", credit_used)
                .with("owed", cost - credit_used)
                .with("production_spent", production_spent)
                .with("unit", id.clone())
                .with("system", self.system.to_string());
                // A ship has one destination and no placement question follows, so its fleet and
                // transport aftermath is settled here. A unit with a choice of destinations does
                // not have one yet, and says so rather than reporting a consequence it cannot know.
                if let [only] = spots.as_slice() {
                    let after = self.standing_after(
                        &types,
                        state,
                        content,
                        *kind,
                        only,
                        i64::try_from(placed).unwrap_or(0),
                    );
                    deltas.extend(limit_deltas(&before, &after));
                    option =
                        placement_facts(option.with("destination", only.clone()), &before, &after);
                } else {
                    option =
                        option.with("placement_pending", i64::try_from(spots.len()).unwrap_or(2));
                }
                options.push(option.previewed(Preview::certain(deltas)));
            }
            // 68.3b -- "a player can choose to produce only one unit; however, they must still
            // pay the entire cost" -- is honoured where it matters and not offered where it does
            // not. When the production limit leaves room for one, `made` is already 1 above and
            // the full cost is charged, which is exactly the rule. Offering it *voluntarily*
            // alongside the pair adds a strictly dominated option to every fighter and infantry
            // purchase: same price, half the units. A decider gains nothing from being asked, and
            // a learner has to spend capacity discovering it is never right.
            //
            // Cabal Amalgamation: "When you produce a unit: You may return 1 captured unit of that
            // type to produce that unit without spending resources." A "when you" trigger has to be
            // a real option in the window, not a refusal after the fact, so the exchange is offered
            // beside the paid build and the player picks. One captured unit buys one unit. The
            // production limit is still spent, because the card waives the cost and not the limit:
            // the exchange ends in the same placing stage `place` charges capacity for.
            if exchange {
                let (limit_after, free_after) =
                    self.post_build_constraints(state, content, sources, &id, 1);
                let mut exchange = ChoiceOption::labelled(
                    format!("exchange|{id}"),
                    PRODUCE_KIND,
                    format!("produce 1x {id} by returning a captured {id}"),
                )
                .with("cost", 0i64)
                .with("printed_cost", printed)
                .with("discount", 0i64)
                .with("count", 1i64)
                .with("placed", 1i64)
                .with("credit", self.credit)
                .with("credit_used", 0i64)
                .with("owed", 0i64)
                .with("production_spent", self.remaining - limit_after)
                .with("unit", id.clone())
                .with("system", self.system.to_string())
                .with("exchange", true);
                let mut deltas = vec![
                    Delta::new(Quantity::ProductionRemaining, self.remaining, limit_after),
                    Delta::new(
                        Quantity::ProductionFreeCapacity,
                        self.free_capacity + self.small_unit_exemptions,
                        free_after,
                    ),
                ];
                if let [only] = spots.as_slice() {
                    let after = self.standing_after(&types, state, content, *kind, only, 1);
                    deltas.extend(limit_deltas(&before, &after));
                    exchange = placement_facts(
                        exchange.with("destination", only.clone()),
                        &before,
                        &after,
                    );
                } else {
                    exchange =
                        exchange.with("placement_pending", i64::try_from(spots.len()).unwrap_or(2));
                }
                options.push(exchange.previewed(Preview::certain(deltas)));
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
        let (target, spot) = placement_target(&self.system, where_to);
        for _ in 0..made {
            let unit = Unit::new(UnitTypeId::new(id), self.player.clone());
            // The Crimson hero card is not a place on the board.
            if target.as_str() == crate::factions::crimson_cards::CARD_SYSTEM {
                crate::factions::crimson_cards::put_on_card(state, &self.player, id);
                self.report
                    .produced
                    .push((UnitTypeId::new(id), where_to.to_owned()));
                continue;
            }
            if spot == SPACE {
                state.system_mut(&target).units.push(unit);
            } else {
                state
                    .system_mut(&target)
                    .planet_units
                    .entry(PlanetId::new(spot))
                    .or_default()
                    .push(unit);
            }
            crate::supply::stage_naaz_mech_placed(
                state,
                &self.player,
                &target,
                &UnitTypeId::new(id),
            );
            self.report
                .produced
                .push((UnitTypeId::new(id), where_to.to_owned()));
        }
        // The Last Bastion's Icon is exhausted by placing the ships in one of its systems.
        if made > 0 && target != self.system {
            crate::factions::bastion::ship_placed(
                state,
                content,
                sources,
                &self.player,
                &self.system,
                &target,
                &UnitTypeId::new(id),
            );
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
        let exempt = if catalogue(content, sources)
            .get(id)
            .is_some_and(|unit| matches!(unit.base_type(), "fighter" | "infantry"))
        {
            (count - free).min(self.small_unit_exemptions)
        } else {
            0
        };
        self.small_unit_exemptions -= exempt;
        self.free_capacity -= free;
        self.remaining -= count - free - exempt;
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
                if self.remaining <= 0 && self.small_unit_exemptions <= 0 {
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
                // Display only (`Choice::details`): ships charged against the fleet pool in this
                // system and the pool's limit (LRR 37), so a client can show the room left.
                let standing = crate::fleet::standing(
                    state,
                    content,
                    sources,
                    &self.player,
                    &self.system,
                    None,
                );
                let mut fleet_supply = serde_json::json!({
                    "used": standing.fleet_charged,
                    "limit": standing.fleet_limit,
                });
                if crate::fleet::is_unlimited(state, &self.player) {
                    fleet_supply["unlimited"] = serde_json::Value::Bool(true);
                }
                Some(
                    Choice::new(
                        self.player.clone(),
                        format!("produce in {} ({} left)", self.system, self.remaining),
                        options,
                    )
                    .contextualized(self.context(state))
                    .detailed("fleet_supply", fleet_supply),
                )
            }
            Stage::Paying {
                owed, cost, paid, ..
            } => {
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
                Some(
                    Choice::new(
                        self.player.clone(),
                        format!("pay {owed} more resources"),
                        options,
                    )
                    .contextualized(
                        payment_context(state, &self.player, Spend::Resources, *cost, *paid)
                            .about(DecisionTarget::System(self.system.clone())),
                    ),
                )
            }
            Stage::Placing { id, made } => {
                self.placement_choice(state, content, sources, id, *made)
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
                    let (cost, discount_used) = types.get(id).map_or((0, 0), |kind| {
                        self.discounted_unit(state, *kind, price_of_under(Some(state), kind).0)
                    });
                    // Spent for real only for the option actually chosen: `build_options` offered
                    // this discount to every option shown, and only one could ever be taken.
                    self.spend_discount(discount_used);
                    // A purchase the credit covers outright goes straight to placing. Entering the
                    // paying stage owing nothing would ask `payment_options` for a bill of zero,
                    // which has no options, and the stage would abort with the unit unplaced.
                    let owed = self.spend_credit(cost);
                    self.stage = if owed > 0 {
                        Stage::Paying {
                            id: id.to_owned(),
                            owed,
                            made,
                            cost,
                            paid: cost - owed,
                        }
                    } else {
                        Stage::Placing {
                            id: id.to_owned(),
                            made,
                        }
                    };
                } else if let Some(id) = option.id.strip_prefix("exchange|") {
                    // The unit is paid for with a captured model of that type, so no resources,
                    // credit, or discount are spent. If the capture is gone -- something returned it
                    // between the offer and this answer -- the exchange does not happen at all, the
                    // same way a bill the player cannot pay ends the stage instead of becoming a
                    // partial purchase.
                    if !crate::factions::hooks_economy::perform_production_unit_exchange(
                        state,
                        content,
                        sources,
                        &self.player,
                        &self.system,
                        id,
                    ) {
                        self.stage = Stage::Done;
                        return Ok(());
                    }
                    self.stage = Stage::Placing {
                        id: id.to_owned(),
                        made: 1,
                    };
                }
            }
            Stage::Paying {
                id,
                owed,
                made,
                cost,
                paid,
            } => {
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
                let paid = paid + worth;
                self.stage = if owed > 0 {
                    Stage::Paying {
                        id,
                        owed,
                        made,
                        cost,
                        paid,
                    }
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
                crate::factions::argent::clear_agent_production_destinations(
                    state,
                    &self.player,
                    &self.system,
                );
                self.placement_timing_done = false;
                self.stage = Stage::Choosing;
            }
        }
        self.settle(state, content, sources, ctx)?;
        // After `settle`, not before: the loop inside it can be what reaches `Done`, and a check
        // ahead of it would miss exactly the uses that ended without another question.
        //
        // Auto-Factories reads the whole use of PRODUCTION, so it fires once where the use ends
        // rather than at each placement -- three ships pay once, not three times. Several paths
        // reach `Done`, so this is a flag rather than a call at each of them.
        if matches!(self.stage, Stage::Done) && !self.settled {
            self.settled = true;
            // A planet-value swap lasts one use of PRODUCTION (Hegemonic Trade Policy).
            end_value_swap(state);
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
            self.announce_produced(state, ctx, &made);
        }
        Ok(())
    }
}

impl ProductionWindow {
    /// Yin Spinner: "After you produce units". Emitted last, once every end-of-use effect has
    /// landed, and only for a use that produced something. Uses the window own timing handle (none
    /// when the caller has no resolver, and then nothing can react). A cancelled or failed
    /// announcement cannot un-produce the units. Emitted unconditionally.
    fn announce_produced(
        &self,
        state: &mut GameState,
        ctx: &mut Resolving<'_>,
        made: &[(UnitTypeId, String)],
    ) {
        if made.is_empty() {
            return;
        }
        let units: Vec<serde_json::Value> = made
            .iter()
            .map(|(kind, place)| serde_json::json!({ "unit_type": kind.as_str(), "place": place }))
            .collect();
        let mut payload = BTreeMap::new();
        payload.insert("player".to_owned(), self.player.to_string().into());
        payload.insert("system".to_owned(), self.system.to_string().into());
        payload.insert(
            "source".to_owned(),
            if self.ability {
                "ability"
            } else {
                "production"
            }
            .into(),
        );
        payload.insert("count".to_owned(), made.len().into());
        payload.insert("units".to_owned(), serde_json::Value::Array(units));
        if ctx.timing.is_none() {
            // No timing handle: nothing can react now. Keep the announcement for
            // `supply::flush_staged_events`, which a caller with a resolver calls later.
            crate::supply::stage_event(state, "UNITS_PRODUCED", &payload);
            return;
        }
        crate::factions::hooks_economy::emit(ctx, state, "UNITS_PRODUCED", payload);
    }
}

impl ProductionWindow {
    /// Advance past any stage that has nothing left to ask.
    fn settle(
        &mut self,
        state: &mut GameState,
        content: &ContentStore,
        sources: SourceSet,
        ctx: &mut Resolving<'_>,
    ) -> Result<(), IllegalChoice> {
        loop {
            match self.stage.clone() {
                Stage::Paying {
                    id,
                    owed,
                    made,
                    cost,
                    paid,
                } => {
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
                            return Ok(());
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
                                return Ok(());
                            };
                            if owed > worth {
                                self.stage = Stage::Paying {
                                    id,
                                    owed: owed - worth,
                                    made,
                                    cost,
                                    paid: paid + worth,
                                };
                                continue; // the next step may itself be degenerate
                            }
                            self.credit += worth - owed;
                            self.stage = Stage::Placing { id, made };
                        }
                        _ => return Ok(()),
                    }
                }
                Stage::Placing { id, made } => {
                    if !self.placement_timing_done {
                        let types = catalogue(content, sources);
                        if types
                            .get(id.as_str())
                            .is_some_and(|kind| kind.is_ground_force() && !kind.is_structure())
                        {
                            let mut payload = BTreeMap::new();
                            payload.insert("player".to_owned(), self.player.to_string().into());
                            payload.insert("system".to_owned(), self.system.to_string().into());
                            payload.insert("count".to_owned(), made.into());
                            ctx.emit(state, "GROUND_FORCES_BEING_PRODUCED", payload)
                                .map_err(|error| IllegalChoice::DeciderFailed {
                                    player: self.player.clone(),
                                    prompt: "ground-force production timing".to_owned(),
                                    reason: error.to_string(),
                                })?;
                        }
                        self.placement_timing_done = true;
                    }
                    let types = catalogue(content, sources);
                    let Some(kind) = types.get(id.as_str()).copied() else {
                        self.stage = Stage::Done;
                        crate::factions::argent::clear_agent_production_destinations(
                            state,
                            &self.player,
                            &self.system,
                        );
                        return Ok(());
                    };
                    let spots = self.spots(state, content, sources, kind);
                    // Exactly one legal placement is not a decision.
                    match spots.as_slice() {
                        [only] => {
                            let only = only.clone();
                            self.place(state, content, sources, &id, &only, made);
                            crate::factions::argent::clear_agent_production_destinations(
                                state,
                                &self.player,
                                &self.system,
                            );
                            self.placement_timing_done = false;
                            self.stage = Stage::Choosing;
                        }
                        [] => {
                            self.stage = Stage::Done;
                            crate::factions::argent::clear_agent_production_destinations(
                                state,
                                &self.player,
                                &self.system,
                            );
                            return Ok(());
                        }
                        _ => return Ok(()),
                    }
                }
                Stage::Choosing => {
                    if (self.remaining <= 0 && self.small_unit_exemptions <= 0)
                        || self.build_options(state, content, sources).is_empty()
                    {
                        self.stage = Stage::Done;
                    }
                    return Ok(());
                }
                // A finished window has nothing left to settle: leaving the loop is mandatory,
                // because `resolve` settles after every answer and a decline ends production.
                Stage::Done => break,
            }
        }
        Ok(())
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
    resolve_timed(state, content, sources, galaxy, table, None, player, system)
}

/// [`resolve`] with a timing handle, so `UNITS_PRODUCED` opens its windows. Callers that have a
/// resolver (Warfare secondary, Construction, hero) pass `Some(handle)`; with `None` this is
/// exactly [`resolve`].
///
/// # Errors
/// [`IllegalChoice`] when a decider answers with something not offered.
#[allow(
    clippy::too_many_arguments,
    reason = "resolve plus the timing handle; mirrors the other production entry points"
)]
pub fn resolve_timed(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    table: &mut Table,
    timing: Option<crate::choice::TimingHandle<'_>>,
    player: &PlayerId,
    system: &SystemId,
) -> Result<ProductionReport, IllegalChoice> {
    let mut window = ProductionWindow::new(state, content, sources, player, system);
    // Harrugh Gefhara (Hacan hero): "When 1 or more of your units use PRODUCTION" -- every use, not
    // only the tactical action's. Warfare's secondary and Construction produce through here, where
    // the hero was never offered. Gated on the hero being ready so a production nobody can make
    // free keeps its sequence untouched.
    let hero_ready = state.player(player).is_some_and(|seat| {
        seat.leaders.get(&ti4_model::id::LeaderId::new("hacanhero"))
            == Some(&ti4_model::state::LeaderStatus::Unlocked)
    });
    if hero_ready && capacity(state, content, sources, player, system) > 0 {
        state.production_seq = state.production_seq.saturating_add(1);
        if crate::leaders::offer_production_hero(state, content, sources, galaxy, table, player)? {
            window.refresh(state, content, sources);
        }
    }
    // Xander Alexin Victori III (Keleres): the producer's commodities as trade goods for this
    // production, offered once as it starts.
    let agent_window = capacity(state, content, sources, player, system) > 0
        && crate::factions::keleres::offer_agent(state, content, sources, galaxy, table, player)?;
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
        timing,
    };
    // Agency Supply Network: another unit's PRODUCTION, resolved beside this one (Warfare reaches
    // here; the tactical action's production step does the same in `game.rs`).
    agency_supply_network(state, &mut ctx, galaxy, player, system)?;
    while let Some(choice) = window.pending_choice(state, content, sources) {
        let answer = match ctx
            .table
            .ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))
        {
            Ok(answer) => answer,
            Err(error) => {
                crate::factions::argent::clear_agent_production_destinations(state, player, system);
                if agent_window {
                    crate::factions::keleres::close_agent_window(state, player);
                }
                return Err(error);
            }
        };
        if let Err(error) = window.resolve(state, &mut ctx, answer) {
            crate::factions::argent::clear_agent_production_destinations(state, player, system);
            if agent_window {
                crate::factions::keleres::close_agent_window(state, player);
            }
            return Err(error);
        }
    }
    crate::factions::argent::clear_agent_production_destinations(state, player, system);
    if agent_window {
        crate::factions::keleres::close_agent_window(state, player);
    }
    end_value_swap(state);
    Ok(window.into_report())
}

/// Produce units outside a tactical action, by an ability: the window of
/// [`ProductionWindow::for_ability`] run to the end against the table, with the caller's
/// `Resolving` (give it a timing handle so `UNITS_PRODUCED` opens its windows).
///
/// Payment, supply, fleet limits and placement choices are the ordinary production flow. The
/// player may stop at any point ("up to"). A decider answering something not offered aborts with
/// the units already produced and paid for left in place, exactly as [`resolve`] does.
///
/// # Errors
/// [`IllegalChoice`] when a decider answers with something not offered.
pub fn produce_by_ability(
    state: &mut GameState,
    ctx: &mut Resolving<'_>,
    galaxy: Option<&Galaxy>,
    player: &PlayerId,
    system: &SystemId,
    limit: Option<i64>,
) -> Result<ProductionReport, IllegalChoice> {
    produce_by_ability_capped(state, ctx, galaxy, player, system, limit, None)
}

/// [`produce_by_ability`] where each unit's printed cost may be at most `max_unit_cost` (Muaat
/// Umbat: "up to 2 units that each have a cost of 4 or less"). `None` is no cap.
///
/// # Errors
/// [`IllegalChoice`] when a decider answers with something not offered.
#[allow(
    clippy::too_many_arguments,
    reason = "produce_by_ability plus the per-unit cost cap"
)]
pub fn produce_by_ability_capped(
    state: &mut GameState,
    ctx: &mut Resolving<'_>,
    galaxy: Option<&Galaxy>,
    player: &PlayerId,
    system: &SystemId,
    limit: Option<i64>,
    max_unit_cost: Option<i64>,
) -> Result<ProductionReport, IllegalChoice> {
    let (content, sources) = (ctx.content, ctx.sources);
    let mut window = ProductionWindow::for_ability(state, content, sources, player, system, limit)
        .with_max_unit_cost(max_unit_cost);
    // Xander Alexin Victori III (Keleres): offered once as this production starts.
    let agent_window =
        crate::factions::keleres::offer_agent(state, content, sources, galaxy, ctx.table, player)?;
    while let Some(choice) = window.pending_choice(state, content, sources) {
        let step = ctx
            .table
            .ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))
            .and_then(|answer| window.resolve(state, ctx, answer));
        if let Err(error) = step {
            if agent_window {
                crate::factions::keleres::close_agent_window(state, player);
            }
            return Err(error);
        }
    }
    if agent_window {
        crate::factions::keleres::close_agent_window(state, player);
    }
    end_value_swap(state);
    Ok(window.into_report())
}

/// Whether [`produce_unit_by_ability`] would offer `unit` in `system` now: it is buildable, the
/// player can pay for it and a spot takes it. The same filter the window applies, asked up front so
/// a card is only offered when its production can happen.
#[must_use]
pub fn can_produce_unit_by_ability(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    unit: &str,
) -> bool {
    let window = ProductionWindow::for_ability(state, content, sources, player, system, Some(1))
        .with_only_unit(Some(unit.to_owned()));
    window.pending_choice(state, content, sources).is_some()
}

/// Produce 1 unit of exactly one type outside a tactical action, by an ability (Nekro
/// `nekroc4y`): [`produce_by_ability`] with a limit of 1 and every other unit type withheld.
///
/// # Errors
/// [`IllegalChoice`] when a decider answers with something not offered.
pub fn produce_unit_by_ability(
    state: &mut GameState,
    ctx: &mut Resolving<'_>,
    galaxy: Option<&Galaxy>,
    player: &PlayerId,
    system: &SystemId,
    unit: &str,
) -> Result<ProductionReport, IllegalChoice> {
    let (content, sources) = (ctx.content, ctx.sources);
    let mut window =
        ProductionWindow::for_ability(state, content, sources, player, system, Some(1))
            .with_only_unit(Some(unit.to_owned()));
    while let Some(choice) = window.pending_choice(state, content, sources) {
        ctx.table
            .ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))
            .and_then(|answer| window.resolve(state, ctx, answer))?;
    }
    end_value_swap(state);
    Ok(window.into_report())
}

/// Agency Supply Network (Keleres `asn`): "Once per action, when you resolve a unit's PRODUCTION
/// ability, you may resolve another of your unit's PRODUCTION abilities in any system."
///
/// Called by the two entry points that resolve a unit's PRODUCTION (the tactical action's
/// production step, `game.rs` `enter_production`, and [`resolve_timed`] for Warfare), once the use
/// in `primary` is open and before its first choice. The Keleres picks one other system holding
/// producers of theirs where they could build something, and that system's PRODUCTION is resolved
/// in full there (payment, placement, limits and blockade as for any use). `primary` is excluded:
/// its units have all been resolved together by the use that opened this window.
///
/// "Once per action" is a mark in `faction_marks` that the Keleres' `ACTION_COMPLETED` window
/// clears, since the resolver's frequency bookkeeping is not saved with the game. Returns whether a
/// second system was resolved; nothing is asked of, or changed for, a player without the technology.
///
/// The second use gets no Sarween/AI Development Algorithm discount: it opens a plain window, and
/// those cards discount "this use" (the question is recorded in `plans/evidence/BF-keleres.md`).
///
/// # Errors
/// [`IllegalChoice`] when a decider answers with something not offered.
pub fn agency_supply_network(
    state: &mut GameState,
    ctx: &mut Resolving<'_>,
    galaxy: Option<&Galaxy>,
    player: &PlayerId,
    primary: &SystemId,
) -> Result<bool, IllegalChoice> {
    let (content, sources) = (ctx.content, ctx.sources);
    if !crate::factions::keleres::asn_ready(state, player)
        || capacity(state, content, sources, player, primary) <= 0
    {
        return Ok(false);
    }
    let systems: Vec<SystemId> = state
        .board
        .keys()
        .filter(|system| *system != primary)
        .filter(|system| {
            capacity(state, content, sources, player, system) > 0
                && ProductionWindow::new(state, content, sources, player, system)
                    .pending_choice(state, content, sources)
                    .is_some()
        })
        .cloned()
        .collect();
    if systems.is_empty() {
        return Ok(false);
    }
    let mut options: Vec<ChoiceOption> = systems
        .iter()
        .map(|system| {
            ChoiceOption::labelled(
                system.as_str(),
                "asn_system",
                format!("also resolve PRODUCTION in system {system}"),
            )
        })
        .collect();
    options.push(ChoiceOption::decline());
    let choice = Choice::new(
        player.clone(),
        "Agency Supply Network: resolve another unit's PRODUCTION in which system",
        options,
    )
    .contextualized(DecisionContext::new(
        player.clone(),
        DecisionSource::FactionAbility("asn".to_owned()),
        "asn_system",
        state.phase,
        state.round,
    ));
    let answer = ctx
        .table
        .ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
    if answer.is_decline() {
        return Ok(false);
    }
    let Some(system) = systems.iter().find(|system| system.as_str() == answer.id) else {
        return Ok(false);
    };
    crate::factions::keleres::asn_mark(state, player);
    let mut window = ProductionWindow::new(state, content, sources, player, system);
    while let Some(next) = window.pending_choice(state, content, sources) {
        let answer = ctx
            .table
            .ask_seeing(&next, &Observed::new(state, content, sources, galaxy))?;
        window.resolve(state, ctx, answer)?;
    }
    end_value_swap(state);
    Ok(true)
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
            here.planet_units
                .entry(planet.clone())
                .or_default()
                .push(ti4_model::units::Unit::new(
                    UnitTypeId::new("spacedock"),
                    mine.clone(),
                ));
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
    /// The acceptance case from `plans/archive/BUG_2026-08-29_PRODUCTION_COMBINED_PAYMENT.md`. Selected as
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
            here.planet_units
                .entry(planet.clone())
                .or_default()
                .push(ti4_model::units::Unit::new(
                    UnitTypeId::new("spacedock"),
                    player.clone(),
                ));
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
            window
                .resolve(&mut state, &mut ctx, answer)
                .expect("resolves");
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
        assert!(state.exhausted_planets.contains(&planet), "the planet paid");
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
            seat.technologies
                .insert(ti4_model::id::TechnologyId::new("mc"));
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
    fn a_lone_payment_option_is_noted_but_never_journaled() {
        let (mut state, _, _) = seated();
        state.player_mut(&player()).unwrap().trade_goods = 2;
        let mut table = Table::new();
        assert!(
            pay(
                &mut state,
                ContentStore::embedded(),
                POK,
                &mut table,
                &player(),
                2,
                Spend::Resources
            )
            .unwrap()
        );
        assert!(
            table.log.is_empty(),
            "a skipped ask must not enter the journal"
        );
        let notes = table.take_auto_resolved();
        assert!(!notes.is_empty());
        assert!(
            notes
                .iter()
                .all(|n| n.player == player() && n.prompt.starts_with("pay "))
        );
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
            cost: 3,
            paid: 0,
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
    fn obs008c1_window_payment_context_carries_full_bill_and_prior_credit() {
        let (mut state, system, planet) = seated();
        state.system_mut(&system).set_control(planet, player());
        state.player_mut(&player()).unwrap().trade_goods = 2;
        let mut window =
            ProductionWindow::new(&state, ContentStore::embedded(), POK, &player(), &system);
        window.stage = Stage::Paying {
            id: "dreadnought".to_owned(),
            owed: 3,
            made: 1,
            cost: 5,
            paid: 2,
        };

        let choice = window
            .pending_choice(&state, ContentStore::embedded(), POK)
            .expect("more than one payment face");
        let context = choice.context.expect("typed payment context");
        assert_eq!(context.subtype, "pay_resources");
        assert_eq!(context.target, Some(DecisionTarget::System(system)));
        assert_eq!(context.outstanding.len(), 1);
        let debt = &context.outstanding[0];
        assert_eq!(debt.kind, ConstraintKind::Resources);
        assert_eq!((debt.amount, debt.paid, debt.remaining()), (5, 2, 3));
    }

    #[test]
    fn obs008c1_each_payment_face_preview_agrees_with_applying_that_face() {
        let content = ContentStore::embedded();
        let (mut state, system, planet) = seated();
        state.system_mut(&system).set_control(planet, player());
        state.player_mut(&player()).unwrap().trade_goods = 2;
        let options = payment_options(&state, content, POK, &player(), Spend::Resources, 0, 1);
        assert!(options.len() >= 2, "planet and trade-good alternatives");

        for option in options {
            let preview = option.preview.as_ref().expect("every face is previewed");
            assert!(preview.is_informative());
            let mut applied = state.clone();
            let worth = apply_payment_option(
                &mut applied,
                content,
                POK,
                &player(),
                Spend::Resources,
                &option,
            )
            .expect("offered face applies");
            let pool_after = available(&applied, content, POK, &player(), Spend::Resources);
            let goods_after = i64::from(applied.player(&player()).unwrap().trade_goods);
            let delta = |quantity| {
                preview
                    .certain_deltas()
                    .iter()
                    .find(|delta| delta.quantity == quantity)
                    .expect("quantity previewed")
            };
            assert_eq!(delta(Quantity::Resources).after, pool_after);
            assert_eq!(delta(Quantity::TradeGoods).after, goods_after);
            assert_eq!(
                option
                    .payload
                    .get("worth")
                    .and_then(serde_json::Value::as_i64),
                Some(worth),
                "the same face value drives debt and application"
            );
        }
    }

    #[test]
    fn obs008c1_shared_payment_context_counts_transaction_credit_as_paid() {
        struct ContextChecking {
            amount: i64,
            paid: i64,
        }
        impl crate::choice::Decider for ContextChecking {
            fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
                let context = choice.context.as_ref().expect("typed payment context");
                let debt = context.outstanding.first().expect("one debt");
                assert_eq!((debt.amount, debt.paid), (self.amount, self.paid));
                assert_eq!(debt.remaining(), self.amount - self.paid);
                Ok(choice.options[0].clone())
            }
        }

        let content = ContentStore::embedded();
        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        state.player_mut(&player()).unwrap().trade_goods = 1;
        let worth = planet_value(content, POK, &planet, Spend::Resources);
        assert!(worth > 0);
        let cost = worth + 1;
        let mut credit = 1;
        let mut table = Table::new();
        table.seat(
            player(),
            Box::new(ContextChecking {
                amount: cost,
                paid: 1,
            }),
        );
        assert!(
            pay_seeing_with_credit(
                &mut state,
                content,
                POK,
                None,
                &mut table,
                &player(),
                cost,
                Spend::Resources,
                &mut credit,
            )
            .expect("payment resolves")
        );
        assert_eq!(
            table.log.records.len(),
            1,
            "the genuine choice was recorded"
        );
        assert!(table.log.records[0].context.is_some());
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
            cost: 1,
            paid: 0,
        };
        window.remaining = 0;
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut table = Table::new();
        let mut ctx = Resolving {
            content: store,
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };
        window.settle(&mut state, store, POK, &mut ctx).unwrap();

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
    fn obs008c2a_build_context_and_preview_agree_with_completed_production() {
        let content = ContentStore::embedded();
        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);
        let mut window = ProductionWindow::new(&state, content, POK, &player(), &system);
        window.credit = 1; // the fighter batch is already paid, so this test reaches placement.

        let choice = window
            .pending_choice(&state, content, POK)
            .expect("production choice");
        let context = choice.context.as_ref().expect("typed production context");
        assert_eq!(context.subtype, "produce_unit");
        assert_eq!(context.target, Some(DecisionTarget::System(system.clone())));
        let capacity = context.outstanding.first().expect("one limit constraint");
        assert_eq!(capacity.kind, ConstraintKind::ProductionCapacity);
        assert_eq!(
            (capacity.amount, capacity.paid, capacity.remaining()),
            (window.limit, 0, window.remaining)
        );

        let fighter = choice
            .options
            .iter()
            .find(|option| option.id.starts_with("build|fighter|"))
            .cloned()
            .expect("fighter batch");
        assert_eq!(
            fighter
                .payload
                .get("available_resources")
                .and_then(serde_json::Value::as_i64),
            Some(available(&state, content, POK, &player(), Spend::Resources))
        );
        assert_eq!(
            fighter
                .payload
                .get("credit_used")
                .and_then(serde_json::Value::as_i64),
            Some(1)
        );
        assert_eq!(
            fighter
                .payload
                .get("owed")
                .and_then(serde_json::Value::as_i64),
            Some(0)
        );
        let preview = fighter.preview.clone().expect("analytic build preview");
        let after = |quantity| {
            preview
                .certain_deltas()
                .iter()
                .find(|delta| delta.quantity == quantity)
                .expect("quantity previewed")
                .after
        };

        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut table = Table::new();
        let mut ctx = Resolving {
            content,
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };
        window.resolve(&mut state, &mut ctx, fighter).unwrap();

        assert_eq!(
            window.remaining,
            after(Quantity::ProductionRemaining),
            "preview and actual limit consumption agree"
        );
        assert_eq!(
            window.free_capacity,
            after(Quantity::ProductionFreeCapacity),
            "preview and actual Bellum allowance agree"
        );
        let continued = window
            .pending_choice(&state, content, POK)
            .expect("continued production choice");
        let capacity = continued
            .context
            .as_ref()
            .and_then(|context| context.outstanding.first())
            .expect("updated limit constraint");
        assert_eq!(
            (capacity.amount, capacity.paid, capacity.remaining()),
            (
                window.limit,
                window.limit - window.remaining,
                window.remaining
            ),
            "the next choice reports capacity already consumed by the completed build"
        );
    }

    #[test]
    fn obs008c2a_capacity_ship_opens_and_fighter_spends_the_free_allowance() {
        let content = ContentStore::embedded();
        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);
        state.player_mut(&player()).unwrap().breakthrough =
            Some(ti4_model::id::BreakthroughId::new("solbt"));
        let mut window = ProductionWindow::new(&state, content, POK, &player(), &system);
        window.credit = 10;

        let resolve =
            |window: &mut ProductionWindow, state: &mut GameState, option: ChoiceOption| {
                let mut dice = crate::dice::Dice::new();
                let mut rng = crate::rng::GameRng::new(0);
                let mut table = Table::new();
                let mut ctx = Resolving {
                    content,
                    sources: POK,
                    dice: &mut dice,
                    rng: &mut rng,
                    table: &mut table,
                    timing: None,
                };
                window.resolve(state, &mut ctx, option).unwrap();
            };

        let carrier = window
            .pending_choice(&state, content, POK)
            .unwrap()
            .options
            .into_iter()
            .find(|option| option.id.starts_with("build|carrier|"))
            .expect("carrier");
        let carrier_preview = carrier.preview.clone().expect("carrier preview");
        resolve(&mut window, &mut state, carrier);
        assert_eq!(
            window.free_capacity,
            carrier_preview
                .certain_deltas()
                .iter()
                .find(|delta| delta.quantity == Quantity::ProductionFreeCapacity)
                .unwrap()
                .after,
            "capacity ship opened the previewed Bellum allowance"
        );
        assert!(window.free_capacity > 0, "the fixture gained an allowance");

        let before_remaining = window.remaining;
        let fighter = window
            .pending_choice(&state, content, POK)
            .unwrap()
            .options
            .into_iter()
            .find(|option| option.id.starts_with("build|fighter|"))
            .expect("fighter");
        let fighter_preview = fighter.preview.clone().expect("fighter preview");
        resolve(&mut window, &mut state, fighter);
        assert_eq!(
            window.remaining, before_remaining,
            "free pair spent no limit"
        );
        assert_eq!(
            window.free_capacity,
            fighter_preview
                .certain_deltas()
                .iter()
                .find(|delta| delta.quantity == Quantity::ProductionFreeCapacity)
                .unwrap()
                .after,
            "fighter consumed exactly the previewed allowance"
        );
    }

    /// Sarween Tools reduces the combined bill, not the printed face: `cost` moves, `printed_cost`
    /// does not, and the amount taken is stated rather than left to be inferred from the two.
    #[test]
    fn sarween_tools_reduces_the_bill_the_option_actually_charges() {
        let content = ContentStore::embedded();
        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);
        state.production_discount_remaining = 1; // as technology::production_used would set it

        let mut window = ProductionWindow::new(&state, content, POK, &player(), &system);
        window.refresh(&state, content, POK);
        let choice = window.pending_choice(&state, content, POK).expect("choice");
        let carrier = choice
            .options
            .iter()
            .find(|option| option.id.starts_with("build|carrier|"))
            .expect("carrier");
        assert_eq!(
            carrier
                .payload
                .get("printed_cost")
                .and_then(serde_json::Value::as_i64),
            Some(3),
            "the face value is untouched"
        );
        assert_eq!(
            carrier
                .payload
                .get("cost")
                .and_then(serde_json::Value::as_i64),
            Some(2)
        );
        assert_eq!(
            carrier
                .payload
                .get("discount")
                .and_then(serde_json::Value::as_i64),
            Some(1)
        );
    }

    /// One use of PRODUCTION has one combined discount, exactly as one combined cost: the second
    /// selection in the same use sees it already spent, whichever build spent it first.
    #[test]
    fn the_discount_is_spent_once_across_two_selections_in_the_same_use() {
        let content = ContentStore::embedded();
        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);
        state.production_discount_remaining = 1;
        let mut window = ProductionWindow::new(&state, content, POK, &player(), &system);
        window.refresh(&state, content, POK);
        window.credit = 10; // paid outright, so both selections reach placement without a bill

        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut table = Table::new();
        let mut ctx = Resolving {
            content,
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };
        let carrier = window
            .pending_choice(&state, content, POK)
            .unwrap()
            .options
            .into_iter()
            .find(|option| option.id.starts_with("build|carrier|"))
            .expect("carrier");
        assert_eq!(
            carrier
                .payload
                .get("discount")
                .and_then(serde_json::Value::as_i64),
            Some(1),
            "the discount is offered to the first selection"
        );
        window.resolve(&mut state, &mut ctx, carrier).unwrap();
        assert_eq!(window.discount_remaining, 0, "spent by the chosen option");

        let second = window
            .pending_choice(&state, content, POK)
            .unwrap()
            .options
            .into_iter()
            .find(|option| option.id.starts_with("build|carrier|"))
            .expect("a second carrier");
        assert_eq!(
            second
                .payload
                .get("discount")
                .and_then(serde_json::Value::as_i64),
            Some(0),
            "nothing left for a second selection in the same use"
        );
        assert_eq!(
            second
                .payload
                .get("cost")
                .and_then(serde_json::Value::as_i64),
            Some(3),
            "the second carrier pays the full printed price"
        );
    }

    /// Harrugh Gefhara's marker, read directly rather than through `use_leader`: the invocation
    /// path (offering "use a leader" as a choice) is a separate, unreached defect recorded in
    /// `plans/BUG_2026-09-04_LEADER_USE_UNREACHABLE.md`. This proves the *consumption* side --
    /// once a marker exists, this use of PRODUCTION genuinely costs nothing -- so fixing the
    /// invocation gap later needs no further change here.
    #[test]
    fn a_free_production_marker_for_this_seq_zeroes_every_build() {
        let content = ContentStore::embedded();
        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);
        state.production_seq = 4;
        state.player_mut(&player()).unwrap().free_production_use = Some(4);

        let mut window = ProductionWindow::new(&state, content, POK, &player(), &system);
        window.refresh(&state, content, POK);
        let carrier = window
            .pending_choice(&state, content, POK)
            .unwrap()
            .options
            .into_iter()
            .find(|option| option.id.starts_with("build|carrier|"))
            .expect("carrier");
        assert_eq!(
            carrier
                .payload
                .get("cost")
                .and_then(serde_json::Value::as_i64),
            Some(0)
        );
        assert_eq!(
            carrier
                .payload
                .get("discount")
                .and_then(serde_json::Value::as_i64),
            Some(3),
            "the whole printed price is discounted away"
        );
        assert_eq!(
            carrier
                .payload
                .get("free_this_use")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
    }

    /// A marker left over from a different production sequence must not make an unrelated later
    /// use free: Harrugh's ability is spent once, for the use it was granted to, not forever.
    #[test]
    fn a_free_production_marker_for_a_different_seq_grants_nothing() {
        let content = ContentStore::embedded();
        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);
        state.production_seq = 5;
        state.player_mut(&player()).unwrap().free_production_use = Some(4);

        let mut window = ProductionWindow::new(&state, content, POK, &player(), &system);
        window.refresh(&state, content, POK);
        let carrier = window
            .pending_choice(&state, content, POK)
            .unwrap()
            .options
            .into_iter()
            .find(|option| option.id.starts_with("build|carrier|"))
            .expect("carrier");
        assert_eq!(
            carrier
                .payload
                .get("free_this_use")
                .and_then(serde_json::Value::as_bool),
            Some(false)
        );
        assert_eq!(
            carrier
                .payload
                .get("cost")
                .and_then(serde_json::Value::as_i64),
            Some(3)
        );
    }

    /// A ship has nowhere else to go, so its fleet and transport aftermath is settled at the build.
    ///
    /// The agreement that matters is with the position the placement actually reaches: the preview
    /// is read before resolving and compared afterwards against the same `fleet::standing` the
    /// end-of-turn enforcement consults. A second implementation of the rule would pass a test that
    /// only checked the preview against itself.
    #[test]
    fn obs008c2b_forced_ship_destination_previews_the_position_it_reaches() {
        let content = ContentStore::embedded();
        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);
        if let Some(seat) = state.player_mut(&player()) {
            seat.fleet_tokens = 1; // one ship of room, so a second one is a visible consequence
        }
        let mut window = ProductionWindow::new(&state, content, POK, &player(), &system);
        window.credit = 10; // paid already, so the build reaches placement in one step

        let carrier = window
            .pending_choice(&state, content, POK)
            .expect("production choice")
            .options
            .into_iter()
            .find(|option| option.id.starts_with("build|carrier|"))
            .expect("carrier");
        assert_eq!(
            carrier
                .payload
                .get("destination")
                .and_then(serde_json::Value::as_str),
            Some(SPACE),
            "a ship states the one destination it can have"
        );
        assert_eq!(
            carrier
                .payload
                .get("placement_pending")
                .and_then(serde_json::Value::as_i64),
            None,
            "nothing is pending when the destination is forced"
        );
        let preview = carrier.preview.clone().expect("build preview");
        let after = |quantity| {
            preview
                .certain_deltas()
                .iter()
                .find(|delta| delta.quantity == quantity)
                .unwrap_or_else(|| panic!("{quantity:?} previewed"))
                .after
        };
        let (headroom, free) = (
            after(Quantity::FleetSupplyHeadroom),
            after(Quantity::CapacityFree),
        );
        let placed = carrier
            .payload
            .get("placed")
            .and_then(serde_json::Value::as_i64)
            .expect("stated arrivals");

        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut table = Table::new();
        let mut ctx = Resolving {
            content,
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };
        window.resolve(&mut state, &mut ctx, carrier).unwrap();

        let standing = crate::fleet::standing(&state, content, POK, &player(), &system, None);
        assert_eq!(
            (standing.fleet_headroom(), standing.capacity_free()),
            (headroom, free),
            "the previewed position is the position the placement reached"
        );
        assert_eq!(
            i64::try_from(window.report.produced.len()).unwrap(),
            placed,
            "the stated arrivals are the units that arrived"
        );
    }

    /// Where a ground force goes decides whether it costs transport, and the options say so.
    ///
    /// Saar's Floating Factory sits in the space area, so an infantry produced here may go to a
    /// planet or into space. Only the second consumes capacity, and this is the case that could
    /// not be answered before the destination was known -- which is why it was split out of
    /// `OBS-008c2a` rather than guessed there.
    #[test]
    fn a_module_destination_in_another_system_is_offered_and_placed_there() {
        fn elsewhere(
            _: &GameState,
            _: &ContentStore,
            _: SourceSet,
            _: &PlayerId,
            _: &SystemId,
            unit: &str,
        ) -> Vec<(SystemId, Option<PlanetId>)> {
            if unit == "fighter" {
                vec![(SystemId::new("elsewhere"), None)]
            } else {
                Vec::new()
            }
        }
        let content = ContentStore::embedded();
        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);
        let hooks = crate::factions::hooks_economy::EconomyHooks {
            production_destinations: Some(elsewhere),
            ..crate::factions::hooks_economy::EconomyHooks::NONE
        };
        crate::factions::hooks_economy::with_test_hooks(hooks, || {
            let mut window = ProductionWindow::new(&state, content, POK, &player(), &system);
            window.credit = 10;
            let fighter = window
                .pending_choice(&state, content, POK)
                .expect("production choice")
                .options
                .into_iter()
                .find(|option| option.id.starts_with("build|fighter|"))
                .expect("fighter");
            let mut dice = crate::dice::Dice::new();
            let mut rng = crate::rng::GameRng::new(0);
            let mut table = Table::new();
            let mut ctx = Resolving {
                content,
                sources: POK,
                dice: &mut dice,
                rng: &mut rng,
                table: &mut table,
                timing: None,
            };
            window.resolve(&mut state, &mut ctx, fighter).unwrap();
            let choice = window
                .pending_choice(&state, content, POK)
                .expect("two destinations: here and elsewhere");
            let ids: Vec<&str> = choice.options.iter().map(|o| o.id.as_str()).collect();
            assert_eq!(ids, ["place|space", "place|elsewhere@space"]);
            let remote = choice.options[1].clone();
            let before = state.system_state(&system).units.len();
            window.resolve(&mut state, &mut ctx, remote).unwrap();
            let landed = state
                .system_state(&SystemId::new("elsewhere"))
                .units
                .clone();
            assert!(landed.iter().all(|unit| unit.owner == player()));
            assert!(!landed.is_empty(), "the fighters went to the other system");
            assert_eq!(state.system_state(&system).units.len(), before);
        });
    }

    #[test]
    fn a_module_destination_is_not_offered_for_a_unit_the_system_cannot_place() {
        fn elsewhere(
            _: &GameState,
            _: &ContentStore,
            _: SourceSet,
            _: &PlayerId,
            _: &SystemId,
            _: &str,
        ) -> Vec<(SystemId, Option<PlanetId>)> {
            vec![(SystemId::new("elsewhere"), None)]
        }
        let content = ContentStore::embedded();
        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);
        // 68.10: another player's ship bars ships here, so no destination is opened for one.
        put(&mut state, &system, "cruiser", &PlayerId::new("enemy"), 1);
        let hooks = crate::factions::hooks_economy::EconomyHooks {
            production_destinations: Some(elsewhere),
            ..crate::factions::hooks_economy::EconomyHooks::NONE
        };
        crate::factions::hooks_economy::with_test_hooks(hooks, || {
            let window = ProductionWindow::new(&state, content, POK, &player(), &system);
            let types = catalogue(content, POK);
            let fighter = *types.get("fighter").unwrap();
            assert!(window.spots(&state, content, POK, fighter).is_empty());
        });
    }

    #[test]
    fn the_produce_choice_reports_fleet_supply_used_and_limit() {
        let content = ContentStore::embedded();
        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);
        put(&mut state, &system, "cruiser", &player(), 2);
        // Fighters ride the capacity of ships, not the pool, so they must not be charged.
        put(&mut state, &system, "fighter", &player(), 2);
        let window = ProductionWindow::new(&state, content, POK, &player(), &system);
        let choice = window
            .pending_choice(&state, content, POK)
            .expect("production choice");
        let standing = crate::fleet::standing(&state, content, POK, &player(), &system, None);
        let supply = choice
            .details
            .get("fleet_supply")
            .expect("fleet_supply detail");
        assert_eq!(supply["used"].as_i64(), Some(standing.fleet_charged));
        assert_eq!(supply["limit"].as_i64(), Some(standing.fleet_limit));
        assert_eq!(
            supply["used"].as_i64(),
            Some(2),
            "two cruisers, no fighters"
        );
        assert_eq!(
            supply["limit"].as_i64(),
            Some(i64::from(crate::fleet::limit(&state, content, &player())))
        );
        assert!(supply.get("unlimited").is_none());
    }

    #[test]
    fn obs008c2b_a_ground_force_placement_separates_space_from_a_planet() {
        let content = ContentStore::embedded();
        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);
        put(&mut state, &system, "saar_spacedock", &player(), 1);
        let mut window = ProductionWindow::new(&state, content, POK, &player(), &system);
        window.credit = 10;

        let infantry = window
            .pending_choice(&state, content, POK)
            .expect("production choice")
            .options
            .into_iter()
            .find(|option| option.id.starts_with("build|infantry|"))
            .expect("infantry");
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut table = Table::new();
        let mut ctx = Resolving {
            content,
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };
        window.resolve(&mut state, &mut ctx, infantry).unwrap();

        let choice = window
            .pending_choice(&state, content, POK)
            .expect("a destination is asked for");
        let context = choice.context.as_ref().expect("typed placement context");
        assert_eq!(context.subtype, "place_unit");
        assert_eq!(context.target, Some(DecisionTarget::System(system.clone())));
        let standing = crate::fleet::standing(&state, content, POK, &player(), &system, None);
        for constraint in &context.outstanding {
            let (amount, paid) = match constraint.kind {
                ConstraintKind::FleetSupply => (standing.fleet_limit, standing.fleet_charged),
                ConstraintKind::TransportCapacity => (standing.transport, standing.consumed),
                other => panic!("unexpected constraint {other:?}"),
            };
            assert_eq!((constraint.amount, constraint.paid), (amount, paid));
        }
        assert_eq!(context.outstanding.len(), 2, "both limits are reported");

        let free_of = |option: &ChoiceOption| {
            option
                .payload
                .get("capacity_free_after")
                .and_then(serde_json::Value::as_i64)
                .expect("capacity aftermath")
        };
        let space = choice
            .options
            .iter()
            .find(|option| option.id == format!("place|{SPACE}"))
            .expect("the space area");
        let ground = choice
            .options
            .iter()
            .find(|option| option.id != format!("place|{SPACE}"))
            .expect("a planet");
        assert!(
            free_of(space) < free_of(ground),
            "an infantry in the space area consumes capacity a landed one does not"
        );
        assert_eq!(
            space
                .payload
                .get("capacity_used")
                .and_then(serde_json::Value::as_i64),
            Some(2),
            "two infantry of the produced pair consume two capacity"
        );
        assert_eq!(
            ground
                .payload
                .get("capacity_used")
                .and_then(serde_json::Value::as_i64),
            Some(0),
            "a landed ground force consumes none"
        );

        let previewed = space
            .preview
            .clone()
            .expect("placement preview")
            .certain_deltas()
            .iter()
            .find(|delta| delta.quantity == Quantity::CapacityFree)
            .expect("capacity previewed")
            .after;
        let taken = space.clone();
        window.resolve(&mut state, &mut ctx, taken).unwrap();
        assert_eq!(
            crate::fleet::standing(&state, content, POK, &player(), &system, None).capacity_free(),
            previewed,
            "the previewed capacity is the capacity the placement reached"
        );
    }

    /// An undecided destination is stated as undecided rather than previewed as no consequence.
    ///
    /// Representation rule 6: a fact that is not yet determined must not arrive as a factual zero.
    /// The infantry here has two destinations and no fleet or transport answer; the ship beside it,
    /// in the same position, has one destination and both answers.
    #[test]
    fn obs008c2b_an_open_destination_is_marked_open_not_previewed_as_nothing() {
        let content = ContentStore::embedded();
        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);
        put(&mut state, &system, "saar_spacedock", &player(), 1);
        let mut window = ProductionWindow::new(&state, content, POK, &player(), &system);
        window.credit = 10;

        let options = window
            .pending_choice(&state, content, POK)
            .expect("production choice")
            .options;
        let quantities = |option: &ChoiceOption| {
            option
                .preview
                .as_ref()
                .expect("a preview")
                .certain_deltas()
                .iter()
                .map(|delta| delta.quantity)
                .collect::<Vec<_>>()
        };

        let infantry = options
            .iter()
            .find(|option| option.id.starts_with("build|infantry|"))
            .expect("infantry");
        assert_eq!(
            infantry
                .payload
                .get("placement_pending")
                .and_then(serde_json::Value::as_i64),
            Some(2),
            "two destinations are still open"
        );
        assert_eq!(
            infantry.payload.get("destination"),
            None,
            "no destination is claimed"
        );
        let open = quantities(infantry);
        assert!(
            !open.contains(&Quantity::FleetSupplyHeadroom)
                && !open.contains(&Quantity::CapacityFree),
            "an undetermined consequence is absent, not zero: {open:?}"
        );
        assert!(
            open.contains(&Quantity::ProductionRemaining),
            "the limits that are already determined are still stated"
        );

        let carrier = options
            .iter()
            .find(|option| option.id.starts_with("build|carrier|"))
            .expect("carrier");
        let settled = quantities(carrier);
        assert!(
            settled.contains(&Quantity::FleetSupplyHeadroom)
                && settled.contains(&Quantity::CapacityFree),
            "the same position answers both for a unit whose destination is forced: {settled:?}"
        );
    }

    /// Tier-C review remediation: these are pre-enforcement violations, not a prediction of what
    /// the owner will remove. Fleet enforcement happens first and can change capacity afterwards.
    #[test]
    fn placement_facts_name_each_pre_enforcement_violation_without_predicting_removals() {
        let before = Standing {
            fleet_limit: 3,
            fleet_charged: 1,
            transport: 4,
            consumed: 2,
            fighters_charged: 0,
            capacity_excess: 0,
        };
        let after = Standing {
            fleet_limit: 3,
            fleet_charged: 4,
            transport: 4,
            consumed: 6,
            fighters_charged: 0,
            capacity_excess: 2,
        };
        let facts = placement_facts(
            ChoiceOption::new("place|space", PLACE_KIND),
            &before,
            &after,
        );

        assert_eq!(
            facts
                .payload
                .get("fleet_excess_after")
                .and_then(serde_json::Value::as_i64),
            Some(1)
        );
        assert_eq!(
            facts
                .payload
                .get("capacity_excess_after")
                .and_then(serde_json::Value::as_i64),
            Some(2)
        );
        assert_eq!(facts.payload.get("units_removed_after"), None);
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
    // -- BF-00b-economy: events, ability production and economy hooks ----------------------------

    use crate::factions::hooks_economy::{EconomyHooks, with_test_hooks};

    /// A seated game where `a` controls a planet holding a space dock and has trade goods to spend.
    fn a_producing_game() -> (GameState, SystemId, PlanetId) {
        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);
        state.player_mut(&player()).unwrap().trade_goods = 20;
        (state, system, planet)
    }

    type Seen = std::sync::Arc<std::sync::Mutex<Vec<BTreeMap<String, serde_json::Value>>>>;

    /// A resolver with one listener on `event_type` that records every payload it sees.
    fn listening_on(event_type: &str) -> (crate::timing::Resolver, Seen) {
        let seen: Seen = Seen::default();
        let sink = seen.clone();
        let mut resolver = crate::timing::Resolver::new(
            vec![player(), PlayerId::new("b")],
            Some(player()),
            Table::default(),
        );
        resolver.register([crate::timing::Ability::new(
            "test:listener",
            player(),
            event_type,
            crate::timing::Relation::After,
            std::sync::Arc::new(move |event, _| {
                sink.lock().unwrap().push(event.payload.clone());
                Ok(())
            }),
        )]);
        (resolver, seen)
    }

    const ARGENT_AGENT_ABILITY: &str =
        "leader:argent:argentagent:GROUND_FORCES_BEING_PRODUCED:when";

    fn argent_agent_production_board(
        remote_owner: Option<PlayerId>,
    ) -> (GameState, Galaxy, SystemId, PlanetId, SystemId, PlanetId) {
        let content = ContentStore::embedded();
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "argent")], POK);
        let mut ids = vec!["18".to_owned()];
        ids.extend(
            crate::fixtures::plain_systems(14)
                .into_iter()
                .filter(|id| !state.board.contains_key(&SystemId::new(id.as_str())))
                .take(6),
        );
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let galaxy = Galaxy::build(content, &refs, POK, 1).expect("the test ring builds");
        let producing = SystemId::new("18");
        let producing_planet = ti4_content::galaxy::planets_in(content, "18", POK)
            .into_iter()
            .find(|planet| !planet.is_placed_during_play())
            .map(|planet| PlanetId::new(planet.id()))
            .expect("the central system has a planet");
        let (remote_system, remote_planet) = galaxy
            .adjacent("18")
            .into_iter()
            .filter_map(|system| {
                let planet = ti4_content::galaxy::planets_in(content, system, POK)
                    .into_iter()
                    .find(|planet| !planet.is_placed_during_play())?;
                Some((SystemId::new(system), PlanetId::new(planet.id())))
            })
            .next()
            .expect("an adjacent system has a planet");

        state
            .system_mut(&producing)
            .set_control(producing_planet.clone(), player());
        crate::fixtures::put_on_planet(
            &mut state,
            &producing,
            &producing_planet,
            "spacedock",
            &player(),
            1,
        );
        if let Some(owner) = remote_owner {
            state
                .system_mut(&remote_system)
                .set_control(remote_planet.clone(), owner);
        }
        state.player_mut(&player()).unwrap().trade_goods = 30;
        state
            .player_mut(&PlayerId::new("b"))
            .unwrap()
            .leaders
            .insert(
                ti4_model::id::LeaderId::new("argentagent"),
                ti4_model::state::LeaderStatus::Readied,
            );
        (
            state,
            galaxy,
            producing,
            producing_planet,
            remote_system,
            remote_planet,
        )
    }

    fn agent_window_services(
        state: &GameState,
        agent_answer: Option<&str>,
    ) -> (
        crate::timing::Resolver,
        crate::event::EventSequence,
        crate::dice::Dice,
        crate::rng::GameRng,
        Table,
    ) {
        let resolver = crate::fixtures::armed_resolver(state);
        let mut table = Table::default();
        if let Some(answer) = agent_answer {
            table.seat(
                PlayerId::new("b"),
                Box::new(crate::choice::Scripted::new([answer])),
            );
        }
        (
            resolver,
            crate::event::EventSequence::new(),
            crate::dice::Dice::new(),
            crate::rng::GameRng::new(1),
            table,
        )
    }

    fn agent_mark_exists(state: &GameState) -> bool {
        state
            .faction_marks
            .keys()
            .any(|key| key.starts_with("argent:argentagent:production:a:"))
    }

    fn infantry_build_offer(choice: &Choice, content: &ContentStore) -> ChoiceOption {
        let types = catalogue(content, POK);
        choice
            .options
            .iter()
            .find(|option| {
                option
                    .id
                    .strip_prefix("build|")
                    .and_then(|rest| rest.split('|').next())
                    .and_then(|id| types.get(id))
                    .is_some_and(|kind| kind.base_type() == "infantry")
            })
            .cloned()
            .expect("one infantry offer")
    }

    fn built_count(option: &ChoiceOption) -> usize {
        usize::try_from(
            option
                .payload
                .get("count")
                .and_then(serde_json::Value::as_i64)
                .expect("build offer carries its unit count"),
        )
        .expect("build count is nonnegative")
    }

    fn infantry_count(state: &GameState, system: &SystemId, planet: &PlanetId) -> usize {
        let types = catalogue(ContentStore::embedded(), POK);
        state
            .system_state(system)
            .on_planet(planet)
            .iter()
            .filter(|unit| {
                unit.owner == player()
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(|kind| kind.base_type() == "infantry")
            })
            .count()
    }

    #[test]
    fn argent_agent_redirects_another_players_infantry_to_their_adjacent_planet() {
        let (mut state, galaxy, producing, source_planet, remote, planet) =
            argent_agent_production_board(Some(player()));
        let content = ContentStore::embedded();
        let mut window = ProductionWindow::new(&state, content, POK, &player(), &producing);
        window.credit = 30;
        let build = infantry_build_offer(
            &window
                .pending_choice(&state, content, POK)
                .expect("ground forces are producible"),
            content,
        );
        let produced = built_count(&build);
        let (mut resolver, mut sequence, mut dice, mut rng, mut table) =
            agent_window_services(&state, Some(ARGENT_AGENT_ABILITY));
        let mut ctx = Resolving {
            content,
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: Some(crate::choice::TimingHandle {
                resolver: &mut resolver,
                sequence: &mut sequence,
                galaxy: Some(&galaxy),
            }),
        };
        window
            .resolve(&mut state, &mut ctx, build)
            .expect("agent window resolves");
        assert_eq!(
            state.player(&PlayerId::new("b")).unwrap().leaders
                [&ti4_model::id::LeaderId::new("argentagent")],
            ti4_model::state::LeaderStatus::Exhausted
        );
        assert!(
            agent_mark_exists(&state),
            "the grant lasts through placement"
        );
        let place = window
            .pending_choice(&state, content, POK)
            .expect("local and adjacent destinations are offered");
        let remote_id = format!("place|{remote}@{planet}");
        let answer = place
            .option(&remote_id)
            .cloned()
            .expect("the producing player's adjacent planet is offered");
        window
            .resolve(&mut state, &mut ctx, answer)
            .expect("remote placement resolves");
        assert_eq!(infantry_count(&state, &remote, &planet), produced);
        assert_eq!(infantry_count(&state, &producing, &source_planet), 0);
        assert!(!agent_mark_exists(&state), "placement expires the grant");
    }

    #[test]
    fn argent_agent_decline_exhaustion_and_unlocked_leave_only_local_ground_forces() {
        for status_and_answer in [
            (ti4_model::state::LeaderStatus::Readied, Some("decline")),
            (ti4_model::state::LeaderStatus::Exhausted, None),
            (ti4_model::state::LeaderStatus::Unlocked, None),
        ] {
            let (mut state, galaxy, producing, source_planet, remote, planet) =
                argent_agent_production_board(Some(player()));
            state
                .player_mut(&PlayerId::new("b"))
                .unwrap()
                .leaders
                .insert(
                    ti4_model::id::LeaderId::new("argentagent"),
                    status_and_answer.0,
                );
            let content = ContentStore::embedded();
            let mut window = ProductionWindow::new(&state, content, POK, &player(), &producing);
            window.credit = 30;
            let build = infantry_build_offer(
                &window.pending_choice(&state, content, POK).unwrap(),
                content,
            );
            let produced = built_count(&build);
            let (mut resolver, mut sequence, mut dice, mut rng, mut table) =
                agent_window_services(&state, status_and_answer.1);
            let mut ctx = Resolving {
                content,
                sources: POK,
                dice: &mut dice,
                rng: &mut rng,
                table: &mut table,
                timing: Some(crate::choice::TimingHandle {
                    resolver: &mut resolver,
                    sequence: &mut sequence,
                    galaxy: Some(&galaxy),
                }),
            };
            window.resolve(&mut state, &mut ctx, build).unwrap();
            assert!(!agent_mark_exists(&state));
            assert_eq!(infantry_count(&state, &producing, &source_planet), produced);
            assert_eq!(infantry_count(&state, &remote, &planet), 0);
            assert_eq!(
                state.player(&PlayerId::new("b")).unwrap().leaders
                    [&ti4_model::id::LeaderId::new("argentagent")],
                status_and_answer.0
            );
        }
    }

    #[test]
    fn argent_agent_has_no_offer_for_another_players_adjacent_planet_or_for_a_ship() {
        // The Argent player cannot redirect someone else's unit to a planet controlled by Argent.
        let (mut state, galaxy, producing, _, remote, planet) =
            argent_agent_production_board(Some(PlayerId::new("b")));
        let content = ContentStore::embedded();
        let mut window = ProductionWindow::new(&state, content, POK, &player(), &producing);
        window.credit = 30;
        let infantry = infantry_build_offer(
            &window.pending_choice(&state, content, POK).unwrap(),
            content,
        );
        let (mut resolver, mut sequence, mut dice, mut rng, mut table) =
            agent_window_services(&state, None);
        let mut ctx = Resolving {
            content,
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: Some(crate::choice::TimingHandle {
                resolver: &mut resolver,
                sequence: &mut sequence,
                galaxy: Some(&galaxy),
            }),
        };
        window.resolve(&mut state, &mut ctx, infantry).unwrap();
        assert!(!agent_mark_exists(&state));
        assert_eq!(
            state.player(&PlayerId::new("b")).unwrap().leaders
                [&ti4_model::id::LeaderId::new("argentagent")],
            ti4_model::state::LeaderStatus::Readied
        );

        // Give the producer an eligible adjacent planet. A ship still does not emit the event.
        state
            .system_mut(&remote)
            .set_control(planet.clone(), player());
        let mut ships = ProductionWindow::new(&state, content, POK, &player(), &producing);
        ships.credit = 30;
        let cruiser = ships
            .pending_choice(&state, content, POK)
            .unwrap()
            .options
            .into_iter()
            .find(|option| option.id == "build|cruiser|1")
            .expect("the dock can produce a cruiser");
        ships.resolve(&mut state, &mut ctx, cruiser).unwrap();
        assert!(!agent_mark_exists(&state));
        assert_eq!(infantry_count(&state, &remote, &planet), 0);
        let probe = ctx
            .timing
            .as_mut()
            .expect("the production has a timing handle")
            .sequence
            .next("PROBE", BTreeMap::new())
            .expect("the sequence remains usable");
        assert_eq!(probe.id, 2, "the ship opened no ground-force event");
    }

    #[test]
    fn argent_agent_temporary_destination_is_cleared_when_production_errors() {
        let (mut state, galaxy, producing, _, _, _) = argent_agent_production_board(Some(player()));
        let content = ContentStore::embedded();
        let mut offer_window = ProductionWindow::new(&state, content, POK, &player(), &producing);
        offer_window.credit = 30;
        let build_id = infantry_build_offer(
            &offer_window
                .pending_choice(&state, content, POK)
                .expect("ground forces are producible"),
            content,
        )
        .id;
        let mut resolver = crate::fixtures::armed_resolver(&state);
        let mut table = Table::default();
        table.seat(
            player(),
            Box::new(crate::choice::Scripted::new([
                build_id,
                "invalid-place".to_owned(),
            ])),
        );
        table.seat(
            PlayerId::new("b"),
            Box::new(crate::choice::Scripted::new([ARGENT_AGENT_ABILITY])),
        );
        let mut sequence = crate::event::EventSequence::new();
        let result = {
            let timing = crate::choice::TimingHandle {
                resolver: &mut resolver,
                sequence: &mut sequence,
                galaxy: Some(&galaxy),
            };
            resolve_timed(
                &mut state,
                content,
                POK,
                Some(&galaxy),
                &mut table,
                Some(timing),
                &player(),
                &producing,
            )
        };
        assert!(
            result.is_err(),
            "the scripted invalid placement aborts the window"
        );
        assert!(
            !agent_mark_exists(&state),
            "an aborted window cannot leak destinations"
        );
    }

    /// Yin Spinner reads this: "After you produce units". The event names the player, the system,
    /// the count and where each unit went, and fires once for the whole use.
    #[test]
    fn units_produced_event_reports_what_was_made_and_where() {
        let (mut state, system, _planet) = a_producing_game();
        let (mut resolver, seen) = listening_on("UNITS_PRODUCED");
        let mut sequence = crate::event::EventSequence::new();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(1);
        let mut table = Table::default();
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

        let report = produce_by_ability(&mut state, &mut ctx, None, &player(), &system, Some(2))
            .expect("resolves");

        assert!(!report.produced.is_empty(), "the first offer was taken");
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1, "one event for the whole use");
        let payload = &seen[0];
        assert_eq!(payload["player"], "a");
        assert_eq!(payload["system"], system.to_string());
        assert_eq!(payload["source"], "ability");
        assert_eq!(payload["count"], report.produced.len());
        let units = payload["units"].as_array().expect("an array of units");
        assert_eq!(units.len(), report.produced.len());
        for (listed, (kind, place)) in units.iter().zip(&report.produced) {
            assert_eq!(listed["unit_type"], kind.as_str());
            assert_eq!(listed["place"], place.as_str());
        }
    }

    /// Emission is unconditional (as combat and ground): with no listener the event still takes an
    /// id and writes timing-log lines, a one-time shift recorded in the evidence.
    #[test]
    fn units_produced_is_emitted_even_when_nothing_listens() {
        let (mut state, system, _planet) = a_producing_game();
        let mut resolver =
            crate::timing::Resolver::new(vec![player()], Some(player()), Table::default());
        let mut sequence = crate::event::EventSequence::new();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(1);
        let mut table = Table::default();
        {
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
            let report =
                produce_by_ability(&mut state, &mut ctx, None, &player(), &system, Some(2))
                    .expect("resolves");
            assert!(!report.produced.is_empty());
        }
        assert!(
            resolver
                .log()
                .iter()
                .any(|line| line.contains("UNITS_PRODUCED")),
            "the emission was logged"
        );
        assert_eq!(sequence.next("PROBE", BTreeMap::new()).unwrap().id, 2);
    }

    /// Ability production: the limit is the ability's own, not the system's, and ground forces may
    /// go on any controlled planet in the system even with no producer on it (Arborec flagship,
    /// commander, hero).
    #[test]
    fn ability_production_has_its_own_limit_and_places_ground_forces_on_controlled_planets() {
        let (mut state, system, planet) = seated();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());
        state.player_mut(&player()).unwrap().trade_goods = 20;
        let content = ContentStore::embedded();

        // No unit here has PRODUCTION: ordinary production opens nothing.
        let ordinary = ProductionWindow::new(&state, content, POK, &player(), &system);
        assert!(ordinary.pending_choice(&state, content, POK).is_none());

        let mut window =
            ProductionWindow::for_ability(&state, content, POK, &player(), &system, Some(1));
        let choice = window
            .pending_choice(&state, content, POK)
            .expect("an offer");
        let infantry = choice
            .options
            .iter()
            .find(|option| option.id.starts_with("build|") && option.id.contains("infantry"))
            .cloned()
            .expect("ground forces are offered with no producer");
        let (mut dice, mut rng, mut inner) = (
            crate::dice::Dice::new(),
            crate::rng::GameRng::new(1),
            Table::new(),
        );
        let mut ctx = Resolving {
            content,
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut inner,
            timing: None,
        };
        window.resolve(&mut state, &mut ctx, infantry).unwrap();
        while let Some(choice) = window.pending_choice(&state, content, POK) {
            let answer = choice.options[0].clone();
            window.resolve(&mut state, &mut ctx, answer).unwrap();
        }
        let report = window.into_report();
        assert_eq!(report.produced.len(), 1, "the limit of one unit held");
        assert_eq!(
            report.produced[0].1,
            planet.to_string(),
            "placed on the controlled planet"
        );
        assert_eq!(report.unused_capacity, 0);
    }

    /// Mentak `mc`: one hook changes what a trade good is worth everywhere it is spent.
    #[test]
    fn a_trade_good_worth_hook_reaches_available_and_payment() {
        let mut state = game(&["a", "b"]);
        state.player_mut(&player()).unwrap().trade_goods = 3;
        let content = ContentStore::embedded();
        assert_eq!(
            available(&state, content, POK, &player(), Spend::Influence),
            3
        );
        let doubled = EconomyHooks {
            trade_good_worth: Some(|_, _, worth| worth * 2),
            ..EconomyHooks::NONE
        };
        with_test_hooks(doubled, || {
            assert_eq!(trade_good_worth(&state, &player()), 2);
            assert_eq!(
                available(&state, content, POK, &player(), Spend::Influence),
                6
            );
            assert_eq!(
                available(&state, content, POK, &player(), Spend::Resources),
                6
            );
        });
    }

    /// Winnu `htp`: a planet's value as this player reads it, for a dock's capacity and for every
    /// payment face.
    #[test]
    fn a_planet_spend_value_hook_reaches_capacity_and_payment_faces() {
        let (mut state, system, planet) = a_producing_game();
        state.player_mut(&player()).unwrap().trade_goods = 0;
        let content = ContentStore::embedded();
        let before_capacity = capacity(&state, content, POK, &player(), &system);
        let before_pool = available(&state, content, POK, &player(), Spend::Resources);
        let richer = EconomyHooks {
            planet_spend_value: Some(|_, _, _, _, kind, value| {
                if kind == Spend::Resources {
                    value + 10
                } else {
                    value
                }
            }),
            ..EconomyHooks::NONE
        };
        with_test_hooks(richer, || {
            assert_eq!(
                capacity(&state, content, POK, &player(), &system),
                before_capacity + 10
            );
            assert_eq!(
                available(&state, content, POK, &player(), Spend::Resources),
                before_pool + 10
            );
            let printed = planet_value_now(&state, content, POK, &planet, Spend::Resources);
            let faces = payment_faces(&state, content, POK, &player(), &planet, Spend::Resources);
            assert!(
                faces
                    .iter()
                    .any(|(kind, worth)| *kind == Spend::Resources && *worth == printed + 10)
            );
        });
        assert_eq!(
            capacity(&state, content, POK, &player(), &system),
            before_capacity
        );
    }

    /// Arborec `mitosis`: a barred producer is not a spot for the unit.
    #[test]
    fn a_cannot_produce_hook_bars_a_producer_from_a_unit() {
        let (state, system, _planet) = a_producing_game();
        let content = ContentStore::embedded();
        let types = catalogue(content, POK);
        let infantry = types.get("infantry").copied().expect("an infantry");
        assert!(!placements(&state, content, POK, &player(), &system, &infantry).is_empty());
        let barred = EconomyHooks {
            cannot_produce: Some(|_, _, _, unit, producer| {
                unit == "infantry" && producer == "spacedock"
            }),
            ..EconomyHooks::NONE
        };
        with_test_hooks(barred, || {
            assert!(placements(&state, content, POK, &player(), &system, &infantry).is_empty());
            let cruiser = types.get("cruiser").copied().expect("a cruiser");
            assert!(
                !placements(&state, content, POK, &player(), &system, &cruiser).is_empty(),
                "only the barred unit is affected"
            );
        });
    }
    /// Mitosis: a barred producer lends the barred unit no PRODUCTION. A dock plus a Hel-Titan on
    /// one planet; with docks barred from infantry, infantry may use only the Hel-Titan's share.
    #[test]
    fn a_barred_producers_production_does_not_count_for_the_barred_unit() {
        let (mut state, system, planet) = a_producing_game();
        put_on_planet(&mut state, &system, &planet, "titans_pds", &player(), 1);
        let content = ContentStore::embedded();
        let types = catalogue(content, POK);
        let infantry = types.get("infantry").copied().expect("an infantry");
        let infantry_batch = |state: &GameState| -> Option<String> {
            let window = ProductionWindow::new(state, content, POK, &player(), &system);
            window
                .pending_choice(state, content, POK)?
                .options
                .into_iter()
                .map(|option| option.id)
                .find(|id| id.starts_with("build|") && id.contains("infantry"))
        };
        assert_eq!(
            infantry_batch(&state)
                .as_deref()
                .map(|id| id.rsplit('|').next()),
            Some(Some("2")),
            "unbarred, a batch of two infantry is offered"
        );
        let barred = EconomyHooks {
            cannot_produce: Some(|_, _, _, unit, producer| {
                unit == "infantry" && producer == "spacedock"
            }),
            ..EconomyHooks::NONE
        };
        with_test_hooks(barred, || {
            let total = capacity(&state, content, POK, &player(), &system);
            let dock = barred_capacity(&state, content, POK, &player(), &system, &infantry);
            assert!(
                dock > 0 && dock < total,
                "the dock is a real share of the total"
            );
            let share = total - dock;
            let expected = share.min(2);
            match infantry_batch(&state) {
                Some(id) => assert_eq!(id.rsplit('|').next(), Some(expected.to_string().as_str())),
                None => assert_eq!(expected, 0),
            }
            // Ships are not barred, so their budget is the whole limit.
            assert_eq!(
                barred_capacity(
                    &state,
                    content,
                    POK,
                    &player(),
                    &system,
                    &types.get("cruiser").copied().unwrap()
                ),
                0
            );
        });
    }

    /// Infantry already produced this use spends the unbarred share: once it is used up, no
    /// further infantry is offered however much of the barred dock's PRODUCTION remains.
    #[test]
    fn infantry_already_produced_spends_the_unbarred_share() {
        let (mut state, system, planet) = a_producing_game();
        put_on_planet(&mut state, &system, &planet, "titans_pds", &player(), 1);
        let content = ContentStore::embedded();
        let infantry = catalogue(content, POK)
            .get("infantry")
            .copied()
            .expect("an infantry");
        let barred = EconomyHooks {
            cannot_produce: Some(|_, _, _, unit, producer| {
                unit == "infantry" && producer == "spacedock"
            }),
            ..EconomyHooks::NONE
        };
        with_test_hooks(barred, || {
            let total = capacity(&state, content, POK, &player(), &system);
            let share =
                total - barred_capacity(&state, content, POK, &player(), &system, &infantry);
            assert!(
                share > 0 && share < total,
                "the dock is a real share of the total"
            );
            let mut window = ProductionWindow::new(&state, content, POK, &player(), &system);
            // As if `share` infantry had already been bought this use, leaving dock PRODUCTION.
            for _ in 0..share {
                window
                    .report
                    .produced
                    .push((UnitTypeId::new("infantry"), planet.to_string()));
            }
            window.remaining = total - share;
            let offered = window
                .pending_choice(&state, content, POK)
                .map(|choice| choice.options)
                .unwrap_or_default();
            assert!(
                !offered
                    .iter()
                    .any(|option| option.id.starts_with("build|") && option.id.contains("infantry")),
                "the unbarred share is spent; the dock cannot make infantry"
            );
        });
    }

    /// With every producer barred, nothing of that unit is offered at all.
    #[test]
    fn infantry_is_not_offered_when_every_producer_is_barred() {
        let (state, system, _planet) = a_producing_game();
        let content = ContentStore::embedded();
        let barred = EconomyHooks {
            cannot_produce: Some(|_, _, _, unit, _| unit == "infantry"),
            ..EconomyHooks::NONE
        };
        with_test_hooks(barred, || {
            let window = ProductionWindow::new(&state, content, POK, &player(), &system);
            let offered = window
                .pending_choice(&state, content, POK)
                .is_some_and(|choice| {
                    choice.options.iter().any(|option| {
                        option.id.starts_with("build|") && option.id.contains("infantry")
                    })
                });
            assert!(!offered);
        });
    }

    /// `resolve_timed` carries the caller's timing handle, so Warfare secondary / Construction can
    /// announce `UNITS_PRODUCED`; with `None` it is `resolve`.
    #[test]
    fn resolve_timed_announces_units_produced_through_the_callers_handle() {
        let (mut state, system, _planet) = a_producing_game();
        let (mut resolver, seen) = listening_on("UNITS_PRODUCED");
        let mut sequence = crate::event::EventSequence::new();
        let mut table = Table::default();
        let report = resolve_timed(
            &mut state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            Some(crate::choice::TimingHandle {
                resolver: &mut resolver,
                sequence: &mut sequence,
                galaxy: None,
            }),
            &player(),
            &system,
        )
        .unwrap();
        assert!(!report.produced.is_empty());
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0]["source"], "production");
    }
}

#[cfg(test)]
mod mobile_dock_and_swap_tests {
    use super::*;
    use crate::fixtures::{a_placed_planet, game, put, put_on_planet};
    use ti4_model::content_types::POK;

    fn pid(id: &str) -> PlayerId {
        PlayerId::new(id)
    }

    // -- mobile space dock (Saar Floating Factory) ------------------------------------------------

    #[test]
    fn a_floating_factory_in_the_space_area_is_a_producer_and_counts_for_capacity() {
        let content = ContentStore::embedded();
        let mut state = game(&["a", "b"]);
        let (system, planet) = a_placed_planet();
        let a = pid("a");
        assert!(mobile_docks(&state, content, POK, &a, &system).is_empty());
        put(&mut state, &system, "saar_spacedock", &a, 1);
        assert_eq!(mobile_docks(&state, content, POK, &a, &system).len(), 1);
        assert!(mobile_docks(&state, content, POK, &pid("b"), &system).is_empty());
        let made = producers(&state, content, POK, &a, &system);
        assert_eq!(made.len(), 1);
        assert_eq!(made[0].1, None, "no planet: its flat printed value applies");
        assert_eq!(capacity(&state, content, POK, &a, &system), 5);

        // An ordinary dock on a planet is not a mobile dock, and adds its own value.
        put_on_planet(&mut state, &system, &planet, "spacedock", &a, 1);
        assert_eq!(mobile_docks(&state, content, POK, &a, &system).len(), 1);
        assert!(capacity(&state, content, POK, &a, &system) > 5);
    }

    #[test]
    fn it_is_still_the_same_plastic_as_a_space_dock() {
        let content = ContentStore::embedded();
        let mut state = game(&["a"]);
        let (system, _) = a_placed_planet();
        let a = pid("a");
        put(&mut state, &system, "saar_spacedock", &a, 3);
        assert_eq!(
            crate::supply::remaining(&state, content, POK, &a, &UnitTypeId::new("spacedock")),
            0,
            "three docks of any kind are the whole box"
        );
    }

    #[test]
    fn an_enemy_ship_with_none_of_its_own_destroys_a_floating_factory() {
        let content = ContentStore::embedded();
        let mut state = game(&["a", "b"]);
        let (system, planet) = a_placed_planet();
        let (a, b) = (pid("a"), pid("b"));
        put(&mut state, &system, "saar_spacedock", &a, 1);
        put_on_planet(&mut state, &system, &planet, "spacedock", &a, 1);

        // No enemy, or only ground forces: not blockaded.
        assert!(destroy_blockaded_mobile_docks(&mut state, content, POK, &system).is_empty());
        put_on_planet(&mut state, &system, &planet, "infantry", &b, 1);
        assert!(destroy_blockaded_mobile_docks(&mut state, content, POK, &system).is_empty());

        // An enemy ship, but the owner has a ship too: not blockaded.
        put(&mut state, &system, "cruiser", &b, 1);
        put(&mut state, &system, "destroyer", &a, 1);
        assert!(destroy_blockaded_mobile_docks(&mut state, content, POK, &system).is_empty());
        assert_eq!(mobile_docks(&state, content, POK, &a, &system).len(), 1);

        // The owner's last ship is gone: the dock is destroyed; the planet dock is not.
        let pos = state
            .system_state(&system)
            .units
            .iter()
            .position(|unit| unit.type_id.as_str() == "destroyer")
            .unwrap();
        state.system_mut(&system).units.remove(pos);
        let gone = destroy_blockaded_mobile_docks(&mut state, content, POK, &system);
        assert_eq!(gone.len(), 1);
        assert_eq!(gone[0].type_id.as_str(), "saar_spacedock");
        assert!(mobile_docks(&state, content, POK, &a, &system).is_empty());
        assert_eq!(
            structures_on(&state, content, POK, &a, &planet, "spacedock"),
            1
        );
        assert_eq!(
            crate::supply::remaining(&state, content, POK, &a, &UnitTypeId::new("spacedock")),
            2,
            "its plastic is back in the box"
        );
        assert!(destroy_blockaded_mobile_docks(&mut state, content, POK, &system).is_empty());
    }

    #[test]
    fn sling_relay_sees_a_floating_factory_as_a_space_dock() {
        let content = ContentStore::embedded();
        let mut state = game(&["a", "b"]);
        let (system, _) = a_placed_planet();
        let a = pid("a");
        state.player_mut(&a).unwrap().trade_goods = 10;
        assert!(!sling_relay_candidates(&state, content, POK, &a).contains_key(&system));
        put(&mut state, &system, "saar_spacedock", &a, 1);
        let with_dock = sling_relay_candidates(&state, content, POK, &a);
        assert!(
            with_dock
                .get(&system)
                .is_some_and(|ships| !ships.is_empty()),
            "a ship can be produced at a system whose only dock floats"
        );
        put(&mut state, &system, "cruiser", &pid("b"), 1);
        assert!(!sling_relay_candidates(&state, content, POK, &a).contains_key(&system));
    }

    // -- Hegemonic Trade Policy value swap --------------------------------------------------------

    /// A placed planet whose resources and influence differ, so a swap is visible.
    fn lopsided_planet() -> (SystemId, PlanetId) {
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
            .find(|(_, planet)| {
                planet_value(content, POK, planet, Spend::Resources)
                    != planet_value(content, POK, planet, Spend::Influence)
            })
            .expect("a planet with unequal values")
    }

    #[test]
    fn a_swap_reads_the_other_value_for_the_swapped_planet_only_and_only_during_its_use() {
        let content = ContentStore::embedded();
        let mut state = game(&["a"]);
        let (_, planet) = lopsided_planet();
        let other = PlanetId::new("elsewhere");
        let res = planet_value_now(&state, content, POK, &planet, Spend::Resources);
        let inf = planet_value_now(&state, content, POK, &planet, Spend::Influence);
        assert_ne!(res, inf);
        assert_eq!(
            swapped_value(&state, content, POK, &planet, Spend::Resources),
            None
        );

        state.production_seq = 3;
        begin_value_swap(&mut state, &planet);
        assert_eq!(swapped_planet(&state), Some(&planet));
        assert_eq!(
            swapped_value(&state, content, POK, &planet, Spend::Resources),
            Some(inf)
        );
        assert_eq!(
            swapped_value(&state, content, POK, &planet, Spend::Influence),
            Some(res)
        );
        assert_eq!(
            swapped_value(&state, content, POK, &other, Spend::Resources),
            None
        );

        // A later use of PRODUCTION does not inherit it, even if nobody cleared it.
        state.production_seq = 4;
        assert_eq!(swapped_planet(&state), None);
        assert_eq!(
            swapped_value(&state, content, POK, &planet, Spend::Resources),
            None
        );
        state.production_seq = 3;
        end_value_swap(&mut state);
        assert_eq!(swapped_planet(&state), None);
        assert!(state.faction_marks.is_empty(), "nothing is left behind");
        assert_eq!(state.production_value_swapped_planet, None);
    }

    #[test]
    fn the_swap_reaches_a_docks_production_through_the_planet_value_hook() {
        use crate::factions::hooks_economy::{EconomyHooks, with_test_hooks};
        let content = ContentStore::embedded();
        let mut state = game(&["a"]);
        let (system, planet) = lopsided_planet();
        let a = pid("a");
        put_on_planet(&mut state, &system, &planet, "spacedock", &a, 1);
        let inf = planet_value_now(&state, content, POK, &planet, Spend::Influence);
        let printed = capacity(&state, content, POK, &a, &system);

        let hooks = EconomyHooks {
            planet_spend_value: Some(|state, content, _, planet, kind, value| {
                swapped_value(state, content, POK, planet, kind).unwrap_or(value)
            }),
            ..EconomyHooks::NONE
        };
        with_test_hooks(hooks, || {
            assert_eq!(
                capacity(&state, content, POK, &a, &system),
                printed,
                "no swap yet"
            );
            begin_value_swap(&mut state, &planet);
            // Space dock: PRODUCTION is the planet's resources + 2; swapped, its influence + 2.
            assert_eq!(capacity(&state, content, POK, &a, &system), inf + 2);
            end_value_swap(&mut state);
            assert_eq!(capacity(&state, content, POK, &a, &system), printed);
        });
    }

    #[test]
    fn a_finished_production_use_ends_the_swap() {
        let content = ContentStore::embedded();
        let mut state = game(&["a"]);
        let (system, planet) = lopsided_planet();
        begin_value_swap(&mut state, &planet);
        let mut table = Table::new();
        // No producing unit: the window closes at once, and the use is over.
        let report = resolve(
            &mut state,
            content,
            POK,
            None,
            &mut table,
            &pid("a"),
            &system,
        )
        .unwrap();
        assert!(report.produced.is_empty());
        assert_eq!(state.production_value_swapped_planet, None);
        assert!(state.faction_marks.is_empty());
    }
}

#[cfg(test)]
mod bf_f3_tests {
    use super::*;
    use crate::factions::hooks_economy::{EconomyHooks, with_test_hooks};
    use crate::fixtures::{a_placed_planet, game, put_on_planet};
    use ti4_model::content_types::POK;

    fn pid(id: &str) -> PlayerId {
        PlayerId::new(id)
    }

    fn dock_game() -> (GameState, SystemId, PlanetId) {
        let mut state = game(&["a", "b"]);
        let (system, planet) = a_placed_planet();
        state
            .system_mut(&system)
            .set_control(planet.clone(), pid("a"));
        put_on_planet(&mut state, &system, &planet, "spacedock", &pid("a"), 1);
        state.player_mut(&pid("a")).unwrap().trade_goods = 20;
        (state, system, planet)
    }

    fn costs(window: &ProductionWindow, state: &GameState) -> Vec<(String, i64)> {
        window
            .pending_choice(state, ContentStore::embedded(), POK)
            .map(|choice| {
                choice
                    .options
                    .iter()
                    .filter_map(|option| {
                        Some((
                            option.payload.get("unit")?.as_str()?.to_owned(),
                            option.payload.get("cost")?.as_i64()?,
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn nomad_grant_arena() -> (GameState, SystemId, PlanetId) {
        let (mut state, system, planet) = dock_game();
        state.player_mut(&pid("a")).unwrap().faction = ti4_model::id::FactionId::new("sol");
        state.player_mut(&pid("a")).unwrap().trade_goods = 0;
        state.exhausted_planets.extend(
            state
                .controlled_planets(&pid("a"))
                .into_iter()
                .map(|(_, p)| p.clone())
                .collect::<Vec<_>>(),
        );
        (state, system, planet)
    }

    #[test]
    fn borrowed_nomad_commander_places_a_free_flagship_without_spending_discounts() {
        let (mut state, system, _) = nomad_grant_arena();
        let content = ContentStore::embedded();
        assert!(crate::promissory::grant_commander_ability(
            &mut state,
            content,
            &pid("a"),
            "nomadcommander"
        ));
        let mut window =
            ProductionWindow::for_ability(&state, content, POK, &pid("a"), &system, Some(2));
        window.discount_remaining = 3;
        window.credit = 1;
        assert!(
            !buildable_for(&state, content, POK, &pid("a")).contains(&"nomad_flagship".to_owned()),
            "borrowed rights do not add another faction's flagship"
        );
        let choice = window.pending_choice(&state, content, POK).unwrap();
        let option = choice
            .options
            .iter()
            .find(|option| option.id == "build|sol_flagship|1")
            .expect("free flagship is affordable with zero resources")
            .clone();
        assert_eq!(option.payload["cost"], 0);
        assert_eq!(option.payload["discount"], 0);
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(1);
        let mut table = Table::default();
        let mut ctx = Resolving {
            content,
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };
        window.resolve(&mut state, &mut ctx, option).unwrap();
        assert!(
            state
                .system_state(&system)
                .units
                .iter()
                .any(|u| u.owner == pid("a") && u.type_id.as_str() == "sol_flagship")
        );
        assert_eq!(window.remaining, 1, "free still consumes production");
        assert_eq!(window.discount_remaining, 3);
        assert_eq!(window.credit, 1);
        assert_eq!(state.player(&pid("a")).unwrap().trade_goods, 0);
    }

    #[test]
    fn nomad_grants_are_recipient_bound_and_a_locked_alliance_is_not_free() {
        let (mut state, system, _) = nomad_grant_arena();
        let content = ContentStore::embedded();
        assert!(crate::promissory::grant_commander_ability(
            &mut state,
            content,
            &pid("b"),
            "nomadcommander"
        ));
        state.player_mut(&pid("b")).unwrap().faction = ti4_model::id::FactionId::new("nomad");
        state.player_mut(&pid("b")).unwrap().leaders.insert(
            ti4_model::id::LeaderId::new("nomadcommander"),
            ti4_model::state::LeaderStatus::Locked,
        );
        state
            .promissory_notes
            .insert("an:nomad".to_owned(), pid("a"));
        state.promissory_faceup.insert("an:nomad".to_owned());
        let window =
            ProductionWindow::for_ability(&state, content, POK, &pid("a"), &system, Some(1));
        assert!(
            !window
                .pending_choice(&state, content, POK)
                .is_some_and(|choice| choice
                    .options
                    .iter()
                    .any(|o| o.id == "build|sol_flagship|1"))
        );
        state.player_mut(&pid("b")).unwrap().leaders.insert(
            ti4_model::id::LeaderId::new("nomadcommander"),
            ti4_model::state::LeaderStatus::Unlocked,
        );
        assert!(
            window
                .pending_choice(&state, content, POK)
                .unwrap()
                .options
                .iter()
                .any(|o| o.id == "build|sol_flagship|1")
        );
    }

    #[test]
    fn cabal_exemptions_are_recipient_bound_and_require_an_unlocked_alliance_owner() {
        let (mut state, system, _) = nomad_grant_arena();
        let content = ContentStore::embedded();
        state.player_mut(&pid("a")).unwrap().trade_goods = 10;
        assert!(crate::promissory::grant_commander_ability(
            &mut state,
            content,
            &pid("b"),
            "cabalcommander"
        ));
        state.player_mut(&pid("b")).unwrap().faction = ti4_model::id::FactionId::new("cabal");
        state.player_mut(&pid("b")).unwrap().leaders.insert(
            ti4_model::id::LeaderId::new("cabalcommander"),
            ti4_model::state::LeaderStatus::Locked,
        );
        state
            .promissory_notes
            .insert("an:cabal".to_owned(), pid("a"));
        state.promissory_faceup.insert("an:cabal".to_owned());
        let window =
            ProductionWindow::for_ability(&state, content, POK, &pid("a"), &system, Some(1));
        assert!(
            !window
                .pending_choice(&state, content, POK)
                .is_some_and(|choice| choice
                    .options
                    .iter()
                    .any(|o| o.id == "build|sol_infantry|2"))
        );
        state.player_mut(&pid("b")).unwrap().leaders.insert(
            ti4_model::id::LeaderId::new("cabalcommander"),
            ti4_model::state::LeaderStatus::Unlocked,
        );
        let window =
            ProductionWindow::for_ability(&state, content, POK, &pid("a"), &system, Some(1));
        assert!(
            window
                .pending_choice(&state, content, POK)
                .unwrap()
                .options
                .iter()
                .any(|o| o.id == "build|sol_infantry|2")
        );
    }

    #[test]
    fn borrowed_cabal_commander_preserves_two_small_unit_exemptions_after_normal_capacity() {
        for cruiser_first in [true, false] {
            let (mut state, system, _) = nomad_grant_arena();
            let content = ContentStore::embedded();
            state.player_mut(&pid("a")).unwrap().trade_goods = 10;
            assert!(crate::promissory::grant_commander_ability(
                &mut state,
                content,
                &pid("a"),
                "cabalcommander"
            ));
            let mut window =
                ProductionWindow::for_ability(&state, content, POK, &pid("a"), &system, Some(1));
            let mut dice = crate::dice::Dice::new();
            let mut rng = crate::rng::GameRng::new(1);
            let mut table = Table::default();
            let mut ctx = Resolving {
                content,
                sources: POK,
                dice: &mut dice,
                rng: &mut rng,
                table: &mut table,
                timing: None,
            };
            let ids = if cruiser_first {
                ["build|cruiser|1", "build|sol_infantry|2"]
            } else {
                ["build|sol_infantry|2", "build|cruiser|1"]
            };
            for id in ids {
                let choice = window
                    .pending_choice(&state, content, POK)
                    .expect("unused exemption keeps production open");
                let option = choice
                    .options
                    .iter()
                    .find(|option| option.id == id)
                    .unwrap_or_else(|| panic!("{id} must be legal; {:?}", choice.options))
                    .clone();
                if cruiser_first && id.contains("infantry") {
                    assert_eq!(window.remaining, 0);
                    assert!(
                        !choice
                            .options
                            .iter()
                            .any(|o| o.id.starts_with("build|sol_mech|")
                                || o.id.starts_with("build|cruiser|")),
                        "only fighters and infantry can use the extra allowance"
                    );
                    assert_eq!(option.payload["production_spent"], 0);
                }
                window.resolve(&mut state, &mut ctx, option).unwrap();
            }
            assert_eq!(window.remaining, 0);
            assert_eq!(window.report.produced.len(), 3);
            assert_eq!(
                state.player(&pid("a")).unwrap().trade_goods,
                7,
                "capacity exemption does not waive resource costs"
            );
            assert!(
                window.pending_choice(&state, content, POK).is_none(),
                "two exempt units exhaust the allowance"
            );
        }
    }

    #[test]
    fn a_module_grants_production_to_a_system_with_no_producer() {
        let content = ContentStore::embedded();
        let mut state = game(&["a", "b"]);
        state.player_mut(&pid("a")).unwrap().trade_goods = 20;
        let (system, _) = a_placed_planet();
        let a = pid("a");
        let types = catalogue(content, POK);
        let infantry = types.get("infantry").copied().expect("an infantry");
        assert_eq!(capacity(&state, content, POK, &a, &system), 0);
        assert!(placements(&state, content, POK, &a, &system, &infantry).is_empty());
        let reactor = EconomyHooks {
            extra_production: Some(|_, _, _, player, _| i64::from(player.as_str() == "a") * 5),
            ..EconomyHooks::NONE
        };
        with_test_hooks(reactor, || {
            assert_eq!(capacity(&state, content, POK, &a, &system), 5);
            assert_eq!(capacity(&state, content, POK, &pid("b"), &system), 0);
            assert!(
                placements(&state, content, POK, &a, &system, &infantry)
                    .contains(&SPACE.to_owned()),
                "a producer in the space area is a spot"
            );
            let cruiser = types.get("cruiser").copied().expect("a cruiser");
            assert_eq!(
                placements(&state, content, POK, &a, &system, &cruiser),
                vec![SPACE.to_owned()]
            );
            let window = ProductionWindow::new(&state, content, POK, &a, &system);
            assert!(!costs(&window, &state).is_empty(), "the window opens");
        });
    }

    #[test]
    fn a_planet_hook_adds_capacity_and_a_ground_spot() {
        let content = ContentStore::embedded();
        let mut state = game(&["a", "b"]);
        let (system, planet) = a_placed_planet();
        let a = pid("a");
        state
            .system_mut(&system)
            .set_control(planet.clone(), a.clone());
        put_on_planet(&mut state, &system, &planet, "pds", &a, 1);
        let types = catalogue(content, POK);
        let infantry = types.get("infantry").copied().expect("an infantry");
        assert_eq!(capacity(&state, content, POK, &a, &system), 0);
        assert!(placements(&state, content, POK, &a, &system, &infantry).is_empty());
        let hololattice = EconomyHooks {
            extra_production_planet: Some(|_, _, _, _, _, _| 1),
            ..EconomyHooks::NONE
        };
        with_test_hooks(hololattice, || {
            assert_eq!(capacity(&state, content, POK, &a, &system), 1);
            assert_eq!(
                placements(&state, content, POK, &a, &system, &infantry),
                vec![planet.to_string()]
            );
        });
    }

    #[test]
    fn a_cost_reduction_lowers_the_combined_bill_and_never_below_zero() {
        let (state, system, _) = dock_game();
        let content = ContentStore::embedded();
        let a = pid("a");
        let plain = ProductionWindow::new(&state, content, POK, &a, &system);
        let base = costs(&plain, &state);
        let cruiser = base
            .iter()
            .find(|(id, _)| id == "cruiser")
            .expect("cruiser")
            .1;
        assert_eq!(cruiser, 2);
        let synthesis = EconomyHooks {
            production_cost_reduction: Some(|_, _, _| 1),
            ..EconomyHooks::NONE
        };
        with_test_hooks(synthesis, || {
            let window = ProductionWindow::new(&state, content, POK, &a, &system);
            let reduced = costs(&window, &state);
            assert_eq!(reduced.iter().find(|(id, _)| id == "cruiser").unwrap().1, 1);
            assert!(reduced.iter().all(|(_, cost)| *cost >= 0));
        });
        let huge = EconomyHooks {
            production_cost_reduction: Some(|_, _, _| 50),
            ..EconomyHooks::NONE
        };
        with_test_hooks(huge, || {
            let window = ProductionWindow::new(&state, content, POK, &a, &system);
            assert!(costs(&window, &state).iter().all(|(_, cost)| *cost == 0));
        });
    }

    #[test]
    fn an_ability_may_cap_the_cost_of_each_unit() {
        let (state, system, _) = dock_game();
        let content = ContentStore::embedded();
        let a = pid("a");
        let open = ProductionWindow::for_ability(&state, content, POK, &a, &system, Some(2));
        let all: Vec<String> = costs(&open, &state).into_iter().map(|(id, _)| id).collect();
        assert!(all.iter().any(|id| id == "dreadnought"));
        let capped = ProductionWindow::for_ability(&state, content, POK, &a, &system, Some(2))
            .with_max_unit_cost(Some(2));
        let some: Vec<String> = costs(&capped, &state)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert!(some.iter().any(|id| id == "cruiser"), "cost 2 fits");
        assert!(
            !some.iter().any(|id| id == "dreadnought"),
            "cost 4 does not"
        );
        assert!(!some.iter().any(|id| id == "carrier"), "cost 3 does not");
    }

    #[test]
    fn production_with_no_timing_stages_units_produced_for_a_later_flush() {
        let content = ContentStore::embedded();
        let mut state = crate::fixtures::seated_game(&[("a", "mentak"), ("b", "sol")], POK);
        let (system, planet) = a_placed_planet();
        state
            .system_mut(&system)
            .set_control(planet.clone(), pid("a"));
        put_on_planet(&mut state, &system, &planet, "spacedock", &pid("a"), 1);
        state.player_mut(&pid("a")).unwrap().trade_goods = 20;
        let mut table = Table::default();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(1);
        {
            let mut quiet = Resolving {
                content,
                sources: POK,
                dice: &mut dice,
                rng: &mut rng,
                table: &mut table,
                timing: None,
            };
            let report =
                produce_by_ability(&mut state, &mut quiet, None, &pid("a"), &system, Some(1))
                    .expect("resolves");
            assert!(!report.produced.is_empty());
            assert_eq!(
                crate::supply::staged_event_types(&state),
                ["UNITS_PRODUCED"]
            );
            assert_eq!(
                crate::supply::flush_staged_events(&mut state, &mut quiet),
                0
            );
        }

        let mut resolver = crate::timing::Resolver::new(
            vec![pid("a"), pid("b")],
            Some(pid("a")),
            Table::default(),
        );
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = seen.clone();
        resolver.register([crate::timing::Ability::new(
            "test:listener",
            pid("a"),
            "UNITS_PRODUCED",
            crate::timing::Relation::After,
            std::sync::Arc::new(move |event, _| {
                sink.lock().unwrap().push(event.payload.clone());
                Ok(())
            }),
        )]);
        let mut sequence = crate::event::EventSequence::new();
        let mut table = Table::default();
        let mut loud = Resolving {
            content,
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
        assert_eq!(crate::supply::flush_staged_events(&mut state, &mut loud), 1);
        assert_eq!(crate::supply::staged_events(&state), 0);
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0]["source"], "ability");
    }

    #[test]
    fn a_game_with_no_module_seat_stages_nothing() {
        let (mut state, system, _) = dock_game();
        let content = ContentStore::embedded();
        let mut table = Table::default();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(1);
        let mut ctx = Resolving {
            content,
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };
        produce_by_ability(&mut state, &mut ctx, None, &pid("a"), &system, Some(1)).unwrap();
        assert_eq!(crate::supply::staged_events(&state), 0);
        assert!(state.faction_marks.is_empty());
    }
}
