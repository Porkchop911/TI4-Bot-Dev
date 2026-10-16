//! The Nekro Virus (`nekro`). See `plans/evidence/BF-nekro.md`.
//!
//! Split for parallel work: this file holds the abilities (Galactic Threat, Technological
//! Singularity, Propagation), the Valefar Assimilators X/Y and the technology-rights predicate
//! they need (`technology::has_technology_text`), the Valefar Assimilator Z breakthrough helper
//! and the Antivirus note; `nekro_units.rs` holds the flagship, mech, agent, hero, commander
//! unlock and the Thunder's Edge technologies.
//!
//! State kept on [`GameState::faction_marks`], all public:
//!
//! | key | value |
//! |---|---|
//! | (model) `Player::assimilated_technologies` | `vax` / `vay` to the faction technology its token sits on |
//! | `nekro:token:<player>:Z` | comma-separated faction aliases whose sheets carry a Z token (at most 7, one per flagship) |
//! | `nekro:propagation:<player>` | researches replaced by Propagation, tokens still to be placed |
//! | `nekro:ts:<system>[\|<planet>]` | `<opponent>\|open` or `<opponent>\|used`, for the combat there |
//! | `nekro:threat:<player>` | `<agenda_seq>\|<outcome>` predicted by Galactic Threat |
//! | `nekro:threat_phase:<player>` | the round whose agenda phase already used Galactic Threat |

use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlayerId, TechnologyId};
use ti4_model::state::GameState;

use super::{FactionModule, Hooks};
use crate::choice::{Choice, ChoiceOption};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::event::Event;
use crate::timing::{Ability, Relation, TimingContext, TimingError};

/// The faction alias; also the faction name in promissory note ids.
pub const FACTION: &str = "nekro";

/// The Valefar Assimilator Z breakthrough.
pub const Z_BREAKTHROUGH: &str = "nekrobt";

/// Faction technologies that cannot work for the Nekro, so a token on one would lend nothing and
/// none is offered one.
///
/// * `executiveorder`: its vote makes the owner the speaker with a vote of their own; the Nekro
///   cannot vote (Galactic Threat), so the text is unusable by design.
pub const TEXT_NOT_LENDABLE: &[&str] = &["executiveorder"];

/// The flagships whose text the Nekro flagship can gain through a Z token: every base and
/// Prophecy of Kings flagship but its own (per-flagship status in the evidence file: some are
/// lent but inert for the Nekro). The Keleres variants share one flagship id.
pub const LENDABLE_FLAGSHIPS: &[&str] = &[
    "arborec_flagship",
    "argent_flagship",
    "bastion_flagship",
    "cabal_flagship",
    "crimson_flagship",
    "deepwrought_flagship",
    "empyrean_flagship",
    "firmament_flagship",
    "ghost_flagship",
    "hacan_flagship",
    "jolnar_flagship",
    "keleres_flagship",
    "l1z1x_flagship",
    "letnev_flagship",
    "mahact_flagship",
    "mentak_flagship",
    "muaat_flagship",
    "naalu_flagship",
    "naaz_flagship",
    "nomad_flagship",
    "ralnel_flagship",
    "sardakk_flagship",
    "sol_flagship",
    "titans_flagship",
    "winnu_flagship",
    "xxcha_flagship",
    "yin_flagship",
    "yssaril_flagship",
];

/// How many Z tokens the Nekro has (operator ruling 2026-10-07).
pub const Z_TOKEN_COUNT: usize = 7;

/// The Nekro's own flagship unit id.
pub const NEKRO_FLAGSHIP: &str = "nekro_flagship";

/// The assimilator tokens and the technology card each belongs to.
const TOKENS: [(char, &str); 2] = [('X', "vax"), ('Y', "vay")];

/// The `faction|nekro|...` choice kind of every assimilation question.
const ASSIMILATE_KIND: &str = "nekro_assimilate";

/// What this faction implements; grows package by package.
pub const MODULE: FactionModule = FactionModule {
    alias: FACTION,
    abilities: &[
        "galactic_threat",
        "technological_singularity",
        "propagation",
    ],
    technologies: super::nekro_units::TECHNOLOGIES,
    units: super::nekro_units::UNITS,
    promissory: &["antivirus"],
    leaders: super::nekro_units::LEADERS,
    breakthroughs: &[Z_BREAKTHROUGH],
    hooks: Hooks {
        timing_abilities: Some(timing_abilities),
        combat: super::nekro_units::COMBAT_HOOKS,
        commander_unlocked: Some(super::nekro_units::commander_unlocked),
        leader_action: Some(super::nekro_units::leader_action),
        use_leader: Some(super::nekro_units::use_leader),
        component_actions: Some(super::nekro_units::component_actions),
        perform_component: Some(super::nekro_units::perform_component),
        ..Hooks::NONE
    },
};

// -- ownership and token state -------------------------------------------------------------------

/// Whether this seat plays the Nekro Virus (and so has its three abilities).
#[must_use]
pub fn is_nekro(state: &GameState, player: &PlayerId) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.faction.as_str() == FACTION)
}

fn owns(state: &GameState, player: &PlayerId, technology: &str) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.technologies.contains(&TechnologyId::new(technology)))
}

fn z_mark(player: &PlayerId) -> String {
    format!("nekro:token:{player}:Z")
}

/// Whether a seat other than `player` owns `technology`.
fn owned_by_another(state: &GameState, player: &PlayerId, technology: &str) -> bool {
    state.players.iter().any(|seat| {
        seat.id != *player && seat.technologies.contains(&TechnologyId::new(technology))
    })
}

/// The technology the card's token sits on, only while it still counts: the Nekro owns the card
/// and another player still owns that technology.
fn live_token(state: &GameState, player: &PlayerId, card: &str) -> Option<String> {
    if !is_nekro(state, player) || !owns(state, player, card) {
        return None;
    }
    // The token lives on the model's `assimilated_technologies`: card alias to the technology.
    let target = state.player(player)?.assimilated_technologies.get(card)?;
    owned_by_another(state, player, target.as_str()).then(|| target.to_string())
}

/// The Valefar Assimilator card (`vax` or `vay`) whose token sits on `technology` and so lends
/// this player its text, if any.
#[must_use]
pub fn assimilated_card(
    state: &GameState,
    player: &PlayerId,
    technology: &str,
) -> Option<&'static str> {
    TOKENS.iter().find_map(|(_, card)| {
        (live_token(state, player, card).as_deref() == Some(technology)).then_some(*card)
    })
}

