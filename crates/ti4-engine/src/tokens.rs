//! Command tokens on the command sheet: the status phase's gain (81.5), the redistribution of a
//! sheet in one decision, and tokens placed on the board from reinforcements.
//!
//! The status phase's gain is not asked token by token: the redistribution that follows it
//! decides where every token ends up, so [`TokenGain`] only counts what each seat gains and
//! [`TokenRedistribution::with_gain`] asks once over the larger sheet. Leadership's gain, which no
//! redistribution follows, asks a pool for each token in `strategy_cards::gain_tokens`. A game
//! from before `RULES_STATUS_GAIN` asks the status phase's gain token by token too
//! ([`TokenGain::pending_choice`]), and its redistribution comes after over what is placed.

use ti4_model::id::{PlayerId, SystemId};
use ti4_model::state::{GameState, TokenPool};

use crate::choice::{Choice, ChoiceOption, IllegalChoice, validate};

/// The choice kind for placing a gained command token.
pub const POOL_KIND: &str = "pool";

/// Tokens each player gains at status step 81.5, before any modifier.
///
/// Sol's Versatile, Cybernetic Enhancements and the L1Z1X promissory note all change this in
/// the oracle. None of those are implemented, so the base is used unmodified; when they land
/// they modify the count handed to [`TokenGain::for_status`], not this constant.
pub const STATUS_TOKENS: u32 = 2;

/// A pool could not be selected from the state that was presented.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TokenGainError {
    #[error("the token-gain window is complete")]
    Complete,
    #[error("player {0} is no longer seated")]
    PlayerMissing(PlayerId),
    #[error("option id {0:?} does not name a command-token pool")]
    UnknownPool(String),
    #[error(transparent)]
    IllegalChoice(#[from] IllegalChoice),
}

/// The pools a gained token may enter, in the oracle's option order.
const POOLS: [(&str, TokenPool, &str); 3] = [
    ("tactic_tokens", TokenPool::Tactic, "tactic pool"),
    ("fleet_tokens", TokenPool::Fleet, "fleet pool"),
    ("strategic_tokens", TokenPool::Strategic, "strategy pool"),
];

/// What each seat gains at status step 81.5, in initiative order.
///
/// From `RULES_STATUS_GAIN` the gained tokens are placed by the redistribution that follows (see
/// [`TokenRedistribution::with_gain`]), so this holds counts, not a decision. An older game places
/// them one at a time, each into a chosen pool ([`Self::pending_choice`], [`Self::resolve`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenGain {
    /// One entry per token owed, in resolution order.
    pending: Vec<PlayerId>,
}

impl TokenGain {
    /// A grant of `count` tokens to each player, in the given order.
    #[must_use]
    pub fn new(players: &[PlayerId], count: u32) -> Self {
        let mut pending = Vec::new();
        for player in players {
            for _ in 0..count {
                pending.push(player.clone());
            }
        }
        Self { pending }
    }

    /// The status phase's grant to each player in initiative order, including faction modifiers.
    ///
    /// # Panics
    /// Panics if a faction's printed status-phase token gain does not fit in an `i32`, which no
    /// corpus value can do.
    #[must_use]
    pub fn for_status(
        state: &GameState,
        content: &ti4_content::ContentStore,
        players: &[PlayerId],
    ) -> Self {
        let mut pending = Vec::new();
        for player in players {
            let count = crate::faction_abilities::status_tokens(
                state,
                content,
                player,
                i32::try_from(STATUS_TOKENS).expect("the printed status gain fits in i32"),
            )
            .max(0);
            for _ in 0..count {
                pending.push(player.clone());
            }
        }
        Self { pending }
    }

    /// How many tokens `player` gains, before the reinforcements cap it (20.4a).
    #[must_use]
    pub fn remaining_for(&self, player: &PlayerId) -> usize {
        self.pending.iter().filter(|next| *next == player).count()
    }

    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.pending.is_empty()
    }

    /// How many tokens the next player still has to place, this one included (before
    /// `RULES_STATUS_GAIN`).
    #[must_use]
    pub fn remaining_for_next_player(&self) -> usize {
        self.pending
            .first()
            .map_or(0, |next| self.remaining_for(next))
    }

    /// The choice for the next token, or `None` once every token is placed (before
    /// `RULES_STATUS_GAIN`).
    #[must_use]
    pub fn pending_choice(&self) -> Option<Choice> {
        let player = self.pending.first()?;
        Some(Choice::new(
            player.clone(),
            "gain a command token into which pool",
            POOLS
                .iter()
                .map(|(id, _, label)| ChoiceOption::labelled(*id, POOL_KIND, *label))
                .collect(),
        ))
    }

    /// Place the next token into the chosen pool (before `RULES_STATUS_GAIN`).
    ///
    /// # Errors
    /// [`TokenGainError::Complete`] when no token remains, [`TokenGainError::IllegalChoice`]
    /// when the answer was not offered, [`TokenGainError::UnknownPool`] when it does not name a
    /// pool, and [`TokenGainError::PlayerMissing`] when the player has left the table.
    pub fn resolve(
        &mut self,
        state: &mut GameState,
        answer: ChoiceOption,
    ) -> Result<TokenPool, TokenGainError> {
        let choice = self.pending_choice().ok_or(TokenGainError::Complete)?;
        let option = validate(&choice, answer)?;
        let Some(pool) = POOLS
            .iter()
            .find(|(id, _, _)| *id == option.id)
            .map(|(_, pool, _)| *pool)
        else {
            return Err(TokenGainError::UnknownPool(option.id));
        };
        if state.player(&choice.player).is_none() {
            return Err(TokenGainError::PlayerMissing(choice.player));
        }
        // 20.4a: a player with no token in reinforcements cannot gain one. The offer still
        // happens -- it is the same decision either way -- and the gain is what comes up empty.
        state.gain_token(&choice.player, pool, 1);
        self.pending.remove(0);
        Ok(pool)
    }
}

