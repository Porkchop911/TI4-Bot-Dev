//! The Empyrean (`empyrean`). See `factions/mod.rs` for the contract and
//! `plans/BASE_FACTIONS_PLAN_2026-10-02.md` for scope; the per-item record is
//! `plans/evidence/BF-empyrean.md`.
//!
//! Split for parallel work: this file holds the faction abilities (Aetherpassage, Dark Whispers,
//! Voidborn), the technologies (Aetherstream, Voidwatch), the hero, the commander unlock and the
//! Void Tether breakthrough; `empyrean_units.rs` holds the flagship, mech, agent and the two
//! promissory notes, plus the combat/card hooks those need.
//!
//! Card texts (latest printing, `crates/ti4-content/content/*.json`):
//!
//! * Aetherpassage: "After a player activates a system: You may allow that player to move their
//!   ships through systems that contain your ships."
//! * Dark Whispers: "During setup, take the additional Empyrean faction promissory note; you have
//!   2 faction promissory notes."
//! * Voidborn: "Nebulae do not affect your ships' movement."
//! * Aetherstream (`as`): "After you or one of your neighbors activates a system that is adjacent
//!   to an anomaly, you may apply +1 to the move value of all of that player's ships during this
//!   tactical action."
//! * Voidwatch (`vw`): "After a player moves ships into a system that contains 1 or more of your
//!   units, they must give you 1 promissory note from their hand, if able."
//! * Conservator Procyon (`empyreanhero`): "ACTION: Place 1 frontier token in each system that does
//!   not contain any planets and does not already have a frontier token. Then, explore each
//!   frontier token that is in a system that contains 1 or more of your ships. Then, purge this
//!   card." Unlock: "Have 3 scored objectives."
//! * Xuange (`empyreancommander`): unlock "Be neighbors with all other players." (The effect is in
//!   `borrowed_commanders_b.rs`.)
//! * Void Tether (`empyreanbt`): "When you activate a system that contains or is adjacent to a unit
//!   or planet you control, you may place or move 1 of your Void Tether tokens onto a border that
//!   system shares with another system; other players do not treat those systems as adjacent to
//!   each other unless you allow it."
//!
//! **Scoping decisions** (also in the evidence file):
//!
//! * The two "allow" clauses (Aetherpassage and the Void Tether exception) are asked of the
//!   Empyrean in the window after the other player activates a system, and last for that
//!   activation (`GameState::activation_seq`), which is when movement happens.
//! * Void Tether tokens are a public list of borders in `faction_marks`
//!   (`empyrean|tether|<player>`), at most [`TETHER_TOKENS`] on the board; with all of them placed,
//!   the activation moves one.
//! * Voidwatch fires once per movement step (`MOVEMENT_FINISHED`), not once per ship.

use std::collections::BTreeSet;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{LeaderId, PlayerId, SystemId};
use ti4_model::state::GameState;

use super::hooks_movement::{MoveSite, MovementHooks};
use super::{FactionModule, Hooks};
use crate::choice::{Choice, ChoiceOption};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::phrases::{Phrase, Subject, Worded};
use crate::timing::{Ability, Relation, TimingContext};

/// The faction alias; also the faction name in promissory note ids.
pub const FACTION: &str = "empyrean";

const HERO: &str = "empyreanhero";
const COMMANDER: &str = "empyreancommander";
const BREAKTHROUGH: &str = "empyreanbt";

/// How many Void Tether tokens exist. The printed text says "1 of your Void Tether tokens" without
/// a count; this is the assumption recorded in the evidence file.
pub const TETHER_TOKENS: usize = 2;

/// `"<activation_seq>|<mover>|<empyrean>"`: the player allowed to pass the Empyrean's ships.
const PASSAGE: &str = "empyrean|passage";
/// `"<activation_seq>|<player>"`: the player whose ships have Aetherstream's +1.
const STREAM: &str = "empyrean|aetherstream";
/// `"<activation_seq>|<player>|<empyrean>"`: the player allowed to ignore the Empyrean's tethers.
const ALLOW: &str = "empyrean|tether_allow";

/// What this faction implements; grows package by package.
pub const MODULE: FactionModule = FactionModule {
    alias: FACTION,
    abilities: &["aetherpassage", "dark_whispers", "voidborn"],
    technologies: &["as", "vw"],
    units: super::empyrean_units::UNITS,
    promissory: super::empyrean_units::PROMISSORY,
    leaders: super::empyrean_units::LEADERS,
    breakthroughs: &[BREAKTHROUGH],
    hooks: Hooks {
        timing_abilities: Some(timing_abilities),
        commander_unlocked: Some(commander_unlocked),
        leader_action: Some(leader_action),
        use_leader: Some(use_leader),
        // Blood Pact / Dark Pact: their ACTION places them faceup (empyrean_units.rs).
        component_actions: Some(super::empyrean_units::component_actions),
        perform_component: Some(super::empyrean_units::perform_component),
        movement: MovementHooks {
            move_bonus: Some(move_bonus),
            ignores_nebulae: Some(ignores_nebulae),
            passable_owners: Some(passable_owners),
            blocked_borders: Some(blocked_borders),
            ..MovementHooks::NONE
        },
        combat: super::empyrean_units::COMBAT_HOOKS,
        cards: super::empyrean_units::CARD_HOOKS,
        ..Hooks::NONE
    },
};

fn decision(state: &GameState, player: &PlayerId, card: &str, subtype: &str) -> DecisionContext {
    DecisionContext::new(
        player.clone(),
        DecisionSource::FactionAbility(card.to_owned()),
        subtype,
        state.phase,
        state.round,
    )
}

fn is_empyrean(state: &GameState, player: &PlayerId) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.faction.as_str() == FACTION)
}

/// Owns the technology, or the Nekro's Valefar Assimilator carries its text.
fn has_technology(state: &GameState, player: &PlayerId, alias: &str) -> bool {
    crate::technology::has_technology_text(state, player, alias)
}

fn has_breakthrough(state: &GameState, player: &PlayerId) -> bool {
    is_empyrean(state, player) && crate::breakthroughs::holds(state, player, BREAKTHROUGH)
}

/// Whether `player` has a ship anywhere other than `except`.
fn has_ship_outside(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    except: &str,
) -> bool {
    let types = ti4_content::units::catalogue(content, sources);
    state.board.iter().any(|(system, board)| {
        system.as_str() != except
            && board.units.iter().any(|unit| {
                &unit.owner == player
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(ti4_content::units::UnitType::is_ship)
            })
    })
}

fn normal(a: &str, b: &str) -> (String, String) {
    if a < b {
        (a.to_owned(), b.to_owned())
    } else {
        (b.to_owned(), a.to_owned())
    }
}

// -- Dark Whispers -------------------------------------------------------------------------------

