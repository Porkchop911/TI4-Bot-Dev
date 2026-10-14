//! Component limitations (LRR 31.4): you cannot field more plastic than you own.
//!
//! Ported from the oracle's `engine/supply.py`, which records what its absence looked like:
//! across eight measured games a single player held **18 carriers against the four in the box,
//! 14 PDS against six, and 10 dreadnoughts against five**. Every bot was obeying its scoring
//! perfectly while doing something impossible, which is why no amount of scoring analysis found
//! it — a player spotted it in one screenshot of a live table.
//!
//! **Fighters and infantry are not capped, and that is a rule rather than an omission.** The box
//! ships cardboard fighter and infantry tokens in 1× and 3× denominations exactly so those two
//! are never limited by plastic, and the rulebook says to substitute if you run out of tokens
//! too. The oracle records capping them as its own first mistake: "58 infantry against the twelve
//! in the box" was its headline, and 58 infantry is legal.
//!
//! **Counts are per player and keyed by base type.** `sol_carrier2` and `carrier` are the same
//! four pieces of plastic — an upgrade swaps the card, not the model — so the cap is read through
//! the unit's base type rather than its id, or a faction upgrade would silently double a fleet.
//!
//! 31.4's escape hatch, removing one of your own from the board to place it elsewhere, is
//! deliberately not modelled here. It is a real option and belongs in the production step as a
//! choice; [`remaining`] answers only "how many may still be placed".

use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlayerId, UnitTypeId};
use ti4_model::state::GameState;

/// Anything not listed here is uncapped, which is how fighters and infantry pass through.
const UNCAPPED: i64 = 99;

/// Plastic per player, fourth edition plus Prophecy of Kings, keyed by base type.
#[must_use]
pub fn plastic(base_type: &str) -> Option<i64> {
    let count = match base_type {
        "flagship" => 1,
        "warsun" => 2,
        "dreadnought" => 5,
        "cruiser" | "destroyer" => 8,
        "carrier" | "mech" => 4,
        "pds" => 6,
        "spacedock" => 3,
        _ => return None,
    };
    Some(count)
}

/// The plastic a unit id corresponds to.
///
/// Read from the corpus rather than by matching on the id: `sol_carrier2` and `letnev_flagship`
/// do not decompose reliably, and a wrong answer here quietly doubles a cap.
#[must_use]
pub fn base_type_of(content: &ContentStore, sources: SourceSet, unit: &UnitTypeId) -> String {
    ti4_content::units::catalogue(content, sources)
        .get(unit.as_str())
        .map_or_else(|| unit.to_string(), |kind| kind.base_type().to_owned())
}

/// How many of this plastic the player has on the board, everywhere.
///
/// The space area and every planet: a carrier in space and a carrier parked over a planet are the
/// same model out of the same box. Captured units count too — they are off the board but still
/// out of their owner's supply.
#[must_use]
pub fn held(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    base_type: &str,
) -> i64 {
    // Every unit is counted against the same immutable source scope. Rebuilding its entire
    // catalogue for each owned unit made production offers scale with repeated catalogue work.
    let types = ti4_content::units::catalogue(content, sources);
    let matches_base = |unit: &UnitTypeId| {
        types
            .get(unit.as_str())
            .map_or(unit.as_str(), |kind| kind.base_type())
            == base_type
    };
    let mut total = 0;
    let mut count = |unit: &ti4_model::units::Unit| {
        if &unit.owner == player && matches_base(&unit.type_id) {
            total += 1;
        }
    };
    for board in state.board.values() {
        for unit in &board.units {
            count(unit);
        }
        for units in board.planet_units.values() {
            for unit in units {
                count(unit);
            }
        }
    }
    for captor in &state.players {
        for (owner, unit) in &captor.captured_units {
            if owner == player && matches_base(unit) {
                total += 1;
            }
        }
    }
    // Ships on the Crimson hero card are off the board and still out of the box.
    total +=
        crate::factions::crimson_cards::held_on_card(state, content, sources, player, base_type);
    total
}

/// How many more of this unit may be placed before the box is empty.
#[must_use]
pub fn remaining(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    unit: &UnitTypeId,
) -> i64 {
    let base = base_type_of(content, sources, unit);
    let Some(limit) = plastic(&base) else {
        return UNCAPPED;
    };
    (limit - held(state, content, sources, player, &base)).max(0)
}

/// How many of `wanted` may actually be placed, given what is left in the box.
///
/// Every ability that puts plastic on the board goes through this rather than doing its own
/// arithmetic, so a new ability gets the rule by using the helper rather than by remembering it.
/// Returns `wanted` unchanged for anything uncapped.
#[must_use]
pub fn allowed(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    unit: &UnitTypeId,
    wanted: usize,
) -> usize {
    if crate::factions::hooks_economy::effect_placement_forbidden(
        state, content, sources, player, unit,
    ) {
        return 0;
    }
    let left = remaining(state, content, sources, player, unit);
    usize::try_from(left).unwrap_or(0).min(wanted)
}

// -- trade goods gained (BF-00b-economy) -----------------------------------------------------------
//
// The engine gains trade goods by `seat.trade_goods += n` at roughly 45 sites in 25 files (listed
// in `plans/evidence/BF-00b-economy.md`); none can report "player X gained N" to a listener, and
// most run with no resolver in reach (a `TimingContext` or a plain `&mut GameState`). Rewriting
// them is outside this package. These helpers are the narrowest route: a call site that can reach
// a resolver replaces its `+=` with one of them and the gain becomes the `TRADE_GOODS_GAINED`
// event Mentak Pillage and the Mentak agent read. Neither changes how many goods are
// gained.

/// Give `player` `amount` trade goods and return how many were gained (`0` for a missing seat or a
/// non-positive amount). The same arithmetic as `seat.trade_goods += amount`, named, so a site
/// that cannot reach a resolver can still be found by grep when it gains one.
pub fn gain_trade_goods(state: &mut GameState, player: &PlayerId, amount: i32) -> i32 {
    if amount <= 0 {
        return 0;
    }
    state.player_mut(player).map_or(0, |seat| {
        seat.trade_goods += amount;
        amount
    })
}

fn goods_payload(
    player: &PlayerId,
    gained: i32,
    source: &str,
) -> std::collections::BTreeMap<String, serde_json::Value> {
    let mut payload = std::collections::BTreeMap::new();
    payload.insert("player".to_owned(), player.to_string().into());
    payload.insert("amount".to_owned(), gained.into());
    payload.insert("source".to_owned(), source.into());
    payload
}

/// [`gain_trade_goods`], then announce it as `TRADE_GOODS_GAINED` (`player`, `amount`, `source`) in
/// the caller's timing handle -- "after a player gains trade goods". `source` is a short label
/// (`"trade_primary"`, `"action_card"`, `"exploration"`, ...). Nothing is emitted for a gain of
/// zero; otherwise emission is unconditional.
pub fn gain_trade_goods_announced(
    state: &mut GameState,
    ctx: &mut crate::choice::Resolving<'_>,
    player: &PlayerId,
    amount: i32,
    source: &str,
) -> i32 {
    let gained = gain_trade_goods(state, player, amount);
    if gained > 0 {
        crate::factions::hooks_economy::emit(
            ctx,
            state,
            "TRADE_GOODS_GAINED",
            goods_payload(player, gained, source),
        );
    }
    gained
}

/// [`gain_trade_goods_announced`] for a rule effect that holds a `TimingContext` and the
/// `Resolver` (the shape of `reactions::announce`).
///
/// # Errors
/// [`crate::timing::TimingError`] when the announcement cannot be resolved; the goods have already
/// been gained.
pub fn gain_trade_goods_via(
    context: &mut crate::timing::TimingContext<'_>,
    resolver: &mut crate::timing::Resolver,
    player: &PlayerId,
    amount: i32,
    source: &str,
) -> Result<i32, crate::timing::TimingError> {
    let gained = gain_trade_goods(context.state, player, amount);
    if gained > 0 {
        let event = context
            .event_sequence
            .next("TRADE_GOODS_GAINED", goods_payload(player, gained, source))?;
        resolver.emit_with_context(context, event, |_, _| {})?;
    }
    Ok(gained)
}

