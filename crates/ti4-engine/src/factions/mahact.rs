//! The Mahact Gene-Sorcerers (`mahact`). See `plans/evidence/BF-mahact.md`.
//!
//! Split for parallel work: this file holds the abilities (Edict, Hubris, Imperia), the foreign
//! command tokens in the fleet pool, the technologies, the agent, the commander unlock, the
//! Scepter of Dominion and the breakthrough; `mahact_units.rs` holds the flagship, Crimson
//! Legionnaire I/II, the Starlancer mech and the hero.
//!
//! Card texts (latest printing, `crates/ti4-content/content/*.json`):
//!
//! * Edict: "Other player's tokens in your fleet pool increase your fleet limit but cannot be
//!   redistributed." and, "When you win a combat: Place 1 command token from your opponent's
//!   reinforcements in your fleet pool if it does not already contain 1 of that player's tokens."
//! * Hubris: "During setup, purge your "Alliance" promissory note. Other players cannot give you
//!   their 'Alliance" promissory note."
//! * Imperia: "While another player's command token is in your fleet pool, you can use the ability
//!   of that player's commander, if it is unlocked."
//! * Genetic Recombination (`gr`): "You may exhaust this card before a player casts votes; that
//!   player must cast at least 1 vote for an outcome of your choice or remove 1 token from their
//!   fleet pool and return it to their reinforcements." (`vote.rs`.)
//! * Jae Mir Kan (`mahactagent`): "When you would spend a command token during the secondary
//!   ability of a strategic action: You may exhaust this card to remove 1 of the active player's
//!   command tokens from the board and use it instead."
//! * Il Na Viroset (`mahactcommander`): unlock "Have 2 other factions' command tokens in your
//!   fleet pool." The effect is shared code (`tactical.rs`, gated on
//!   `promissory::has_commander_ability`).
//! * Scepter of Dominion (`scepter`): "At the start of the strategy phase: Choose 1 non-home
//!   system that contains your units, each other player who has a token on the Mahact player's
//!   command sheet places a token from their reinforcements in that system. Then, return this
//!   card to the Mahact player."
//! * Vaults of the Heir (`mahactbt`): "ACTION: Exhaust this card and purge 1 of your technologies
//!   to gain 1 relic."
//!
//! **The foreign tokens.** A foreign command token in the Mahact fleet pool is a command token in
//! that pool: it counts toward the Mahact player's fleet supply (`fleet::limit`, under the Fleet
//! Regulations cap like any other token in the pool), and it leaves its owner's reinforcements for
//! as long as it is held ([`reinforcements`]; the owner cannot place or gain it meanwhile).
//! Returning it ([`remove_token`]) gives it back to its owner's reinforcements. It is not on the
//! Mahact sheet's own pools, so the Mahact player's `fleet_tokens` count never includes it, and
//! "cannot be redistributed" holds because redistribution works on that count.
//!
//! **Scoping decisions** (also in the evidence file):
//!
//! * The two Edict windows are the space combat win (`SPACE_COMBAT_WON`) and the ground combat win
//!   (`GROUND_COMBAT_ENDED` with a `winner`). Several space opponents cannot occur at this engine's
//!   table sizes, but if two are eligible the Mahact player picks.
//! * Hubris purges the Alliance at the deal (`promissory::deal`); the refusal to receive one is
//!   `promissory::may_receive`, checked where transactions generate and validate offers.
//! * "Command sheet" in the Scepter means the fleet pool tokens held here.
//! * A Scepter whose window comes resolves (no "may"); with no legal system it still returns.

use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{LeaderId, PlayerId, SystemId, TechnologyId};
use ti4_model::state::{GameState, LeaderStatus};

use super::hooks_strategy::{SecondaryWaiver, StrategyHooks};
use super::{FactionModule, Hooks};
use crate::choice::{Choice, ChoiceOption};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::event::Event;
use crate::timing::{Ability, Relation, TimingContext, TimingError};

/// The faction alias; also the faction name in promissory note ids.
pub const FACTION: &str = "mahact";

const EDICT: &str = "edict";
const HUBRIS: &str = "hubris";
const IMPERIA: &str = "imperia";
/// Genetic Recombination.
pub const RECOMBINATION: &str = "gr";
const SCEPTER: &str = "scepter";
const BREAKTHROUGH: &str = "mahactbt";
const AGENT: &str = "mahactagent";
const COMMANDER: &str = "mahactcommander";

/// The component-action option id prefix.
const OPTION_PREFIX: &str = "faction|mahact|";
/// Prefix of the Vaults of the Heir options: `faction|mahact|vaults|<technology>`.
const VAULTS_PREFIX: &str = "vaults|";
/// Prefix of the agent's waiver ids: `agent|<system>`.
const AGENT_WAIVER_PREFIX: &str = "agent|";

/// What this faction implements; grows package by package.
pub const MODULE: FactionModule = FactionModule {
    alias: FACTION,
    abilities: &[EDICT, HUBRIS, IMPERIA],
    technologies: &[RECOMBINATION, "cl2"],
    units: super::mahact_units::UNITS,
    promissory: &[SCEPTER],
    leaders: super::mahact_units::LEADERS,
    breakthroughs: &[BREAKTHROUGH],
    hooks: Hooks {
        timing_abilities: Some(timing_abilities),
        component_actions: Some(component_actions),
        perform_component: Some(perform_component),
        commander_unlocked: Some(commander_unlocked),
        combat: super::mahact_units::COMBAT_HOOKS,
        leader_action: Some(super::mahact_units::leader_action),
        use_leader_timed: Some(super::mahact_units::use_leader_timed),
        strategy: StrategyHooks {
            secondary_waivers: Some(secondary_waivers),
            secondary_waived: Some(secondary_waived),
            ..StrategyHooks::NONE
        },
        ..Hooks::NONE
    },
};

// -- the foreign tokens in the fleet pool ----------------------------------------------------------

/// Prefix of every Mahact fleet-pool mark.
const MARK_PREFIX: &str = "mahact:fleet:";

/// The faction mark holding the other players' command tokens in `mahact`'s fleet pool:
/// `mahact:fleet:<mahact player>` = comma-separated owner ids, sorted, one entry per token. The
/// mark is absent while the pool holds none.
#[must_use]
pub fn fleet_mark(mahact: &PlayerId) -> String {
    format!("{MARK_PREFIX}{mahact}")
}

/// The owners of the other players' command tokens in `mahact`'s fleet pool, one entry per
/// token, sorted. The shared read every Mahact rule uses (Edict, Imperia, commander unlock,
/// Arvicon Rex, Starlancer, Scepter of Dominion). Public information.
#[must_use]
pub fn fleet_pool_owners(state: &GameState, mahact: &PlayerId) -> Vec<PlayerId> {
    state
        .faction_marks
        .get(&fleet_mark(mahact))
        .map(|list| {
            list.split(',')
                .filter(|id| !id.is_empty())
                .map(PlayerId::new)
                .collect()
        })
        .unwrap_or_default()
}