/// Display-only facts for a command-token decision (see [`Choice::details`]): the pools as they
/// stand now, the tokens still in reinforcements, and what the decision hands out or moves.
///
/// `mode` is `"buy"` (nothing free to place; the decision is Leadership's influence purchase),
/// `"gain"` (with `tokens_to_place` tokens still to place, this one included) or
/// `"redistribute"` (with the total held, plus `tokens_to_place` tokens gained in the same
/// decision when it is given: see [`TokenRedistribution::with_gain`]).
#[must_use]
pub fn with_pool_details(
    choice: Choice,
    state: &GameState,
    mode: &str,
    tokens_to_place: Option<usize>,
) -> Choice {
    let (tactic, fleet, strategic) = state.player(&choice.player).map_or((0, 0, 0), |seat| {
        (seat.tactic_tokens, seat.fleet_tokens, seat.strategic_tokens)
    });
    let reinforcements = state.tokens_in_reinforcements(&choice.player);
    let choice = choice
        .detailed("kind", "command_tokens")
        .detailed("mode", mode)
        .detailed(
            "pools",
            serde_json::json!({ "tactic": tactic, "fleet": fleet, "strategic": strategic }),
        );
    // Letnev's Armada (`faction_abilities::fleet_supply`): two more non-fighter ships per system
    // than the fleet pool allows. Display only; the panel shows it next to the fleet pool.
    let choice = if state
        .player(&choice.player)
        .is_some_and(|seat| seat.faction.as_str() == "letnev")
    {
        choice.detailed("fleet_supply_bonus", 2)
    } else {
        choice
    };
    match (mode, tokens_to_place) {
        ("gain" | "buy", Some(count)) => choice
            .detailed("reinforcements", reinforcements)
            .detailed("tokens_to_place", count),
        ("redistribute", Some(gained)) if gained > 0 => {
            // A redistribution that also gains (the status phase): `pools` is the sheet before
            // the gain, `total` the arrangement to choose, and the gained tokens are already out
            // of the reinforcements it reports.
            let gained = i32::try_from(gained).unwrap_or(i32::MAX);
            choice
                .detailed("reinforcements", (reinforcements - gained).max(0))
                .detailed("total", tactic + fleet + strategic + gained)
                .detailed("gained", gained)
        }
        ("redistribute" | "restack", _) => choice
            .detailed("reinforcements", reinforcements)
            .detailed("total", tactic + fleet + strategic),
        _ => choice.detailed("reinforcements", reinforcements),
    }
}

/// The choice kind for redistributing already-held command tokens across pools in one decision.
pub const REDISTRIBUTE_KIND: &str = "redistribute";

/// A redistribution was asked for or answered when it could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RedistributeError {
    #[error("the redistribution window is complete")]
    Complete,
    #[error("player {0} is no longer seated")]
    PlayerMissing(PlayerId),
    #[error("option id {0:?} does not name a legal token distribution")]
    Malformed(String),
    #[error(transparent)]
    IllegalChoice(#[from] IllegalChoice),
}

/// Every way to split `total` indistinguishable tokens across the three pools, as choice
/// options, restricted to arrangements that keep at least two tokens in the fleet pool.
///
/// A fleet pool of zero or one leaves a faction effectively unable to move its ships (movement
/// is paid from the fleet pool), which is how the policy learned to empty it; those final
/// arrangements are not offered at all rather than merely scored down. With `fleet >= 2`,
/// splitting `n` tokens has `C(n, 2) = n * (n - 1) / 2` solutions: the base starting pools
/// (3 tactic, 3 fleet, 2 strategic -- 8 tokens) offer 28; a typical status-phase redistribution,
/// asked after 81.5's first sentence has already granted 2 more, holds 10 tokens and offers 45.
/// `total` itself is bounded by [`ti4_model::state::TOKENS_PER_FACTION`] (16, LRR 20.4's
/// reinforcement cap: every token is either on the command sheet or on the board, and a
/// redistribution only ever moves sheet tokens), which puts a hard ceiling of `C(16, 2) = 120`
/// options on any legal redistribution. When `total < 2` no split keeps the fleet pool above one,
/// so every split is offered instead -- the window must stay answerable.
fn distribution_options(total: i32, floor: i32) -> Vec<ChoiceOption> {
    let min_fleet = if total >= floor { floor } else { 0 };
    let mut options = Vec::new();
    for tactic in 0..=total {
        for fleet in min_fleet..=(total - tactic) {
            let strategic = total - tactic - fleet;
            options.push(ChoiceOption::labelled(
                format!("{tactic}|{fleet}|{strategic}"),
                REDISTRIBUTE_KIND,
                format!("tactic {tactic} / fleet {fleet} / strategy {strategic}"),
            ));
        }
    }
    options
}