/// How many Valefar Assimilator cards have their token on a technology. Part B's commander
/// unlock: "A Valefar Assimilator technology counts only if its X or Y token is on a technology."
#[must_use]
pub fn assimilators_with_tokens(state: &GameState, player: &PlayerId) -> usize {
    TOKENS
        .iter()
        .filter(|(_, card)| live_token(state, player, card).is_some())
        .count()
}

/// The factions whose sheets carry a Z token (in placement order), while the breakthrough is
/// held. Placements are permanent.
#[must_use]
pub fn z_assimilated_factions(state: &GameState, player: &PlayerId) -> Vec<String> {
    if !is_nekro(state, player) || !crate::breakthroughs::holds(state, player, Z_BREAKTHROUGH) {
        return Vec::new();
    }
    state
        .faction_marks
        .get(&z_mark(player))
        .map(|marks| {
            marks
                .split(',')
                .filter(|faction| !faction.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// The flagship unit ids whose text the Nekro flagship has switched on (one per Z token).
#[must_use]
pub fn z_assimilated_flagships(state: &GameState, player: &PlayerId) -> Vec<String> {
    z_assimilated_factions(state, player)
        .iter()
        .map(|faction| flagship_of(faction))
        .collect()
}

/// Whether the Z token on `flagship_id`'s faction has switched that flagship's text on for a
/// Nekro flagship owned by `owner`.
#[must_use]
pub fn z_lends(state: &GameState, owner: &PlayerId, flagship_id: &str) -> bool {
    z_assimilated_factions(state, owner)
        .iter()
        .any(|faction| flagship_of(faction) == flagship_id)
}

/// Place a Z token on `faction`'s sheet. Permanent; refused (`false`) when all seven tokens are
/// placed or that faction's flagship already carries one (the Keleres variants share a flagship).
pub fn place_z(state: &mut GameState, nekro: &PlayerId, faction: &str) -> bool {
    let mut placed = z_assimilated_factions(state, nekro);
    if placed.len() >= Z_TOKEN_COUNT
        || placed
            .iter()
            .any(|held| flagship_of(held) == flagship_of(faction))
    {
        return false;
    }
    placed.push(faction.to_owned());
    state.faction_marks.insert(z_mark(nekro), placed.join(","));
    true
}

fn flagship_of(faction: &str) -> String {
    if faction.starts_with("keleres") {
        "keleres_flagship".to_owned()
    } else {
        format!("{faction}_flagship")
    }
}

// -- Propagation ---------------------------------------------------------------------------------

fn propagation_mark(player: &PlayerId) -> String {
    format!("nekro:propagation:{player}")
}

/// Whether researching is replaced for this player (Propagation).
#[must_use]
pub fn propagation_replaces_research(state: &GameState, player: &PlayerId) -> bool {
    is_nekro(state, player)
}

/// A research the Nekro would have made became 3 command tokens: record it. The game announces
/// it as `PROPAGATION_RESEARCH` at its next step so the token pools are chosen in a window.
pub fn note_propagation(state: &mut GameState, player: &PlayerId) {
    let mark = propagation_mark(player);
    let pending = state
        .faction_marks
        .get(&mark)
        .and_then(|count| count.parse::<u32>().ok())
        .unwrap_or(0);
    state
        .faction_marks
        .insert(mark, pending.saturating_add(1).to_string());
}

/// Seats owing Propagation tokens, once per pending research, in seat order (for the game's
/// announcement step).
#[must_use]
pub fn pending_propagation(state: &GameState) -> Vec<PlayerId> {
    let mut owed = Vec::new();
    for seat in &state.players {
        let count = state
            .faction_marks
            .get(&propagation_mark(&seat.id))
            .and_then(|count| count.parse::<u32>().ok())
            .unwrap_or(0);
        for _ in 0..count {
            owed.push(seat.id.clone());
        }
    }
    owed
}

/// "When you would research a technology: Gain 3 command tokens instead." Mandatory.
fn propagation(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("ability:{owner_name}:propagation:PROPAGATION_RESEARCH:after"),
        seat.clone(),
        "PROPAGATION_RESEARCH",
        Relation::After,
        Arc::new(move |event, _, context| {
            if event.text("player") != Some(owner.as_str()) {
                return Ok(());
            }
            let mark = propagation_mark(&owner);
            let pending = context
                .state
                .faction_marks
                .get(&mark)
                .and_then(|count| count.parse::<u32>().ok())
                .unwrap_or(0);
            if pending == 0 {
                return Ok(());
            }
            if pending == 1 {
                context.state.faction_marks.remove(&mark);
            } else {
                context
                    .state
                    .faction_marks
                    .insert(mark, (pending - 1).to_string());
            }
            crate::strategy_cards::gain_tokens(
                context.state,
                context.content,
                context.sources,
                context.galaxy,
                context.table,
                &owner,
                3,
            )
            .map_err(TimingError::IllegalChoice)
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event.text("player") == Some(condition_owner.as_str())
            && is_nekro(context.state, &condition_owner)
            && context
                .state
                .faction_marks
                .contains_key(&propagation_mark(&condition_owner))
    }))
}

// -- gaining another player's technology ---------------------------------------------------------

/// The options for gaining something of `source`'s through one of the Nekro's faction abilities:
/// each technology of theirs the Nekro lacks (a faction technology of another faction is not the
/// Nekro's to gain, 90.11), and each assimilation the Valefar cards offer instead. Ids are
/// `<kind>|<source>|<technology or faction>` with kind `gain`, `x`, `y` or `z`.
fn assimilation_options(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    nekro: &PlayerId,
    source: &PlayerId,
) -> Vec<ChoiceOption> {
    let (Some(me), Some(them)) = (state.player(nekro), state.player(source)) else {
        return Vec::new();
    };
    if nekro == source {
        return Vec::new();
    }
    let active = crate::technology::active_aliases(content, sources);
    // Last Bastion's Nip and Tuck: "The Nekro Virus cannot place assimilator tokens on your
    // components." Gaining a technology outright is not placing a token, so `gain` stays.
    let tokens_forbidden =
        crate::promissory::has_commander_ability(state, source, "bastioncommander");
    let targeted = |tech: &TechnologyId| {
        TOKENS
            .iter()
            .any(|(_, card)| live_token(state, nekro, card).as_deref() == Some(tech.as_str()))
    };
    let mut options = Vec::new();
    for tech in &them.technologies {
        if !active.contains(tech) {
            continue;
        }
        let faction = crate::technology::faction_of(content, tech);
        let name = crate::technology::name(content, tech);
        if !me.technologies.contains(tech) && faction.is_none_or(|f| f == me.faction.as_str()) {
            options.push(ChoiceOption::labelled(
                format!("gain|{source}|{tech}"),
                ASSIMILATE_KIND,
                format!("gain {name} from {source}"),
            ));
        }
        // A unit upgrade's text is the stat block of a unit the Valefar card is not, so a token
        // on one would lend nothing: it is not offered (evidence: rules question 2).
        if faction.is_some()
            && !tokens_forbidden
            && !crate::technology::is_unit_upgrade(content, tech)
            && !targeted(tech)
            && !TEXT_NOT_LENDABLE.contains(&tech.as_str())
        {
            for (token, card) in TOKENS {
                if owns(state, nekro, card) && live_token(state, nekro, card).is_none() {
                    options.push(ChoiceOption::labelled(
                        format!("{}|{source}|{tech}", token.to_ascii_lowercase()),
                        ASSIMILATE_KIND,
                        format!("place the {token} token on {name} of {source}"),
                    ));
                }
            }
        }
    }
    let placed = z_assimilated_factions(state, nekro);
    if crate::breakthroughs::holds(state, nekro, Z_BREAKTHROUGH)
        && !tokens_forbidden
        && placed.len() < Z_TOKEN_COUNT
        && !placed
            .iter()
            .any(|faction| flagship_of(faction) == flagship_of(them.faction.as_str()))
        && LENDABLE_FLAGSHIPS.contains(&flagship_of(them.faction.as_str()).as_str())
    {
        options.push(ChoiceOption::labelled(
            format!("z|{source}|{}", them.faction),
            ASSIMILATE_KIND,
            format!("place the Z token on the faction sheet of {source}"),
        ));
    }
    options
}

/// Whether `source` is an opponent the Nekro could take something from now.
fn can_take_from(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    nekro: &PlayerId,
    source: &PlayerId,
) -> bool {
    !assimilation_options(state, content, sources, nekro, source).is_empty()
}

/// Ask which gain the Nekro makes and apply it. `sources` are the players the ability allows;
/// the choice is mandatory once the ability is used (its "may" was the window's accept/decline).
fn take_from(
    context: &mut TimingContext<'_>,
    nekro: &PlayerId,
    ability: &str,
    sources: &[PlayerId],
) -> Result<(), TimingError> {
    let mut options = Vec::new();
    for source in sources {
        options.extend(assimilation_options(
            context.state,
            context.content,
            context.sources,
            nekro,
            source,
        ));
    }
    if options.is_empty() {
        return Ok(());
    }
    let choice = Choice::new(
        nekro.clone(),
        "gain a technology, or assimilate instead",
        options,
    )
    .contextualized(DecisionContext::new(
        nekro.clone(),
        DecisionSource::FactionAbility(ability.to_owned()),
        "assimilate",
        context.state.phase,
        context.state.round,
    ));
    let answer = context
        .ask_seeing(&choice)
        .map_err(TimingError::IllegalChoice)?;
    let mut parts = answer.id.splitn(3, '|');
    let (Some(kind), Some(_source), Some(what)) = (parts.next(), parts.next(), parts.next()) else {
        return Ok(());
    };
    match kind {
        "gain" => {
            crate::technology::grant(context.state, nekro, &TechnologyId::new(what));
            crate::technology::apply_unit_upgrades(
                context.state,
                context.content,
                context.sources,
                nekro,
            );
        }
        "x" | "y" => {
            let card = if kind == "x" { "vax" } else { "vay" };
            if let Some(seat) = context.state.player_mut(nekro) {
                seat.assimilated_technologies
                    .insert(card.to_owned(), TechnologyId::new(what));
            }
        }
        "z" => {
            place_z(context.state, nekro, what);
        }
        _ => {}
    }
    Ok(())
}

// -- Technological Singularity ------------------------------------------------------------------

/// The combat a destruction belongs to: a space combat by its system, a ground combat by its
/// system and planet.
fn combat_key(event: &Event, ground: bool) -> Option<String> {
    let system = event.text("system")?;
    if ground {
        Some(format!("nekro:ts:{system}|{}", event.text("planet")?))
    } else {
        Some(format!("nekro:ts:{system}"))
    }
}

/// The opponent the Nekro has in this combat, from the combat's start event.
fn opponent_in(event: &Event, nekro: &PlayerId) -> Option<PlayerId> {
    let (attacker, defender) = (event.text("attacker")?, event.text("defender")?);
    if attacker == nekro.as_str() {
        Some(PlayerId::new(defender))
    } else if defender == nekro.as_str() {
        Some(PlayerId::new(attacker))
    } else {
        None
    }
}

/// Antivirus: a faceup copy in the opponent's play area bars Technological Singularity against them.
fn antivirus_bars(state: &GameState, nekro: &PlayerId, opponent: &PlayerId) -> bool {
    let note =
        crate::promissory::note_id("antivirus", &crate::promissory::faction_name(state, nekro));
    state.promissory_notes.get(&note) == Some(opponent) && state.promissory_faceup.contains(&note)
}

/// The two halves of the once-per-combat bookkeeping: open the combat when it starts, close it
/// when it ends. Written before any destruction of that combat is announced.
fn combat_marks(owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    let mut marks = Vec::new();
    for (name, ground, clear) in [
        ("SPACE_COMBAT_STARTED", false, false),
        ("SPACE_COMBAT_ENDED", false, true),
        ("GROUND_COMBAT_STARTED", true, false),
        ("GROUND_COMBAT_ENDED", true, true),
    ] {
        let (holder, claimant) = (seat.clone(), seat.clone());
        let action = if clear { "close" } else { "open" };
        marks.push(
            Ability::stateful(
                format!("ability:{owner_name}:technological_singularity_{action}:{name}:after"),
                seat.clone(),
                name,
                Relation::After,
                Arc::new(move |event, _, context| {
                    let Some(key) = combat_key(event, ground) else {
                        return Ok(());
                    };
                    if clear {
                        context.state.faction_marks.remove(&key);
                    } else if let Some(opponent) = opponent_in(event, &holder) {
                        context
                            .state
                            .faction_marks
                            .insert(key, format!("{opponent}|open"));
                    }
                    Ok(())
                }),
            )
            .with_stateful_condition(Arc::new(move |event, _, context| {
                is_nekro(context.state, &claimant) && opponent_in(event, &claimant).is_some()
            })),
        );
    }
    marks
}

/// Whether the destruction in `event` opens Technological Singularity for `nekro`: an opponent's
/// unit, in a combat the Nekro is in, that has not used it yet, with something to take and no
/// faceup Antivirus.
fn singularity_ready(
    event: &Event,
    context: &TimingContext<'_>,
    nekro: &PlayerId,
    ground: bool,
) -> Option<(String, PlayerId)> {
    if !is_nekro(context.state, nekro) {
        return None;
    }
    if ground {
        if event.text("cause") != Some("ground_combat") {
            return None;
        }
    } else if !crate::combat::destroyed_during_combat(event) {
        return None;
    }
    let key = combat_key(event, ground)?;
    let mark = context.state.faction_marks.get(&key)?;
    let (opponent, status) = mark.split_once('|')?;
    let victim = event.text("player")?;
    if status != "open" || victim != opponent {
        return None;
    }
    let opponent = PlayerId::new(opponent);
    (!antivirus_bars(context.state, nekro, &opponent)
        && can_take_from(
            context.state,
            context.content,
            context.sources,
            nekro,
            &opponent,
        ))
    .then_some((key, opponent))
}

/// Technological Singularity: "Once per combat, after 1 of your opponent's units is destroyed:
/// You may gain 1 technology that is owned by that player."
fn singularity(owner_name: &str, seat: &PlayerId, event_name: &str, ground: bool) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("ability:{owner_name}:technological_singularity:{event_name}:after"),
        seat.clone(),
        event_name,
        Relation::After,
        Arc::new(move |event, _, context| {
            let Some((key, opponent)) = singularity_ready(event, context, &owner, ground) else {
                return Ok(());
            };
            context
                .state
                .faction_marks
                .insert(key, format!("{opponent}|used"));
            take_from(context, &owner, "technological_singularity", &[opponent])
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        singularity_ready(event, context, &condition_owner, ground).is_some()
    }))
}

// -- Galactic Threat ----------------------------------------------------------------------------

fn threat_mark(player: &PlayerId) -> String {
    format!("nekro:threat:{player}")
}

fn threat_phase_mark(player: &PlayerId) -> String {
    format!("nekro:threat_phase:{player}")
}

/// Galactic Threat, prediction half: "Once per agenda phase, after an agenda is revealed: You may
/// predict aloud the outcome of that agenda."
fn threat_predict(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("ability:{owner_name}:galactic_threat:AGENDA_REVEALED:after"),
        seat.clone(),
        "AGENDA_REVEALED",
        Relation::After,
        Arc::new(move |_, _, context| {
            if !threat_ready(context, &owner) {
                return Ok(());
            }
            let Some(predicted) = crate::action_cards::predicted_outcome(
                context,
                &owner,
                "Galactic Threat: predict the agenda outcome",
            ) else {
                return Ok(());
            };
            let round = context.state.round.to_string();
            let seq = context.state.agenda_seq;
            context
                .state
                .faction_marks
                .insert(threat_phase_mark(&owner), round);
            context
                .state
                .faction_marks
                .insert(threat_mark(&owner), format!("{seq}|{predicted}"));
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |_, _, context| {
        threat_ready(context, &condition_owner)
    }))
}

fn threat_ready(context: &TimingContext<'_>, nekro: &PlayerId) -> bool {
    is_nekro(context.state, nekro)
        && !context.state.agenda_choices.is_empty()
        && context
            .state
            .faction_marks
            .get(&threat_phase_mark(nekro))
            .is_none_or(|round| *round != context.state.round.to_string())
}

/// The players who voted for `outcome` on the agenda being resolved and have something to take.
fn threat_sources(context: &TimingContext<'_>, nekro: &PlayerId, outcome: &str) -> Vec<PlayerId> {
    context
        .state
        .agenda_votes
        .iter()
        .filter(|(voter, voted)| *voter != nekro && voted.as_str() == outcome)
        .map(|(voter, _)| voter.clone())
        .filter(|voter| {
            can_take_from(
                context.state,
                context.content,
                context.sources,
                nekro,
                voter,
            )
        })
        .collect()
}

/// The sources when this resolution fulfils a standing prediction.
fn threat_payout(
    event: &Event,
    context: &TimingContext<'_>,
    nekro: &PlayerId,
) -> Option<Vec<PlayerId>> {
    if !is_nekro(context.state, nekro)
        || context
            .state
            .transient_flags
            .has(ti4_model::state::TransientFlags::AGENDA_DISCARDED)
    {
        return None;
    }
    let held = context.state.faction_marks.get(&threat_mark(nekro))?;
    let (seq, predicted) = held.split_once('|')?;
    if seq != context.state.agenda_seq.to_string() || event.text("player") != Some(predicted) {
        return None;
    }
    let sources = threat_sources(context, nekro, predicted);
    (!sources.is_empty()).then_some(sources)
}

/// Galactic Threat, payout half: "If your prediction is correct, gain 1 technology that is owned
/// by a player who voted how you predicted."
fn threat_resolved(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("ability:{owner_name}:galactic_threat:AGENDA_RESOLVED:after"),
        seat.clone(),
        "AGENDA_RESOLVED",
        Relation::After,
        Arc::new(move |event, _, context| {
            let Some(sources) = threat_payout(event, context, &owner) else {
                return Ok(());
            };
            context.state.faction_marks.remove(&threat_mark(&owner));
            take_from(context, &owner, "galactic_threat", &sources)
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        threat_payout(event, context, &condition_owner).is_some()
    }))
}