/// Whether `player` plays the Mahact.
#[must_use]
pub fn is_mahact(state: &GameState, player: &PlayerId) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.faction.as_str() == FACTION)
}

/// Whether a faction purges its Alliance note during setup (Hubris).
#[must_use]
pub(crate) fn purges_alliance(faction: &str) -> bool {
    faction == FACTION
}

/// How many foreign command tokens `player`'s fleet pool holds (0 unless they are the Mahact).
/// Read by `fleet::limit`.
#[must_use]
pub(crate) fn foreign_tokens(state: &GameState, player: &PlayerId) -> i32 {
    if !is_mahact(state, player) {
        return 0;
    }
    i32::try_from(fleet_pool_owners(state, player).len()).unwrap_or(i32::MAX)
}

/// How many command tokens of `owner` sit in a Mahact fleet pool, and so are not in their
/// reinforcements.
#[must_use]
pub fn held_by_mahact(state: &GameState, owner: &PlayerId) -> i32 {
    state.tokens_held_by_mahact(owner)
}

/// The command tokens `owner` has in reinforcements. The model already subtracts the tokens held in
/// a Mahact fleet pool (`GameState::tokens_held_by_mahact`, which reads the marks of
/// [`fleet_mark`]), so every reader, `gain_token` included, agrees.
#[must_use]
pub fn reinforcements(state: &GameState, owner: &PlayerId) -> i32 {
    state.tokens_in_reinforcements(owner)
}

fn store(state: &mut GameState, mahact: &PlayerId, mut owners: Vec<PlayerId>) {
    owners.sort();
    let key = fleet_mark(mahact);
    if owners.is_empty() {
        state.faction_marks.remove(&key);
    } else {
        let list: Vec<&str> = owners.iter().map(PlayerId::as_str).collect();
        state.faction_marks.insert(key, list.join(","));
    }
}

/// Place one of `owner`'s command tokens from their reinforcements in `mahact`'s fleet pool.
///
/// `false`, changing nothing, unless `mahact` plays the Mahact, `owner` is another seated player and
/// has a token in reinforcements. Whether the pool already holds one of theirs is the caller's rule
/// (Edict allows one; a later effect may allow more).
pub fn add_token(state: &mut GameState, mahact: &PlayerId, owner: &PlayerId) -> bool {
    if owner == mahact
        || !is_mahact(state, mahact)
        || state.player(owner).is_none()
        || reinforcements(state, owner) <= 0
    {
        return false;
    }
    let mut owners = fleet_pool_owners(state, mahact);
    owners.push(owner.clone());
    store(state, mahact, owners);
    true
}

/// Remove one of `owner`'s command tokens from `mahact`'s fleet pool and return it to their
/// reinforcements. `false`, changing nothing, if the pool holds none of theirs.
pub fn remove_token(state: &mut GameState, mahact: &PlayerId, owner: &PlayerId) -> bool {
    let mut owners = fleet_pool_owners(state, mahact);
    let Some(at) = owners.iter().position(|held| held == owner) else {
        return false;
    };
    owners.remove(at);
    store(state, mahact, owners);
    true
}

// -- shared helpers --------------------------------------------------------------------------------

fn decision(state: &GameState, player: &PlayerId, card: &str, subtype: &str) -> DecisionContext {
    DecisionContext::new(
        player.clone(),
        DecisionSource::FactionAbility(card.to_owned()),
        subtype,
        state.phase,
        state.round,
    )
}

fn leader_status(state: &GameState, player: &PlayerId, leader: &str) -> Option<LeaderStatus> {
    state
        .player(player)
        .and_then(|seat| seat.leaders.get(&LeaderId::new(leader)).copied())
}

/// Put one question to `who`. Every Mahact choice (Edict's opponent, the Scepter's system) is
/// asked here.
fn ask_among(
    context: &mut TimingContext<'_>,
    who: &PlayerId,
    prompt: String,
    card: &str,
    subtype: &str,
    options: Vec<ChoiceOption>,
) -> Result<ChoiceOption, TimingError> {
    let choice = Choice::new(who.clone(), prompt, options).contextualized(decision(
        context.state,
        who,
        card,
        subtype,
    ));
    context
        .ask_seeing(&choice)
        .map_err(TimingError::IllegalChoice)
}

// -- Imperia -------------------------------------------------------------------------------------

/// Imperia: whether `player`, a Mahact, can use `commander`'s ability because that commander's
/// owner has a command token in their fleet pool and the commander is unlocked. Called from
/// `promissory::has_commander_ability`, which every commander route consults.
#[must_use]
pub(crate) fn imperia_grants(state: &GameState, player: &PlayerId, commander: &str) -> bool {
    if !is_mahact(state, player) {
        return false;
    }
    let leader = LeaderId::new(commander);
    fleet_pool_owners(state, player).iter().any(|owner| {
        owner != player
            && state
                .player(owner)
                .is_some_and(|seat| seat.leaders.get(&leader) == Some(&LeaderStatus::Unlocked))
    })
}

// -- Edict ---------------------------------------------------------------------------------------

/// The opponents of a combat win this event reports, and the winner: a space win names `player`
/// and `opponents`; a ground combat names `winner` with its `attacker` and `defender`.
fn combat_win(event: &Event) -> Option<(PlayerId, Vec<PlayerId>)> {
    if let Some(winner) = event.text("winner") {
        let opponents = ["attacker", "defender"]
            .iter()
            .filter_map(|key| event.text(key))
            .filter(|side| *side != winner)
            .map(PlayerId::new)
            .collect();
        return Some((PlayerId::new(winner), opponents));
    }
    let winner = event.text("player")?;
    let opponents = event
        .payload
        .get("opponents")
        .and_then(serde_json::Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(serde_json::Value::as_str)
                .map(PlayerId::new)
                .collect()
        })
        .unwrap_or_default();
    Some((PlayerId::new(winner), opponents))
}

/// Opponents whose token Edict could place now for `mahact`: the Mahact won, the opponent's token
/// is in their reinforcements, and the pool holds none of theirs yet.
fn edict_targets(state: &GameState, mahact: &PlayerId, event: &Event) -> Vec<PlayerId> {
    let Some((winner, opponents)) = combat_win(event) else {
        return Vec::new();
    };
    if winner != *mahact || !is_mahact(state, mahact) {
        return Vec::new();
    }
    let held = fleet_pool_owners(state, mahact);
    let mut targets: Vec<PlayerId> = opponents
        .into_iter()
        .filter(|opponent| {
            opponent != mahact
                && state.player(opponent).is_some()
                && !held.contains(opponent)
                && reinforcements(state, opponent) > 0
        })
        .collect();
    targets.sort();
    targets.dedup();
    targets
}