fn parse_distribution(option_id: &str) -> Option<(i32, i32, i32)> {
    let mut parts = option_id.split('|');
    let tactic = parts.next()?.parse().ok()?;
    let fleet = parts.next()?.parse().ok()?;
    let strategic = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((tactic, fleet, strategic))
}

/// A single decision that moves any number of a player's already-held command tokens between
/// pools (LRR 81.5's second sentence, and Warfare's matching clause).
///
/// The oracle-derived mechanic this replaces (`strategy_cards::redistribute_tokens`) offered one
/// single-token move at a time, repeated until the player declined -- legal under the rule, since
/// 81.5 places no limit on how a player gets from one arrangement to another, but not what the
/// rule *asks*. A player looks at everything on their command sheet and picks where it all ends
/// up; they do not narrate a sequence of individual moves. Presenting it as a sequence also means
/// a policy scores each move in isolation, with no view of the arrangement the moves are heading
/// toward -- which is how a chain of locally-reasonable single-token moves empties the fleet pool
/// without anything ever having chosen that outcome. This window asks once, over every legal final
/// arrangement, so the engine can only reach a distribution someone actually picked.
///
/// The offered arrangement depends on the pools as they stand when asked (a redistribution has no
/// count of its own to fix in advance, unlike [`TokenGain`]), so -- unlike that window -- the
/// choice is rebuilt from live state each time it is asked for rather than fixed at construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenRedistribution {
    player: PlayerId,
    /// Tokens gained from reinforcements in the same decision (the status phase's 81.5 gain).
    gained: i32,
    resolved: bool,
}

impl TokenRedistribution {
    /// Open the window for whatever `player` currently holds.
    #[must_use]
    pub const fn new(player: PlayerId) -> Self {
        Self::with_gain(player, 0)
    }

    /// Open the window for what `player` holds plus `gained` tokens from reinforcements.
    ///
    /// LRR 81.5 gains two tokens and then lets the player redistribute every token held, so
    /// asking where each gained token goes first is a choice with no consequence: the
    /// redistribution decides the final sheet either way. The status phase asks once, over the
    /// arrangements of the larger total. The caller caps `gained` by the reinforcements (20.4a);
    /// `resolve` sets the pools absolutely, so the gained tokens leave the (computed)
    /// reinforcements by being on the sheet.
    #[must_use]
    pub const fn with_gain(player: PlayerId, gained: i32) -> Self {
        Self {
            player,
            gained,
            resolved: false,
        }
    }

    /// Whether this window has been answered, or never had anything to redistribute.
    #[must_use]
    pub fn is_complete(&self, state: &GameState) -> bool {
        self.pending_choice(state).is_none()
    }

    /// The tokens on `player`'s command sheet right now, across all three pools.
    fn held_total(&self, state: &GameState) -> i32 {
        state.player(&self.player).map_or(0, |seat| {
            seat.tactic_tokens + seat.fleet_tokens + seat.strategic_tokens
        })
    }

    /// The fewest tokens an offered arrangement keeps in the fleet pool. Two for everyone (the
    /// policy guard above); Letnev's Armada keeps its ships legal on a small pool (the per-system
    /// limit is the pool plus two), so the rules-allowed minimum of none is offered from
    /// `RULES_LETNEV_FLEET_POOL` on.
    fn fleet_floor(&self, state: &GameState) -> i32 {
        let letnev = state
            .player(&self.player)
            .is_some_and(|seat| seat.faction.as_str() == "letnev");
        if letnev && state.rules_version >= ti4_model::state::RULES_LETNEV_FLEET_POOL {
            0
        } else {
            2
        }
    }

    /// The one choice this window ever asks, built fresh from `state`. `None` once resolved, or
    /// if the player holds nothing to redistribute.
    #[must_use]
    pub fn pending_choice(&self, state: &GameState) -> Option<Choice> {
        if self.resolved {
            return None;
        }
        let total = self.held_total(state) + self.gained;
        (total > 0).then(|| {
            Choice::new(
                self.player.clone(),
                "redistribute your command tokens",
                distribution_options(total, self.fleet_floor(state)),
            )
        })
    }

    /// Apply the chosen final arrangement, replacing whatever the player held before.
    ///
    /// Sets the three pools directly rather than moving tokens one at a time: the chosen option's
    /// id already names the complete arrangement, so there is nothing to accumulate.
    ///
    /// # Errors
    /// [`RedistributeError::Complete`] once already resolved (or if the player never held
    /// anything to redistribute), [`RedistributeError::IllegalChoice`] when the answer was not
    /// offered, [`RedistributeError::Malformed`] when its id does not parse as three pool counts
    /// (only reachable if something other than [`distribution_options`] built the choice), and
    /// [`RedistributeError::PlayerMissing`] when the player has left the table.
    pub fn resolve(
        &mut self,
        state: &mut GameState,
        answer: ChoiceOption,
    ) -> Result<(i32, i32, i32), RedistributeError> {
        let choice = self
            .pending_choice(state)
            .ok_or(RedistributeError::Complete)?;
        let option = validate(&choice, answer)?;
        let Some((tactic, fleet, strategic)) = parse_distribution(&option.id) else {
            return Err(RedistributeError::Malformed(option.id));
        };
        let seat = state
            .player_mut(&self.player)
            .ok_or_else(|| RedistributeError::PlayerMissing(self.player.clone()))?;
        seat.tactic_tokens = tactic;
        seat.fleet_tokens = fleet;
        seat.strategic_tokens = strategic;
        self.resolved = true;
        Ok((tactic, fleet, strategic))
    }
}