/// Dark Whispers: "During setup, take the additional Empyrean faction promissory note; you have 2
/// faction promissory notes." Called when the seat is deployed as the Empyrean. A note already in
/// the map (an earlier deal, or one lent out) is left where it is.
pub(crate) fn deal_notes(state: &mut GameState, content: &ContentStore, player: &PlayerId) {
    if !is_empyrean(state, player) {
        return;
    }
    let aliases: Vec<String> = ti4_content::factions::get(content, FACTION)
        .map(|faction| {
            faction
                .promissory_notes()
                .into_iter()
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default();
    for alias in aliases {
        state
            .promissory_notes
            .entry(crate::promissory::note_id(&alias, FACTION))
            .or_insert_with(|| player.clone());
    }
}

// -- Voidborn ------------------------------------------------------------------------------------

fn ignores_nebulae(state: &GameState, mover: &PlayerId) -> bool {
    is_empyrean(state, mover)
}

// -- Aetherpassage -------------------------------------------------------------------------------

fn passage_ready(
    context: &TimingContext<'_>,
    event: &crate::event::Event,
    owner: &PlayerId,
) -> bool {
    let Some(player) = event.text("player") else {
        return false;
    };
    let Some(system) = event.text("system") else {
        return false;
    };
    is_empyrean(context.state, owner)
        && player != owner.as_str()
        && has_ship_outside(
            context.state,
            context.content,
            context.sources,
            owner,
            system,
        )
        && has_ship_outside(
            context.state,
            context.content,
            context.sources,
            &PlayerId::new(player),
            system,
        )
}

/// "After a player activates a system: You may allow that player to move their ships through
/// systems that contain your ships." The permission is scoped to that activation.
fn aetherpassage(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("ability:{owner_name}:aetherpassage:SYSTEM_ACTIVATED:after"),
        seat.clone(),
        "SYSTEM_ACTIVATED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            if !passage_ready(context, event, &owner) {
                return Ok(());
            }
            let Some(player) = event.text("player") else {
                return Ok(());
            };
            let seq = context.state.activation_seq;
            context
                .state
                .faction_marks
                .insert(PASSAGE.to_owned(), format!("{seq}|{player}|{owner}"));
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        passage_ready(context, event, &condition_owner)
    }))
}

/// Players whose ships do not block `mover` during this activation.
fn passable_owners(state: &GameState, mover: &PlayerId) -> Vec<PlayerId> {
    let Some(mark) = state.faction_marks.get(PASSAGE) else {
        return Vec::new();
    };
    let mut parts = mark.split('|');
    let (Some(seq), Some(who), Some(owner)) = (parts.next(), parts.next(), parts.next()) else {
        return Vec::new();
    };
    if seq.parse::<u32>().ok() == Some(state.activation_seq) && who == mover.as_str() {
        vec![PlayerId::new(owner)]
    } else {
        Vec::new()
    }
}

// -- Aetherstream --------------------------------------------------------------------------------

fn adjacent_to_anomaly(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &Galaxy,
    viewer: &PlayerId,
    system: &str,
) -> bool {
    let adjacency = crate::movement::PlayerAdjacency::new(state, content, sources, galaxy, viewer);
    let rifts = super::hooks_movement::extra_gravity_rifts(state);
    adjacency.neighbours(system).iter().any(|other| {
        rifts.contains(other)
            || ti4_content::galaxy::system(content, other, sources)
                .is_some_and(|tile| tile.is_anomaly())
    })
}

fn stream_ready(
    context: &TimingContext<'_>,
    event: &crate::event::Event,
    owner: &PlayerId,
) -> bool {
    let (Some(player), Some(system), Some(galaxy)) =
        (event.text("player"), event.text("system"), context.galaxy)
    else {
        return false;
    };
    let player = PlayerId::new(player);
    has_technology(context.state, owner, "as")
        && (&player == owner
            || crate::transactions::are_neighbours(context.state, galaxy, owner, &player))
        && has_ship_outside(
            context.state,
            context.content,
            context.sources,
            &player,
            system,
        )
        && adjacent_to_anomaly(
            context.state,
            context.content,
            context.sources,
            galaxy,
            &player,
            system,
        )
}

/// "After you or one of your neighbors activates a system that is adjacent to an anomaly, you may
/// apply +1 to the move value of all of that player's ships during this tactical action."
fn aetherstream(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("technology:{owner_name}:as:SYSTEM_ACTIVATED:after"),
        seat.clone(),
        "SYSTEM_ACTIVATED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            if !stream_ready(context, event, &owner) {
                return Ok(());
            }
            let Some(player) = event.text("player") else {
                return Ok(());
            };
            let seq = context.state.activation_seq;
            context
                .state
                .faction_marks
                .insert(STREAM.to_owned(), format!("{seq}|{player}"));
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        stream_ready(context, event, &condition_owner)
    }))
}

/// +1 to every ship of the player Aetherstream was applied to, during that activation.
fn move_bonus(state: &GameState, site: &MoveSite<'_>) -> i32 {
    let Some(mark) = state.faction_marks.get(STREAM) else {
        return 0;
    };
    let Some((seq, who)) = mark.split_once('|') else {
        return 0;
    };
    i32::from(seq.parse::<u32>().ok() == Some(state.activation_seq) && who == site.player.as_str())
}

// -- Voidwatch -----------------------------------------------------------------------------------

/// The promissory notes in `player`'s hand, as the option ids they would be given by.
fn hand(state: &GameState, content: &ContentStore, player: &PlayerId) -> Vec<String> {
    let mut notes = crate::promissory::available_notes(state, content, player);
    if let Some(support) = crate::promissory::available_support(state, player) {
        notes.push(support);
    }
    notes
}

fn watch_ready(context: &TimingContext<'_>, event: &crate::event::Event, owner: &PlayerId) -> bool {
    let (Some(player), Some(system)) = (event.text("player"), event.text("system")) else {
        return false;
    };
    let player = PlayerId::new(player);
    let here = context.state.system_state(&SystemId::new(system));
    has_technology(context.state, owner, "vw")
        && &player != owner
        && event.integer("ships_moved").unwrap_or(0) > 0
        && (here.units.iter().any(|unit| &unit.owner == owner)
            || here
                .planet_units
                .values()
                .flatten()
                .any(|unit| &unit.owner == owner))
        && !hand(context.state, context.content, &player).is_empty()
}

/// "After a player moves ships into a system that contains 1 or more of your units, they must give
/// you 1 promissory note from their hand, if able." The mover chooses which.
fn voidwatch(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("technology:{owner_name}:vw:MOVEMENT_FINISHED:after"),
        seat.clone(),
        "MOVEMENT_FINISHED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            if !watch_ready(context, event, &owner) {
                return Ok(());
            }
            let Some(player) = event.text("player").map(PlayerId::new) else {
                return Ok(());
            };
            let notes = hand(context.state, context.content, &player);
            let worded: Vec<(String, Worded)> = notes
                .iter()
                .map(|note| {
                    (
                        format!("note|{note}"),
                        Phrase::of("give {note} to the Empyrean").with(
                            "note",
                            note,
                            Subject::note(context.content, note),
                        ),
                    )
                })
                .collect();
            let options: Vec<ChoiceOption> = worded
                .iter()
                .map(|(id, worded)| ChoiceOption::labelled(id, "promissory", worded.label()))
                .collect();
            let choice = Choice::new(
                player.clone(),
                "Voidwatch: which promissory note to give".to_owned(),
                options,
            )
            .phrased_options(&worded)
            .contextualized(decision(context.state, &player, "vw", "voidwatch_note"));
            let answer = context
                .ask_seeing(&choice)
                .map_err(crate::timing::TimingError::IllegalChoice)?;
            let Some(note) = notes
                .iter()
                .find(|note| format!("note|{note}") == answer.id)
                .cloned()
            else {
                return Ok(());
            };
            if note.starts_with(crate::promissory::SUPPORT_PREFIX) {
                crate::promissory::receive(context.state, &owner, &note);
            } else {
                crate::promissory::take(context.state, context.content, &owner, &note);
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        watch_ready(context, event, &condition_owner)
    }))
}

// -- Void Tether ---------------------------------------------------------------------------------

fn tether_key(owner: &PlayerId) -> String {
    format!("empyrean|tether|{owner}")
}

/// The borders `owner` has tethered, normalised and sorted.
fn tethers(state: &GameState, owner: &PlayerId) -> BTreeSet<(String, String)> {
    state
        .faction_marks
        .get(&tether_key(owner))
        .map(|text| {
            text.split(';')
                .filter_map(|pair| pair.split_once('|'))
                .map(|(a, b)| normal(a, b))
                .collect()
        })
        .unwrap_or_default()
}