// -- staged announcements (BF-F3) ----------------------------------------------------------------
//
// A site with a table but no timing handle (strategy-card primaries, relics, leader hooks, a
// production run with `Resolving::timing == None`) cannot announce an event. It STAGES it instead:
// the typed payload is kept in `GameState::faction_marks` under `staged:event:NNNNNN` and
// announced by [`flush_staged_events`], which the coordinator calls wherever a `Resolving` with a
// timing handle is in reach (after a leader action, a component action, a strategy-card primary).
// Staging happens only when a registered faction module is seated ([`staging_enabled`]), so a game
// with no module player records nothing and behaves exactly as before.

/// `private:#…` rows are visible to no seat (`ti4_model::view::mark_visible_to`), like the ground
/// staging rows: staged events are engine bookkeeping, not state any viewer should see.
const STAGED_EVENT_PREFIX: &str = "private:#staged:event:";

/// Whether a seated player's faction has a registered module, i.e. whether anything could react to
/// a staged event. With none, staging is skipped and state is untouched.
#[must_use]
pub fn staging_enabled(state: &GameState) -> bool {
    state.seating_order.iter().any(|player| {
        state.player(player).is_some_and(|seat| {
            // The Obsidian is the Firmament's other face: a seat that became it keeps the staging
            // the Firmament had (part B gives it a module of its own).
            seat.faction.as_str() == crate::factions::firmament::OBSIDIAN
                || crate::factions::MODULES
                    .iter()
                    .any(|module| module.alias == seat.faction.as_str())
        })
    })
}

/// Announce an actual Naaz mech placement so Absolute Synergy can resolve before the next action.
/// Other faction and unit placements preserve their existing event stream.
pub(crate) fn stage_naaz_mech_placed(
    state: &mut GameState,
    player: &PlayerId,
    system: &ti4_model::id::SystemId,
    unit: &UnitTypeId,
) {
    if unit.as_str() == "naaz_mech"
        && state
            .player(player)
            .is_some_and(|seat| seat.faction.as_str() == "naaz")
    {
        stage_event(
            state,
            "NAAZ_MECH_PLACED",
            &std::collections::BTreeMap::from([
                ("player".to_owned(), player.to_string().into()),
                ("system".to_owned(), system.to_string().into()),
            ]),
        );
    }
}

/// Keep a typed event for [`flush_staged_events`]. Returns whether it was staged (`false` when
/// [`staging_enabled`] is false).
pub fn stage_event(
    state: &mut GameState,
    event_type: &str,
    payload: &std::collections::BTreeMap<String, serde_json::Value>,
) -> bool {
    if !staging_enabled(state) {
        return false;
    }
    // Next index after the highest still staged, not the row count: a flush removes rows from the
    // front, and a reaction staging during it must not overwrite a row still waiting.
    let index = state
        .faction_marks
        .range(STAGED_EVENT_PREFIX.to_owned()..)
        .take_while(|(key, _)| key.starts_with(STAGED_EVENT_PREFIX))
        .filter_map(|(key, _)| key[STAGED_EVENT_PREFIX.len()..].parse::<usize>().ok())
        .max()
        .map_or(0, |highest| highest + 1);
    let record = serde_json::json!({ "type": event_type, "payload": payload });
    state.faction_marks.insert(
        format!("{STAGED_EVENT_PREFIX}{index:06}"),
        record.to_string(),
    );
    true
}

/// How many events are staged and not yet announced.
#[must_use]
pub fn staged_events(state: &GameState) -> usize {
    state
        .faction_marks
        .keys()
        .filter(|key| key.starts_with(STAGED_EVENT_PREFIX))
        .count()
}

/// The `type` of each staged event, in staging order.
#[must_use]
pub fn staged_event_types(state: &GameState) -> Vec<String> {
    state
        .faction_marks
        .iter()
        .filter(|(key, _)| key.starts_with(STAGED_EVENT_PREFIX))
        .filter_map(|(_, text)| serde_json::from_str::<serde_json::Value>(text).ok())
        .filter_map(|record| record.get("type")?.as_str().map(ToOwned::to_owned))
        .collect()
}

/// Announce every staged event (`UNITS_PRODUCED`, `TRADE_GOODS_GAINED`, `STRATEGY_TOKEN_SPENT`)
/// through `ctx`, in the order staged, clearing each first so a reaction that stages more events
/// is handled in the same call. Returns how many were announced; with `ctx.timing == None` it does
/// nothing and leaves them staged. This compatibility API retains its legacy behavior of
/// swallowing timing errors; new game-driver code should use [`try_flush_staged_events`].
///
/// THE FLUSH FUNCTION: the coordinator calls this after leader actions, component actions, and
/// strategy-card primaries / relic uses (`game.rs`), wherever a timing handle is in reach.
pub fn flush_staged_events(state: &mut GameState, ctx: &mut crate::choice::Resolving<'_>) -> usize {
    if ctx.timing.is_none() {
        return 0;
    }
    let mut announced = 0;
    while let Some(key) = state
        .faction_marks
        .keys()
        .find(|key| key.starts_with(STAGED_EVENT_PREFIX))
        .cloned()
    {
        let Some(text) = state.faction_marks.remove(&key) else {
            break;
        };
        let Ok(record) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let (Some(kind), Some(payload)) = (
            record.get("type").and_then(serde_json::Value::as_str),
            record
                .get("payload")
                .and_then(|payload| payload.as_object()),
        ) else {
            continue;
        };
        let payload: std::collections::BTreeMap<String, serde_json::Value> = payload
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        crate::factions::hooks_economy::emit(ctx, state, kind, payload);
        announced += 1;
    }
    announced
}

/// Fallibly announce all staged events in their deterministic order, including events staged by
/// reactions to earlier events. When a reaction fails, restore the state, dice, and RNG to their
/// values immediately before that event and leave its staged row available for retry.
///
/// Malformed staged records retain the compatibility behavior of being discarded. With no timing
/// handle, this returns `Ok(0)` and leaves every staged row untouched.
///
/// # Errors
/// Returns the first [`crate::timing::TimingError`] from the timing resolver. The failing event's
/// staged row is restored alongside its pre-emission game state.
pub fn try_flush_staged_events(
    state: &mut GameState,
    ctx: &mut crate::choice::Resolving<'_>,
) -> Result<usize, crate::timing::TimingError> {
    if ctx.timing.is_none() {
        return Ok(0);
    }
    let mut announced = 0;
    while let Some(key) = state
        .faction_marks
        .keys()
        .find(|key| key.starts_with(STAGED_EVENT_PREFIX))
        .cloned()
    {
        let Some(text) = state.faction_marks.get(&key).cloned() else {
            break;
        };
        let Ok(record) = serde_json::from_str::<serde_json::Value>(&text) else {
            state.faction_marks.remove(&key);
            continue;
        };
        let (Some(kind), Some(payload)) = (
            record.get("type").and_then(serde_json::Value::as_str),
            record
                .get("payload")
                .and_then(|payload| payload.as_object()),
        ) else {
            state.faction_marks.remove(&key);
            continue;
        };
        let kind = kind.to_owned();
        let payload: std::collections::BTreeMap<String, serde_json::Value> = payload
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();

        let before_state = state.clone();
        let before_log = ctx.table.log.clone();
        let before_dice = ctx.dice.clone();
        let before_rng = ctx.rng.clone();
        let before_timing = ctx
            .timing
            .as_ref()
            .map(|handle| (handle.sequence.clone(), handle.resolver.checkpoint()));
        state.faction_marks.remove(&key);
        if let Err(error) = ctx.emit(state, &kind, payload) {
            if let (Some(handle), Some((sequence, resolver))) = (ctx.timing.as_mut(), before_timing)
            {
                *handle.sequence = sequence;
                handle.resolver.restore(resolver);
            }
            ctx.table.log = before_log;
            *state = before_state;
            *ctx.dice = before_dice;
            *ctx.rng = before_rng;
            return Err(error);
        }
        announced += 1;
    }
    Ok(announced)
}