/// "When you win a combat: Place 1 command token from your opponent's reinforcements in your fleet
/// pool if it does not already contain 1 of that player's tokens." One ability per way of winning.
fn edict(owner_name: &str, seat: &PlayerId, event_type: &'static str) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("ability:{owner_name}:{EDICT}:{event_type}:when"),
        seat.clone(),
        event_type,
        Relation::When,
        Arc::new(move |event, _resolver, context| {
            let targets = edict_targets(context.state, &owner, event);
            let target = match targets.as_slice() {
                [] => return Ok(()),
                [only] => only.clone(),
                _ => {
                    let options = targets
                        .iter()
                        .map(|who| {
                            ChoiceOption::labelled(
                                who.to_string(),
                                "edict",
                                format!("place {who}'s command token in your fleet pool"),
                            )
                        })
                        .collect();
                    let answer = ask_among(
                        context,
                        &owner,
                        "Edict: whose command token goes in your fleet pool".to_owned(),
                        EDICT,
                        "edict_opponent",
                        options,
                    )?;
                    PlayerId::new(answer.id)
                }
            };
            if targets.contains(&target) {
                add_token(context.state, &owner, &target);
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        !edict_targets(context.state, &condition_owner, event).is_empty()
    }))
}

// -- Genetic Recombination (the vote window is in vote.rs) ------------------------------------------

/// The Mahact seat that could use Genetic Recombination before `voter` casts votes: it owns the
/// technology, has not exhausted it, and is not the voter.
#[must_use]
pub(crate) fn recombination_holder(state: &GameState, voter: &PlayerId) -> Option<PlayerId> {
    // The Mahact's own card, or the Nekro's Valefar Assimilator carrying its text (only a Mahact
    // can own the card, so the faction test is the ownership test).
    state
        .players
        .iter()
        .find(|seat| {
            seat.id != *voter
                && crate::technology::technology_text_ready(state, &seat.id, RECOMBINATION)
        })
        .map(|seat| seat.id.clone())
}

/// Exhaust the holder's Genetic Recombination (or the Valefar Assimilator carrying its text).
pub(crate) fn exhaust_recombination(state: &mut GameState, holder: &PlayerId) {
    crate::technology::exhaust_technology_text(state, holder, RECOMBINATION);
}

/// Whether `voter` has a token in their fleet pool to remove.
#[must_use]
pub(crate) fn can_pay_tribute(state: &GameState, voter: &PlayerId) -> bool {
    state
        .player(voter)
        .is_some_and(|seat| seat.fleet_tokens > 0)
}

/// Remove 1 token from the voter's fleet pool and return it to their reinforcements.
pub(crate) fn pay_tribute(state: &mut GameState, voter: &PlayerId) -> bool {
    if !can_pay_tribute(state, voter) {
        return false;
    }
    state.gain_token(voter, ti4_model::state::TokenPool::Fleet, -1);
    true
}

// -- Jae Mir Kan: secondary token ------------------------------------------------------------------

/// Systems holding a command token of `primary`, in system order.
fn tokens_on_board(state: &GameState, primary: &PlayerId) -> Vec<SystemId> {
    state
        .board
        .iter()
        .filter(|(_, board)| board.command_tokens.contains(primary))
        .map(|(system, _)| system.clone())
        .collect()
}

/// What a player calls the system in a prompt: its planets ("Mecatol Rex", "Bakal and Alio Prima"),
/// else the tile number. The tile number follows, so two systems never read alike.
fn system_name(system: &SystemId) -> String {
    let content = ContentStore::embedded();
    let planets: Vec<String> =
        ti4_content::galaxy::planets_in(content, system.as_str(), ti4_model::content_types::POK)
            .iter()
            .filter_map(|planet| planet.name().map(str::to_owned))
            .collect();
    if planets.is_empty() {
        format!("system {system}")
    } else {
        format!("{} (system {system})", planets.join(" and "))
    }
}

/// "When you would spend a command token during the secondary ability of a strategic action: You
/// may exhaust this card to remove 1 of the active player's command tokens from the board and use
/// it instead." Offered beside the ordinary follow as one waiver per system holding a token.
fn secondary_waivers(
    state: &GameState,
    _content: &ContentStore,
    follower: &PlayerId,
    primary: &PlayerId,
    _card: &str,
) -> Vec<SecondaryWaiver> {
    if follower == primary
        || !is_mahact(state, follower)
        || leader_status(state, follower, AGENT) != Some(LeaderStatus::Readied)
    {
        return Vec::new();
    }
    tokens_on_board(state, primary)
        .into_iter()
        .map(|system| {
            let worded = crate::phrases::Phrase::of(&format!(
                "exhaust Jae Mir Kan: use {{primary}}'s command token from {} instead of a token",
                system_name(&system)
            ))
            .with("primary", primary, crate::phrases::Subject::seat(primary));
            SecondaryWaiver {
                id: format!("{AGENT_WAIVER_PREFIX}{system}"),
                label: worded.label(),
                worded: Some(worded),
            }
        })
        .collect()
}

/// Pay the waiver: exhaust the agent and take the active player's token off that system (it goes
/// to their reinforcements, which is where an unplaced token is).
fn secondary_waived(
    state: &mut GameState,
    _content: &ContentStore,
    follower: &PlayerId,
    primary: &PlayerId,
    waiver: &str,
) {
    let Some(system) = waiver.strip_prefix(AGENT_WAIVER_PREFIX) else {
        return;
    };
    let system = SystemId::new(system);
    let present = state
        .board
        .get(&system)
        .is_some_and(|board| board.command_tokens.contains(primary));
    if !present
        || !is_mahact(state, follower)
        || !crate::leaders::exhaust(state, follower, &LeaderId::new(AGENT))
    {
        return;
    }
    state.system_mut(&system).command_tokens.remove(primary);
}

// -- Il Na Viroset: unlock -------------------------------------------------------------------------

/// "Have 2 other factions' command tokens in your fleet pool": two tokens of different owners.
fn commander_unlocked(
    state: &GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    _galaxy: Option<&ti4_content::galaxy::Galaxy>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    if leader.as_str() != COMMANDER {
        return None;
    }
    let mut owners = fleet_pool_owners(state, player);
    owners.retain(|owner| owner != player);
    owners.dedup();
    Some(owners.len() >= 2)
}

// -- Scepter of Dominion -----------------------------------------------------------------------------

/// The Mahact seat whose Scepter `holder` holds, if they do.
fn scepter_owner(state: &GameState, holder: &PlayerId) -> Option<PlayerId> {
    let mahact = crate::promissory::seat_of(state, FACTION)?;
    (mahact != *holder
        && state
            .promissory_notes
            .get(&crate::promissory::note_id(SCEPTER, FACTION))
            == Some(holder))
    .then_some(mahact)
}