fn store_tethers(state: &mut GameState, owner: &PlayerId, borders: &BTreeSet<(String, String)>) {
    let text = borders
        .iter()
        .map(|(a, b)| format!("{a}|{b}"))
        .collect::<Vec<_>>()
        .join(";");
    if text.is_empty() {
        state.faction_marks.remove(&tether_key(owner));
    } else {
        state.faction_marks.insert(tether_key(owner), text);
    }
}

/// The Void Tether borders of `owner` that `viewer` (another player) treats as closed right now.
fn allowed_this_activation(state: &GameState, viewer: &PlayerId, owner: &PlayerId) -> bool {
    state.faction_marks.get(ALLOW).is_some_and(|mark| {
        let mut parts = mark.split('|');
        matches!(
            (parts.next(), parts.next(), parts.next()),
            (Some(seq), Some(who), Some(by))
                if seq.parse::<u32>().ok() == Some(state.activation_seq)
                    && who == viewer.as_str()
                    && by == owner.as_str()
        )
    })
}

fn blocked_borders(state: &GameState, viewer: &PlayerId) -> Vec<(String, String)> {
    state
        .players
        .iter()
        .filter(|seat| &seat.id != viewer && has_breakthrough(state, &seat.id))
        .filter(|seat| !allowed_this_activation(state, viewer, &seat.id))
        .flat_map(|seat| tethers(state, &seat.id))
        .collect()
}

/// One legal use of the breakthrough.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Tether {
    Place((String, String)),
    Move {
        from: (String, String),
        to: (String, String),
    },
}

impl Tether {
    fn key(&self) -> String {
        match self {
            Self::Place((a, b)) => format!("place|{a}|{b}"),
            Self::Move { from, to } => format!("move|{}|{}|{}|{}", from.0, from.1, to.0, to.1),
        }
    }

    fn label(&self) -> String {
        match self {
            Self::Place((a, b)) => format!("place a Void Tether on the border of {a} and {b}"),
            Self::Move { from, to } => format!(
                "move the Void Tether on {} and {} to the border of {} and {}",
                from.0, from.1, to.0, to.1
            ),
        }
    }
}

/// Every placement and move the Empyrean may make for the activation of `system`.
fn tether_options(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &Galaxy,
    owner: &PlayerId,
    system: &str,
) -> Vec<Tether> {
    let presence = crate::transactions::presence(state, owner);
    let adjacency = crate::movement::PlayerAdjacency::new(state, content, sources, galaxy, owner);
    let neighbours = adjacency.neighbours(system);
    let here = presence.contains(&SystemId::new(system))
        || neighbours
            .iter()
            .any(|other| presence.contains(&SystemId::new(other.as_str())));
    if !here {
        return Vec::new();
    }
    // "A border that system shares with another system": a hex edge, not a wormhole link.
    let borders: BTreeSet<(String, String)> = neighbours
        .iter()
        .filter(|other| galaxy.distance(system, other) == Some(1))
        .map(|other| normal(system, other))
        .collect();
    let placed = tethers(state, owner);
    let free: Vec<&(String, String)> = borders.iter().filter(|b| !placed.contains(*b)).collect();
    let mut found = Vec::new();
    if placed.len() < TETHER_TOKENS {
        found.extend(free.iter().map(|b| Tether::Place((*b).clone())));
    }
    for from in &placed {
        for to in &free {
            found.push(Tether::Move {
                from: from.clone(),
                to: (*to).clone(),
            });
        }
    }
    found
}

fn tether_ready(
    context: &TimingContext<'_>,
    event: &crate::event::Event,
    owner: &PlayerId,
) -> bool {
    let (Some(player), Some(system), Some(galaxy)) =
        (event.text("player"), event.text("system"), context.galaxy)
    else {
        return false;
    };
    player == owner.as_str()
        && has_breakthrough(context.state, owner)
        && !tether_options(
            context.state,
            context.content,
            context.sources,
            galaxy,
            owner,
            system,
        )
        .is_empty()
}

/// "When you activate a system that contains or is adjacent to a unit or planet you control, you
/// may place or move 1 of your Void Tether tokens onto a border that system shares with another
/// system."
fn tether_place(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("breakthrough:{owner_name}:{BREAKTHROUGH}:SYSTEM_ACTIVATED:after"),
        seat.clone(),
        "SYSTEM_ACTIVATED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            if !tether_ready(context, event, &owner) {
                return Ok(());
            }
            let (Some(system), Some(galaxy)) = (event.text("system"), context.galaxy) else {
                return Ok(());
            };
            let found = tether_options(
                context.state,
                context.content,
                context.sources,
                galaxy,
                &owner,
                system,
            );
            let options: Vec<ChoiceOption> = found
                .iter()
                .map(|tether| ChoiceOption::labelled(tether.key(), "tether", tether.label()))
                .collect();
            let choice = Choice::new(
                owner.clone(),
                "Void Tether: which border".to_owned(),
                options,
            )
            .contextualized(decision(
                context.state,
                &owner,
                BREAKTHROUGH,
                "tether_border",
            ));
            let answer = context
                .ask_seeing(&choice)
                .map_err(crate::timing::TimingError::IllegalChoice)?;
            let Some(picked) = found.iter().find(|tether| tether.key() == answer.id) else {
                return Ok(());
            };
            let mut borders = tethers(context.state, &owner);
            match picked {
                Tether::Place(border) => {
                    borders.insert(border.clone());
                }
                Tether::Move { from, to } => {
                    borders.remove(from);
                    borders.insert(to.clone());
                }
            }
            store_tethers(context.state, &owner, &borders);
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        tether_ready(context, event, &condition_owner)
    }))
}

/// How far (in hexes) from the active system a tether can still matter to the activator's
/// movement: a ship with Move 3 reaches a border whose nearer end is 2 away from the active system
/// (the border costs one step, leaving the last hop). Borders further out only matter to a fleet
/// that already stands on one of their ends, which is tested separately.
const TETHER_REACH: i32 = 2;

/// Whether a placed tether could affect `player`'s movement for the activation of `system`: one of
/// its ends is within [`TETHER_REACH`] of the active system, or holds a unit of `player`. This is a
/// deliberate over-approximation (ship Move values are not consulted), so the question is never
/// skipped when a tether could matter, and is not asked when every tether is out of play.
fn tether_relevant(
    state: &GameState,
    galaxy: Option<&Galaxy>,
    owner: &PlayerId,
    player: &PlayerId,
    system: &str,
) -> bool {
    let borders = tethers(state, owner);
    let Some(galaxy) = galaxy else {
        return !borders.is_empty(); // no map to judge by: stay conservative
    };
    let presence = crate::transactions::presence(state, player);
    borders.iter().any(|(a, b)| {
        [a, b].into_iter().any(|end| {
            presence.contains(&SystemId::new(end.as_str()))
                || galaxy
                    .distance(system, end)
                    .is_some_and(|distance| distance <= TETHER_REACH)
        })
    })
}

fn allow_ready(context: &TimingContext<'_>, event: &crate::event::Event, owner: &PlayerId) -> bool {
    let (Some(player), Some(system)) = (event.text("player"), event.text("system")) else {
        return false;
    };
    player != owner.as_str()
        && has_breakthrough(context.state, owner)
        && tether_relevant(
            context.state,
            context.galaxy,
            owner,
            &PlayerId::new(player),
            system,
        )
}