/// Stage `TRADE_GOODS_GAINED` for a gain the caller has already made (a site that adds to
/// `trade_goods` itself, e.g. while holding a borrow of the seat). A no-op for `amount <= 0` and
/// when no faction module is seated.
pub fn note_trade_goods_gained(
    state: &mut GameState,
    player: &PlayerId,
    amount: i32,
    source: &str,
) {
    if amount > 0 {
        stage_event(
            state,
            "TRADE_GOODS_GAINED",
            &goods_payload(player, amount, source),
        );
    }
}

/// Stage `COMMODITIES_GAINED` (`player`, `amount`) for a gain of commodities the caller has already
/// made, so Siphon (an Obsidian plot) can answer it. Staged only while a Siphon watches `player`:
/// nothing else listens, so every other game keeps its event stream exactly. A no-op for
/// `amount <= 0`.
pub fn note_commodities_gained(state: &mut GameState, player: &PlayerId, amount: i32) {
    if amount > 0 && crate::factions::firmament_plots::siphon_watches(state, player) {
        stage_event(
            state,
            crate::factions::firmament_plots::COMMODITIES_GAINED,
            &std::collections::BTreeMap::from([
                ("player".to_owned(), player.to_string().into()),
                ("amount".to_owned(), i64::from(amount).into()),
            ]),
        );
    }
}

/// [`gain_trade_goods`] for a site with no timing handle: the gain is made now and its
/// `TRADE_GOODS_GAINED` is staged for [`flush_staged_events`].
pub fn gain_trade_goods_staged(
    state: &mut GameState,
    player: &PlayerId,
    amount: i32,
    source: &str,
) -> i32 {
    let gained = gain_trade_goods(state, player, amount);
    if gained > 0 {
        stage_event(
            state,
            "TRADE_GOODS_GAINED",
            &goods_payload(player, gained, source),
        );
    }
    gained
}

// -- fixed trade-good spends (Keleres agent) -----------------------------------------------------
//
// "Spend N trade goods" costs with no payment window go through here, so Xander Alexin Victori III
// ("allow any player to spend commodities as if they were trade goods") reaches them all. The shape
// is: gate on [`potential_goods`] (what the agent could add), [`open_goods_window`] before the
// decision, re-gate on [`spendable_goods`] (what the window actually allows), decide, [`spend_goods`]
// (atomic, commodities first), then [`close_goods_window`]. Spent goods return to the supply, so the
// count simply decreases. Without a seated Keleres every one of these is the plain trade-good count.

/// Trade goods plus the commodities an open agent window lets `payer` spend as trade goods.
#[must_use]
pub(crate) fn spendable_goods(state: &GameState, payer: &PlayerId) -> i64 {
    let goods = state
        .player(payer)
        .map_or(0, |seat| i64::from(seat.trade_goods.max(0)));
    goods + crate::factions::keleres::spendable_commodities(state, payer)
}

/// [`spendable_goods`] plus what a readied Keleres agent could still grant (offered at
/// [`open_goods_window`]). The up-front legality gate of an option that costs trade goods.
#[must_use]
pub(crate) fn potential_goods(state: &GameState, payer: &PlayerId) -> i64 {
    let granted = if crate::factions::keleres::agent_could_grant(state, payer) {
        state
            .player(payer)
            .map_or(0, |seat| i64::from(seat.commodities.max(0)))
    } else {
        0
    };
    spendable_goods(state, payer) + granted
}

/// Offer the Keleres agent for a spend of at least `needed` trade goods by `payer`, once per window.
/// Returns `true` if it opened a window, which the caller must close with [`close_goods_window`].
/// Nothing is offered (and `false` returned) when the agent could not make `needed` reachable.
///
/// # Errors
/// [`IllegalChoice`] when a decider answers something not offered.
pub(crate) fn open_goods_window(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    table: &mut crate::choice::Table,
    payer: &PlayerId,
    needed: i64,
) -> Result<bool, crate::choice::IllegalChoice> {
    if potential_goods(state, payer) < needed {
        return Ok(false);
    }
    crate::factions::keleres::offer_agent(state, content, sources, galaxy, table, payer)
}

/// End a window [`open_goods_window`] opened.
pub(crate) fn close_goods_window(state: &mut GameState, payer: &PlayerId, opened: bool) {
    if opened {
        crate::factions::keleres::close_agent_window(state, payer);
    }
}

/// Spend `amount` trade goods, commodities an open agent window allows first. All of it or none:
/// `false`, state untouched, when [`spendable_goods`] falls short.
pub(crate) fn spend_goods(state: &mut GameState, payer: &PlayerId, amount: i32) -> bool {
    if amount <= 0 {
        return true;
    }
    if spendable_goods(state, payer) < i64::from(amount) {
        return false;
    }
    let commodities = i32::try_from(crate::factions::keleres::spendable_commodities(
        state, payer,
    ))
    .unwrap_or(0);
    let Some(seat) = state.player_mut(payer) else {
        return false;
    };
    let from_commodities = amount.min(commodities);
    seat.commodities -= from_commodities;
    seat.trade_goods -= amount - from_commodities;
    true
}

/// Run `body` inside a goods window for `payer` ([`open_goods_window`] before, close after, also
/// when `body` returns early). `None` when the offer itself was answered illegally.
pub(crate) fn with_goods_window<T>(
    context: &mut crate::timing::TimingContext<'_>,
    payer: &PlayerId,
    needed: i64,
    body: impl FnOnce(&mut crate::timing::TimingContext<'_>) -> T,
) -> Option<T> {
    let opened = open_goods_window(
        context.state,
        context.content,
        context.sources,
        context.galaxy,
        context.table,
        payer,
        needed,
    )
    .ok()?;
    let result = body(context);
    close_goods_window(context.state, payer, opened);
    Some(result)
}

/// A whole fixed spend with nothing to decide between the offer and the payment: offer the agent,
/// then spend `amount`. `Ok(false)`, nothing spent, when it cannot be afforded.
///
/// # Errors
/// [`IllegalChoice`] when a decider answers the agent offer with something not offered.
pub(crate) fn pay_goods_seeing(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    table: &mut crate::choice::Table,
    payer: &PlayerId,
    amount: i32,
) -> Result<bool, crate::choice::IllegalChoice> {
    let opened = open_goods_window(
        state,
        content,
        sources,
        galaxy,
        table,
        payer,
        i64::from(amount),
    )?;
    let paid = spend_goods(state, payer, amount);
    close_goods_window(state, payer, opened);
    Ok(paid)
}

fn token_spent_payload(
    player: &PlayerId,
    reason: &str,
) -> std::collections::BTreeMap<String, serde_json::Value> {
    let mut payload = std::collections::BTreeMap::new();
    payload.insert("player".to_owned(), player.to_string().into());
    payload.insert("reason".to_owned(), reason.into());
    payload
}

/// Spend 1 token from `player`'s strategy pool and announce `STRATEGY_TOKEN_SPENT` (`player`,
/// `reason`), for Muaat Magmus ("after you spend a token from your strategy pool"). Returns whether
/// a token was spent (`false`, and nothing announced, when the pool is empty).
pub fn spend_strategy_token_announced(
    state: &mut GameState,
    ctx: &mut crate::choice::Resolving<'_>,
    player: &PlayerId,
    reason: &str,
) -> bool {
    let spent = state
        .player_mut(player)
        .is_some_and(|seat| seat.spend_token(ti4_model::state::TokenPool::Strategic));
    if spent {
        crate::factions::hooks_economy::emit(
            ctx,
            state,
            "STRATEGY_TOKEN_SPENT",
            token_spent_payload(player, reason),
        );
    }
    spent
}

/// Stage `STRATEGY_TOKEN_SPENT` for a token the caller has already spent from `player`'s strategy
/// pool (a site that pays the token itself). A no-op when no faction module is seated.
pub fn note_strategy_token_spent(state: &mut GameState, player: &PlayerId, reason: &str) {
    stage_event(
        state,
        "STRATEGY_TOKEN_SPENT",
        &token_spent_payload(player, reason),
    );
}

/// [`spend_strategy_token_announced`] for a site with no timing handle: the event is staged.
pub fn spend_strategy_token_staged(state: &mut GameState, player: &PlayerId, reason: &str) -> bool {
    let spent = state
        .player_mut(player)
        .is_some_and(|seat| seat.spend_token(ti4_model::state::TokenPool::Strategic));
    if spent {
        stage_event(
            state,
            "STRATEGY_TOKEN_SPENT",
            &token_spent_payload(player, reason),
        );
    }
    spent
}

// -- capture (BF-00d-strategy) ----------------------------------------------------------------------
//
// Captured units live in `Player::captured_units` of the player who holds them, as `(owner, unit
// type)`. They are off the board and out of the owner's reinforcements: `held` above counts them
// against the owner, so `remaining`/`allowed` (31.4) already see a captured model as gone, and
// nothing here has to adjust a count. The functions below only move a model between the three
// places it can be (the board, a reinforcements box, a captor's sheet), each atomically.
//
// Rules, from the Prophecy of Kings Living Rules Reference "Capture" entry as the task brief gave
// it (the rule text is not in the content corpus, so none of these is checked against a printed
// source; see the rules questions in `plans/evidence/BF-00d-strategy.md`): a captured unit is
// removed from play and placed on the capturing player's faction sheet; it is not in its owner's
// reinforcements, so its owner cannot place it; the captor may return it to its owner's
// reinforcements.

/// What `captor` is holding captured, as `(owner, unit type)` in capture order.
#[must_use]
pub fn captured_by<'a>(state: &'a GameState, captor: &PlayerId) -> &'a [(PlayerId, UnitTypeId)] {
    state
        .player(captor)
        .map_or(&[], |seat| seat.captured_units.as_slice())
}