/// Non-home systems that contain `holder`'s units.
fn scepter_systems(state: &GameState, holder: &PlayerId) -> Vec<SystemId> {
    let homes: std::collections::BTreeSet<&SystemId> = state
        .players
        .iter()
        .filter_map(|seat| seat.home_system.as_ref())
        .collect();
    state
        .board
        .iter()
        .filter(|(system, board)| {
            !homes.contains(system)
                && board
                    .units
                    .iter()
                    .chain(board.planet_units.values().flatten())
                    .any(|unit| unit.owner == *holder)
        })
        .map(|(system, _)| system.clone())
        .collect()
}

/// Who the Scepter makes place a token in `system`: the owners of the tokens in the Mahact fleet
/// pool other than `holder`, without one there already and with one in reinforcements.
fn scepter_payers(
    state: &GameState,
    mahact: &PlayerId,
    holder: &PlayerId,
    system: &SystemId,
) -> Vec<PlayerId> {
    let mut payers = fleet_pool_owners(state, mahact);
    payers.retain(|payer| payer != holder);
    payers.dedup();
    payers.retain(|payer| {
        let placed = state
            .board
            .get(system)
            .is_some_and(|board| board.command_tokens.contains(payer));
        !placed && reinforcements(state, payer) > 0
    });
    payers
}

/// The option label of a Scepter system: who would place a token there (display only; the
/// decision records ids, not labels).
fn scepter_label(state: &GameState, payers: &[PlayerId], system: &SystemId) -> String {
    if payers.is_empty() {
        return format!(
            "nobody places a command token in {system} (only other players with a token in the \
             Mahact fleet pool place one)"
        );
    }
    let names: Vec<String> = payers
        .iter()
        .map(|payer| crate::promissory::faction_name(state, payer))
        .collect();
    let verb = if names.len() == 1 { "places" } else { "place" };
    format!("{} {verb} a command token in {system}", names.join(", "))
}

/// "At the start of the strategy phase: Choose 1 non-home system that contains your units, each
/// other player who has a token on the Mahact player's command sheet places a token from their
/// reinforcements in that system. Then, return this card to the Mahact player."
fn scepter(owner_name: &str, seat: &PlayerId) -> Ability {
    let (holder, condition_holder) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("promissory:{owner_name}:{SCEPTER}:STRATEGY_PHASE_BEGAN:after"),
        seat.clone(),
        "STRATEGY_PHASE_BEGAN",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            let Some(mahact) = scepter_owner(context.state, &holder) else {
                return Ok(());
            };
            let systems = scepter_systems(context.state, &holder);
            let system = match systems.as_slice() {
                [] => None,
                [only] => Some(only.clone()),
                _ => {
                    let options = systems
                        .iter()
                        .map(|system| {
                            ChoiceOption::labelled(
                                system.to_string(),
                                "scepter",
                                scepter_label(
                                    context.state,
                                    &scepter_payers(context.state, &mahact, &holder, system),
                                    system,
                                ),
                            )
                        })
                        .collect();
                    let answer = ask_among(
                        context,
                        &holder,
                        "Scepter of Dominion: choose a system".to_owned(),
                        SCEPTER,
                        "scepter_system",
                        options,
                    )?;
                    systems.into_iter().find(|s| s.as_str() == answer.id)
                }
            };
            if let Some(system) = system {
                // Placing one token never changes another payer's reinforcements or tokens here,
                // so deciding them all first is the same as checking each in turn.
                for payer in scepter_payers(context.state, &mahact, &holder, &system) {
                    context
                        .state
                        .system_mut(&system)
                        .command_tokens
                        .insert(payer);
                }
            }
            crate::promissory::give_back(
                context.state,
                &crate::promissory::note_id(SCEPTER, FACTION),
            );
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        scepter_owner(context.state, &condition_holder).is_some()
    }))
}

// -- Vaults of the Heir ------------------------------------------------------------------------------

/// `faction_marks` key: present while `player`'s Vaults of the Heir is exhausted.
fn vault_key(player: &PlayerId) -> String {
    format!("mahact:vaults:exhausted:{player}")
}

/// The technologies `player` could purge for Vaults of the Heir now: it holds the breakthrough,
/// ready, and the relic deck still has a card.
fn vault_technologies(state: &GameState, player: &PlayerId) -> Vec<TechnologyId> {
    if !is_mahact(state, player)
        || !crate::breakthroughs::holds(state, player, BREAKTHROUGH)
        || state.faction_marks.contains_key(&vault_key(player))
        || state.relic_deck.is_empty()
    {
        return Vec::new();
    }
    state
        .player(player)
        .map(|seat| seat.technologies.iter().cloned().collect())
        .unwrap_or_default()
}

/// "ACTION: Exhaust this card and purge 1 of your technologies to gain 1 relic." One option per
/// technology that can be purged.
fn component_actions(
    state: &GameState,
    _content: &ContentStore,
    player: &PlayerId,
) -> Vec<ChoiceOption> {
    vault_technologies(state, player)
        .into_iter()
        .map(|tech| {
            ChoiceOption::labelled(
                format!("{OPTION_PREFIX}{VAULTS_PREFIX}{tech}"),
                crate::faction_abilities::ACTION_KIND,
                format!("Vaults of the Heir: exhaust and purge {tech} to gain a relic"),
            )
        })
        .collect()
}

fn perform_component(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    option: &ChoiceOption,
) -> bool {
    let Some(tech) = option
        .id
        .strip_prefix(OPTION_PREFIX)
        .and_then(|rest| rest.strip_prefix(VAULTS_PREFIX))
    else {
        return false;
    };
    let tech = TechnologyId::new(tech);
    if !vault_technologies(context.state, player).contains(&tech) {
        return false;
    }
    context
        .state
        .faction_marks
        .insert(vault_key(player), "1".to_owned());
    crate::technology::purge(
        context.state,
        context.content,
        context.sources,
        player,
        &tech,
    );
    crate::relics::gain(context.state, player);
    true
}

/// The card readies with everything else in the status phase.
fn vault_readies(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("breakthrough:{owner_name}:{BREAKTHROUGH}_ready:STATUS_PHASE_ENDED:after"),
        seat.clone(),
        "STATUS_PHASE_ENDED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            context.state.faction_marks.remove(&vault_key(&owner));
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        context
            .state
            .faction_marks
            .contains_key(&vault_key(&condition_owner))
    }))
}

// -- timing registration -----------------------------------------------------------------------------