// -- Antivirus ----------------------------------------------------------------------------------

/// The Antivirus notes `holder` keeps in hand: the Nekro's own copy is not theirs to play.
fn antivirus_in_hand(state: &GameState, holder: &PlayerId) -> Option<String> {
    state
        .promissory_notes
        .iter()
        .find(|(note, held_by)| {
            *held_by == holder
                && crate::promissory::alias_of(note) == "antivirus"
                && !state.promissory_faceup.contains(*note)
                && crate::promissory::owner_of(note)
                    .is_some_and(|name| name != crate::promissory::faction_name(state, holder))
        })
        .map(|(note, _)| note.clone())
}

/// Antivirus, placement: "At the start of a combat: Place this card faceup in your play area."
fn antivirus_placed(owner_name: &str, seat: &PlayerId, event_name: &str) -> Ability {
    let (holder, condition_holder) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("promissory:{owner_name}:antivirus:{event_name}:after"),
        seat.clone(),
        event_name,
        Relation::After,
        Arc::new(move |_, _, context| {
            if let Some(note) = antivirus_in_hand(context.state, &holder) {
                context.state.promissory_faceup.insert(note);
            }
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |_, _, context| {
        antivirus_in_hand(context.state, &condition_holder).is_some()
    }))
}

// -- registration -------------------------------------------------------------------------------