/// How many of `owner`'s models of this base type are sitting captured, with anyone.
#[must_use]
pub fn captured_of(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    owner: &PlayerId,
    base_type: &str,
) -> usize {
    state
        .players
        .iter()
        .flat_map(|seat| &seat.captured_units)
        .filter(|(who, unit)| who == owner && base_type_of(content, sources, unit) == base_type)
        .count()
}

/// Capture a unit standing on the board: take it off `system` (its space area, or `planet`) and
/// onto `captor`'s sheet. Returns `false`, changing nothing, when the unit is not there, when the
/// captor or owner is not seated, or when they are the same player.
///
/// Mentak and Vuil'raith capture units that are being destroyed: the caller removes it from the
/// destruction and captures it instead, so no `SHIP_DESTROYED`-style announcement is made here.
pub fn capture_from_board(
    state: &mut GameState,
    captor: &PlayerId,
    system: &ti4_model::id::SystemId,
    planet: Option<&ti4_model::id::PlanetId>,
    unit: &ti4_model::units::Unit,
) -> bool {
    if captor == &unit.owner
        || state.player(captor).is_none()
        || state.player(&unit.owner).is_none()
    {
        return false;
    }
    let Some(board) = state.board.get_mut(system) else {
        return false;
    };
    let pool = match planet {
        Some(planet) => board.planet_units.get_mut(planet),
        None => Some(&mut board.units),
    };
    let Some(pool) = pool else {
        return false;
    };
    let Some(index) = pool.iter().position(|found| found == unit) else {
        return false;
    };
    pool.remove(index);
    if let Some(seat) = state.player_mut(captor) {
        seat.captured_units
            .push((unit.owner.clone(), unit.type_id.clone()));
    }
    true
}

/// Capture a unit out of `owner`'s reinforcements (a captor effect that says "from your
/// opponent's reinforcements"). Needs a model left in the box (31.4: `remaining`); uncapped
/// fighters and infantry always have one. Returns `false`, changing nothing, otherwise.
pub fn capture_from_reinforcements(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    captor: &PlayerId,
    owner: &PlayerId,
    unit: &UnitTypeId,
) -> bool {
    if captor == owner
        || state.player(captor).is_none()
        || state.player(owner).is_none()
        || remaining(state, content, sources, owner, unit) == 0
    {
        return false;
    }
    if let Some(seat) = state.player_mut(captor) {
        seat.captured_units.push((owner.clone(), unit.clone()));
    }
    true
}

/// Return one captured model of `owner`'s base type from `captor`'s sheet to `owner`'s
/// reinforcements. The unit type that was captured is returned, `None` (nothing changes) when the
/// captor holds none of that base type for that owner.
///
/// "Reinforcements" is not stored: a model is in the box exactly when it is neither on the board
/// nor captured, so removing the record is the return.
pub fn return_captured(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    captor: &PlayerId,
    owner: &PlayerId,
    base_type: &str,
) -> Option<UnitTypeId> {
    let seat = state.player_mut(captor)?;
    let index = seat.captured_units.iter().position(|(who, unit)| {
        who == owner && base_type_of(content, sources, unit) == base_type
    })?;
    let unit = seat.captured_units.remove(index).1;
    Some(crate::factions::hooks_economy::captured_unit_return_form(
        state, content, sources, owner, &unit,
    ))
}

/// Return everything `captor` holds captured to its owners, e.g. when the captor leaves the game.
/// Returns how many models went back. The caller decides when this applies; the rule is recorded
/// as a question in the evidence file.
pub fn release_all_captured(state: &mut GameState, captor: &PlayerId) -> usize {
    state
        .player_mut(captor)
        .map_or(0, |seat| std::mem::take(&mut seat.captured_units).len())
}

fn capture_payload(
    captor: &PlayerId,
    owner: &PlayerId,
    unit: &UnitTypeId,
    source: &str,
) -> std::collections::BTreeMap<String, serde_json::Value> {
    let mut payload = std::collections::BTreeMap::new();
    payload.insert("player".to_owned(), captor.to_string().into());
    payload.insert("owner".to_owned(), owner.to_string().into());
    payload.insert("unit".to_owned(), unit.to_string().into());
    payload.insert("source".to_owned(), source.into());
    payload
}

/// [`capture_from_board`], then announce `UNIT_CAPTURED` (`player` = the captor, `owner`, `unit`,
/// `source`) in the caller's timing handle. Nothing is emitted when the capture did not happen.
pub fn capture_from_board_announced(
    state: &mut GameState,
    ctx: &mut crate::choice::Resolving<'_>,
    captor: &PlayerId,
    system: &ti4_model::id::SystemId,
    planet: Option<&ti4_model::id::PlanetId>,
    unit: &ti4_model::units::Unit,
    source: &str,
) -> bool {
    let done = capture_from_board(state, captor, system, planet, unit);
    if done {
        crate::factions::hooks_economy::emit(
            ctx,
            state,
            "UNIT_CAPTURED",
            capture_payload(captor, &unit.owner, &unit.type_id, source),
        );
    }
    done
}