fn timing_abilities(state: &GameState, owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    let mut abilities = vec![
        edict(owner_name, seat, "SPACE_COMBAT_WON"),
        edict(owner_name, seat, "GROUND_COMBAT_ENDED"),
        scepter(owner_name, seat),
        vault_readies(owner_name, seat),
    ];
    abilities.extend(super::mahact_units::timing_abilities(
        state, owner_name, seat,
    ));
    abilities
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use ti4_model::content_types::DEFAULT;

    use crate::choice::{Scripted, Table};
    use crate::vote::VoteWindow;

    fn a() -> PlayerId {
        PlayerId::new("a")
    }
    fn b() -> PlayerId {
        PlayerId::new("b")
    }
    fn c() -> PlayerId {
        PlayerId::new("c")
    }
    fn content() -> &'static ContentStore {
        ContentStore::embedded()
    }
    fn game() -> GameState {
        crate::fixtures::seated_game(&[("a", "mahact"), ("b", "xxcha"), ("c", "hacan")], DEFAULT)
    }
    fn scripted(answers: &[&str]) -> Table {
        Table::with_default(Box::new(Scripted::new(
            answers.iter().map(|s| (*s).to_owned()),
        )))
    }
    fn emit(
        state: &mut GameState,
        table: &mut Table,
        kind: &str,
        pairs: &[(&str, serde_json::Value)],
    ) {
        let mut resolver = crate::fixtures::armed_resolver(state);
        crate::fixtures::with_context(state, DEFAULT, None, table, |ctx| {
            let payload: BTreeMap<String, serde_json::Value> = pairs
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect();
            let event = ctx.event_sequence.next(kind, payload).expect("an event id");
            resolver
                .emit_with_context(ctx, event, |_, _| {})
                .expect("emits");
        });
    }
    fn won_space(state: &mut GameState, winner: &str, opponents: &[&str]) {
        emit(
            state,
            &mut scripted(&[]),
            "SPACE_COMBAT_WON",
            &[
                ("player", winner.into()),
                ("system", "1".into()),
                ("opponents", serde_json::json!(opponents)),
            ],
        );
    }
    fn unlock(state: &mut GameState, who: &PlayerId, leader: &str) {
        state
            .player_mut(who)
            .unwrap()
            .leaders
            .insert(LeaderId::new(leader), LeaderStatus::Unlocked);
    }

    // -- the pool ------------------------------------------------------------------------------

    #[test]
    fn a_foreign_token_counts_in_the_fleet_limit_and_leaves_its_owners_reinforcements() {
        let mut state = game();
        let limit = crate::fleet::limit(&state, content(), &a());
        let before = reinforcements(&state, &b());
        assert!(add_token(&mut state, &a(), &b()));
        assert_eq!(fleet_pool_owners(&state, &a()), vec![b()]);
        assert_eq!(crate::fleet::limit(&state, content(), &a()), limit + 1);
        assert_eq!(reinforcements(&state, &b()), before - 1);
        assert_eq!(
            state.player(&a()).unwrap().fleet_tokens,
            3,
            "not redistributable"
        );
        // Nobody else's limit moves.
        assert_eq!(foreign_tokens(&state, &b()), 0);
        assert!(remove_token(&mut state, &a(), &b()));
        assert_eq!(reinforcements(&state, &b()), before, "returned");
        assert!(!state.faction_marks.contains_key(&fleet_mark(&a())));
        assert!(!remove_token(&mut state, &a(), &b()));
        // A seat that is not the Mahact has no pool to put tokens in.
        assert!(!add_token(&mut state, &b(), &c()));
        assert!(!add_token(&mut state, &a(), &a()));
    }

    #[test]
    fn an_owner_cannot_regain_a_mahact_held_token() {
        use ti4_model::state::TokenPool;
        let mut state = game();
        // b has exactly one token in reinforcements, and it goes into the Mahact pool.
        while state.tokens_in_reinforcements(&b()) > 1 {
            state.player_mut(&b()).unwrap().fleet_tokens += 1;
        }
        assert!(add_token(&mut state, &a(), &b()));
        assert_eq!(state.tokens_in_reinforcements(&b()), 0);
        assert_eq!(state.gain_token(&b(), TokenPool::Tactic, 1), 0, "model cap");
        assert_eq!(held_by_mahact(&state, &b()), 1);
        // Back in the owner's reinforcements, it can be gained again.
        assert!(remove_token(&mut state, &a(), &b()));
        assert_eq!(state.gain_token(&b(), TokenPool::Tactic, 1), 1);
    }

    #[test]
    fn a_token_cannot_be_placed_without_one_in_reinforcements() {
        let mut state = game();
        while reinforcements(&state, &b()) > 0 {
            state.player_mut(&b()).unwrap().fleet_tokens += 1;
        }
        let before = state.clone();
        assert!(!add_token(&mut state, &a(), &b()));
        assert!(state == before);
    }

    // -- Edict ---------------------------------------------------------------------------------

    #[test]
    fn edict_takes_the_opponents_token_once_after_a_space_win() {
        let mut state = game();
        won_space(&mut state, "a", &["b"]);
        assert_eq!(fleet_pool_owners(&state, &a()), vec![b()]);
        won_space(&mut state, "a", &["b"]);
        assert_eq!(fleet_pool_owners(&state, &a()), vec![b()], "only one each");
        won_space(&mut state, "a", &["c"]);
        assert_eq!(fleet_pool_owners(&state, &a()), vec![b(), c()]);
        // Losing places nothing.
        let mut lost = game();
        won_space(&mut lost, "b", &["a"]);
        assert!(fleet_pool_owners(&lost, &a()).is_empty());
    }

    #[test]
    fn edict_works_after_a_ground_combat_win_and_needs_a_token_to_take() {
        let mut state = game();
        emit(
            &mut state,
            &mut scripted(&[]),
            "GROUND_COMBAT_ENDED",
            &[
                ("system", "1".into()),
                ("planet", "x".into()),
                ("attacker", "a".into()),
                ("defender", "b".into()),
                ("winner", "a".into()),
            ],
        );
        assert_eq!(fleet_pool_owners(&state, &a()), vec![b()]);
        let mut empty = game();
        while reinforcements(&empty, &c()) > 0 {
            empty.player_mut(&c()).unwrap().fleet_tokens += 1;
        }
        won_space(&mut empty, "a", &["c"]);
        assert!(fleet_pool_owners(&empty, &a()).is_empty());
    }

    #[test]
    fn two_eligible_opponents_are_the_mahact_players_choice() {
        let mut state = game();
        emit(
            &mut state,
            &mut scripted(&["c"]),
            "SPACE_COMBAT_WON",
            &[
                ("player", "a".into()),
                ("system", "1".into()),
                ("opponents", serde_json::json!(["b", "c"])),
            ],
        );
        assert_eq!(fleet_pool_owners(&state, &a()), vec![c()]);
    }

    #[test]
    fn games_without_the_mahact_are_untouched_by_every_window() {
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        let before = state.clone();
        won_space(&mut state, "a", &["b"]);
        emit(&mut state, &mut scripted(&[]), "STRATEGY_PHASE_BEGAN", &[]);
        emit(&mut state, &mut scripted(&[]), "STATUS_PHASE_ENDED", &[]);
        assert!(state == before);
        assert!(state.faction_marks.is_empty());
    }

    // -- Hubris --------------------------------------------------------------------------------

    #[test]
    fn hubris_purges_the_alliance_and_refuses_anyone_elses() {
        let mut state = game();
        crate::promissory::deal(&mut state, content(), DEFAULT);
        assert!(!state.promissory_notes.contains_key("an:mahact"));
        assert!(state.promissory_notes.contains_key("an:xxcha"));
        assert!(state.promissory_notes.contains_key("cf:mahact"));
        assert!(!crate::promissory::may_receive(&state, &a(), "an:xxcha"));
        assert!(crate::promissory::may_receive(&state, &a(), "cf:xxcha"));
        assert!(crate::promissory::may_receive(&state, &b(), "an:hacan"));

        unlock(&mut state, &b(), "xxchacommander");
        state.phase = ti4_model::state::Phase::Agenda;
        let galaxy = crate::fixtures::plain_hub().galaxy;
        let offer = crate::transactions::Offer {
            proposer: b(),
            partner: a(),
            given: crate::transactions::Terms {
                promissory: Some("an:xxcha".to_owned()),
                ..crate::transactions::Terms::default()
            },
            received: crate::transactions::Terms::default(),
        };
        assert!(matches!(
            crate::transactions::why_illegal(&state, content(), &galaxy, &offer),
            Some(crate::transactions::OfferError::NoteRefused(..))
        ));
    }

    #[test]
    fn the_deal_builder_never_offers_an_alliance_to_a_mahact_seat() {
        use crate::diplomacy::builder::{ContactScope, Draft, item_options};
        let mut state = game();
        crate::promissory::deal(&mut state, content(), DEFAULT);
        unlock(&mut state, &b(), "xxchacommander");
        // b holds its own Alliance, so it is offerable to anyone Hubris allows.
        state.promissory_notes.insert("an:xxcha".to_owned(), b());
        let note = "diplomacy|note|an:xxcha";
        let physical = ContactScope {
            physical: true,
            ..ContactScope::default()
        };
        let offers = |to: PlayerId| {
            let draft = Draft::new(b(), to);
            item_options(&state, content(), &physical, &draft)
                .iter()
                .any(|option| option.id == note)
        };
        assert!(
            offers(c()),
            "another seat may be offered it: {:?}",
            item_options(&state, content(), &physical, &Draft::new(b(), c()))
                .iter()
                .map(|option| option.id.clone())
                .collect::<Vec<_>>()
        );
        assert!(!offers(a()), "Hubris: not the Mahact");
        // Asking: the other seat is the giver, the builder the receiver.
        // (a asks b: b gives, a receives.)
        let mut asking = Draft::new(a(), b());
        asking.asking = true;
        let options = item_options(&state, content(), &physical, &asking);
        assert!(!options.iter().any(|option| option.id == note));
        // The initial candidates never put it in a deal with the Mahact either.
        state.diplomacy.enabled = true;
        let galaxy = crate::fixtures::plain_hub().galaxy;
        let bundles = crate::diplomacy::candidates::generate_initial_candidates(
            &crate::diplomacy::candidates::CandidateContext {
                state: &state,
                content: content(),
                galaxy: &galaxy,
                proposer: &b(),
                recipient: &a(),
                agenda: None,
            },
        );
        assert!(!format!("{bundles:?}").contains("an:xxcha"));
        let to_c = crate::diplomacy::candidates::generate_initial_candidates(
            &crate::diplomacy::candidates::CandidateContext {
                state: &state,
                content: content(),
                galaxy: &galaxy,
                proposer: &b(),
                recipient: &c(),
                agenda: None,
            },
        );
        assert!(format!("{to_c:?}").contains("an:xxcha"));
    }

    // -- Imperia -------------------------------------------------------------------------------

    #[test]
    fn imperia_lends_unlocked_commanders_of_token_owners_through_their_real_routes() {
        let mut state = game();
        unlock(&mut state, &b(), "xxchacommander");
        unlock(&mut state, &c(), "hacancommander");
        assert!(!crate::leaders::elder_qanoj(&state, &a()), "no tokens yet");
        add_token(&mut state, &a(), &b());
        assert!(crate::leaders::elder_qanoj(&state, &a()), "Elder Qanoj");
        assert!(!crate::promissory::has_commander_ability(
            &state,
            &a(),
            "hacancommander"
        ));
        add_token(&mut state, &a(), &c());
        assert!(crate::promissory::has_commander_ability(
            &state,
            &a(),
            "hacancommander"
        ));
        // Route 1: Elder Qanoj's extra vote on every planet in the real vote window.
        let planet = state.controlled_planets(&a())[0].1.clone();
        let influence = crate::vote::influence_of(&state, content(), DEFAULT, &planet);
        state.speaker = c();
        let mut window = VoteWindow::new(&state, "x", vec!["for".into(), "against".into()]);
        window.open(&state, content(), DEFAULT);
        while window
            .pending_choice(&state, content(), DEFAULT)
            .is_some_and(|q| q.player != a())
        {
            let q = window.pending_choice(&state, content(), DEFAULT).unwrap();
            let decline = q.options.iter().find(|o| o.is_decline()).unwrap().clone();
            window
                .resolve(&mut state, content(), DEFAULT, decline)
                .unwrap();
        }
        let q = window.pending_choice(&state, content(), DEFAULT).unwrap();
        assert_eq!(q.player, a());
        let vote_for = q.options.iter().find(|o| o.id == "for").unwrap().clone();
        window
            .resolve(&mut state, content(), DEFAULT, vote_for)
            .unwrap();
        let q = window.pending_choice(&state, content(), DEFAULT).unwrap();
        let exhaust = q.options.iter().find(|o| !o.is_decline()).unwrap();
        assert!(
            exhaust.label.contains(&format!("{} votes", influence + 1)),
            "{}",
            exhaust.label
        );
        // Route 2: Gila's trade-goods votes.
        state.player_mut(&a()).unwrap().trade_goods = 2;
        let exhaust = exhaust.clone();
        window
            .resolve(&mut state, content(), DEFAULT, exhaust)
            .unwrap();
        let mut guard = 0;
        while let Some(q) = window.pending_choice(&state, content(), DEFAULT) {
            if q.player == a() && q.options.iter().any(|o| o.id == "spend|2") {
                assert!(q.options.iter().any(|o| o.label.contains("4 votes")));
                return;
            }
            let pick = q
                .options
                .iter()
                .find(|o| !o.is_decline())
                .unwrap_or(&q.options[0])
                .clone();
            window
                .resolve(&mut state, content(), DEFAULT, pick)
                .unwrap();
            guard += 1;
            assert!(guard < 20, "Gila never offered");
        }
        panic!("Gila never offered");
    }

    #[test]
    fn imperia_needs_the_commander_unlocked_and_the_token_held() {
        let mut state = game();
        add_token(&mut state, &a(), &b());
        assert!(!crate::leaders::elder_qanoj(&state, &a()), "still locked");
        unlock(&mut state, &b(), "xxchacommander");
        remove_token(&mut state, &a(), &b());
        assert!(!crate::leaders::elder_qanoj(&state, &a()), "token returned");
        // Nobody else gets it from a pool they do not own.
        add_token(&mut state, &a(), &b());
        assert!(!crate::promissory::has_commander_ability(
            &state,
            &c(),
            "xxchacommander"
        ));
    }

    // -- Genetic Recombination -----------------------------------------------------------------

    fn recombination_game() -> GameState {
        let mut state = game();
        state
            .player_mut(&a())
            .unwrap()
            .technologies
            .insert(TechnologyId::new(RECOMBINATION));
        state.speaker = a();
        state
    }

    fn pick(window: &mut VoteWindow, state: &mut GameState, id: &str) {
        let q = window.pending_choice(state, content(), DEFAULT).unwrap();
        let option = q.options.iter().find(|o| o.id == id).unwrap_or_else(|| {
            panic!(
                "{id} not in {:?}",
                q.options.iter().map(|o| &o.id).collect::<Vec<_>>()
            )
        });
        window
            .resolve(state, content(), DEFAULT, option.clone())
            .unwrap();
    }

    #[test]
    fn recombination_binds_a_voter_who_chooses_to_vote() {
        let mut state = recombination_game();
        let mut window = VoteWindow::new(&state, "x", vec!["for".into(), "against".into()]);
        window.open(&state, content(), DEFAULT);
        // b votes first (speaker a is last); the holder is asked before b casts.
        let q = window.pending_choice(&state, content(), DEFAULT).unwrap();
        assert_eq!(q.player, a());
        pick(&mut window, &mut state, "recombine|against");
        assert!(
            state
                .player(&a())
                .unwrap()
                .exhausted_technologies
                .contains(&TechnologyId::new(RECOMBINATION))
        );
        let q = window.pending_choice(&state, content(), DEFAULT).unwrap();
        assert_eq!(q.player, b());
        pick(&mut window, &mut state, "tribute|vote");
        let q = window.pending_choice(&state, content(), DEFAULT).unwrap();
        assert_eq!(q.options.len(), 1, "only the demanded outcome");
        assert_eq!(q.options[0].id, "against");
        pick(&mut window, &mut state, "against");
        let q = window.pending_choice(&state, content(), DEFAULT).unwrap();
        assert!(
            !q.options.iter().any(|o| o.is_decline()),
            "first planet is forced"
        );
    }

    #[test]
    fn recombination_lets_a_voter_pay_a_fleet_token_instead() {
        let mut state = recombination_game();
        let tokens = state.player(&b()).unwrap().fleet_tokens;
        let mut window = VoteWindow::new(&state, "x", vec!["for".into(), "against".into()]);
        window.open(&state, content(), DEFAULT);
        pick(&mut window, &mut state, "recombine|for");
        pick(&mut window, &mut state, "tribute|token");
        assert_eq!(state.player(&b()).unwrap().fleet_tokens, tokens - 1);
        let q = window.pending_choice(&state, content(), DEFAULT).unwrap();
        assert!(q.options.iter().any(|o| o.is_decline()), "free to abstain");
        // The holder is not asked again for the same voter, nor with the card exhausted.
        pick(&mut window, &mut state, "decline");
        let q = window.pending_choice(&state, content(), DEFAULT).unwrap();
        assert_ne!(q.player, a(), "exhausted: c votes next unprompted");
    }

    #[test]
    fn recombination_forces_the_only_possible_way_and_can_be_declined() {
        let mut state = recombination_game();
        state.player_mut(&b()).unwrap().fleet_tokens = 0;
        let mut window = VoteWindow::new(&state, "x", vec!["for".into(), "against".into()]);
        window.open(&state, content(), DEFAULT);
        pick(&mut window, &mut state, "recombine|for");
        let q = window.pending_choice(&state, content(), DEFAULT).unwrap();
        assert_eq!(q.options.len(), 1, "no token to give: must vote for");
        let mut other = recombination_game();
        let mut window = VoteWindow::new(&other, "x", vec!["for".into(), "against".into()]);
        window.open(&other, content(), DEFAULT);
        pick(&mut window, &mut other, "decline");
        assert!(
            !other
                .player(&a())
                .unwrap()
                .exhausted_technologies
                .contains(&TechnologyId::new(RECOMBINATION))
        );
        // Without the technology nobody is asked.
        let plain = game();
        let mut window = VoteWindow::new(&plain, "x", vec!["for".into(), "against".into()]);
        window.open(&plain, content(), DEFAULT);
        assert_ne!(
            window
                .pending_choice(&plain, content(), DEFAULT)
                .unwrap()
                .player,
            a()
        );
    }

    // -- Jae Mir Kan ---------------------------------------------------------------------------

    #[test]
    fn the_agent_pays_a_secondary_with_the_active_players_board_token() {
        let mut state = game();
        let sys = SystemId::new("27");
        state.system_mut(&sys).command_tokens.insert(b());
        let waivers = secondary_waivers(&state, content(), &a(), &b(), "Trade");
        assert_eq!(waivers.len(), 1);
        assert_eq!(waivers[0].id, "agent|27");
        assert!(
            waivers[0].label.contains("(system 27)") && !waivers[0].label.contains("from 27"),
            "the system is named by its planets: {}",
            waivers[0].label
        );
        // The label names the active player by id; the waiver says which seat it is.
        let phrase = waivers[0].worded.as_ref().expect("named").phrase();
        assert_eq!(phrase.refs["primary"], crate::phrases::Subject::seat(&b()));
        assert_eq!(
            crate::phrases::rendered(&phrase, &[("primary", b().as_str())]),
            waivers[0].label
        );
        assert!(secondary_waivers(&state, content(), &c(), &b(), "Trade").is_empty());
        assert!(secondary_waivers(&state, content(), &a(), &a(), "Trade").is_empty());
        let tokens = state.player(&a()).unwrap().strategic_tokens;
        secondary_waived(&mut state, content(), &a(), &b(), "agent|27");
        assert!(!state.system_state(&sys).command_tokens.contains(&b()));
        assert_eq!(
            state.player(&a()).unwrap().leaders[&LeaderId::new(AGENT)],
            LeaderStatus::Exhausted
        );
        assert_eq!(state.player(&a()).unwrap().strategic_tokens, tokens);
        assert!(secondary_waivers(&state, content(), &a(), &b(), "Trade").is_empty());
    }

    // -- the commander -------------------------------------------------------------------------

    #[test]
    fn the_commander_unlocks_with_tokens_of_two_other_factions() {
        let mut state = game();
        let leader = LeaderId::new(COMMANDER);
        let ask = |s: &GameState| commander_unlocked(s, content(), DEFAULT, None, &a(), &leader);
        assert_eq!(ask(&state), Some(false));
        add_token(&mut state, &a(), &b());
        assert_eq!(ask(&state), Some(false));
        add_token(&mut state, &a(), &c());
        assert_eq!(ask(&state), Some(true));
        assert_eq!(
            commander_unlocked(&state, content(), DEFAULT, None, &a(), &LeaderId::new("x")),
            None
        );
        let unlocked = crate::leaders::check_unlocks(&mut state, content(), DEFAULT, None, &a());
        assert!(unlocked.contains(&leader));
    }

    // -- Scepter of Dominion -------------------------------------------------------------------

    #[test]
    fn the_scepter_places_the_pool_owners_tokens_and_returns() {
        let mut state = game();
        add_token(&mut state, &a(), &b());
        add_token(&mut state, &a(), &c());
        let sys = SystemId::new("27");
        crate::fixtures::put(&mut state, &sys, "carrier", &c(), 1);
        crate::promissory::take(&mut state, content(), &c(), "scepter:mahact");
        emit(&mut state, &mut scripted(&[]), "STRATEGY_PHASE_BEGAN", &[]);
        let here = state.system_state(&sys);
        assert!(here.command_tokens.contains(&b()));
        assert!(
            !here.command_tokens.contains(&c()),
            "the holder places none"
        );
        assert_eq!(state.promissory_notes["scepter:mahact"], a(), "returned");
    }

    #[test]
    fn the_scepter_returns_even_with_nowhere_to_go_and_never_fires_unheld() {
        let mut state = game();
        add_token(&mut state, &a(), &b());
        let before = state.clone();
        emit(&mut state, &mut scripted(&[]), "STRATEGY_PHASE_BEGAN", &[]);
        assert!(state == before, "held by its owner: nothing");
        crate::promissory::take(&mut state, content(), &c(), "scepter:mahact");
        let placed = |s: &GameState| {
            s.board
                .values()
                .filter(|board| board.command_tokens.contains(&b()))
                .count()
        };
        let tokens = placed(&state);
        emit(&mut state, &mut scripted(&[]), "STRATEGY_PHASE_BEGAN", &[]);
        assert_eq!(placed(&state), tokens, "home systems are excluded");
        assert_eq!(state.promissory_notes["scepter:mahact"], a());
    }

    /// fb_1791905479: the holder's own token was the only one in the Mahact fleet pool, so nobody
    /// placed a token, yet every system option promised "each other player places a command
    /// token". The labels now name who would place one, or say that nobody would.
    #[test]
    fn the_scepter_system_options_say_who_places_a_token() {
        use crate::choice::{Capturing, FirstOption};
        let mut state = game();
        add_token(&mut state, &a(), &c());
        for system in ["27", "26"] {
            crate::fixtures::put(&mut state, &SystemId::new(system), "carrier", &c(), 1);
        }
        crate::promissory::take(&mut state, content(), &c(), "scepter:mahact");
        let (capturing, seen) = Capturing::new(Box::new(FirstOption));
        let mut table = Table::with_default(Box::new(capturing));
        let before = state.clone();
        emit(&mut state, &mut table, "STRATEGY_PHASE_BEGAN", &[]);
        let asked = seen.borrow();
        let choice = asked
            .iter()
            .find(|choice| choice.prompt.starts_with("Scepter of Dominion"))
            .expect("the holder chooses a system");
        for option in &choice.options {
            assert!(
                option.label.contains("nobody places a command token"),
                "{}",
                option.label
            );
        }
        assert!(
            state
                .board
                .iter()
                .all(|(id, board)| board.command_tokens == before.system_state(id).command_tokens),
            "and indeed nobody did"
        );

        let mut state = game();
        add_token(&mut state, &a(), &b());
        add_token(&mut state, &a(), &c());
        for system in ["27", "26"] {
            crate::fixtures::put(&mut state, &SystemId::new(system), "carrier", &c(), 1);
        }
        crate::promissory::take(&mut state, content(), &c(), "scepter:mahact");
        let (capturing, seen) = Capturing::new(Box::new(FirstOption));
        let mut table = Table::with_default(Box::new(capturing));
        emit(&mut state, &mut table, "STRATEGY_PHASE_BEGAN", &[]);
        let asked = seen.borrow();
        let choice = asked
            .iter()
            .find(|choice| choice.prompt.starts_with("Scepter of Dominion"))
            .unwrap();
        assert!(
            choice.options[0]
                .label
                .starts_with("xxcha places a command token in "),
            "{}",
            choice.options[0].label
        );
    }

    // -- Vaults of the Heir --------------------------------------------------------------------

    #[test]
    fn the_vaults_purge_a_technology_for_a_relic_and_ready_in_the_status_phase() {
        let mut state = game();
        state.player_mut(&a()).unwrap().breakthrough =
            Some(ti4_model::id::BreakthroughId::new(BREAKTHROUGH));
        state.relic_deck.clear();
        assert!(
            component_actions(&state, content(), &a()).is_empty(),
            "no relic left"
        );
        state.relic_deck = vec![ti4_model::id::RelicId::new("stuff")];
        let options = component_actions(&state, content(), &a());
        assert_eq!(
            options.len(),
            state.player(&a()).unwrap().technologies.len()
        );
        let tech = TechnologyId::new("pi");
        assert!(state.player(&a()).unwrap().technologies.contains(&tech));
        let option = options
            .iter()
            .find(|o| o.id.ends_with("|pi"))
            .expect("pi")
            .clone();
        let mut table = scripted(&[]);
        let done = crate::fixtures::with_context(&mut state, DEFAULT, None, &mut table, |ctx| {
            perform_component(ctx, &a(), &option)
        });
        assert!(done);
        let seat = state.player(&a()).unwrap();
        assert!(!seat.technologies.contains(&tech));
        assert_eq!(seat.relics.len(), 1);
        assert!(
            component_actions(&state, content(), &a()).is_empty(),
            "exhausted"
        );
        emit(&mut state, &mut scripted(&[]), "STATUS_PHASE_ENDED", &[]);
        state.relic_deck = vec![ti4_model::id::RelicId::new("more")];
        assert!(
            !component_actions(&state, content(), &a()).is_empty(),
            "readied"
        );
        // Without the breakthrough there is nothing.
        let mut plain = game();
        plain.relic_deck = vec![ti4_model::id::RelicId::new("stuff")];
        assert!(component_actions(&plain, content(), &a()).is_empty());
        assert!(component_actions(&plain, content(), &b()).is_empty());
    }
}