// -- placing a token on the board from a reinforcements pile (BF-00b-economy) ----------------------

/// Whether a command token of `owner` may be put in `system` now: the owner has one in
/// reinforcements (20.4: "if a player would gain a command token but has none available in their
/// reinforcements, that player cannot gain that command token" extends to placing one) and does
/// not already have a token there (a system holds one token per player).
#[must_use]
pub fn can_place_command_token(
    state: &GameState,
    owner: &PlayerId,
    system: &ti4_model::id::SystemId,
) -> bool {
    state.player(owner).is_some()
        && state.tokens_in_reinforcements(owner) > 0
        && !state
            .board
            .get(system)
            .is_some_and(|here| here.command_tokens.contains(owner))
}

/// Stage the typed event for a command token that was just placed through a route without a
/// resolver at hand. A game with no registered faction modules is a no-op, matching
/// [`crate::supply::stage_event`]. The coordinator announces this with
/// [`crate::supply::flush_staged_events`].
pub fn stage_command_token_placed(
    state: &mut GameState,
    player: &PlayerId,
    system: &SystemId,
    pool: crate::factions::hooks_cards::TokenPool,
) -> bool {
    crate::supply::stage_event(
        state,
        crate::factions::hooks_cards::COMMAND_TOKEN_PLACED,
        &crate::factions::hooks_cards::command_token_placed_payload(player, system, pool),
    )
}

/// Place one of `owner`'s command tokens from `owner`'s **reinforcements** (not from a command
/// sheet pool) in `system`.
///
/// For Arborec Stymie: "You may place 1 command token from that player's reinforcements in any
/// non-home system" -- `owner` is *that player*, the one who moved, and the card's holder is only
/// the one who decides. The caller picks the system and checks any restriction on it (Stymie's
/// non-home rule: `ti4_content::galaxy::is_home_system`); this checks only what the component
/// pile allows. Returns `false` and changes nothing when [`can_place_command_token`] says no.
pub fn place_command_token_from_reinforcements(
    state: &mut GameState,
    owner: &PlayerId,
    system: &ti4_model::id::SystemId,
) -> bool {
    if !can_place_command_token(state, owner, system) {
        return false;
    }
    state.system_mut(system).place_token(owner.clone());
    stage_command_token_placed(
        state,
        owner,
        system,
        crate::factions::hooks_cards::TokenPool::Reinforcements,
    );
    true
}

// -- Creuss wormhole tokens ----------------------------------------------------------------------

/// The two wormhole tokens a Creuss player owns, keyed as `GameState::wormhole_tokens` keys them.
/// (The gamma tokens come from exploration and agenda effects and are keyed `GAMMA`.)
pub const CREUSS_TOKENS: [&str; 2] = ["ALPHA", "BETA"];

/// A wormhole token could not be placed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WormholeTokenError {
    #[error("{0:?} is not a Creuss wormhole token (alpha or beta)")]
    NotACreussToken(String),
    #[error("system {0} is not on the map")]
    NotOnTheMap(SystemId),
    #[error("the {0} token is already in that system")]
    AlreadyThere(String),
}

/// Where a Creuss wormhole token is, if it is on the map. `kind` is `"ALPHA"` or `"BETA"`.
#[must_use]
pub fn creuss_token_at<'a>(state: &'a GameState, kind: &str) -> Option<&'a SystemId> {
    state.wormhole_tokens.get(kind)
}

/// Place or move a Creuss wormhole token into a system, returning where it was (Wormhole
/// Generator, and any effect that says "place or move a Creuss wormhole token").
///
/// The token then gives its system a wormhole of that kind for **everyone**: `laws::apply_to_galaxy`
/// rebuilds `Galaxy::token_wormholes` from `GameState::wormhole_tokens` each step, and a wormhole
/// there links to every other system holding the same kind (LRR 101, 102). The caller checks the
/// card's own destination condition (see [`wormhole_generator_destinations`]); this checks only
/// what cannot be true of any legal placement. Atomic.
///
/// # Errors
/// [`WormholeTokenError`] for a kind that is not a Creuss token, a system not on the map, or a
/// token that is already in that system.
pub fn place_creuss_token(
    state: &mut GameState,
    galaxy: &ti4_content::galaxy::Galaxy,
    kind: &str,
    system: &SystemId,
) -> Result<Option<SystemId>, WormholeTokenError> {
    if !CREUSS_TOKENS.contains(&kind) {
        return Err(WormholeTokenError::NotACreussToken(kind.to_owned()));
    }
    if galaxy.coord_of(system.as_str()).is_none() {
        return Err(WormholeTokenError::NotOnTheMap(system.clone()));
    }
    if state.wormhole_tokens.get(kind) == Some(system) {
        return Err(WormholeTokenError::AlreadyThere(kind.to_owned()));
    }
    Ok(state
        .wormhole_tokens
        .insert(kind.to_owned(), system.clone()))
}