/// "... other players do not treat those systems as adjacent to each other unless you allow it."
/// The Empyrean is asked once another player has activated a system, and the permission lasts for
/// that activation.
fn tether_allow(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("breakthrough:{owner_name}:{BREAKTHROUGH}_allow:SYSTEM_ACTIVATED:after"),
        seat.clone(),
        "SYSTEM_ACTIVATED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            if !allow_ready(context, event, &owner) {
                return Ok(());
            }
            let Some(player) = event.text("player") else {
                return Ok(());
            };
            let seq = context.state.activation_seq;
            context
                .state
                .faction_marks
                .insert(ALLOW.to_owned(), format!("{seq}|{player}|{owner}"));
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        allow_ready(context, event, &condition_owner)
    }))
}

// -- timing registration -------------------------------------------------------------------------

fn timing_abilities(state: &GameState, owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    let mut abilities = vec![
        aetherpassage(owner_name, seat),
        aetherstream(owner_name, seat),
        voidwatch(owner_name, seat),
        tether_place(owner_name, seat),
        tether_allow(owner_name, seat),
    ];
    abilities.extend(super::empyrean_units::timing_abilities(
        state, owner_name, seat,
    ));
    abilities
}

// -- Xuange --------------------------------------------------------------------------------------

/// "Be neighbors with all other players."
fn commander_unlocked(
    state: &GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    galaxy: Option<&Galaxy>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    if leader.as_str() != COMMANDER {
        return None;
    }
    let Some(galaxy) = galaxy else {
        return Some(false); // without a map there is no "adjacent"
    };
    let others: Vec<&PlayerId> = state
        .players
        .iter()
        .map(|seat| &seat.id)
        .filter(|id| *id != player)
        .collect();
    Some(
        !others.is_empty()
            && others
                .iter()
                .all(|other| crate::transactions::are_neighbours(state, galaxy, player, other)),
    )
}

// -- Conservator Procyon -------------------------------------------------------------------------

/// The hero can always be used by its owner: placing and exploring may have nothing to do on a
/// crowded map, but the card still resolves and is purged.
fn leader_action(
    state: &GameState,
    _content: &ContentStore,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    (leader.as_str() == HERO).then(|| is_empyrean(state, player))
}