/// [`return_captured`], then announce `CAPTURED_UNIT_RETURNED` (`player` = the captor, `owner`,
/// `unit`, `source`). Nothing is emitted when nothing was returned.
pub fn return_captured_announced(
    state: &mut GameState,
    ctx: &mut crate::choice::Resolving<'_>,
    captor: &PlayerId,
    owner: &PlayerId,
    base_type: &str,
    source: &str,
) -> Option<UnitTypeId> {
    let returned = return_captured(state, ctx.content, ctx.sources, captor, owner, base_type)?;
    crate::factions::hooks_economy::emit(
        ctx,
        state,
        "CAPTURED_UNIT_RETURNED",
        capture_payload(captor, owner, &returned, source),
    );
    Some(returned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{a_placed_planet, game, put, put_on_planet};
    use ti4_model::content_types::POK;

    fn player() -> PlayerId {
        PlayerId::new("a")
    }

    #[test]
    fn catalogue_reuse_preserves_counts_for_every_corpus_unit_source_scope_and_location() {
        let content = ContentStore::embedded();
        let a = player();
        let b = PlayerId::new("b");
        let mut state = game(&["a", "b"]);
        let (system, planet) = a_placed_planet();
        let types = ti4_content::units::catalogue(content, ti4_model::content_types::DEFAULT);
        // Include unknown ids: falling back to the original id is part of the existing contract.
        let mut ids: Vec<&str> = types.keys().copied().collect();
        ids.push("unknown_supply_fixture");
        for (index, id) in ids.iter().enumerate() {
            let owner = if index % 2 == 0 { &a } else { &b };
            match index % 3 {
                0 => put(&mut state, &system, id, owner, 1),
                1 => put_on_planet(&mut state, &system, &planet, id, owner, 1),
                _ => state
                    .player_mut(&b)
                    .expect("captor")
                    .captured_units
                    .push((owner.clone(), UnitTypeId::new(*id))),
            }
        }
        // The reference deliberately calls the original public resolver per unit. It is slow,
        // but independently prices all corpus ids under each scope, including excluded records.
        for sources in [
            ti4_model::content_types::BASE,
            POK,
            ti4_model::content_types::DEFAULT,
        ] {
            for owner in [&a, &b, &PlayerId::new("absent")] {
                let mut expected: std::collections::BTreeMap<String, i64> =
                    std::collections::BTreeMap::new();
                for board in state.board.values() {
                    for unit in board
                        .units
                        .iter()
                        .chain(board.planet_units.values().flatten())
                    {
                        if &unit.owner == owner {
                            *expected
                                .entry(base_type_of(content, sources, &unit.type_id))
                                .or_default() += 1;
                        }
                    }
                }
                for captor in &state.players {
                    for (original_owner, unit) in &captor.captured_units {
                        if original_owner == owner {
                            *expected
                                .entry(base_type_of(content, sources, unit))
                                .or_default() += 1;
                        }
                    }
                }
                expected
                    .entry("missing_base_fixture".to_owned())
                    .or_default();
                for (base, count) in expected {
                    assert_eq!(
                        held(&state, content, sources, owner, &base),
                        count,
                        "{sources:?} {owner} {base}"
                    );
                }
            }
        }
    }

    #[test]
    fn fighters_and_infantry_are_not_plastic() {
        // The oracle's own recorded mistake: capping these was wrong. The box ships cardboard
        // tokens for both exactly so they are never limited, and 58 infantry is legal.
        assert_eq!(plastic("fighter"), None);
        assert_eq!(plastic("infantry"), None);
        assert_eq!(plastic("carrier"), Some(4));

        let state = game(&["a"]);
        assert_eq!(
            allowed(
                &state,
                ContentStore::embedded(),
                POK,
                &player(),
                &UnitTypeId::new("infantry"),
                50
            ),
            50,
            "an uncapped unit passes through untouched"
        );
    }

    #[test]
    fn the_cap_counts_what_is_already_on_the_board() {
        let mut state = game(&["a"]);
        let (system, _) = a_placed_planet();
        let carrier = UnitTypeId::new("carrier");

        assert_eq!(
            allowed(
                &state,
                ContentStore::embedded(),
                POK,
                &player(),
                &carrier,
                6
            ),
            4,
            "four in the box"
        );

        put(&mut state, &system, "carrier", &player(), 3);
        assert_eq!(
            allowed(
                &state,
                ContentStore::embedded(),
                POK,
                &player(),
                &carrier,
                6
            ),
            1,
            "three are already out"
        );

        put(&mut state, &system, "carrier", &player(), 1);
        assert_eq!(
            allowed(
                &state,
                ContentStore::embedded(),
                POK,
                &player(),
                &carrier,
                6
            ),
            0,
            "and now the box is empty"
        );
    }

    #[test]
    fn a_carrier_over_a_planet_is_the_same_model_as_one_in_space() {
        let mut state = game(&["a"]);
        let (system, planet) = a_placed_planet();
        put(&mut state, &system, "carrier", &player(), 2);
        put_on_planet(&mut state, &system, &planet, "carrier", &player(), 2);

        assert_eq!(
            held(&state, ContentStore::embedded(), POK, &player(), "carrier"),
            4,
            "counted wherever they sit"
        );
    }

    #[test]
    fn an_upgrade_is_the_same_plastic() {
        // `sol_carrier2` and `carrier` are the same four models. Reading the id rather than the
        // base type would let a faction upgrade double the fleet.
        let content = ContentStore::embedded();
        let upgraded = ti4_content::units::catalogue(content, POK)
            .iter()
            .find(|(id, kind)| kind.base_type() == "carrier" && **id != "carrier")
            .map(|(id, _)| UnitTypeId::new(*id));
        let Some(upgraded) = upgraded else {
            return; // this corpus has no carrier upgrade
        };

        let mut state = game(&["a"]);
        let (system, _) = a_placed_planet();
        put(&mut state, &system, upgraded.as_str(), &player(), 4);

        assert_eq!(
            allowed(
                &state,
                content,
                POK,
                &player(),
                &UnitTypeId::new("carrier"),
                2
            ),
            0,
            "four upgraded carriers are four carriers"
        );
    }

    #[test]
    fn another_players_fleet_is_not_yours() {
        let mut state = game(&["a", "b"]);
        let (system, _) = a_placed_planet();
        put(&mut state, &system, "carrier", &PlayerId::new("b"), 4);

        assert_eq!(
            allowed(
                &state,
                ContentStore::embedded(),
                POK,
                &player(),
                &UnitTypeId::new("carrier"),
                4
            ),
            4,
            "the cap is per player"
        );
    }

    #[test]
    fn a_captured_unit_is_still_out_of_its_owners_supply() {
        let mut state = game(&["a", "b"]);
        state
            .player_mut(&PlayerId::new("b"))
            .unwrap()
            .captured_units = vec![(player(), UnitTypeId::new("carrier")); 4];

        assert_eq!(
            allowed(
                &state,
                ContentStore::embedded(),
                POK,
                &player(),
                &UnitTypeId::new("carrier"),
                4
            ),
            0,
            "held by somebody else, but still not in the box"
        );
    }
    type Seen = std::sync::Arc<
        std::sync::Mutex<Vec<std::collections::BTreeMap<String, serde_json::Value>>>,
    >;

    fn resolver_listening(seen: &Seen) -> crate::timing::Resolver {
        let sink = seen.clone();
        let mut resolver = crate::timing::Resolver::new(
            vec![player()],
            Some(player()),
            crate::choice::Table::default(),
        );
        resolver.register([crate::timing::Ability::new(
            "test:pillage",
            player(),
            "TRADE_GOODS_GAINED",
            crate::timing::Relation::After,
            std::sync::Arc::new(move |event, _| {
                sink.lock().unwrap().push(event.payload.clone());
                Ok(())
            }),
        )]);
        resolver
    }

    #[test]
    fn gaining_trade_goods_through_the_helper_emits_the_event_with_its_payload() {
        let mut state = game(&["a"]);
        state.player_mut(&player()).unwrap().trade_goods = 1;
        let seen = Seen::default();
        let mut resolver = resolver_listening(&seen);
        let mut sequence = crate::event::EventSequence::new();
        let mut table = crate::choice::Table::default();
        crate::fixtures::with_context(&mut state, POK, None, &mut table, |context| {
            let gained =
                gain_trade_goods_via(context, &mut resolver, &player(), 3, "test").unwrap();
            assert_eq!(gained, 3);
            assert_eq!(context.state.player(&player()).unwrap().trade_goods, 4);
        });
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0]["player"], "a");
        assert_eq!(seen[0]["amount"], 3);
        assert_eq!(seen[0]["source"], "test");
        drop(seen);

        // The `Resolving` flavour announces the same event.
        let seen2 = Seen::default();
        let mut resolver = resolver_listening(&seen2);
        let (mut dice, mut rng) = (crate::dice::Dice::new(), crate::rng::GameRng::new(0));
        let mut ctx = crate::choice::Resolving {
            content: ti4_content::ContentStore::embedded(),
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
        assert_eq!(
            gain_trade_goods_announced(&mut state, &mut ctx, &player(), 2, "again"),
            2
        );
        assert_eq!(state.player(&player()).unwrap().trade_goods, 6);
        assert_eq!(seen2.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_zero_gain_emits_nothing_and_a_real_one_always_does() {
        let mut state = game(&["a"]);
        let mut resolver = crate::timing::Resolver::new(
            vec![player()],
            Some(player()),
            crate::choice::Table::default(),
        );
        let mut table = crate::choice::Table::default();
        crate::fixtures::with_context(&mut state, POK, None, &mut table, |context| {
            assert_eq!(
                gain_trade_goods_via(context, &mut resolver, &player(), 2, "quiet").unwrap(),
                2
            );
            assert_eq!(
                gain_trade_goods_via(context, &mut resolver, &player(), 0, "none").unwrap(),
                0
            );
        });
        assert_eq!(
            resolver
                .log()
                .iter()
                .filter(|line| line.starts_with("emit TRADE_GOODS_GAINED"))
                .count(),
            1,
            "one emission, for the non-zero gain only"
        );
    }
}

#[cfg(test)]
mod capture_tests {
    use super::*;
    use crate::fixtures::{a_placed_planet, game, put, put_on_planet};
    use ti4_model::content_types::POK;
    use ti4_model::units::Unit;

    fn pid(id: &str) -> PlayerId {
        PlayerId::new(id)
    }

    fn carrier(owner: &str) -> Unit {
        Unit::new(UnitTypeId::new("carrier"), pid(owner))
    }

    #[test]
    fn a_captured_ship_leaves_the_board_but_not_its_owners_plastic() {
        let content = ContentStore::embedded();
        let mut state = game(&["a", "b"]);
        let (system, _) = a_placed_planet();
        put(&mut state, &system, "carrier", &pid("b"), 4);
        assert_eq!(
            remaining(&state, content, POK, &pid("b"), &UnitTypeId::new("carrier")),
            0
        );

        assert!(capture_from_board(
            &mut state,
            &pid("a"),
            &system,
            None,
            &carrier("b")
        ));
        assert_eq!(state.system_state(&system).units_of(&pid("b")).len(), 3);
        assert_eq!(
            captured_by(&state, &pid("a")),
            &[(pid("b"), UnitTypeId::new("carrier"))]
        );
        assert_eq!(captured_of(&state, content, POK, &pid("b"), "carrier"), 1);
        assert_eq!(
            held(&state, content, POK, &pid("b"), "carrier"),
            4,
            "still out of the box"
        );
        assert_eq!(
            allowed(
                &state,
                content,
                POK,
                &pid("b"),
                &UnitTypeId::new("carrier"),
                1
            ),
            0,
            "31.4: the owner cannot place the captured model"
        );
        assert_eq!(
            held(&state, content, POK, &pid("a"), "carrier"),
            0,
            "it is not the captor's"
        );
    }

    #[test]
    fn a_returned_model_goes_back_to_its_owners_reinforcements() {
        let content = ContentStore::embedded();
        let mut state = game(&["a", "b"]);
        let (system, _) = a_placed_planet();
        put(&mut state, &system, "carrier", &pid("b"), 4);
        capture_from_board(&mut state, &pid("a"), &system, None, &carrier("b"));

        assert_eq!(
            return_captured(&mut state, content, POK, &pid("a"), &pid("b"), "carrier"),
            Some(UnitTypeId::new("carrier"))
        );
        assert!(captured_by(&state, &pid("a")).is_empty());
        assert_eq!(
            allowed(
                &state,
                content,
                POK,
                &pid("b"),
                &UnitTypeId::new("carrier"),
                1
            ),
            1,
            "the model is in the box again"
        );
        // Nothing left to return, or the wrong owner or type: nothing changes.
        assert_eq!(
            return_captured(&mut state, content, POK, &pid("a"), &pid("b"), "carrier"),
            None
        );
        capture_from_board(&mut state, &pid("a"), &system, None, &carrier("b"));
        assert_eq!(
            return_captured(&mut state, content, POK, &pid("a"), &pid("b"), "cruiser"),
            None
        );
        assert_eq!(
            return_captured(&mut state, content, POK, &pid("b"), &pid("b"), "carrier"),
            None
        );
        assert_eq!(captured_by(&state, &pid("a")).len(), 1);
    }

    #[test]
    fn a_ground_force_is_captured_off_its_planet() {
        let mut state = game(&["a", "b"]);
        let (system, planet) = a_placed_planet();
        put_on_planet(&mut state, &system, &planet, "mech", &pid("b"), 1);
        let mech = Unit::new(UnitTypeId::new("mech"), pid("b"));
        // The wrong place is refused; the planet works.
        assert!(!capture_from_board(
            &mut state,
            &pid("a"),
            &system,
            None,
            &mech
        ));
        assert!(capture_from_board(
            &mut state,
            &pid("a"),
            &system,
            Some(&planet),
            &mech
        ));
        assert!(
            state
                .system_state(&system)
                .planet_units
                .get(&planet)
                .is_none_or(Vec::is_empty)
        );
        assert_eq!(captured_by(&state, &pid("a")).len(), 1);
    }

    #[test]
    fn an_impossible_capture_changes_nothing() {
        let mut state = game(&["a", "b"]);
        let (system, _) = a_placed_planet();
        put(&mut state, &system, "carrier", &pid("b"), 1);
        let before = serde_json::to_value(&state).unwrap();
        // Not there; own unit; unseated captor.
        assert!(!capture_from_board(
            &mut state,
            &pid("a"),
            &system,
            None,
            &Unit::new(UnitTypeId::new("dreadnought"), pid("b"))
        ));
        assert!(!capture_from_board(
            &mut state,
            &pid("b"),
            &system,
            None,
            &carrier("b")
        ));
        assert!(!capture_from_board(
            &mut state,
            &pid("zed"),
            &system,
            None,
            &carrier("b")
        ));
        assert!(!capture_from_board(
            &mut state,
            &pid("a"),
            &ti4_model::id::SystemId::new("none"),
            None,
            &carrier("b")
        ));
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
    }

    #[test]
    fn capturing_from_reinforcements_needs_a_model_in_the_box() {
        let content = ContentStore::embedded();
        let mut state = game(&["a", "b"]);
        let (system, _) = a_placed_planet();
        let dread = UnitTypeId::new("dreadnought");
        put(&mut state, &system, "dreadnought", &pid("b"), 5);
        assert!(!capture_from_reinforcements(
            &mut state,
            content,
            POK,
            &pid("a"),
            &pid("b"),
            &dread
        ));
        assert!(captured_by(&state, &pid("a")).is_empty());

        state.system_mut(&system).units.pop();
        assert!(capture_from_reinforcements(
            &mut state,
            content,
            POK,
            &pid("a"),
            &pid("b"),
            &dread
        ));
        assert_eq!(remaining(&state, content, POK, &pid("b"), &dread), 0);
        // Fighters are uncapped: always capturable.
        assert!(capture_from_reinforcements(
            &mut state,
            content,
            POK,
            &pid("a"),
            &pid("b"),
            &UnitTypeId::new("fighter")
        ));
    }

    #[test]
    fn a_captor_leaving_returns_everything() {
        let content = ContentStore::embedded();
        let mut state = game(&["a", "b", "c"]);
        let (system, _) = a_placed_planet();
        put(&mut state, &system, "carrier", &pid("b"), 2);
        put(&mut state, &system, "cruiser", &pid("c"), 1);
        capture_from_board(&mut state, &pid("a"), &system, None, &carrier("b"));
        capture_from_board(
            &mut state,
            &pid("a"),
            &system,
            None,
            &Unit::new(UnitTypeId::new("cruiser"), pid("c")),
        );
        assert_eq!(release_all_captured(&mut state, &pid("a")), 2);
        assert!(captured_by(&state, &pid("a")).is_empty());
        assert_eq!(held(&state, content, POK, &pid("b"), "carrier"), 1);
        assert_eq!(release_all_captured(&mut state, &pid("a")), 0);
    }

    #[test]
    fn announced_capture_and_return_report_success_and_stay_quiet_otherwise() {
        let mut state = game(&["a", "b"]);
        let (system, _) = a_placed_planet();
        put(&mut state, &system, "carrier", &pid("b"), 1);
        let content = ContentStore::embedded();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut table = crate::choice::Table::new();
        let mut ctx = crate::choice::Resolving {
            content,
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };
        assert!(capture_from_board_announced(
            &mut state,
            &mut ctx,
            &pid("a"),
            &system,
            None,
            &carrier("b"),
            "test"
        ));
        assert!(!capture_from_board_announced(
            &mut state,
            &mut ctx,
            &pid("a"),
            &system,
            None,
            &carrier("b"),
            "test"
        ));
        assert_eq!(
            return_captured_announced(
                &mut state,
                &mut ctx,
                &pid("a"),
                &pid("b"),
                "carrier",
                "test"
            ),
            Some(UnitTypeId::new("carrier"))
        );
        assert_eq!(
            return_captured_announced(
                &mut state,
                &mut ctx,
                &pid("a"),
                &pid("b"),
                "carrier",
                "test"
            ),
            None
        );
    }
}

#[cfg(test)]
mod bf_f3_tests {
    use super::*;
    use ti4_model::content_types::POK;

    type Seen = std::sync::Arc<
        std::sync::Mutex<Vec<std::collections::BTreeMap<String, serde_json::Value>>>,
    >;

    fn listening(event: &str) -> (crate::timing::Resolver, Seen) {
        let seen = Seen::default();
        let sink = seen.clone();
        let mut resolver = crate::timing::Resolver::new(
            vec![PlayerId::new("a"), PlayerId::new("b")],
            Some(PlayerId::new("a")),
            crate::choice::Table::default(),
        );
        resolver.register([crate::timing::Ability::new(
            "test:listener",
            PlayerId::new("a"),
            event,
            crate::timing::Relation::After,
            std::sync::Arc::new(move |event, _| {
                sink.lock().unwrap().push(event.payload.clone());
                Ok(())
            }),
        )]);
        (resolver, seen)
    }

    #[test]
    fn staging_during_a_flush_never_overwrites_a_waiting_row() {
        // A reaction to a flushed event may stage another; the new row must not reuse a key
        // that is still waiting (indices come from the highest staged row, not the count).
        let mut state = crate::fixtures::seated_game(
            &[("a", "saar"), ("b", "sol")],
            ti4_model::content_types::DEFAULT,
        );
        let payload = std::collections::BTreeMap::new();
        for _ in 0..3 {
            assert!(super::stage_event(&mut state, "TEST_EVENT", &payload));
        }
        let first = state
            .faction_marks
            .keys()
            .find(|key| key.starts_with(super::STAGED_EVENT_PREFIX))
            .cloned()
            .expect("staged");
        state.faction_marks.remove(&first);
        assert!(super::stage_event(&mut state, "LATE_EVENT", &payload));
        assert_eq!(super::staged_events(&state), 3, "nothing overwritten");
        assert!(
            state
                .faction_marks
                .keys()
                .all(|key| !key.starts_with("staged:")),
            "staging rows are private"
        );
    }

    #[test]
    fn staged_gains_flush_in_order_and_only_with_a_timing_handle() {
        let content = ContentStore::embedded();
        let mut state = crate::fixtures::seated_game(&[("a", "mentak"), ("b", "sol")], POK);
        let a = PlayerId::new("a");
        assert_eq!(gain_trade_goods_staged(&mut state, &a, 2, "one"), 2);
        assert_eq!(gain_trade_goods_staged(&mut state, &a, 0, "none"), 0);
        assert_eq!(gain_trade_goods_staged(&mut state, &a, 1, "two"), 1);
        assert_eq!(staged_events(&state), 2);

        let (mut resolver, seen) = listening("TRADE_GOODS_GAINED");
        let mut sequence = crate::event::EventSequence::new();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(1);
        let mut table = crate::choice::Table::new();
        let mut quiet = crate::choice::Resolving {
            content,
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };
        assert_eq!(flush_staged_events(&mut state, &mut quiet), 0);
        assert_eq!(staged_events(&state), 2, "left staged");
        let mut table = crate::choice::Table::new();
        let mut loud = crate::choice::Resolving {
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
        assert_eq!(flush_staged_events(&mut state, &mut loud), 2);
        assert_eq!(staged_events(&state), 0);
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0]["source"], "one");
        assert_eq!(seen[1]["source"], "two");
    }

    #[test]
    fn strict_flush_restores_failed_reaction_and_keeps_event_for_retry() {
        let content = ContentStore::embedded();
        let mut state = crate::fixtures::seated_game(&[("a", "mentak"), ("b", "sol")], POK);
        assert!(stage_event(
            &mut state,
            "STRICT_FAILURE",
            &Default::default()
        ));
        let before_state = state.clone();
        let mut resolver = crate::timing::Resolver::new(
            vec![PlayerId::new("a"), PlayerId::new("b")],
            Some(PlayerId::new("a")),
            crate::choice::Table::default(),
        );
        resolver.register([crate::timing::Ability::stateful(
            "test:strict_failure",
            PlayerId::new("a"),
            "STRICT_FAILURE",
            crate::timing::Relation::After,
            std::sync::Arc::new(|_, _, context| {
                context
                    .state
                    .player_mut(&PlayerId::new("a"))
                    .unwrap()
                    .trade_goods += 4;
                context.dice.roll(context.rng, 1, "strict rollback", None);
                let choice = crate::choice::Choice::new(
                    PlayerId::new("a"),
                    "invalid scripted reaction",
                    vec![crate::choice::ChoiceOption::labelled(
                        "valid", "test", "valid",
                    )],
                );
                context
                    .ask_seeing(&choice)
                    .map_err(crate::timing::TimingError::IllegalChoice)?;
                Ok(())
            }),
        )]);
        let mut sequence = crate::event::EventSequence::new();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(9);
        let before_rng = rng.clone();
        let mut table =
            crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new([
                "not-offered",
            ])));
        let mut resolving = crate::choice::Resolving {
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

        assert!(matches!(
            try_flush_staged_events(&mut state, &mut resolving),
            Err(crate::timing::TimingError::IllegalChoice(_))
        ));
        assert_eq!(state, before_state, "reaction mutations roll back");
        assert_eq!(staged_events(&state), 1, "failed row remains retryable");
        assert!(resolving.dice.rolled("strict rollback").is_empty());
        let mut expected_rng = before_rng.clone();
        let expected_probe = expected_rng.die("retry probe", 10);
        assert_eq!(resolving.rng.die("retry probe", 10), expected_probe);
        assert_eq!(
            *resolving.timing.as_ref().unwrap().sequence,
            crate::event::EventSequence::new()
        );
        assert!(resolving.timing.as_ref().unwrap().resolver.log().is_empty());
        assert!(
            resolving
                .timing
                .as_ref()
                .unwrap()
                .resolver
                .applied_events()
                .is_empty()
        );
        *resolving.rng = before_rng;
        assert_eq!(try_flush_staged_events(&mut state, &mut resolving), Ok(1));
        assert_eq!(staged_events(&state), 0);
        assert_eq!(
            state.player(&PlayerId::new("a")).unwrap().trade_goods,
            before_state
                .player(&PlayerId::new("a"))
                .unwrap()
                .trade_goods
                + 4
        );
    }

    #[test]
    fn strict_ground_flush_restores_failed_reaction_and_keeps_event_for_retry() {
        let content = ContentStore::embedded();
        let mut state = crate::fixtures::seated_game(&[("a", "mentak"), ("b", "sol")], POK);
        crate::factions::hooks_ground::stage_ground_force_destroyed(
            &mut state,
            &ti4_model::id::SystemId::new("18"),
            &ti4_model::id::PlanetId::new("mr"),
            &ti4_model::units::Unit::new(UnitTypeId::new("infantry"), PlayerId::new("a")),
            "test",
        );
        let before_state = state.clone();
        let mut resolver = crate::timing::Resolver::new(
            vec![PlayerId::new("a"), PlayerId::new("b")],
            Some(PlayerId::new("a")),
            crate::choice::Table::default(),
        );
        resolver.register([crate::timing::Ability::stateful(
            "test:strict_failure",
            PlayerId::new("a"),
            "GROUND_FORCE_DESTROYED",
            crate::timing::Relation::After,
            std::sync::Arc::new(|_, _, context| {
                context
                    .state
                    .player_mut(&PlayerId::new("a"))
                    .unwrap()
                    .trade_goods += 4;
                context.dice.roll(context.rng, 1, "strict rollback", None);
                let choice = crate::choice::Choice::new(
                    PlayerId::new("a"),
                    "invalid scripted reaction",
                    vec![crate::choice::ChoiceOption::labelled(
                        "valid", "test", "valid",
                    )],
                );
                context
                    .ask_seeing(&choice)
                    .map_err(crate::timing::TimingError::IllegalChoice)?;
                Ok(())
            }),
        )]);
        let mut sequence = crate::event::EventSequence::new();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(9);
        let before_rng = rng.clone();
        let mut table =
            crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new([
                "not-offered",
            ])));
        let mut resolving = crate::choice::Resolving {
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

        assert!(matches!(
            crate::factions::hooks_ground::try_announce_staged_events(&mut state, &mut resolving),
            Err(crate::timing::TimingError::IllegalChoice(_))
        ));
        assert_eq!(state, before_state, "reaction mutations roll back");
        assert!(
            crate::factions::hooks_ground::has_staged_events(&state),
            "failed row remains retryable"
        );
        assert!(resolving.dice.rolled("strict rollback").is_empty());
        let mut expected_rng = before_rng.clone();
        let expected_probe = expected_rng.die("retry probe", 10);
        assert_eq!(resolving.rng.die("retry probe", 10), expected_probe);
        assert_eq!(
            *resolving.timing.as_ref().unwrap().sequence,
            crate::event::EventSequence::new()
        );
        assert!(resolving.timing.as_ref().unwrap().resolver.log().is_empty());
        assert!(
            resolving
                .timing
                .as_ref()
                .unwrap()
                .resolver
                .applied_events()
                .is_empty()
        );
        *resolving.rng = before_rng;
        assert_eq!(
            crate::factions::hooks_ground::try_announce_staged_events(&mut state, &mut resolving),
            Ok(())
        );
        assert!(!crate::factions::hooks_ground::has_staged_events(&state));
        assert_eq!(
            state.player(&PlayerId::new("a")).unwrap().trade_goods,
            before_state
                .player(&PlayerId::new("a"))
                .unwrap()
                .trade_goods
                + 4
        );
    }

    #[test]
    fn strict_flush_processes_nested_events_after_already_waiting_rows() {
        let content = ContentStore::embedded();
        let mut state = crate::fixtures::seated_game(&[("a", "mentak"), ("b", "sol")], POK);
        assert!(stage_event(&mut state, "FIRST", &Default::default()));
        assert!(stage_event(&mut state, "SECOND", &Default::default()));
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut abilities = Vec::new();
        for event_type in ["FIRST", "SECOND", "NESTED"] {
            let seen_event = seen.clone();
            let nested = event_type == "FIRST";
            abilities.push(crate::timing::Ability::stateful(
                format!("test:ordered:{event_type}"),
                PlayerId::new("a"),
                event_type,
                crate::timing::Relation::After,
                std::sync::Arc::new(move |event, _, context| {
                    seen_event.lock().unwrap().push(event.event_type.clone());
                    if nested {
                        stage_event(context.state, "NESTED", &Default::default());
                    }
                    Ok(())
                }),
            ));
        }
        let mut resolver = crate::timing::Resolver::new(
            vec![PlayerId::new("a"), PlayerId::new("b")],
            Some(PlayerId::new("a")),
            crate::choice::Table::default(),
        );
        resolver.register(abilities);
        let mut sequence = crate::event::EventSequence::new();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(10);
        let mut table = crate::choice::Table::new();
        let mut resolving = crate::choice::Resolving {
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

        assert_eq!(try_flush_staged_events(&mut state, &mut resolving), Ok(3));
        assert_eq!(
            *seen.lock().unwrap(),
            ["FIRST", "SECOND", "NESTED"],
            "nested rows follow rows that were already waiting"
        );
        assert_eq!(staged_events(&state), 0);
    }

    #[test]
    fn nothing_is_staged_without_a_module_seat() {
        let mut state = crate::fixtures::game(&["a", "b"]);
        assert_eq!(
            gain_trade_goods_staged(&mut state, &PlayerId::new("a"), 3, "x"),
            3
        );
        assert_eq!(staged_events(&state), 0);
        assert!(state.faction_marks.is_empty());
    }

    #[test]
    fn spending_a_strategy_token_announces_or_stages_the_event() {
        let content = ContentStore::embedded();
        let a = PlayerId::new("a");
        let mut state = crate::fixtures::seated_game(&[("a", "muaat"), ("b", "sol")], POK);
        state.gain_token(&a, ti4_model::state::TokenPool::Strategic, 3);
        let before = state
            .player(&a)
            .unwrap()
            .tokens(ti4_model::state::TokenPool::Strategic);

        assert!(spend_strategy_token_staged(&mut state, &a, "test"));
        assert_eq!(staged_event_types(&state), ["STRATEGY_TOKEN_SPENT"]);

        let (mut resolver, seen) = listening("STRATEGY_TOKEN_SPENT");
        let mut sequence = crate::event::EventSequence::new();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(1);
        let mut table = crate::choice::Table::new();
        let mut ctx = crate::choice::Resolving {
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
        assert!(spend_strategy_token_announced(
            &mut state, &mut ctx, &a, "direct"
        ));
        assert_eq!(
            state
                .player(&a)
                .unwrap()
                .tokens(ti4_model::state::TokenPool::Strategic),
            before - 2
        );
        assert_eq!(flush_staged_events(&mut state, &mut ctx), 1);
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0]["reason"], "direct");
        assert_eq!(seen[1]["reason"], "test");
        assert_eq!(seen[1]["player"], "a");

        let mut empty = crate::fixtures::seated_game(&[("a", "muaat"), ("b", "sol")], POK);
        empty
            .player_mut(&a)
            .unwrap()
            .spend_token(ti4_model::state::TokenPool::Strategic);
        while spend_strategy_token_staged(&mut empty, &a, "drain") {}
        let staged = staged_events(&empty);
        assert!(!spend_strategy_token_staged(&mut empty, &a, "none"));
        assert_eq!(
            staged_events(&empty),
            staged,
            "an empty pool announces nothing"
        );
    }
    #[test]
    fn maximum_forbids_reinforcement_mechs_without_changing_the_box_count() {
        let content = ContentStore::embedded();
        let mut state = crate::fixtures::seated_game(
            &[("a", "naaz"), ("b", "sol")],
            ti4_model::content_types::DEFAULT,
        );
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let (system, planet) = crate::fixtures::a_placed_planet();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "naaz_voltron", &a, 1);
        let mech = UnitTypeId::new("naaz_mech");
        assert!(
            remaining(
                &state,
                content,
                ti4_model::content_types::DEFAULT,
                &a,
                &mech
            ) > 0
        );
        assert_eq!(
            allowed(
                &state,
                content,
                ti4_model::content_types::DEFAULT,
                &a,
                &mech,
                1
            ),
            0
        );
        assert_eq!(
            allowed(
                &state,
                content,
                ti4_model::content_types::DEFAULT,
                &a,
                &UnitTypeId::new("mech"),
                1
            ),
            0
        );
        assert_eq!(
            allowed(
                &state,
                content,
                ti4_model::content_types::DEFAULT,
                &b,
                &UnitTypeId::new("sol_mech"),
                1
            ),
            1
        );
        assert_eq!(
            allowed(
                &state,
                content,
                ti4_model::content_types::DEFAULT,
                &a,
                &UnitTypeId::new("infantry"),
                1
            ),
            1
        );
        assert!(
            capture_from_reinforcements(
                &mut state,
                content,
                ti4_model::content_types::DEFAULT,
                &b,
                &a,
                &mech
            ),
            "capture consumes box plastic without placing or producing a mech"
        );
    }
}