/// Take a Creuss wormhole token off the map, returning where it was.
pub fn remove_creuss_token(state: &mut GameState, kind: &str) -> Option<SystemId> {
    if CREUSS_TOKENS.contains(&kind) {
        state.wormhole_tokens.remove(kind)
    } else {
        None
    }
}

/// Every `(token kind, system)` Wormhole Generator may place or move a token into for `player`:
/// "either a system that contains a planet you control or a non-home system that does not contain
/// another player's ships". Both tokens, each into every such system it is not already in; map
/// order within a kind. The Creuss Gate is not a home system and so qualifies when empty of others.
#[must_use]
pub fn wormhole_generator_destinations(
    state: &GameState,
    content: &ti4_content::ContentStore,
    sources: ti4_model::content_types::SourceSet,
    galaxy: &ti4_content::galaxy::Galaxy,
    player: &PlayerId,
) -> Vec<(&'static str, SystemId)> {
    let types = ti4_content::units::catalogue(content, sources);
    let homes = ti4_content::galaxy::home_systems(content, sources);
    let mut systems = Vec::new();
    for id in galaxy.system_ids_holding_things() {
        let here = state.board.get(&SystemId::new(id));
        let controls = here.is_some_and(|system| system.controls_a_planet(player));
        let others_ships = here.is_some_and(|system| {
            system.units.iter().any(|unit| {
                &unit.owner != player
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(ti4_content::units::UnitType::is_ship)
            })
        });
        if controls || (!homes.contains(id) && !others_ships) {
            systems.push(SystemId::new(id));
        }
    }
    let mut found = Vec::new();
    for kind in CREUSS_TOKENS {
        for system in &systems {
            if state.wormhole_tokens.get(kind) != Some(system) {
                found.push((kind, system.clone()));
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use ti4_content::ContentStore;
    use ti4_model::content_types::POK;
    use ti4_model::state::TokenPool;

    use super::*;
    use crate::setup::start_game;

    fn game() -> (GameState, [PlayerId; 2]) {
        let players = [PlayerId::new("a"), PlayerId::new("b")];
        let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        (state, players)
    }

    #[test]
    fn a_gain_decision_carries_the_pools_the_reinforcements_and_the_count() {
        let (mut state, players) = game();
        {
            let seat = state.player_mut(&players[0]).unwrap();
            seat.tactic_tokens = 3;
            seat.fleet_tokens = 4;
            seat.strategic_tokens = 2;
        }
        let ask = Choice::new(
            players[0].clone(),
            "gain a command token into which pool",
            vec![ChoiceOption::new("tactic_tokens", POOL_KIND)],
        );
        let choice = with_pool_details(ask, &state, "gain", Some(2));
        assert_eq!(choice.details["kind"], "command_tokens");
        assert_eq!(choice.details["mode"], "gain");
        assert_eq!(choice.details["tokens_to_place"], 2);
        assert_eq!(choice.details["pools"]["tactic"], 3);
        assert_eq!(choice.details["pools"]["fleet"], 4);
        assert_eq!(choice.details["pools"]["strategic"], 2);
        assert_eq!(
            choice.details["reinforcements"],
            state.tokens_in_reinforcements(&players[0])
        );
        assert!(choice.details.get("total").is_none());
    }

    #[test]
    fn a_redistribution_decision_carries_the_pools_and_the_total_held() {
        let (mut state, players) = game();
        {
            let seat = state.player_mut(&players[0]).unwrap();
            seat.tactic_tokens = 3;
            seat.fleet_tokens = 4;
            seat.strategic_tokens = 2;
        }
        let window = TokenRedistribution::new(players[0].clone());
        let choice = with_pool_details(
            window.pending_choice(&state).unwrap(),
            &state,
            "redistribute",
            None,
        );
        assert_eq!(choice.details["mode"], "redistribute");
        assert_eq!(choice.details["total"], 9);
        assert!(choice.details.get("tokens_to_place").is_none());
        // The offered arrangements are unchanged by the details.
        assert_eq!(choice.options.len(), distribution_options(9, 2).len());
    }

    #[test]
    fn status_gain_applies_versatile_to_sol_only() {
        let (mut state, players) = game();
        state.player_mut(&players[0]).unwrap().faction = ti4_model::id::FactionId::new("sol");
        state.player_mut(&players[1]).unwrap().faction = ti4_model::id::FactionId::new("hacan");

        let window = TokenGain::for_status(&state, ContentStore::embedded(), &players);

        assert_eq!(
            window
                .pending
                .iter()
                .filter(|player| **player == players[0])
                .count(),
            3,
            "Sol gains the two base tokens plus Versatile's one"
        );
        assert_eq!(
            window
                .pending
                .iter()
                .filter(|player| **player == players[1])
                .count(),
            2,
            "Versatile does not alter another faction's grant"
        );
    }

    #[test]
    fn a_zero_token_grant_owes_nothing() {
        // Nothing in the base game grants zero, but an ability may reduce the count.
        let (_, players) = game();
        let window = TokenGain::new(&players, 0);
        assert_eq!(window.remaining_for(&players[0]), 0);
    }

    // ─── TokenRedistribution: one decision over every legal final arrangement ─────────────

    #[test]
    fn redistribution_is_a_single_decision_not_a_forced_sequence_of_single_token_moves() {
        // The bug this replaces: a sequence of single-token-move choices could walk the fleet
        // pool down to zero one step at a time, with no single decision that chose that
        // outcome. Here the player picks the whole arrangement -- including moving every token
        // out of one pool -- in exactly one `resolve` call, and every other pool total is
        // reachable in that same one call too.
        let (mut state, players) = game();
        let player = players[0].clone();
        // Base pools: tactic 3, fleet 3, strategic 2 (8 total).
        let mut window = TokenRedistribution::new(player.clone());
        let choice = window
            .pending_choice(&state)
            .expect("8 tokens held, so a choice is offered");
        assert_eq!(choice.player, player);

        // Moving six of the eight tokens into the tactic pool in one decision.
        let answer = choice
            .option("6|2|0")
            .expect("tactic 6 / fleet 2 / strategy 0 is offered");
        let (tactic, fleet, strategic) = window.resolve(&mut state, answer.clone()).unwrap();
        assert_eq!((tactic, fleet, strategic), (6, 2, 0));

        let seat = state.player(&player).unwrap();
        assert_eq!(
            (seat.tactic_tokens, seat.fleet_tokens, seat.strategic_tokens),
            (6, 2, 0)
        );
        assert!(
            window.is_complete(&state),
            "the single decision resolved the whole window"
        );
    }

    #[test]
    fn arrangements_that_do_not_keep_the_fleet_pool_above_one_are_not_offered() {
        // A fleet pool of zero or one leaves a faction unable to move its ships; those final
        // arrangements are excluded from the choice entirely rather than scored down.
        let (state, players) = game();
        let window = TokenRedistribution::new(players[0].clone());
        let choice = window.pending_choice(&state).unwrap();

        for option in &choice.options {
            let (_, fleet, _) = parse_distribution(&option.id).expect("every option parses");
            assert!(
                fleet >= 2,
                "offered arrangement keeps the fleet pool above one: {}",
                option.id
            );
        }
        assert!(
            choice.option("0|0|8").is_none(),
            "emptying the fleet pool is not offered"
        );
        assert!(
            choice.option("4|1|3").is_none(),
            "a single fleet token is not offered"
        );
        assert!(choice.option("3|2|3").is_some());
    }

    #[test]
    fn letnev_may_redistribute_below_two_fleet_tokens() {
        let (mut state, players) = game();
        state.player_mut(&players[0]).unwrap().faction = ti4_model::id::FactionId::new("letnev");
        let window = TokenRedistribution::new(players[0].clone());
        let choice = window.pending_choice(&state).unwrap();
        assert!(
            choice.option("0|0|8").is_some(),
            "an empty fleet pool is offered"
        );
        assert!(choice.option("4|1|3").is_some());
        assert_eq!(
            choice.options.len(),
            45,
            "C(10, 2) splits of 8 tokens, no floor"
        );
        // Everyone else keeps the floor, and so does a legacy Letnev game.
        let other = TokenRedistribution::new(players[1].clone());
        assert!(
            other
                .pending_choice(&state)
                .unwrap()
                .option("4|1|3")
                .is_none()
        );
        state.rules_version = ti4_model::state::RULES_LETNEV_FLEET_POOL - 1;
        let legacy = window.pending_choice(&state).unwrap();
        assert!(legacy.option("4|1|3").is_none());
        assert_eq!(legacy.options.len(), 28);
    }

    #[test]
    fn only_letnev_reports_the_armada_fleet_supply_bonus() {
        let (mut state, players) = game();
        state.player_mut(&players[0]).unwrap().faction = ti4_model::id::FactionId::new("letnev");
        let choice = |player: &PlayerId| {
            let window = TokenRedistribution::new(player.clone());
            with_pool_details(
                window.pending_choice(&state).unwrap(),
                &state,
                "redistribute",
                None,
            )
        };
        assert_eq!(
            choice(&players[0]).details.get("fleet_supply_bonus"),
            Some(&serde_json::json!(2))
        );
        assert!(
            choice(&players[1])
                .details
                .get("fleet_supply_bonus")
                .is_none()
        );
    }

    #[test]
    fn holding_one_token_still_offers_an_answerable_window() {
        // With fewer than two tokens held no split keeps the fleet pool above one; every split
        // is offered instead so the window cannot dead-end.
        let (mut state, players) = game();
        if let Some(seat) = state.player_mut(&players[0]) {
            seat.tactic_tokens = 1;
            seat.fleet_tokens = 0;
            seat.strategic_tokens = 0;
        }
        let window = TokenRedistribution::new(players[0].clone());
        let choice = window.pending_choice(&state).unwrap();
        assert_eq!(
            choice.options.len(),
            3,
            "all three one-token splits stay offered"
        );
    }

    #[test]
    fn each_offered_split_of_the_held_total_is_offered_exactly_once() {
        let (mut state, players) = game();
        let player = players[0].clone();
        if let Some(seat) = state.player_mut(&player) {
            seat.tactic_tokens = 2;
            seat.fleet_tokens = 2;
            seat.strategic_tokens = 0;
        }
        let window = TokenRedistribution::new(player);
        let choice = window.pending_choice(&state).unwrap();

        // C(4, 2) = 6 splits of 4 tokens across 3 pools with at least two in the fleet pool.
        assert_eq!(
            choice.options.len(),
            6,
            "the bound this fix promises: n(n-1)/2"
        );
        let mut seen: std::collections::BTreeSet<(i32, i32, i32)> =
            std::collections::BTreeSet::new();
        for option in &choice.options {
            let split = parse_distribution(&option.id).expect("every option parses");
            assert_eq!(split.0 + split.1 + split.2, 4, "conserves the held total");
            assert!(seen.insert(split), "no split is offered twice");
        }
        // Keeping the current arrangement is one of the offered options, not a separate decline.
        assert!(choice.option("2|2|0").is_some());
    }

    #[test]
    fn a_status_phase_gain_of_two_yields_the_bound_this_fix_promises() {
        // The report's headline number: base pools (8) plus 81.5's own gain (2) is a typical
        // status-phase redistribution, and it stays well inside the combinatorial bound.
        let (state, players) = game();
        let window = TokenRedistribution::new(players[0].clone());
        assert_eq!(
            window.pending_choice(&state).unwrap().options.len(),
            28,
            "8 held tokens (the starting 3/3/2 pools): C(8,2)"
        );

        let mut ten = state.clone();
        if let Some(seat) = ten.player_mut(&players[0]) {
            seat.tactic_tokens += 2; // as if 81.5's gain step had already run
        }
        let window = TokenRedistribution::new(players[0].clone());
        assert_eq!(
            window.pending_choice(&ten).unwrap().options.len(),
            45,
            "10 held tokens after a typical status-phase gain: C(10,2)"
        );

        // The worst case a rules-legal game can reach: every token in reinforcements (LRR 20.4,
        // 16 per faction) sitting on the sheet at once.
        let mut sixteen = state.clone();
        if let Some(seat) = sixteen.player_mut(&players[0]) {
            seat.tactic_tokens = 16;
            seat.fleet_tokens = 0;
            seat.strategic_tokens = 0;
        }
        let window = TokenRedistribution::new(players[0].clone());
        assert_eq!(
            window.pending_choice(&sixteen).unwrap().options.len(),
            120,
            "the hard ceiling: C(16,2), reachable only at the full 16-token reinforcement cap"
        );
    }

    #[test]
    fn holding_no_tokens_offers_no_redistribution() {
        let (mut state, players) = game();
        if let Some(seat) = state.player_mut(&players[0]) {
            seat.tactic_tokens = 0;
            seat.fleet_tokens = 0;
            seat.strategic_tokens = 0;
        }
        let window = TokenRedistribution::new(players[0].clone());
        assert!(window.is_complete(&state));
        assert!(window.pending_choice(&state).is_none());
    }

    #[test]
    fn an_unoffered_redistribution_answer_moves_nothing() {
        let (mut state, players) = game();
        let player = players[0].clone();
        let mut window = TokenRedistribution::new(player);
        let before = state.clone();

        let error = window
            .resolve(&mut state, ChoiceOption::new("99|99|99", REDISTRIBUTE_KIND))
            .unwrap_err();

        assert!(matches!(error, RedistributeError::IllegalChoice(_)));
        assert!(state.identical(&before), "state must not have moved");
        assert!(
            !window.is_complete(&state),
            "the window is still owed an answer"
        );
    }

    #[test]
    fn resolving_a_complete_redistribution_window_is_refused() {
        let (mut state, players) = game();
        let player = players[0].clone();
        let mut window = TokenRedistribution::new(player);
        let choice = window.pending_choice(&state).unwrap();
        let option = choice.options.first().unwrap().clone();
        window.resolve(&mut state, option.clone()).unwrap();

        assert_eq!(
            window.resolve(&mut state, option),
            Err(RedistributeError::Complete)
        );
    }
    #[test]
    fn a_command_token_comes_from_the_owners_reinforcements() {
        let (mut state, players) = game();
        let system = ti4_model::id::SystemId::new("some_system");
        // Fresh game: sixteen tokens in all, the sheet holds some, the rest are reinforcements.
        let before = state.tokens_in_reinforcements(&players[1]);
        assert!(before > 0, "the fixture leaves reinforcements");
        let sheet = state.player(&players[1]).unwrap().total_tokens();

        assert!(place_command_token_from_reinforcements(
            &mut state,
            &players[1],
            &system
        ));

        assert!(state.board[&system].command_tokens.contains(&players[1]));
        assert_eq!(state.tokens_in_reinforcements(&players[1]), before - 1);
        assert_eq!(
            state.player(&players[1]).unwrap().total_tokens(),
            sheet,
            "the command sheet is untouched"
        );
        assert!(
            !place_command_token_from_reinforcements(&mut state, &players[1], &system),
            "one token per player per system"
        );
        assert_eq!(state.tokens_in_reinforcements(&players[1]), before - 1);
    }

    #[test]
    fn no_reinforcements_means_no_token_and_no_change() {
        let (mut state, players) = game();
        let system = ti4_model::id::SystemId::new("some_system");
        let spare = state.tokens_in_reinforcements(&players[0]);
        state.gain_token(&players[0], TokenPool::Fleet, spare);
        assert_eq!(state.tokens_in_reinforcements(&players[0]), 0);

        let snapshot = state.board.clone();
        assert!(!place_command_token_from_reinforcements(
            &mut state,
            &players[0],
            &system
        ));
        assert_eq!(state.board.len(), snapshot.len(), "no system was created");
        assert!(
            !state
                .board
                .get(&system)
                .is_some_and(|here| here.command_tokens.contains(&players[0]))
        );
    }

    #[test]
    fn placed_token_events_are_not_staged_without_a_seated_faction_module() {
        let (mut state, players) = game();
        let system = ti4_model::id::SystemId::new("stage_probe");
        assert!(!crate::supply::staging_enabled(&state));
        assert!(!stage_command_token_placed(
            &mut state,
            &players[0],
            &system,
            crate::factions::hooks_cards::TokenPool::Reinforcements,
        ));
        assert_eq!(crate::supply::staged_events(&state), 0);
    }

    // -- Creuss wormhole tokens ---------------------------------------------------------------

    fn ring() -> ti4_content::galaxy::Galaxy {
        ti4_content::galaxy::Galaxy::build(
            ContentStore::embedded(),
            &["18", "19", "20", "21", "22", "23", "24", "39"],
            POK,
            2,
        )
        .unwrap()
    }

    #[test]
    fn a_creuss_token_links_its_system_to_every_wormhole_of_its_kind() {
        let (mut state, _) = game();
        let mut galaxy = ring();
        // Tile 39 prints an alpha wormhole and sits on the second ring; 18 is the centre.
        assert!(!galaxy.are_adjacent("18", "39"), "not beside each other");
        let was = place_creuss_token(&mut state, &galaxy, "ALPHA", &SystemId::new("18")).unwrap();
        assert_eq!(was, None);
        crate::laws::apply_to_galaxy(&state, &mut galaxy);
        assert!(
            galaxy.are_adjacent("18", "39"),
            "the token is an alpha wormhole for everyone"
        );
        // Moving it away unlinks the old system again.
        let was = place_creuss_token(&mut state, &galaxy, "ALPHA", &SystemId::new("19")).unwrap();
        assert_eq!(was, Some(SystemId::new("18")));
        crate::laws::apply_to_galaxy(&state, &mut galaxy);
        assert!(!galaxy.are_adjacent("18", "39"));
        assert!(galaxy.are_adjacent("19", "39"));
    }

    #[test]
    fn a_refused_placement_changes_nothing() {
        let (mut state, _) = game();
        let galaxy = ring();
        assert_eq!(
            place_creuss_token(&mut state, &galaxy, "GAMMA", &SystemId::new("19")),
            Err(WormholeTokenError::NotACreussToken("GAMMA".to_owned()))
        );
        assert_eq!(
            place_creuss_token(&mut state, &galaxy, "BETA", &SystemId::new("nowhere")),
            Err(WormholeTokenError::NotOnTheMap(SystemId::new("nowhere")))
        );
        assert!(state.wormhole_tokens.is_empty());
        place_creuss_token(&mut state, &galaxy, "BETA", &SystemId::new("19")).unwrap();
        let placed = state.wormhole_tokens.clone();
        assert_eq!(
            place_creuss_token(&mut state, &galaxy, "BETA", &SystemId::new("19")),
            Err(WormholeTokenError::AlreadyThere("BETA".to_owned()))
        );
        assert_eq!(state.wormhole_tokens, placed);
        assert_eq!(
            remove_creuss_token(&mut state, "BETA"),
            Some(SystemId::new("19"))
        );
        assert!(state.wormhole_tokens.is_empty());
        assert_eq!(remove_creuss_token(&mut state, "GAMMA"), None);
    }

    #[test]
    fn wormhole_generator_offers_controlled_planets_and_ship_free_non_home_systems() {
        let (mut state, [a, b]) = game();
        let content = ContentStore::embedded();
        let galaxy = ring();
        let ship = |owner: &PlayerId| {
            ti4_model::units::Unit::new(ti4_model::id::UnitTypeId::new("cruiser"), owner.clone())
        };
        // 19 holds a rival's ship, 20 holds one of mine, 21 a rival's ship over a planet I hold.
        state.system_mut(&SystemId::new("19")).units.push(ship(&b));
        state.system_mut(&SystemId::new("20")).units.push(ship(&a));
        state.system_mut(&SystemId::new("21")).units.push(ship(&b));
        state
            .system_mut(&SystemId::new("21"))
            .set_control(ti4_model::id::PlanetId::new("anyplanet"), a.clone());
        let offered: std::collections::BTreeSet<String> =
            wormhole_generator_destinations(&state, content, POK, &galaxy, &a)
                .into_iter()
                .filter(|(kind, _)| *kind == "ALPHA")
                .map(|(_, system)| system.to_string())
                .collect();
        assert!(
            !offered.contains("19"),
            "a rival's ships and no planet of mine"
        );
        assert!(offered.contains("20"), "my own ships do not bar it");
        assert!(
            offered.contains("21"),
            "a planet I control, ships notwithstanding"
        );
        assert!(offered.contains("22"), "an empty non-home system");
        // A token already in a system is not offered that system again.
        place_creuss_token(&mut state, &galaxy, "ALPHA", &SystemId::new("22")).unwrap();
        let again = wormhole_generator_destinations(&state, content, POK, &galaxy, &a);
        assert!(!again.contains(&("ALPHA", SystemId::new("22"))));
        assert!(again.contains(&("BETA", SystemId::new("22"))));
    }
}