fn timing_abilities(state: &GameState, owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    let mut abilities = vec![
        propagation(owner_name, seat),
        threat_predict(owner_name, seat),
        threat_resolved(owner_name, seat),
        singularity(owner_name, seat, "SHIP_DESTROYED", false),
        singularity(owner_name, seat, "GROUND_FORCE_DESTROYED", true),
        antivirus_placed(owner_name, seat, "SPACE_COMBAT_STARTED"),
        antivirus_placed(owner_name, seat, "GROUND_COMBAT_STARTED"),
    ];
    abilities.extend(combat_marks(owner_name, seat));
    abilities.extend(super::nekro_units::timing_abilities(
        state, owner_name, seat,
    ));
    abilities
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use ti4_model::content_types::DEFAULT;
    use ti4_model::id::SystemId;

    use crate::choice::{Scripted, Table};

    fn a() -> PlayerId {
        PlayerId::new("a")
    }
    fn b() -> PlayerId {
        PlayerId::new("b")
    }

    /// The Nekro (`a`) against a faction that owns a faction technology with an effect.
    fn game() -> GameState {
        let mut state = crate::fixtures::seated_game(&[("a", FACTION), ("b", "yssaril")], DEFAULT);
        let b = state.player_mut(&b()).unwrap();
        b.technologies.insert(TechnologyId::new("mi"));
        b.technologies.insert(TechnologyId::new("tp"));
        b.technologies.insert(TechnologyId::new("sar"));
        state
    }

    fn payload(pairs: &[(&str, &str)]) -> BTreeMap<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), serde_json::Value::from(*value)))
            .collect()
    }

    /// A table that fails the test if anything is asked.
    fn silent() -> Table {
        Table::with_default(Box::new(Scripted::new(["never offered"])))
    }

    fn scripted(answers: &[&str]) -> Table {
        Table::with_default(Box::new(Scripted::new(
            answers.iter().map(|answer| (*answer).to_owned()),
        )))
    }

    fn emit(state: &mut GameState, table: &mut Table, event_type: &str, pairs: &[(&str, &str)]) {
        let mut resolver = crate::fixtures::armed_resolver(state);
        crate::fixtures::with_context(state, DEFAULT, None, table, |context| {
            let event = context
                .event_sequence
                .next(event_type, payload(pairs))
                .expect("an event id");
            resolver
                .emit_with_context(context, event, |_, _| {})
                .expect("the window resolves");
        });
    }

    fn owned(state: &GameState, who: &PlayerId, tech: &str) -> bool {
        state
            .player(who)
            .unwrap()
            .technologies
            .contains(&TechnologyId::new(tech))
    }

    fn tokens(state: &GameState, who: &PlayerId) -> u32 {
        let seat = state.player(who).unwrap();
        u32::try_from(seat.tactic_tokens + seat.fleet_tokens + seat.strategic_tokens).unwrap()
    }

    fn give(state: &mut GameState, who: &PlayerId, tech: &str) {
        state
            .player_mut(who)
            .unwrap()
            .technologies
            .insert(TechnologyId::new(tech));
    }

    fn home(state: &GameState, who: &PlayerId) -> SystemId {
        state.player(who).unwrap().home_system.clone().unwrap()
    }

    // -- neutrality ------------------------------------------------------------------------------

    #[test]
    fn a_game_without_a_nekro_is_untouched() {
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "yssaril")], DEFAULT);
        state.agenda_choices = vec!["for".to_owned(), "against".to_owned()];
        let system = home(&state, &a());
        let before = state.clone();
        for (event, pairs) in [
            ("AGENDA_REVEALED", vec![("agenda", "x")]),
            ("AGENDA_RESOLVED", vec![("agenda", "x"), ("player", "for")]),
            ("PROPAGATION_RESEARCH", vec![("player", "a")]),
            (
                "SPACE_COMBAT_STARTED",
                vec![
                    ("system", system.as_str()),
                    ("attacker", "a"),
                    ("defender", "b"),
                ],
            ),
            (
                "SHIP_DESTROYED",
                vec![
                    ("system", system.as_str()),
                    ("player", "b"),
                    ("unit", "cruiser"),
                ],
            ),
            (
                "GROUND_FORCE_DESTROYED",
                vec![
                    ("system", system.as_str()),
                    ("player", "b"),
                    ("planet", "p"),
                ],
            ),
            (
                "SPACE_COMBAT_ENDED",
                vec![
                    ("system", system.as_str()),
                    ("attacker", "a"),
                    ("defender", "b"),
                ],
            ),
        ] {
            emit(&mut state, &mut silent(), event, &pairs);
        }
        assert_eq!(state, before);
        // A seat that is not the Nekro researches as ever.
        assert!(crate::technology::research(
            &mut state,
            ti4_content::ContentStore::embedded(),
            DEFAULT,
            &a(),
            &TechnologyId::new("aida"),
        ));
        assert!(owned(&state, &a(), "aida"));
        assert!(
            state
                .faction_marks
                .keys()
                .all(|key| !key.starts_with("nekro:"))
        );
        assert!(!crate::technology::has_technology_text(&state, &a(), "mi"));
    }

    // -- Propagation -----------------------------------------------------------------------------

    #[test]
    fn the_technology_card_gives_the_nekro_tokens_instead_of_a_technology() {
        let content = ti4_content::ContentStore::embedded();
        let mut state = game();
        state.player_mut(&a()).unwrap().trade_goods = 0;
        let owned_before = state.player(&a()).unwrap().technologies.clone();
        let first = crate::technology::researchable(&state, content, DEFAULT, &a())
            .into_iter()
            .next()
            .expect("something to research");
        // The primary's mandatory research, then the optional 6-resource one is declined.
        let mut table = scripted(&[first.as_str(), "decline"]);
        crate::strategy_cards::primary(
            &mut state,
            content,
            DEFAULT,
            None,
            &mut table,
            &a(),
            "pok7technology",
        )
        .unwrap();
        assert_eq!(state.player(&a()).unwrap().technologies, owned_before);
        assert_eq!(pending_propagation(&state), vec![a()]);

        // The game opens the window at its next step; the owner places the three tokens.
        let before = tokens(&state, &a());
        emit(
            &mut state,
            &mut scripted(&["fleet_tokens", "fleet_tokens", "strategic_tokens"]),
            "PROPAGATION_RESEARCH",
            &[("player", "a"), ("pending", "1")],
        );
        assert_eq!(tokens(&state, &a()), before + 3);
        assert!(pending_propagation(&state).is_empty());
    }

    #[test]
    fn the_game_announces_a_replaced_research_at_its_next_step() {
        let content = ti4_content::ContentStore::embedded();
        let mut state = game();
        let first = crate::technology::researchable(&state, content, DEFAULT, &a())
            .into_iter()
            .next()
            .unwrap();
        assert!(crate::technology::research(
            &mut state,
            content,
            DEFAULT,
            &a(),
            &first
        ));
        assert!(!owned(&state, &a(), first.as_str()));
        let before = tokens(&state, &a());
        let mut game = crate::game::Game::new(state, content);
        let _ = game.step();
        assert_eq!(tokens(&game.state, &a()), before + 3);
        assert!(pending_propagation(&game.state).is_empty());
    }

    // -- Technological Singularity ---------------------------------------------------------------

    fn start_combat(state: &mut GameState, system: &SystemId) {
        emit(
            state,
            &mut silent(),
            "SPACE_COMBAT_STARTED",
            &[
                ("system", system.as_str()),
                ("attacker", "a"),
                ("defender", "b"),
            ],
        );
    }

    fn destroyed(state: &mut GameState, table: &mut Table, system: &SystemId, who: &str) {
        let mut resolver = crate::fixtures::armed_resolver(state);
        crate::fixtures::with_context(state, DEFAULT, None, table, |context| {
            let mut data = payload(&[
                ("system", system.as_str()),
                ("player", who),
                ("unit", "cruiser"),
                ("cause", "combat"),
            ]);
            data.insert("during_space_combat".to_owned(), true.into());
            let event = context.event_sequence.next("SHIP_DESTROYED", data).unwrap();
            resolver
                .emit_with_context(context, event, |_, _| {})
                .unwrap();
        });
    }

    const SINGULARITY: &str = "ability:nekro:technological_singularity:SHIP_DESTROYED:after";

    #[test]
    fn a_destroyed_opposing_unit_lets_the_nekro_gain_a_technology_once_per_combat() {
        let mut state = game();
        let system = home(&state, &a());
        start_combat(&mut state, &system);
        // Their own unit lost, or somebody else's: nothing.
        destroyed(&mut state, &mut silent(), &system, "a");
        destroyed(&mut state, &mut scripted(&["decline"]), &system, "b");
        assert!(!owned(&state, &a(), "sar"));

        destroyed(
            &mut state,
            &mut scripted(&[SINGULARITY, "gain|b|sar"]),
            &system,
            "b",
        );
        assert!(owned(&state, &a(), "sar"));
        // Once per combat: the next loss offers nothing.
        destroyed(&mut state, &mut silent(), &system, "b");
        // A faction technology of another faction is not the Nekro's to gain.
        assert!(!owned(&state, &a(), "mi"));
        // The next combat opens it afresh.
        emit(
            &mut state,
            &mut silent(),
            "SPACE_COMBAT_ENDED",
            &[
                ("system", system.as_str()),
                ("attacker", "a"),
                ("defender", "b"),
            ],
        );
        start_combat(&mut state, &system);
        destroyed(
            &mut state,
            &mut scripted(&[SINGULARITY, "gain|b|nm"]),
            &system,
            "b",
        );
        assert!(owned(&state, &a(), "nm"));
    }

    #[test]
    fn a_faceup_antivirus_bars_the_singularity_and_returns_on_activation() {
        let content = ti4_content::ContentStore::embedded();
        let mut state = game();
        let system = home(&state, &a());
        crate::promissory::take(&mut state, content, &b(), "antivirus:nekro");
        assert!(
            !state.promissory_faceup.contains("antivirus:nekro"),
            "waits in hand"
        );
        // "At the start of a combat: Place this card faceup in your play area."
        emit(
            &mut state,
            &mut scripted(&["promissory:yssaril:antivirus:SPACE_COMBAT_STARTED:after"]),
            "SPACE_COMBAT_STARTED",
            &[
                ("system", system.as_str()),
                ("attacker", "a"),
                ("defender", "b"),
            ],
        );
        assert!(state.promissory_faceup.contains("antivirus:nekro"));
        destroyed(&mut state, &mut silent(), &system, "b");
        assert!(!owned(&state, &a(), "sar"));
        // "If you activate a system that contains 1 or more of the Nekro player's units, return
        // this card to the Nekro player."
        let elsewhere = home(&state, &b());
        crate::promissory::spend_support_on_activation(&mut state, &b(), &elsewhere);
        assert!(state.promissory_faceup.contains("antivirus:nekro"));
        crate::promissory::spend_support_on_activation(&mut state, &b(), &system);
        assert!(!state.promissory_faceup.contains("antivirus:nekro"));
        assert_eq!(state.promissory_notes.get("antivirus:nekro"), Some(&a()));
        // With the note home, the combat's destruction yields the technology again.
        destroyed(
            &mut state,
            &mut scripted(&[SINGULARITY, "gain|b|sar"]),
            &system,
            "b",
        );
        assert!(owned(&state, &a(), "sar"));
    }

    // -- Galactic Threat -------------------------------------------------------------------------

    const THREAT: &str = "ability:nekro:galactic_threat:AGENDA_REVEALED:after";

    fn reveal(state: &mut GameState, table: &mut Table) {
        state.agenda_seq += 1;
        state.agenda_choices = vec!["for".to_owned(), "against".to_owned()];
        emit(state, table, "AGENDA_REVEALED", &[("agenda", "x")]);
    }

    #[test]
    fn a_correct_prediction_gains_a_technology_of_a_voter_who_voted_that_way() {
        let mut state = game();
        reveal(&mut state, &mut scripted(&[THREAT, "for"]));
        assert_eq!(
            state
                .faction_marks
                .get(&threat_mark(&a()))
                .map(String::as_str),
            Some("1|for")
        );
        // Once per agenda phase: a second reveal in the same round offers nothing.
        reveal(&mut state, &mut silent());
        state.agenda_seq = 1;

        // Wrong prediction: nothing, and the voters' technologies stay theirs.
        state.agenda_votes.insert(b(), "against".to_owned());
        emit(
            &mut state,
            &mut silent(),
            "AGENDA_RESOLVED",
            &[("agenda", "x"), ("player", "against")],
        );
        // Right outcome, but nobody who voted for it: nothing.
        emit(
            &mut state,
            &mut silent(),
            "AGENDA_RESOLVED",
            &[("agenda", "x"), ("player", "for")],
        );
        assert!(!owned(&state, &a(), "sar"));

        state.agenda_votes.insert(b(), "for".to_owned());
        emit(
            &mut state,
            &mut scripted(&["gain|b|sar"]),
            "AGENDA_RESOLVED",
            &[("agenda", "x"), ("player", "for")],
        );
        assert!(owned(&state, &a(), "sar"));
    }

    #[test]
    fn the_nekro_cannot_vote() {
        let content = ti4_content::ContentStore::embedded();
        let mut state = game();
        state.speaker = a();
        let window =
            crate::vote::VoteWindow::new(&state, "x", vec!["for".into(), "against".into()]);
        let choice = window
            .pending_choice(&state, content, DEFAULT)
            .expect("a vote");
        assert_eq!(choice.player, b());
    }

    // -- Valefar Assimilators --------------------------------------------------------------------

    #[test]
    fn a_token_on_a_faction_technology_lends_its_text_without_owning_it() {
        let content = ti4_content::ContentStore::embedded();
        let mut state = game();
        give(&mut state, &a(), "vax");
        give(&mut state, &a(), "vay");
        state.player_mut(&b()).unwrap().action_cards =
            vec![ti4_model::id::ActionCardId::new("sabo1")];
        let mageon = |state: &GameState| {
            crate::factions::component_actions(state, content, &a())
                .iter()
                .any(|option| option.id.starts_with("faction|yssaril|mi|"))
        };
        assert!(!mageon(&state));
        assert!(!crate::technology::has_technology_text(&state, &a(), "mi"));

        let colours_before = crate::technology::owned_colours(&state, content, &a());
        // The Singularity: the faction technology cannot be gained, but can carry a token.
        let system = home(&state, &a());
        start_combat(&mut state, &system);
        destroyed(
            &mut state,
            &mut scripted(&[SINGULARITY, "x|b|mi"]),
            &system,
            "b",
        );
        assert!(!owned(&state, &a(), "mi"), "assimilated, never owned");
        assert!(crate::technology::has_technology_text(&state, &a(), "mi"));
        assert_eq!(assimilated_card(&state, &a(), "mi"), Some("vax"));
        assert_eq!(assimilators_with_tokens(&state, &a()), 1);
        assert!(
            mageon(&state),
            "the text works through the faction's own gate"
        );
        // It counts for no prerequisite: the owned colours are what they were.
        assert_eq!(
            crate::technology::owned_colours(&state, content, &a()),
            colours_before
        );
        // It exhausts the Valefar card, not the owner's technology.
        assert!(crate::technology::exhaust_technology_text(
            &mut state,
            &a(),
            "mi"
        ));
        assert!(
            state
                .player(&a())
                .unwrap()
                .exhausted_technologies
                .contains(&TechnologyId::new("vax"))
        );
        assert!(
            !state
                .player(&b())
                .unwrap()
                .exhausted_technologies
                .contains(&TechnologyId::new("mi"))
        );
        assert!(!mageon(&state));

        // The other token may not go on the same technology, and a lost technology ends the text.
        assert!(
            !assimilation_options(&state, content, DEFAULT, &a(), &b())
                .iter()
                .any(|option| option.id == "y|b|mi")
        );
        state
            .player_mut(&b())
            .unwrap()
            .technologies
            .remove(&TechnologyId::new("mi"));
        assert!(!crate::technology::has_technology_text(&state, &a(), "mi"));
    }

    #[test]
    fn an_assimilated_nes_and_the_lifted_exclusions_work_for_the_nekro() {
        let content = ti4_content::ContentStore::embedded();
        let mut state = crate::fixtures::seated_game(&[("a", FACTION), ("b", "letnev")], DEFAULT);
        give(&mut state, &b(), "nes");
        give(&mut state, &b(), "l4");
        give(&mut state, &b(), "executiveorder");
        assert!(!crate::combat::non_euclidean_shielding(&state, &a()));
        let ids: Vec<String> = assimilation_options(&state, content, DEFAULT, &a(), &b())
            .into_iter()
            .map(|option| option.id)
            .collect();
        assert!(ids.contains(&"x|b|nes".to_owned()), "{ids:?}");
        assert!(ids.contains(&"y|b|l4".to_owned()), "{ids:?}");
        assert!(
            !ids.iter().any(|id| id.ends_with("|executiveorder")),
            "the Nekro cannot vote: not lendable"
        );
        state
            .player_mut(&a())
            .unwrap()
            .assimilated_technologies
            .insert("vax".to_owned(), TechnologyId::new("nes"));
        assert!(crate::combat::non_euclidean_shielding(&state, &a()));
        // The owner losing the technology ends the text.
        state
            .player_mut(&b())
            .unwrap()
            .technologies
            .remove(&TechnologyId::new("nes"));
        assert!(!crate::combat::non_euclidean_shielding(&state, &a()));
    }

    #[test]
    fn a_nekro_is_offered_a_thunders_edge_faction_tech_through_the_singularity() {
        let content = ti4_content::ContentStore::embedded();
        let mut state = crate::fixtures::seated_game(&[("a", FACTION), ("b", "bastion")], DEFAULT);
        give(&mut state, &a(), "vax");
        give(&mut state, &b(), "proxima");
        let options = |sources| -> Vec<String> {
            assimilation_options(&state, content, sources, &a(), &b())
                .into_iter()
                .map(|option| option.id)
                .collect()
        };
        let full = options(DEFAULT);
        assert!(full.contains(&"x|b|proxima".to_owned()), "{full:?}");
        assert!(
            !options(ti4_model::content_types::POK)
                .iter()
                .any(|id| id.ends_with("|proxima"))
        );
        // The real route: a destroyed Bastion ship opens the Singularity and the token goes on Proxima.
        let system = home(&state, &a());
        start_combat(&mut state, &system);
        destroyed(
            &mut state,
            &mut scripted(&[SINGULARITY, "x|b|proxima"]),
            &system,
            "b",
        );
        assert!(crate::technology::has_technology_text(
            &state,
            &a(),
            "proxima"
        ));
        assert!(!owned(&state, &a(), "proxima"), "assimilated, not owned");
    }

    #[test]
    fn nip_and_tuck_bars_every_assimilator_token_on_its_owners_components() {
        let content = ti4_content::ContentStore::embedded();
        let mut state = crate::fixtures::seated_game(&[("a", FACTION), ("b", "bastion")], DEFAULT);
        with_z_breakthrough(&mut state);
        give(&mut state, &a(), "vax");
        give(&mut state, &a(), "vay");
        give(&mut state, &b(), "mi");
        give(&mut state, &b(), "proxima");
        give(&mut state, &b(), "aida");
        state
            .player_mut(&a())
            .unwrap()
            .technologies
            .remove(&TechnologyId::new("aida"));
        let ids = |state: &GameState| -> Vec<String> {
            assimilation_options(state, content, DEFAULT, &a(), &b())
                .into_iter()
                .map(|option| option.id)
                .collect()
        };
        let open = ids(&state);
        for wanted in ["gain|b|aida", "x|b|mi", "y|b|mi", "z|b|bastion"] {
            assert!(open.contains(&wanted.to_owned()), "{wanted} in {open:?}");
        }
        // Nip and Tuck, unlocked: gaining a technology stays, every token option goes.
        state.player_mut(&b()).unwrap().leaders.insert(
            ti4_model::id::LeaderId::new("bastioncommander"),
            ti4_model::state::LeaderStatus::Unlocked,
        );
        let shut = ids(&state);
        assert!(shut.contains(&"gain|b|aida".to_owned()), "{shut:?}");
        assert!(
            !shut
                .iter()
                .any(|id| id.starts_with("x|") || id.starts_with("y|") || id.starts_with("z|")),
            "{shut:?}"
        );
    }

    #[test]
    fn a_unit_upgrade_is_gained_but_never_assimilated() {
        let content = ti4_content::ContentStore::embedded();
        let mut state = crate::fixtures::seated_game(&[("a", FACTION), ("b", "sol")], DEFAULT);
        give(&mut state, &a(), "vax");
        give(&mut state, &b(), "ac2");
        let ids: Vec<String> = assimilation_options(&state, content, DEFAULT, &a(), &b())
            .into_iter()
            .map(|option| option.id)
            .collect();
        assert!(
            !ids.iter().any(|id| id.starts_with("gain|b|ac2")),
            "faction technology of another faction"
        );
        assert!(!ids.iter().any(|id| id.ends_with("|ac2")), "{ids:?}");
    }

    fn with_z_breakthrough(state: &mut GameState) {
        state.player_mut(&a()).unwrap().breakthrough =
            Some(ti4_model::id::BreakthroughId::new(Z_BREAKTHROUGH));
    }

    #[test]
    fn the_z_token_is_offered_and_placed_through_the_ability_route() {
        let content = ti4_content::ContentStore::embedded();
        let mut state = game();
        with_z_breakthrough(&mut state);
        assert!(
            assimilation_options(&state, content, DEFAULT, &a(), &b())
                .iter()
                .any(|option| option.id == "z|b|yssaril")
        );
        // Without the breakthrough no Z option is offered.
        let bare = game();
        assert!(
            !assimilation_options(&bare, content, DEFAULT, &a(), &b())
                .iter()
                .any(|option| option.id.starts_with("z|"))
        );
        let system = home(&state, &a());
        start_combat(&mut state, &system);
        destroyed(
            &mut state,
            &mut scripted(&[SINGULARITY, "z|b|yssaril"]),
            &system,
            "b",
        );
        assert_eq!(z_assimilated_factions(&state, &a()), vec!["yssaril"]);
        assert!(
            !assimilation_options(&state, content, DEFAULT, &a(), &b())
                .iter()
                .any(|option| option.id.starts_with("z|")),
            "no second Z on one faction"
        );
        assert!(
            assimilation_options(&state, content, DEFAULT, &a(), &b())
                .iter()
                .any(|option| option.id.starts_with("gain|") || option.id.starts_with("x|")),
            "the other choices remain"
        );
    }

    #[test]
    fn the_flagship_text_toggle_is_off_by_default_and_follows_the_tokens() {
        let mut state = game();
        with_z_breakthrough(&mut state);
        let text = |state: &GameState, owner: &PlayerId, unit: &str, id: &str| {
            crate::factions::flagship_has_text(state, owner, unit, id)
        };
        // Off by default; a flagship always has its own text.
        assert!(!text(&state, &a(), NEKRO_FLAGSHIP, "yssaril_flagship"));
        assert!(text(&state, &b(), "yssaril_flagship", "yssaril_flagship"));
        assert!(place_z(&mut state, &a(), "yssaril"));
        assert!(text(&state, &a(), NEKRO_FLAGSHIP, "yssaril_flagship"));
        // Only that faction's text, only for the Nekro flagship, only for the Nekro.
        assert!(!text(&state, &a(), NEKRO_FLAGSHIP, "sol_flagship"));
        assert!(!text(&state, &a(), "cruiser", "yssaril_flagship"));
        assert!(!text(&state, &b(), NEKRO_FLAGSHIP, "yssaril_flagship"));
        // Without the breakthrough the token switches nothing on.
        state.player_mut(&a()).unwrap().breakthrough = None;
        assert!(!text(&state, &a(), NEKRO_FLAGSHIP, "yssaril_flagship"));
    }

    #[test]
    fn at_most_seven_z_tokens_and_none_twice_on_a_faction() {
        let mut state = game();
        with_z_breakthrough(&mut state);
        let seven = [
            "arborec", "argent", "cabal", "empyrean", "ghost", "hacan", "jolnar",
        ];
        for faction in seven {
            assert!(place_z(&mut state, &a(), faction), "{faction}");
        }
        assert_eq!(z_assimilated_factions(&state, &a()).len(), Z_TOKEN_COUNT);
        assert!(!place_z(&mut state, &a(), "sol"), "the eighth token");
        assert!(!place_z(&mut state, &a(), "hacan"), "a faction twice");
        let content = ti4_content::ContentStore::embedded();
        assert!(
            !assimilation_options(&state, content, DEFAULT, &a(), &b())
                .iter()
                .any(|option| option.id.starts_with("z|")),
            "no token left to offer"
        );
        assert_eq!(z_assimilated_factions(&state, &a()).len(), Z_TOKEN_COUNT);
        // The Keleres variants share one flagship, so one token covers all three.
        let mut keleres = game();
        with_z_breakthrough(&mut keleres);
        assert!(place_z(&mut keleres, &a(), "keleresm"));
        assert!(!place_z(&mut keleres, &a(), "keleresx"));
        assert_eq!(flagship_of("keleresx"), "keleres_flagship");
    }
}