/// "ACTION: Place 1 frontier token in each system that does not contain any planets and does not
/// already have a frontier token. Then, explore each frontier token that is in a system that
/// contains 1 or more of your ships. Then, purge this card." (The purge is the shared code's.)
///
/// "Does not contain any planets" is the engine's own rule for where a frontier token goes
/// ([`crate::exploration::frontier_systems`]): a space station is not a planet to land on. Needs the
/// map; without one nothing is placed and the use is refused.
fn use_leader(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    if leader.as_str() != HERO {
        return None;
    }
    let galaxy = context.galaxy?;
    if !is_empyrean(context.state, player) {
        return Some(false);
    }
    // A failed exploration must not count as a use (and so must not purge the card): keep what is
    // needed to put everything back, as the shared hero path does.
    let before = context.state.clone();
    let before_dice = context.dice.clone();
    let before_rng = context.rng.clone();
    let placed = crate::exploration::frontier_systems(context.content, context.sources, galaxy);
    context.state.frontier_tokens.extend(placed);
    let types = ti4_content::units::catalogue(context.content, context.sources);
    let targets: Vec<SystemId> = context
        .state
        .frontier_tokens
        .iter()
        .filter(|system| {
            context.state.system_state(system).units.iter().any(|unit| {
                &unit.owner == player
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(ti4_content::units::UnitType::is_ship)
            })
        })
        .cloned()
        .collect();
    let mut resolving = crate::choice::Resolving {
        content: context.content,
        sources: context.sources,
        dice: context.dice,
        rng: context.rng,
        table: context.table,
        timing: None,
    };
    let mut failed = false;
    for system in targets {
        // Every target holds a token and one of the player's ships, so `None` means the draw itself
        // could not resolve (for example an empty deck).
        if crate::exploration::explore_frontier(context.state, &mut resolving, player, &system)
            .is_none()
        {
            failed = true;
            break;
        }
    }
    if failed {
        *context.state = before;
        *context.dice = before_dice;
        *context.rng = before_rng;
        return Some(false);
    }
    Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use ti4_model::content_types::DEFAULT;

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
        crate::fixtures::seated_game(&[("a", FACTION), ("b", "sol")], DEFAULT)
    }
    /// The same game with every unit and planet control off the board, so a test map made of
    /// ordinary systems is not polluted by a home system that shares an id with one of them.
    fn blank(mut state: GameState) -> GameState {
        for system in state.board.keys().cloned().collect::<Vec<_>>() {
            let board = state.system_mut(&system);
            board.units.clear();
            board.planet_units.clear();
            board.planet_control.clear();
        }
        state
    }
    fn scripted(answers: &[&str]) -> crate::choice::Table {
        crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new(
            answers.iter().map(|s| (*s).to_owned()),
        )))
    }
    fn payload(pairs: &[(&str, &str)]) -> BTreeMap<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), serde_json::Value::String((*v).to_owned())))
            .collect()
    }

    /// Emit one typed event through the armed resolver; returns what the table was asked.
    fn emit(
        state: &mut GameState,
        galaxy: Option<&Galaxy>,
        table: &mut crate::choice::Table,
        event_type: &str,
        mut data: BTreeMap<String, serde_json::Value>,
    ) -> Vec<String> {
        let mut resolver = crate::fixtures::armed_resolver(state);
        crate::fixtures::with_context(state, DEFAULT, galaxy, table, |ctx| {
            let event = ctx
                .event_sequence
                .next(event_type, std::mem::take(&mut data))
                .expect("an event id");
            resolver
                .emit_with_context(ctx, event, |_, _| {})
                .expect("the window resolves");
        });
        table
            .log
            .records
            .iter()
            .flat_map(|record| record.offered.iter().cloned())
            .collect()
    }

    fn activate(
        state: &mut GameState,
        galaxy: &Galaxy,
        table: &mut crate::choice::Table,
        who: &PlayerId,
        system: &str,
    ) -> Vec<String> {
        state.activation_seq += 1;
        emit(
            state,
            Some(galaxy),
            table,
            "SYSTEM_ACTIVATED",
            payload(&[("player", who.as_str()), ("system", system)]),
        )
    }

    fn grant_tech(state: &mut GameState, who: &PlayerId, alias: &str) {
        state
            .player_mut(who)
            .unwrap()
            .technologies
            .insert(ti4_model::id::TechnologyId::new(alias));
    }

    fn grant_bt(state: &mut GameState, who: &PlayerId) {
        state.player_mut(who).unwrap().breakthrough =
            Some(ti4_model::id::BreakthroughId::new(BREAKTHROUGH));
    }

    fn rules<'a>(
        hub: &'a crate::fixtures::Hub,
        state: &GameState,
        mover: &PlayerId,
        active: &str,
    ) -> crate::movement::MovementRules<'a> {
        crate::movement::MovementRules::with_laws(
            &hub.galaxy,
            content(),
            DEFAULT,
            active,
            crate::movement::Board::for_player(state, content(), DEFAULT, mover),
            Some(state),
        )
    }

    // -- Dark Whispers ---------------------------------------------------------------------------

    #[test]
    fn dark_whispers_gives_the_empyrean_both_notes_and_nobody_else() {
        let state = game();
        for note in ["blood_pact:empyrean", "dark_pact:empyrean"] {
            assert_eq!(state.promissory_notes.get(note), Some(&a()), "{note}");
        }
        assert!(
            state
                .promissory_notes
                .keys()
                .filter(|note| note.ends_with(":empyrean"))
                .all(|note| ["blood_pact:empyrean", "dark_pact:empyrean"].contains(&note.as_str())),
            "the generic notes are not the Empyrean's"
        );
        let plain = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        assert!(
            plain
                .promissory_notes
                .keys()
                .all(|note| !note.contains("_pact")),
            "no Empyrean, no pacts"
        );
        // Dealt again by the setup path, it is the same two.
        let mut dealt = game();
        crate::promissory::deal(&mut dealt, content(), DEFAULT);
        assert_eq!(dealt.promissory_notes.get("dark_pact:empyrean"), Some(&a()));
        assert_eq!(
            dealt.promissory_notes.get("blood_pact:empyrean"),
            Some(&a())
        );
    }

    // -- Voidborn --------------------------------------------------------------------------------

    #[test]
    fn voidborn_lets_empyrean_ships_cross_and_leave_a_nebula_only_theirs() {
        let state = blank(game());
        let nebula = crate::fixtures::a_system_where("nebula");
        let through = crate::fixtures::hub_with_centre(&nebula);
        let origin = through.outer[0].clone();
        let target = through.across(&origin);
        // 59.1a: nobody else may pass through a nebula; 2 moves is only reachable across it.
        assert!(!rules(&through, &state, &b(), &target).can_reach(&origin, 2));
        assert!(rules(&through, &state, &a(), &target).can_reach(&origin, 2));
        // 59.2: a ship starting in a nebula treats its move as 1; not the Empyrean's.
        let inside = crate::fixtures::hub_with_outer(&nebula);
        let far = inside.across(&nebula);
        assert!(!rules(&inside, &state, &b(), &far).can_reach(&nebula, 2));
        assert!(rules(&inside, &state, &a(), &far).can_reach(&nebula, 2));
        // Nothing changes for a table without the Empyrean.
        let plain = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        assert!(!rules(&through, &plain, &a(), &target).can_reach(&origin, 2));
        assert!(!ignores_nebulae(&plain, &a()));
    }

    // -- Aetherpassage ---------------------------------------------------------------------------

    const PASSAGE_ID: &str = "ability:empyrean:aetherpassage:SYSTEM_ACTIVATED:after";

    fn blockade_board() -> (
        GameState,
        crate::fixtures::Hub,
        SystemId,
        SystemId,
        SystemId,
    ) {
        let mut state = blank(crate::fixtures::seated_game(
            &[("a", FACTION), ("b", "sol"), ("c", "hacan")],
            DEFAULT,
        ));
        let hub = crate::fixtures::plain_hub();
        let origin = SystemId::new(hub.outer[0].as_str());
        let target = SystemId::new(hub.across(&hub.outer[0]).as_str());
        let centre = SystemId::new(hub.centre.as_str());
        crate::fixtures::put(&mut state, &origin, "destroyer", &b(), 1);
        crate::fixtures::put(&mut state, &centre, "cruiser", &a(), 1);
        // The Empyrean needs a ship that is not in the active system; the centre serves.
        (state, hub, origin, target, centre)
    }

    fn sol_reaches(
        state: &GameState,
        hub: &crate::fixtures::Hub,
        origin: &SystemId,
        target: &SystemId,
    ) -> bool {
        crate::tactical::movable_into(state, content(), DEFAULT, &hub.galaxy, &b(), target)
            .iter()
            .any(|movable| &movable.origin == origin)
    }

    #[test]
    fn aetherpassage_lets_the_activator_pass_empyrean_ships_for_that_activation_only() {
        let (mut state, hub, origin, target, _) = blockade_board();
        assert!(!sol_reaches(&state, &hub, &origin, &target), "58.4b blocks");
        let mut table = scripted(&[PASSAGE_ID]);
        activate(&mut state, &hub.galaxy, &mut table, &b(), target.as_str());
        assert!(sol_reaches(&state, &hub, &origin, &target), "allowed");
        // The permission belongs to that activation: the next one has none.
        state.activation_seq += 1;
        assert!(!sol_reaches(&state, &hub, &origin, &target));
    }

    #[test]
    fn aetherpassage_is_offered_under_its_printed_name() {
        struct Capture(std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>);
        impl crate::choice::Decider for Capture {
            fn choose(
                &mut self,
                choice: &Choice,
            ) -> Result<ChoiceOption, crate::choice::IllegalChoice> {
                let mut seen = self.0.lock().unwrap();
                seen.extend(
                    choice
                        .options
                        .iter()
                        .map(|o| (o.id.clone(), o.label.clone())),
                );
                Ok(choice
                    .options
                    .iter()
                    .find(|o| o.is_decline())
                    .unwrap()
                    .clone())
            }
        }
        let (mut state, hub, _, target, _) = blockade_board();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut table = crate::choice::Table::with_default(Box::new(Capture(seen.clone())));
        activate(&mut state, &hub.galaxy, &mut table, &b(), target.as_str());
        let seen = seen.lock().unwrap();
        let passage = seen
            .iter()
            .find(|(id, _)| id == PASSAGE_ID)
            .expect("offered");
        assert_eq!(passage.1, "Aetherpassage (after system activated)");
    }

    #[test]
    fn aetherpassage_is_optional_and_does_not_lift_a_third_players_blockade() {
        let (mut state, hub, origin, target, centre) = blockade_board();
        let mut declined = scripted(&["decline"]);
        activate(
            &mut state,
            &hub.galaxy,
            &mut declined,
            &b(),
            target.as_str(),
        );
        assert!(!sol_reaches(&state, &hub, &origin, &target), "declined");
        assert!(state.faction_marks.get(PASSAGE).is_none());

        crate::fixtures::put(&mut state, &centre, "cruiser", &c(), 1);
        let mut table = scripted(&[PASSAGE_ID]);
        activate(&mut state, &hub.galaxy, &mut table, &b(), target.as_str());
        assert!(
            !sol_reaches(&state, &hub, &origin, &target),
            "the Hacan ship in the same system still blocks"
        );
    }

    #[test]
    fn aetherpassage_leaves_a_blocker_that_holds_no_foreign_ship_alone() {
        let (mut state, hub, origin, target, centre) = blockade_board();
        // The Empyrean's only ship is elsewhere, so the centre holds no foreign ship at all.
        state.system_mut(&centre).units.clear();
        crate::fixtures::put(
            &mut state,
            &SystemId::new(hub.outer[4].as_str()),
            "cruiser",
            &a(),
            1,
        );
        let mut table = scripted(&[PASSAGE_ID]);
        activate(&mut state, &hub.galaxy, &mut table, &b(), target.as_str());
        assert!(state.faction_marks.contains_key(PASSAGE), "passage granted");
        // Another hook's blocker on the centre: no ship there to pass, so it must stay.
        let mut board = crate::movement::Board::for_player(&state, content(), DEFAULT, &b());
        board.enemy_ships.insert(centre.to_string());
        let rules = crate::movement::MovementRules::with_laws(
            &hub.galaxy,
            content(),
            DEFAULT,
            target.as_str(),
            board,
            Some(&state),
        );
        assert!(!rules.can_reach(origin.as_str(), 2), "the blocker survives");
    }

    #[test]
    fn aetherpassage_is_not_offered_without_an_empyrean_seat_or_for_their_own_activation() {
        let mut plain = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        let hub = crate::fixtures::plain_hub();
        let target = hub.outer[0].clone();
        crate::fixtures::put(
            &mut plain,
            &SystemId::new(hub.centre.as_str()),
            "cruiser",
            &a(),
            1,
        );
        crate::fixtures::put(
            &mut plain,
            &SystemId::new(hub.outer[1].as_str()),
            "cruiser",
            &b(),
            1,
        );
        let before = plain.clone();
        let mut table = scripted(&[]);
        let asked = activate(&mut plain, &hub.galaxy, &mut table, &b(), &target);
        assert!(
            asked.iter().all(|prompt| !prompt.contains("aetherpassage")),
            "{asked:?}"
        );
        assert!(plain.faction_marks.is_empty());
        assert_eq!(plain.promissory_notes, before.promissory_notes);

        let (mut state, hub, _, target, _) = blockade_board();
        let mut table = scripted(&[PASSAGE_ID]);
        activate(&mut state, &hub.galaxy, &mut table, &a(), target.as_str());
        assert!(state.faction_marks.get(PASSAGE).is_none(), "own activation");
    }

    // -- Aetherstream ----------------------------------------------------------------------------

    const STREAM_ID: &str = "technology:empyrean:as:SYSTEM_ACTIVATED:after";

    /// A hub whose centre is an asteroid field; sol and the Empyrean sit on adjacent ring systems.
    fn stream_board() -> (GameState, crate::fixtures::Hub, SystemId) {
        let mut state = blank(game());
        let asteroid = crate::fixtures::a_system_where("asteroid field");
        let hub = crate::fixtures::hub_with_centre(&asteroid);
        crate::fixtures::put(
            &mut state,
            &SystemId::new(hub.outer[0].as_str()),
            "carrier",
            &a(),
            1,
        );
        crate::fixtures::put(
            &mut state,
            &SystemId::new(hub.outer[1].as_str()),
            "carrier",
            &b(),
            1,
        );
        grant_tech(&mut state, &a(), "as");
        let target = SystemId::new(hub.outer[3].as_str());
        (state, hub, target)
    }

    fn bonus_for(state: &GameState, who: &PlayerId) -> i32 {
        let kind = ti4_content::units::unit_type(content(), "carrier", DEFAULT).unwrap();
        let origin = SystemId::new("18");
        move_bonus(
            state,
            &MoveSite {
                player: who,
                origin: &origin,
                index: Some(0),
                ship: &kind,
            },
        )
    }

    #[test]
    fn aetherstream_adds_one_move_for_a_neighbour_activating_next_to_an_anomaly() {
        let (mut state, hub, target) = stream_board();
        let mut table = scripted(&[STREAM_ID]);
        activate(&mut state, &hub.galaxy, &mut table, &b(), target.as_str());
        assert_eq!(bonus_for(&state, &b()), 1);
        assert_eq!(bonus_for(&state, &a()), 0, "that player's ships only");
        // ... and it reaches the real move value.
        let kind = ti4_content::units::unit_type(content(), "carrier", DEFAULT).unwrap();
        let origin = SystemId::new(hub.outer[1].as_str());
        assert_eq!(
            crate::tactical::effective_move_value(&state, &kind, &b(), &origin),
            i32::try_from(kind.move_value()).unwrap() + 1
        );
        state.activation_seq += 1;
        assert_eq!(bonus_for(&state, &b()), 0, "this tactical action only");
    }

    #[test]
    fn aetherstream_also_serves_the_empyrean_and_needs_the_tech_an_anomaly_and_a_neighbour() {
        let (mut state, hub, target) = stream_board();
        let mut table = scripted(&[STREAM_ID]);
        activate(&mut state, &hub.galaxy, &mut table, &a(), target.as_str());
        assert_eq!(bonus_for(&state, &a()), 1, "you");

        // No technology.
        let (mut without, hub2, target2) = stream_board();
        without
            .player_mut(&a())
            .unwrap()
            .technologies
            .retain(|t| t.as_str() != "as");
        let mut table = scripted(&["decline"]);
        activate(
            &mut without,
            &hub2.galaxy,
            &mut table,
            &b(),
            target2.as_str(),
        );
        assert!(without.faction_marks.get(STREAM).is_none());

        // Not next to an anomaly: the ring system across the centre is only adjacent to the
        // centre (an anomaly) and its ring neighbours, so use a plain hub instead.
        let mut plain_state = game();
        let plain = crate::fixtures::plain_hub();
        crate::fixtures::put(
            &mut plain_state,
            &SystemId::new(plain.outer[0].as_str()),
            "carrier",
            &a(),
            1,
        );
        crate::fixtures::put(
            &mut plain_state,
            &SystemId::new(plain.outer[1].as_str()),
            "carrier",
            &b(),
            1,
        );
        grant_tech(&mut plain_state, &a(), "as");
        let mut table = scripted(&["decline"]);
        activate(
            &mut plain_state,
            &plain.galaxy,
            &mut table,
            &b(),
            plain.outer[3].as_str(),
        );
        assert!(
            plain_state.faction_marks.get(STREAM).is_none(),
            "no anomaly nearby"
        );

        // Not a neighbour.
        let (mut far, hub3, target3) = stream_board();
        for outer in &hub3.outer {
            far.system_mut(&SystemId::new(outer.as_str())).units.clear();
        }
        far.system_mut(&SystemId::new(hub3.centre.as_str()))
            .units
            .clear();
        crate::fixtures::put(&mut far, &SystemId::new("56"), "carrier", &a(), 1);
        crate::fixtures::put(
            &mut far,
            &SystemId::new(hub3.outer[1].as_str()),
            "carrier",
            &b(),
            1,
        );
        let mut table = scripted(&["decline"]);
        activate(&mut far, &hub3.galaxy, &mut table, &b(), target3.as_str());
        assert!(far.faction_marks.get(STREAM).is_none(), "not neighbours");
    }

    // -- Voidwatch -------------------------------------------------------------------------------

    fn finished(
        state: &mut GameState,
        table: &mut crate::choice::Table,
        system: &str,
        moved: &str,
    ) {
        let mut data = payload(&[("player", "b"), ("system", system)]);
        data.insert(
            "ships_moved".to_owned(),
            moved.parse::<u32>().unwrap().into(),
        );
        emit(state, None, table, "MOVEMENT_FINISHED", data);
    }

    #[test]
    fn voidwatch_takes_a_note_the_mover_chooses_when_they_move_in_beside_empyrean_units() {
        let mut state = game();
        grant_tech(&mut state, &a(), "vw");
        let home = state.player(&a()).unwrap().home_system.clone().unwrap();
        let hand_before = hand(&state, content(), &b());
        let pick = hand_before
            .iter()
            .find(|note| !note.starts_with(crate::promissory::SUPPORT_PREFIX))
            .cloned()
            .expect("a note in hand");
        let mut table = scripted(&[&format!("note|{pick}")]);
        finished(&mut state, &mut table, home.as_str(), "1");
        assert_eq!(state.promissory_notes.get(&pick), Some(&a()), "given");
        assert_eq!(hand(&state, content(), &b()).len() + 1, hand_before.len());
    }

    /// Each note of the question says which note it is (`details.phrases`), by its record.
    #[test]
    fn voidwatch_names_each_note_of_its_question() {
        let mut state = game();
        grant_tech(&mut state, &a(), "vw");
        let home = state.player(&a()).unwrap().home_system.clone().unwrap();
        let notes = hand(&state, content(), &b());
        assert!(notes.len() > 1, "a question");
        let (mut table, seen) = crate::phrases::capturing(&[]);
        finished(&mut state, &mut table, home.as_str(), "1");
        let seen = seen.borrow();
        let choice = seen
            .iter()
            .find(|choice| choice.prompt.starts_with("Voidwatch"))
            .expect("asked");
        let hints = crate::phrases::phrased(choice);
        for note in &notes {
            let phrase = &hints.options[&format!("note|{note}")];
            assert_eq!(phrase.text, "give {note} to the Empyrean");
            assert_eq!(
                phrase.refs["note"],
                crate::phrases::Subject::note(content(), note)
            );
        }
    }

    #[test]
    fn voidwatch_may_take_the_movers_support_for_the_throne() {
        let mut state = game();
        grant_tech(&mut state, &a(), "vw");
        let home = state.player(&a()).unwrap().home_system.clone().unwrap();
        let support = crate::promissory::support("sol");
        let mut table = scripted(&[&format!("note|{support}")]);
        finished(&mut state, &mut table, home.as_str(), "1");
        assert_eq!(
            state.support_holders.get(&b()),
            Some(&a()),
            "the card went to a"
        );
    }

    #[test]
    fn voidwatch_needs_the_tech_an_empyrean_unit_and_a_move_and_is_silent_for_other_tables() {
        let home = |state: &GameState| state.player(&a()).unwrap().home_system.clone().unwrap();
        // No technology.
        let mut state = game();
        let before = state.promissory_notes.clone();
        let system = home(&state);
        let mut table = scripted(&[]);
        finished(&mut state, &mut table, system.as_str(), "1");
        assert_eq!(state.promissory_notes, before);
        // Technology but no ship moved.
        grant_tech(&mut state, &a(), "vw");
        finished(&mut state, &mut table, system.as_str(), "0");
        assert_eq!(state.promissory_notes, before);
        // A system with none of their units.
        finished(&mut state, &mut table, "18", "1");
        assert_eq!(state.promissory_notes, before);
        // No Empyrean at all.
        let mut plain = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        let before = plain.promissory_notes.clone();
        finished(&mut plain, &mut table, "18", "1");
        assert_eq!(plain.promissory_notes, before);
    }

    #[test]
    fn voidwatch_by_the_empyrean_itself_takes_nothing() {
        let mut state = blank(game());
        grant_tech(&mut state, &a(), "vw");
        let home = state.player(&a()).unwrap().home_system.clone().unwrap();
        let before = state.promissory_notes.clone();
        let mut data = payload(&[("player", "a"), ("system", home.as_str())]);
        data.insert("ships_moved".to_owned(), 1u32.into());
        let mut table = scripted(&[]);
        emit(&mut state, None, &mut table, "MOVEMENT_FINISHED", data);
        assert_eq!(state.promissory_notes, before);
    }

    // -- Conservator Procyon ---------------------------------------------------------------------

    #[test]
    fn the_hero_places_frontier_tokens_then_explores_those_under_its_ships() {
        let mut state = game();
        let content = content();
        let hub = crate::fixtures::plain_hub();
        let planetless = crate::exploration::frontier_systems(content, DEFAULT, &hub.galaxy);
        // The hub is built from planet systems; add an anomaly without planets to the ring.
        let nebula = crate::fixtures::a_system_where("nebula");
        let ring = crate::fixtures::hub_with_outer(&nebula);
        let expected: BTreeSet<SystemId> =
            crate::exploration::frontier_systems(content, DEFAULT, &ring.galaxy)
                .into_iter()
                .collect();
        assert!(expected.contains(&SystemId::new(nebula.as_str())));
        assert!(planetless.len() <= expected.len());
        // One token already on the board must not be duplicated or lost.
        state.frontier_tokens.clear();
        let under = SystemId::new(nebula.as_str());
        crate::fixtures::put(&mut state, &under, "cruiser", &a(), 1);
        let decks = state.exploration_decks.get("FRONTIER").map_or(0, Vec::len);
        assert!(decks > 0, "the frontier deck is dealt");
        let mut table = scripted(&[]);
        let used = crate::fixtures::with_context(
            &mut state,
            DEFAULT,
            Some(&ring.galaxy),
            &mut table,
            |ctx| use_leader(ctx, &a(), &LeaderId::new(HERO)),
        );
        assert_eq!(used, Some(true));
        let mut remaining = expected.clone();
        remaining.remove(&under);
        assert_eq!(
            state.frontier_tokens, remaining,
            "the explored token is gone"
        );
        assert_eq!(state.exploration_log.len(), 1, "exactly one exploration");
        assert_eq!(state.exploration_log[0].deck, "FRONTIER");
        assert_eq!(state.exploration_log[0].player, a());
    }

    #[test]
    fn a_failed_exploration_is_not_a_use_and_changes_nothing() {
        let mut state = game();
        let nebula = crate::fixtures::a_system_where("nebula");
        let ring = crate::fixtures::hub_with_outer(&nebula);
        state.frontier_tokens.clear();
        crate::fixtures::put(
            &mut state,
            &SystemId::new(nebula.as_str()),
            "cruiser",
            &a(),
            1,
        );
        state
            .exploration_decks
            .insert("FRONTIER".to_owned(), Vec::new());
        let before = state.clone();
        let mut table = scripted(&[]);
        let used = crate::fixtures::with_context(
            &mut state,
            DEFAULT,
            Some(&ring.galaxy),
            &mut table,
            |ctx| use_leader(ctx, &a(), &LeaderId::new(HERO)),
        );
        assert_eq!(used, Some(false), "the hero did not resolve");
        assert_eq!(state, before, "no token placed, nothing explored");
    }

    #[test]
    fn the_hero_is_the_owners_alone_and_needs_the_map() {
        let state = game();
        let hero = LeaderId::new(HERO);
        assert_eq!(leader_action(&state, content(), &a(), &hero), Some(true));
        assert_eq!(leader_action(&state, content(), &b(), &hero), Some(false));
        assert_eq!(
            leader_action(&state, content(), &a(), &LeaderId::new("solhero")),
            None
        );
        let mut state = state;
        let mut table = scripted(&[]);
        let before = state.clone();
        let used = crate::fixtures::with_context(&mut state, DEFAULT, None, &mut table, |ctx| {
            use_leader(ctx, &a(), &hero)
        });
        assert_eq!(used, None, "no map, nothing happens");
        assert_eq!(state, before);
    }

    // -- Xuange ----------------------------------------------------------------------------------

    #[test]
    fn the_commander_unlocks_only_beside_every_other_player() {
        let mut state =
            crate::fixtures::seated_game(&[("a", FACTION), ("b", "sol"), ("c", "hacan")], DEFAULT);
        let hub = crate::fixtures::plain_hub();
        let commander = LeaderId::new(COMMANDER);
        for system in state.board.keys().cloned().collect::<Vec<_>>() {
            state.system_mut(&system).units.clear();
            state.system_mut(&system).planet_units.clear();
        }
        let unit_at = |state: &mut GameState, id: &str, who: &PlayerId| {
            crate::fixtures::put(state, &SystemId::new(id), "cruiser", who, 1);
        };
        unit_at(&mut state, &hub.outer[0], &a());
        unit_at(&mut state, &hub.outer[1], &b());
        let check = |state: &GameState| {
            commander_unlocked(
                state,
                content(),
                DEFAULT,
                Some(&hub.galaxy),
                &a(),
                &commander,
            )
        };
        assert_eq!(check(&state), Some(false), "only one neighbour");
        unit_at(&mut state, &hub.outer[5], &c());
        assert_eq!(check(&state), Some(true), "both are adjacent");
        assert_eq!(
            commander_unlocked(&state, content(), DEFAULT, None, &a(), &commander),
            Some(false),
            "no map"
        );
        assert_eq!(
            commander_unlocked(
                &state,
                content(),
                DEFAULT,
                Some(&hub.galaxy),
                &a(),
                &LeaderId::new("solcommander")
            ),
            None
        );
        // Through the shared check: the leader unlocks.
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(commander.clone(), ti4_model::state::LeaderStatus::Locked);
        let unlocked =
            crate::leaders::check_unlocks(&mut state, content(), DEFAULT, Some(&hub.galaxy), &a());
        assert!(unlocked.contains(&commander));
    }

    // -- Void Tether -----------------------------------------------------------------------------

    const TETHER_ID: &str = "breakthrough:empyrean:empyreanbt:SYSTEM_ACTIVATED:after";
    const ALLOW_ID: &str = "breakthrough:empyrean:empyreanbt_allow:SYSTEM_ACTIVATED:after";

    fn tether_board() -> (GameState, crate::fixtures::Hub) {
        let mut state = blank(game());
        let hub = crate::fixtures::plain_hub();
        crate::fixtures::put(
            &mut state,
            &SystemId::new(hub.outer[0].as_str()),
            "carrier",
            &a(),
            1,
        );
        grant_bt(&mut state, &a());
        (state, hub)
    }

    #[test]
    fn void_tether_places_a_token_on_a_border_of_the_activated_system() {
        let (mut state, hub) = tether_board();
        let outer = hub.outer[0].clone();
        let id = format!(
            "place|{}|{}",
            normal(&hub.centre, &outer).0,
            normal(&hub.centre, &outer).1
        );
        let mut table = scripted(&[TETHER_ID, &id]);
        activate(&mut state, &hub.galaxy, &mut table, &a(), &hub.centre);
        let borders = tethers(&state, &a());
        assert_eq!(borders, BTreeSet::from([normal(&hub.centre, &outer)]));
        // It is public state.
        assert!(state.faction_marks.contains_key(&tether_key(&a())));
    }

    #[test]
    fn void_tether_closes_the_border_to_other_players_but_not_to_the_empyrean_or_when_allowed() {
        let (mut state, hub) = tether_board();
        let outer = hub.outer[0].clone();
        let across = hub.across(&outer);
        crate::fixtures::put(
            &mut state,
            &SystemId::new(outer.as_str()),
            "destroyer",
            &b(),
            1,
        );
        crate::fixtures::put(
            &mut state,
            &SystemId::new(outer.as_str()),
            "destroyer",
            &a(),
            1,
        );
        store_tethers(
            &mut state,
            &a(),
            &BTreeSet::from([normal(&hub.centre, &outer)]),
        );
        // Movement: across the centre needs the tethered border.
        assert!(!rules(&hub, &state, &b(), &across).can_reach(&outer, 2));
        assert!(rules(&hub, &state, &a(), &across).can_reach(&outer, 2));
        // Adjacency questions.
        let sol =
            crate::movement::PlayerAdjacency::new(&state, content(), DEFAULT, &hub.galaxy, &b());
        let own =
            crate::movement::PlayerAdjacency::new(&state, content(), DEFAULT, &hub.galaxy, &a());
        assert!(!sol.are_adjacent(&hub.centre, &outer));
        assert!(!sol.are_adjacent(&outer, &hub.centre));
        assert!(own.are_adjacent(&hub.centre, &outer));
        // "Unless you allow it": asked after the other player activates, for that activation.
        let mut table = scripted(&[ALLOW_ID]);
        activate(&mut state, &hub.galaxy, &mut table, &b(), &across);
        assert!(rules(&hub, &state, &b(), &across).can_reach(&outer, 2));
        state.activation_seq += 1;
        assert!(!rules(&hub, &state, &b(), &across).can_reach(&outer, 2));
        // A table without the breakthrough is untouched.
        let mut plain = game();
        store_tethers(
            &mut plain,
            &a(),
            &BTreeSet::from([normal(&hub.centre, &outer)]),
        );
        assert!(
            blocked_borders(&plain, &b()).is_empty(),
            "tokens without the breakthrough close nothing"
        );
    }

    #[test]
    fn the_allow_question_is_only_asked_when_a_tether_could_matter() {
        let (mut state, hub) = tether_board();
        // A tether far off the map's reach of the active system and clear of b's units: the
        // activation asks nothing.
        store_tethers(
            &mut state,
            &a(),
            &BTreeSet::from([normal("far-one", "far-two")]),
        );
        let mut table = scripted(&[]);
        let asked = activate(&mut state, &hub.galaxy, &mut table, &b(), &hub.centre);
        assert!(!asked.iter().any(|id| id == ALLOW_ID), "{asked:?}");
        // A tether touching the active system is relevant.
        store_tethers(
            &mut state,
            &a(),
            &BTreeSet::from([normal(&hub.centre, &hub.outer[0])]),
        );
        assert!(tether_relevant(
            &state,
            Some(&hub.galaxy),
            &a(),
            &b(),
            &hub.centre
        ));
        // One far beyond the reach and clear of b's units is not.
        let mut far_state = state.clone();
        store_tethers(
            &mut far_state,
            &a(),
            &BTreeSet::from([normal("far-one", "far-two")]),
        );
        assert!(!tether_relevant(
            &far_state,
            Some(&hub.galaxy),
            &a(),
            &b(),
            &hub.centre
        ));
        // ...until b has a unit on one of its ends.
        let mut owned = far_state.clone();
        crate::fixtures::put(&mut owned, &SystemId::new("far-one"), "destroyer", &b(), 1);
        assert!(tether_relevant(
            &owned,
            Some(&hub.galaxy),
            &a(),
            &b(),
            &hub.centre
        ));
    }

    #[test]
    fn void_tether_moves_a_token_once_all_are_placed_and_needs_a_nearby_unit() {
        let (mut state, hub) = tether_board();
        let outer = hub.outer[0].clone();
        let others: Vec<String> = hub.outer[1..=2].to_vec();
        let mut placed = BTreeSet::new();
        placed.insert(normal(&hub.centre, &others[0]));
        placed.insert(normal(&hub.centre, &others[1]));
        placed.insert(normal(&hub.centre, &hub.outer[3]));
        store_tethers(&mut state, &a(), &placed);
        let options = tether_options(&state, content(), DEFAULT, &hub.galaxy, &a(), &hub.centre);
        assert!(!options.is_empty());
        assert!(
            options.iter().all(|o| matches!(o, Tether::Move { .. })),
            "all tokens placed"
        );
        let from = normal(&hub.centre, &others[0]);
        let to = normal(&hub.centre, &outer);
        let id = format!("move|{}|{}|{}|{}", from.0, from.1, to.0, to.1);
        let mut table = scripted(&[TETHER_ID, &id]);
        activate(&mut state, &hub.galaxy, &mut table, &a(), &hub.centre);
        let now = tethers(&state, &a());
        assert!(!now.contains(&from) && now.contains(&to) && now.len() == 3);

        // No unit or planet of the Empyrean in or next to the system: nothing to do.
        let mut empty = game();
        grant_bt(&mut empty, &a());
        for system in empty.board.keys().cloned().collect::<Vec<_>>() {
            empty.system_mut(&system).units.clear();
            empty.system_mut(&system).planet_units.clear();
            empty.system_mut(&system).planet_control.clear();
        }
        assert!(
            tether_options(&empty, content(), DEFAULT, &hub.galaxy, &a(), &hub.centre).is_empty()
        );
    }

    // -- neutrality ------------------------------------------------------------------------------

    #[test]
    fn a_table_without_the_empyrean_is_untouched_by_every_hook() {
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        let hub = crate::fixtures::plain_hub();
        let before = state.clone();
        let mut table = scripted(&[]);
        let asked = activate(&mut state, &hub.galaxy, &mut table, &a(), &hub.centre);
        assert!(asked.is_empty(), "{asked:?}");
        let mut expected = before;
        expected.activation_seq += 1;
        assert_eq!(state, expected);
        assert!(passable_owners(&state, &a()).is_empty());
        assert!(blocked_borders(&state, &a()).is_empty());
        assert!(!ignores_nebulae(&state, &a()));
        assert_eq!(bonus_for(&state, &a()), 0);
    }
}
