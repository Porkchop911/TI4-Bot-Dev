//! Promissory notes (LRR 69).
//!
//! Ported from the oracle's `engine/promissory.py`. Transactions carried a promissory field long
//! before this existed, but no player owned a note and moving one changed no state — so every
//! note in the game was a name on an offer that did nothing.
//!
//! Support for the Throne is the scoring consequence of that gap, and the reason this is worth
//! having at all: receiving it is worth a victory point, and it is the one note whose *position*
//! scores. It lives in [`GameState::support_holders`] rather than the general note map for
//! exactly that reason, and a great deal of scoring reads it directly.
//!
//! Every note says "then, return this card": a note is a loan, not a sale. [`give_back`] is what
//! makes that true, and it is why parting with one is priced below what receiving it is worth.

use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;

/// Support for the Throne is keyed by its owner rather than by an alias.
pub const SUPPORT_PREFIX: &str = "support:";

/// The notes every player owns a copy of (69.1), by alias.
///
/// Support is handled separately — it is the one whose position scores a point.
pub const GENERIC: &[&str] = &["cf", "ps", "ta", "an"];

/// Generic corpus records are keyed with this prefix in place of an owner faction.
const GENERIC_PREFIX: &str = "<color>_";

/// Public durable mark for a commander ability granted directly to a player.
///
/// The grant belongs to the recipient, not to the faction whose commander card names it.
pub const COMMANDER_ABILITY_PREFIX: &str = "commander_ability:";

fn commander_ability_mark(player: &PlayerId, commander: &str) -> String {
    format!("{COMMANDER_ABILITY_PREFIX}{player}:{commander}")
}

/// Whether a note lives faceup in a play area rather than in hand (69.3).
///
/// Read from the accepted corpus's `playArea` field instead of a hard-coded alias list:
/// a faction record applies only to its own owner, and a generic `<color>` record applies
/// to every owner faction. Unknown aliases are held in hand.
pub fn is_play_area(content: &ContentStore, note: &str) -> bool {
    let Some(owner) = owner_of(note) else {
        return false;
    };
    let alias = alias_of(note);
    let record = content
        .get(ContentType::PromissoryNotes, alias)
        .or_else(|| {
            content.get(
                ContentType::PromissoryNotes,
                &format!("{GENERIC_PREFIX}{alias}"),
            )
        });
    match (record, record.and_then(|r| r.text("faction"))) {
        // A faction record binds to its owner: `convoys` is a play-area note for Hacan's copy
        // and nothing else.
        (Some(record), Some(faction)) => record.flag("playArea") && faction == owner,
        // A generic `<color>` record applies to every owner faction.
        (Some(record), None) => record.flag("playArea"),
        (None, _) => false,
    }
}

/// This owner's Support for the Throne, by the faction name that owns it.
#[must_use]
pub fn support(owner_name: &str) -> String {
    format!("{SUPPORT_PREFIX}{owner_name}")
}

/// A note id: the printed alias, and whose copy it is — the owner's **faction name**, exactly as
/// in the oracle (whose player ids are its factions). Seating a different vocabulary would mint
/// note ids no shared checkpoint has weights for.
#[must_use]
pub fn note_id(alias: &str, owner_name: &str) -> String {
    format!("{alias}:{owner_name}")
}

/// The printed alias a note id carries.
#[must_use]
pub fn alias_of(note: &str) -> &str {
    note.split_once(':').map_or(note, |(alias, _)| alias)
}

/// The faction name a note belongs to, or `None` if it is not a note id at all.
#[must_use]
pub fn owner_of(note: &str) -> Option<String> {
    if let Some(owner) = note.strip_prefix(SUPPORT_PREFIX) {
        return Some(owner.to_owned());
    }
    note.split_once(':').map(|(_, owner)| owner.to_owned())
}

/// The faction a player plays — the identity embedded in every note id that player owns.
#[must_use]
pub fn faction_name(state: &GameState, player: &PlayerId) -> String {
    state
        .player(player)
        .map(|seat| seat.faction.as_str().to_owned())
        .unwrap_or_default()
}

/// The seat playing a given faction name: the first match in seating order.
///
/// Deterministic rather than random. A duplicate-faction table (two seats, one faction) is
/// outside what the oracle can express at all — its player ids *are* factions — so on such a
/// Rust-only scaffold the earlier seat simply shadows the later one's notes.
#[must_use]
pub fn seat_of(state: &GameState, name: &str) -> Option<PlayerId> {
    state
        .seating_order
        .iter()
        .find(|id| {
            state
                .player(id)
                .is_some_and(|seat| seat.faction.as_str() == name)
        })
        .cloned()
}

/// Put every player's own notes in their hand at setup (69.1).
pub fn deal(state: &mut GameState, content: &ContentStore, sources: SourceSet) {
    let mut hands = std::collections::BTreeMap::new();
    for seat in &state.players {
        let mut aliases: Vec<String> = GENERIC.iter().map(|alias| (*alias).to_owned()).collect();
        // Mahact, Hubris: "During setup, purge your Alliance promissory note."
        if crate::factions::mahact::purges_alliance(seat.faction.as_str()) {
            aliases.retain(|alias| alias != "an");
        }
        // A faction's own note, read from the corpus rather than a hard-coded table: a faction
        // whose note this engine does not know simply deals four instead of five.
        aliases.extend(
            ti4_content::factions::get(content, seat.faction.as_str())
                .map(|faction| {
                    faction
                        .promissory_notes()
                        .into_iter()
                        .map(ToOwned::to_owned)
                        .collect::<Vec<String>>()
                })
                .unwrap_or_default(),
        );
        for alias in aliases {
            // The id carries the faction's name; the map value stays the seat that holds it.
            hands.insert(note_id(&alias, seat.faction.as_str()), seat.id.clone());
        }
    }
    let _ = sources;
    state.promissory_notes = hands;
}

/// Give a note to a holder, faceup if that is where the card lives (69.3).
pub fn take(state: &mut GameState, content: &ContentStore, holder: &PlayerId, note: &str) {
    state
        .promissory_notes
        .insert(note.to_owned(), holder.clone());
    // Antivirus is a play-area note, but its text places it ("At the start of a combat: Place this
    // card faceup"): it waits in hand until the holder does (`factions::nekro`).
    if is_play_area(content, note)
        && !is_action_placed(alias_of(note))
        && alias_of(note) != "antivirus"
    {
        state.promissory_faceup.insert(note.to_owned());
    }
}

/// Trade Convoys' alias. Its text opens "ACTION: Place this card faceup in your play area", so
/// unlike the notes that go faceup on receipt it stays in the holder's hand until they spend an
/// action on it ([`play_convoys`]).
const CONVOYS: &str = "convoys";

/// The Empyrean's Blood Pact and Dark Pact open the same way as Trade Convoys ("ACTION: Place
/// this card faceup in your play area"), so they too wait in hand for the ACTION.
const BLOOD_PACT: &str = "blood_pact";
const DARK_PACT: &str = "dark_pact";

/// The Deepwrought's Share Knowledge ("ACTION: Place this card faceup in your play area and gain ...").
const SHARE_KNOWLEDGE: &str = "shareknowledge";

/// The Crimson Rebellion's Sever ("ACTION: place this card face up in your play area, and place the
/// sever token in a system that contains your units ...").
const SEVER: &str = "sever";

/// Whether a note's own text places it faceup with an ACTION rather than on receipt.
fn is_action_placed(alias: &str) -> bool {
    matches!(
        alias,
        CONVOYS | BLOOD_PACT | DARK_PACT | SHARE_KNOWLEDGE | SEVER
    )
}

/// The notes `player` holds in hand (not yet faceup) whose ACTION places them in their play area
/// (Trade Convoys, Blood Pact, Dark Pact), in note-id order. The owner's own copy is not offered:
/// nobody plays a card they own.
#[must_use]
pub fn action_notes_in_hand(state: &GameState, player: &PlayerId) -> Vec<String> {
    state
        .promissory_notes
        .iter()
        .filter(|(note, holder)| {
            *holder == player
                && is_action_placed(alias_of(note))
                && !state.promissory_faceup.contains(*note)
                && owner_of(note).is_some_and(|name| name != faction_name(state, player))
        })
        .map(|(note, _)| note.clone())
        .collect()
}

/// A note's ACTION: place it faceup in `player`'s play area.
///
/// Returns whether it was placed; nothing changes unless `player` holds `note` in hand and its
/// text is an ACTION of this kind.
pub fn play_action_note(state: &mut GameState, player: &PlayerId, note: &str) -> bool {
    if !action_notes_in_hand(state, player)
        .iter()
        .any(|held| held == note)
    {
        return false;
    }
    state.promissory_faceup.insert(note.to_owned());
    true
}

/// The Trade Convoys `player` holds in hand (not yet faceup) and could play with its ACTION.
/// Hacan's own copy is not offered: nobody negotiates with a card they own.
#[must_use]
pub fn convoys_in_hand(state: &GameState, player: &PlayerId) -> Option<String> {
    state
        .promissory_notes
        .iter()
        .find(|(note, holder)| {
            *holder == player
                && alias_of(note) == CONVOYS
                && !state.promissory_faceup.contains(*note)
                && owner_of(note).is_some_and(|name| name != faction_name(state, player))
        })
        .map(|(note, _)| note.clone())
}

/// Trade Convoys' ACTION: place the card faceup in the holder's play area.
///
/// Returns whether a card was placed (nothing changes when the holder has none in hand).
pub fn play_convoys(state: &mut GameState, player: &PlayerId) -> bool {
    let Some(note) = convoys_in_hand(state, player) else {
        return false;
    };
    state.promissory_faceup.insert(note);
    true
}

/// Return a note to its owner once it has done its work.
///
/// Every note in the game says "then, return this card". A note is a loan, not a sale, which is
/// why parting with one is worth less than receiving one.
pub fn give_back(state: &mut GameState, note: &str) {
    let Some(name) = owner_of(note) else {
        return;
    };
    // The oracle cannot name a note whose faction nobody plays, so this engine has no seat to
    // return it to either — treat it as already home rather than mint a ghost player.
    let Some(owner) = seat_of(state, &name) else {
        return;
    };
    if state.promissory_notes.get(note) == Some(&owner) {
        return; // already home
    }
    state.promissory_notes.insert(note.to_owned(), owner);
    state.promissory_faceup.remove(note);
}

/// Who holds a particular note, or `None` if its owner still has it.
#[must_use]
pub fn holder_of(state: &GameState, alias: &str, owner: &PlayerId) -> Option<PlayerId> {
    let name = faction_name(state, owner);
    let note = note_id(alias, &name);
    // Compared by faction, not seat: the id carries the faction, so seats sharing a faction share
    // one note id, and a seat comparison would read a player's own card as lent out.
    match state.promissory_notes.get(&note) {
        Some(holder) if faction_name(state, holder) != name => Some(holder.clone()),
        _ => None,
    }
}

/// Every note this player holds, their own or received, in a stable order.
#[must_use]
pub fn held_by(state: &GameState, player: &PlayerId) -> Vec<String> {
    state
        .promissory_notes
        .iter()
        .filter(|(_, holder)| *holder == player)
        .map(|(note, _)| note.clone())
        .collect()
}

/// Notes this player could put on the table.
///
/// Holding is what makes a note offerable — the oracle prices and offers notes by who holds
/// them, not who owns them; a lent-out note sits in your hand and is yours to sell from there.
/// Two filters narrow that: a note faceup in a play area has already been played and is doing
/// its work where it sits (69.3), and an Alliance conveys a commander ability *only while it is
/// unlocked*, so before then it conveys precisely nothing and the oracle withholds it rather
/// than price a null card.
#[must_use]
pub fn available_notes(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
) -> Vec<String> {
    let mut notes = Vec::new();
    for note in held_by(state, player) {
        if state.promissory_faceup.contains(&note) {
            continue;
        }
        let Some(owner_name) = owner_of(&note) else {
            continue;
        };
        if alias_of(&note) == "an" {
            // Nobody plays the faction an Alliance names on a duplicate-free table, and with no
            // seat there is no commander to unlock — withhold it in that case as well.
            let Some(owner) = seat_of(state, &owner_name) else {
                continue;
            };
            if !commander_unlocked(state, content, &owner) {
                continue;
            }
        }
        notes.push(note);
    }
    notes
}

/// Whether `receiver` may be given `note`: Mahact's Hubris says "Other players cannot give you
/// their 'Alliance' promissory note." Every transfer between players asks this of the receiver.
#[must_use]
pub fn may_receive(state: &GameState, receiver: &PlayerId, note: &str) -> bool {
    !(alias_of(note) == "an" && crate::factions::mahact::is_mahact(state, receiver))
}

/// This player's Support for the Throne, if they still hold it.
#[must_use]
pub fn available_support(state: &GameState, player: &PlayerId) -> Option<String> {
    let name = faction_name(state, player);
    (!state.support_holders.contains_key(player)).then(|| support(&name))
}

/// Put another player's Support faceup in a play area and award its point.
///
/// Returns `false` when the note cannot move: nobody may hold their own Support, and a Support
/// already lent out cannot be lent again.
pub fn receive(state: &mut GameState, holder: &PlayerId, note: &str) -> bool {
    let Some(name) = owner_of(note) else {
        return false;
    };
    // Same ghost-seat guard as [`give_back`]: the oracle cannot name a note whose faction
    // nobody plays.
    let Some(owner) = seat_of(state, &name) else {
        return false;
    };
    if &owner == holder || state.support_holders.contains_key(&owner) {
        return false;
    }
    state.support_holders.insert(owner, holder.clone());
    crate::objectives::adjust_victory_points(state, holder, 1, "support_for_the_throne");
    true
}

/// Return a Support to its owner, and take the point back with it.
///
/// The point follows the card. A holder who kept the victory point after the note went home
/// would be scoring for something they no longer have.
pub fn return_support(state: &mut GameState, owner: &PlayerId) -> bool {
    let Some(holder) = state.support_holders.remove(owner) else {
        return false;
    };
    let before = state.player(&holder).map(|seat| seat.victory_points);
    if let Some(seat) = state.player_mut(&holder) {
        seat.victory_points = (seat.victory_points - 1).max(0);
    }
    // The ledger follows the point too. `receive` records the +1, and every VP-source report
    // reads the ledger rather than the seat total, so a return that only touched the total left
    // those reports crediting a note the holder had already given back.
    let after = state.player(&holder).map(|seat| seat.victory_points);
    if let (Some(before), Some(after)) = (before, after)
        && after != before
    {
        state.note_vp(
            &holder,
            (after - before).try_into().unwrap_or(-1),
            "support_for_the_throne",
        );
    }
    true
}

/// Owners whose Support for the Throne is spent by `activator` activating `system`.
///
/// The printed card (69.3): "When you activate a system that contains 1 or more of the
/// `<color>` player's units... lose 1 victory point and return this card to the `<color>`
/// player." Unlike Ceasefire, whose card names the *move* that follows activation (see
/// [`denies_movement_into`]), Support's trigger is the activation itself — so this reads
/// whatever the system already holds at that step rather than waiting on a move.
#[must_use]
pub fn support_triggered_by_activation(
    state: &GameState,
    activator: &PlayerId,
    system: &ti4_model::id::SystemId,
) -> Vec<PlayerId> {
    let board = state.system_state(system);
    let mut present: std::collections::BTreeSet<&PlayerId> =
        board.units.iter().map(|unit| &unit.owner).collect();
    present.extend(
        board
            .planet_units
            .values()
            .flatten()
            .map(|unit| &unit.owner),
    );
    state
        .support_holders
        .iter()
        .filter(|(_, holder)| *holder == activator)
        .filter(|(owner, _)| present.contains(owner))
        .map(|(owner, _)| owner.clone())
        .collect()
}

/// Return every Support triggered by activating `system`, and report whose it was.
///
/// Wraps [`support_triggered_by_activation`] and [`return_support`] the way [`use_ceasefire`]
/// wraps [`denies_movement_into`] and [`give_back`]: one call for whatever drives the
/// `SYSTEM_ACTIVATED` step, so an activator holding two lent-out Supports against owners both
/// present pays both back in one pass instead of making the caller loop by hand.
pub fn spend_support_on_activation(
    state: &mut GameState,
    activator: &PlayerId,
    system: &ti4_model::id::SystemId,
) -> Vec<PlayerId> {
    let owners = support_triggered_by_activation(state, activator, system);
    for owner in &owners {
        return_support(state, owner);
    }
    // Alliance has the same trigger: "When you activate a system that contains 1 or more of the
    // <color> player's units, return this card to the <color> player." Returned here so every
    // activation path that spends Support also sends Alliances home.
    for note in alliances_returned_by_activation(state, activator, system) {
        give_back(state, &note);
    }
    // Trade Convoys: "If you activate a system that contains 1 or more of the Hacan player's
    // units, return this card to the Hacan player."
    for note in convoys_returned_by_activation(state, activator, system) {
        give_back(state, &note);
    }
    // Blood Pact and Dark Pact: "If you activate a system that contains 1 or more of the Empyrean
    // player's units, return this card to the Empyrean player."
    // Antivirus: "If you activate a system that contains 1 or more of the Nekro player's units,
    // return this card to the Nekro player."
    for alias in [BLOOD_PACT, DARK_PACT, "antivirus"] {
        for note in faceup_returned_by_activation(state, activator, system, alias) {
            give_back(state, &note);
        }
    }
    owners
}

/// The faceup Trade Convoys `activator` holds whose owner has a unit in `system`.
#[must_use]
pub fn convoys_returned_by_activation(
    state: &GameState,
    activator: &PlayerId,
    system: &ti4_model::id::SystemId,
) -> Vec<String> {
    faceup_returned_by_activation(state, activator, system, CONVOYS)
}

/// The faceup Alliances `activator` holds whose owner has a unit in `system`.
#[must_use]
pub fn alliances_returned_by_activation(
    state: &GameState,
    activator: &PlayerId,
    system: &ti4_model::id::SystemId,
) -> Vec<String> {
    faceup_returned_by_activation(state, activator, system, "an")
}

/// The faceup notes of `alias` `activator` holds whose owner has a unit in `system`.
fn faceup_returned_by_activation(
    state: &GameState,
    activator: &PlayerId,
    system: &ti4_model::id::SystemId,
    alias: &str,
) -> Vec<String> {
    let board = state.system_state(system);
    let present: std::collections::BTreeSet<&PlayerId> = board
        .units
        .iter()
        .chain(board.planet_units.values().flatten())
        .map(|unit| &unit.owner)
        .collect();
    state
        .promissory_notes
        .iter()
        .filter(|(note, holder)| {
            *holder == activator
                && alias_of(note) == alias
                && state.promissory_faceup.contains(*note)
        })
        .filter(|(note, _)| {
            owner_of(note)
                .and_then(|name| seat_of(state, &name))
                .is_some_and(|owner| owner != *activator && present.contains(&owner))
        })
        .map(|(note, _)| note.clone())
        .collect()
}

/// Trade Convoys: its holder may transact with the whole table, not only their neighbours.
///
/// Only while the card is faceup in a play area — that is where it lives once lent (69.3), and
/// a note still in hand has not been played at all.
#[must_use]
pub fn reaches_anyone(state: &GameState, player: &PlayerId) -> bool {
    state.promissory_notes.iter().any(|(note, holder)| {
        holder == player && alias_of(note) == "convoys" && state.promissory_faceup.contains(note)
    })
}

/// Ceasefire: the holder stops the owner moving into a system they occupy.
///
/// Checked at the movement step rather than at activation, because that is the moment the denial
/// actually bites — the card triggers on the owner activating a system holding the holder's
/// units, and what it denies is the move that follows.
#[must_use]
pub fn denies_movement_into(
    state: &GameState,
    mover: &PlayerId,
    system: &ti4_model::id::SystemId,
) -> bool {
    let name = faction_name(state, mover);
    let board = state.system_state(system);
    let mut present: std::collections::BTreeSet<&PlayerId> =
        board.units.iter().map(|unit| &unit.owner).collect();
    present.extend(
        board
            .planet_units
            .values()
            .flatten()
            .map(|unit| &unit.owner),
    );

    let note = note_id("cf", &name);
    state
        .promissory_notes
        .get(&note)
        .is_some_and(|holder| lent_out(state, holder, mover, &name) && present.contains(holder))
}

/// Whether `holder` holds `owner_name`'s note as a loan rather than as its own copy.
///
/// Seats without distinct factions share a name, so their notes share an id; a holder of that same
/// faction holds its own copy, which denies nothing.
fn lent_out(state: &GameState, holder: &PlayerId, mover: &PlayerId, owner_name: &str) -> bool {
    holder != mover && faction_name(state, holder) != owner_name
}

/// Spend the Ceasefire that just denied a movement, returning it to its owner.
///
/// A note is a loan: it does its work once and goes home. Leaving it in the holder's play area
/// would deny every future move into that system for the rest of the game.
pub fn use_ceasefire(state: &mut GameState, mover: &PlayerId) -> bool {
    let name = faction_name(state, mover);
    let note = note_id("cf", &name);
    let held = state
        .promissory_notes
        .get(&note)
        .is_some_and(|holder| lent_out(state, holder, mover, &name));
    if held {
        give_back(state, &note);
    }
    held
}

/// Owners whose commander this player may use through a faceup Alliance (69.3).
///
/// Only unlocked commanders count, which is what the card says.
#[must_use]
pub fn commander_ability_from(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
) -> Vec<PlayerId> {
    let mut found: Vec<PlayerId> = state
        .promissory_notes
        .iter()
        .filter(|(note, holder)| {
            *holder == player && alias_of(note) == "an" && state.promissory_faceup.contains(*note)
        })
        .filter_map(|(note, _)| owner_of(note).and_then(|name| seat_of(state, &name)))
        .filter(|owner| owner != player)
        .filter(|owner| commander_unlocked(state, content, owner))
        .collect();
    found.sort();
    found.dedup();
    found
}

/// Whether this player's commander is unlocked, which is all Alliance conveys.
#[must_use]
pub fn commander_unlocked(state: &GameState, content: &ContentStore, owner: &PlayerId) -> bool {
    state.player(owner).is_some_and(|seat| {
        seat.leaders.iter().any(|(leader, status)| {
            *status == ti4_model::state::LeaderStatus::Unlocked
                && crate::leaders::kind_of(content, leader)
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("commander"))
        })
    })
}

/// Whether `player` currently has the named commander's ability.
///
/// This combines the player's own unlocked commander, an ordinary faceup Alliance from a seated
/// owner whose exact commander is unlocked, and durable public grants such as Yin's breakthrough.
/// A direct grant does not require the named faction to be seated or its commander to be unlocked.
#[must_use]
pub fn has_commander_ability(state: &GameState, player: &PlayerId, commander: &str) -> bool {
    if state
        .faction_marks
        .contains_key(&commander_ability_mark(player, commander))
        || state.player(player).is_some_and(|seat| {
            seat.leaders.get(&ti4_model::id::LeaderId::new(commander))
                == Some(&ti4_model::state::LeaderStatus::Unlocked)
        })
    {
        return true;
    }

    // Mahact, Imperia: another player's commander while their token is in the fleet pool.
    if crate::factions::mahact::imperia_grants(state, player, commander) {
        return true;
    }

    state.promissory_notes.iter().any(|(note, holder)| {
        holder == player
            && alias_of(note) == "an"
            && state.promissory_faceup.contains(note)
            && owner_of(note)
                .and_then(|owner| seat_of(state, &owner))
                .is_some_and(|owner| {
                    owner != *player
                        && state.player(&owner).is_some_and(|seat| {
                            seat.leaders.get(&ti4_model::id::LeaderId::new(commander))
                                == Some(&ti4_model::state::LeaderStatus::Unlocked)
                        })
                })
    })
}

/// Grant a named corpus commander ability directly to a player as a public durable mark.
///
/// This intentionally does not require a seated faction owner or unlocked commander record.
/// Returns false for unknown/non-commander records, unknown recipients, and duplicate grants.
pub fn grant_commander_ability(
    state: &mut GameState,
    content: &ContentStore,
    player: &PlayerId,
    commander: &str,
) -> bool {
    if state.player(player).is_none() {
        return false;
    }
    let is_commander = content
        .get(ContentType::Leaders, commander)
        .and_then(|record| record.text("type"))
        .is_some_and(|kind| kind.eq_ignore_ascii_case("commander"));
    if !is_commander {
        return false;
    }
    let mark = commander_ability_mark(player, commander);
    if state.faction_marks.contains_key(&mark) {
        return false;
    }
    state.faction_marks.insert(mark, "granted".to_owned());
    true
}

/// Military Support: "At the start of the Sol player's turn: Remove 1 token from the Sol player's
/// strategy pool, if able, and return it to their reinforcements. Then, you may place 2 infantry
/// from your reinforcements on any planet you control. Then, return this card to the Sol player."
///
/// The card resolves whenever its window comes (operator ruling, 2026-10-06: only clauses that
/// say "may" are optional). The token removal ("if able") and the return always happen; the only
/// choice is the infantry: the holder picks a planet they control or declines. Nothing is asked
/// when the holder has no planet or the box holds no infantry for them. The holder's own infantry
/// are placed (their upgrade, if they own it), within what the box holds. The placement question
/// comes first so an illegal answer leaves everything unchanged.
///
/// # Errors
/// [`crate::choice::IllegalChoice`] when the holder answers with something not offered; nothing
/// has changed.
pub fn turn_started(
    context: &mut crate::timing::TimingContext<'_>,
    player: &PlayerId,
) -> Result<bool, crate::choice::IllegalChoice> {
    let Some(holder) = holder_of(context.state, "ms", player) else {
        return Ok(false);
    };
    let placed = crate::action_cards::place_units_choosing(
        context,
        &holder,
        "infantry",
        2,
        crate::action_cards::PlacementTarget::ControlledPlanet,
        None,
        true,
        "ms",
        crate::action_cards::PlacementLimits::Respect,
    )?;
    let _ = placed; // placing is the card's only "may"; the rest resolves either way
    if let Some(seat) = context.state.player_mut(player)
        && seat.tokens(ti4_model::state::TokenPool::Strategic) > 0
    {
        seat.gain_token_uncapped(ti4_model::state::TokenPool::Strategic, -1);
        crate::supply::note_strategy_token_spent(context.state, player, "military_support");
    }
    let name = faction_name(context.state, player);
    give_back(context.state, &note_id("ms", &name));
    Ok(true)
}

/// Trade Agreement: "When the <color> player replenishes commodities: The <color> player gives you
/// all of their commodities. Then, return this card to the <color> player."
///
/// Call wherever `player`'s commodities are replenished. Commodities given to another player
/// become that player's trade goods, which is where they land. Returns the holder that was paid.
/// Priced for trading since the port, it never paid out, so a traded Trade Agreement bought
/// nothing.
pub fn trade_agreement_on_replenish(state: &mut GameState, player: &PlayerId) -> Option<PlayerId> {
    // Every replenish site calls this, so it is also where the replenish is announced (staged) for
    // Cabal's The Stillness of Stars, before this card can take the commodities.
    crate::factions::cabal::note_replenished(state, player);
    let holder = holder_of(state, "ta", player)?;
    let given = state.player(player).map_or(0, |seat| seat.commodities);
    if given <= 0 {
        return None;
    }
    if let Some(seat) = state.player_mut(player) {
        seat.commodities = 0;
    }
    if let Some(seat) = state.player_mut(&holder) {
        seat.trade_goods += given;
    }
    crate::supply::note_trade_goods_gained(state, &holder, given, "trade_agreement");
    let name = faction_name(state, player);
    give_back(state, &note_id("ta", &name));
    Some(holder)
}

/// How many of other factions' `alias` notes this player holds.
#[must_use]
pub fn held_foreign(state: &GameState, player: &PlayerId, alias: &str) -> usize {
    let own = faction_name(state, player);
    state
        .promissory_notes
        .iter()
        .filter(|(note, holder)| {
            *holder == player
                && alias_of(note) == alias
                && owner_of(note).is_some_and(|owner| owner != own)
        })
        .count()
}

/// Return every `alias` note held by a player other than its owner.
pub fn return_all_foreign(state: &mut GameState, alias: &str) {
    let lent: Vec<String> = state
        .promissory_notes
        .iter()
        .filter(|(note, holder)| {
            // By faction, as in `holder_of`: seats sharing a faction share the note id.
            alias_of(note) == alias
                && owner_of(note).is_some_and(|owner| owner != faction_name(state, holder))
        })
        .map(|(note, _)| note.clone())
        .collect();
    for note in lent {
        give_back(state, &note);
    }
}

/// What a particular Trade Agreement is worth: its owner's commodity value.
///
/// The card is generic only on paper. It hands over everything its owner replenishes — six
/// commodities for Hacan against two for Letnev — so pricing them alike would make the single
/// most valuable card a trading faction owns cost what the least valuable one does.
#[must_use]
pub fn trade_agreement_worth(state: &GameState, content: &ContentStore, note: &str) -> f64 {
    let _ = state; // kept for call-site symmetry with the oracle's game-taking form
    let default = 2.5;
    let Some(name) = owner_of(note) else {
        return default;
    };
    // "generic" is this engine's scaffolding faction (the oracle's player ids are its factions),
    // and a name the corpus does not know prices at the flat rate rather than panicking.
    if name == "generic" {
        return default;
    }
    let Some(faction) = ti4_content::factions::get(content, &name) else {
        return default;
    };
    // Slightly under face value: the commodities arrive only when the owner next replenishes,
    // which may be a round away or never if the game ends first.
    0.8 * f64::from(faction.commodities())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::game;
    use ti4_model::content_types::POK;
    use ti4_model::id::{FactionId, LeaderId};
    use ti4_model::state::LeaderStatus;

    fn a() -> PlayerId {
        PlayerId::new("a")
    }
    fn b() -> PlayerId {
        PlayerId::new("b")
    }

    /// Note scaffolding: distinct factions per seat, dealt *after* seating. Two "generic" seats
    /// would mint colliding ids (`cf:generic` twice) — a table the oracle cannot build at all,
    /// because its player ids are its factions.
    fn game_hacan_jolnar() -> GameState {
        let mut state = game(&["a", "b"]);
        state.player_mut(&a()).unwrap().faction = FactionId::new("hacan");
        state.player_mut(&b()).unwrap().faction = FactionId::new("jolnar");
        deal(&mut state, ContentStore::embedded(), POK);
        state
    }

    #[test]
    fn a_note_id_carries_whose_copy_it_is() {
        // The owner is the FACTION NAME (the oracle's player id), not the seat: two engines must
        // mint one shared vocabulary from the same table.
        assert_eq!(note_id("cf", "hacan"), "cf:hacan");
        assert_eq!(alias_of("cf:hacan"), "cf");
        assert_eq!(owner_of("cf:hacan"), Some("hacan".to_owned()));
        assert_eq!(owner_of(&support("jolnar")), Some("jolnar".to_owned()));
        assert_eq!(owner_of("nonsense"), None);
    }

    #[test]
    fn setup_deals_every_player_their_own_notes() {
        let state = game_hacan_jolnar();
        let content = ContentStore::embedded();
        let mine = available_notes(&state, content, &a());
        assert!(
            mine.iter().any(|n| n == "cf:hacan"),
            "own Ceasefire: {mine:?}"
        );
        assert!(mine.iter().any(|n| n == "convoys:hacan"));
        assert!(!mine.iter().any(|n| n == "cf:jolnar"), "not b's copy");
        assert!(
            !mine.iter().any(|n| n == "an:hacan"),
            "Alliance waits for the commander"
        );
    }

    #[test]
    fn deal_mints_faction_notes_only_once_the_faction_is_known() {
        // `start_game` deals before factions are seated: on a generic table both seats collide
        // into one key, and no faction note exists at all. Seating then redealing is what the
        // rollout does, and it is idempotent — no note has moved yet at setup.
        let mut state = game(&["a", "b"]);
        let content = ContentStore::embedded();
        assert_eq!(
            state.promissory_notes.len(),
            4,
            "two generic seats share four keys"
        );
        assert!(
            state.promissory_notes.values().all(|holder| *holder == b()),
            "and the later seat shadows the first"
        );

        state.player_mut(&a()).unwrap().faction = FactionId::new("hacan");
        state.player_mut(&b()).unwrap().faction = FactionId::new("jolnar");
        deal(&mut state, content, POK);

        assert!(state.promissory_notes.contains_key("convoys:hacan"));
        assert!(state.promissory_notes.contains_key("ra:jolnar"));
        assert_eq!(state.promissory_notes.len(), 10, "five per faction");
    }

    #[test]
    fn a_lent_out_note_is_offerable_by_whoever_holds_it() {
        // The oracle prices and offers notes by who HOLDS them: a loan sits in your hand and is
        // yours to sell from there. Rust's former ownership filter was an engine-local rule,
        // retired with the identity alignment.
        let mut state = game_hacan_jolnar();
        let content = ContentStore::embedded();

        take(&mut state, content, &b(), "cf:hacan");

        assert_eq!(holder_of(&state, "cf", &a()), Some(b()));
        assert!(
            !available_notes(&state, content, &a())
                .iter()
                .any(|n| n == "cf:hacan"),
            "a no longer holds it"
        );
        assert!(
            available_notes(&state, content, &b())
                .iter()
                .any(|n| n == "cf:hacan"),
            "but b may sell what it holds"
        );
    }

    #[test]
    fn a_note_is_a_loan_and_goes_home() {
        let mut state = game_hacan_jolnar();
        let content = ContentStore::embedded();
        take(&mut state, content, &b(), "cf:hacan");

        give_back(&mut state, "cf:hacan");

        assert_eq!(holder_of(&state, "cf", &a()), None, "back with its owner");
        assert!(
            available_notes(&state, content, &a())
                .iter()
                .any(|n| n == "cf:hacan")
        );
    }

    #[test]
    fn a_faceup_note_sits_in_the_play_area() {
        // 69.3: Alliance and Trade Convoys are played faceup; a note in a play area is public,
        // and it stops being offerable while it does its work there. Ceasefire, by contrast, is
        // held.
        let mut state = game_hacan_jolnar();
        let content = ContentStore::embedded();

        take(&mut state, content, &b(), "an:hacan");
        assert!(state.promissory_faceup.contains("an:hacan"));
        assert!(
            !available_notes(&state, content, &b())
                .iter()
                .any(|n| n == "an:hacan"),
            "out of hand while it sits faceup"
        );

        // Trade Convoys is Hacan's card: lent to b, it sits in b's hand until b spends the ACTION
        // that places it faceup. (A key like `convoys:jolnar` cannot exist — Jolnar owns no such
        // note.)
        take(&mut state, content, &b(), "convoys:hacan");
        assert!(!state.promissory_faceup.contains("convoys:hacan"));
        assert!(play_convoys(&mut state, &b()));
        assert!(state.promissory_faceup.contains("convoys:hacan"));
        assert!(
            !available_notes(&state, content, &b())
                .iter()
                .any(|n| n == "convoys:hacan"),
            "convoys does its work where it sits"
        );

        take(&mut state, content, &b(), "cf:hacan");
        assert!(
            !state.promissory_faceup.contains("cf:hacan"),
            "Ceasefire is held"
        );

        give_back(&mut state, "an:hacan");
        assert!(
            !state.promissory_faceup.contains("an:hacan"),
            "and it leaves the play area when it goes home"
        );
    }

    #[test]
    fn an_alliance_goes_home_when_its_holder_activates_a_system_with_the_owners_units() {
        let mut state = game_hacan_jolnar();
        let content = ContentStore::embedded();
        take(&mut state, content, &b(), "an:hacan");
        assert!(state.promissory_faceup.contains("an:hacan"));
        let system = ti4_model::id::SystemId::new("alliance-test");
        // No Hacan unit there: the card stays.
        assert!(spend_support_on_activation(&mut state, &b(), &system).is_empty());
        assert_eq!(state.promissory_notes.get("an:hacan"), Some(&b()));
        state
            .system_mut(&system)
            .units
            .push(ti4_model::units::Unit::new(
                ti4_model::id::UnitTypeId::new("cruiser"),
                a(),
            ));
        spend_support_on_activation(&mut state, &b(), &system);
        assert_eq!(
            state.promissory_notes.get("an:hacan"),
            Some(&a()),
            "returned"
        );
        assert!(!state.promissory_faceup.contains("an:hacan"));
    }

    #[test]
    fn alliance_is_withheld_until_the_commander_unlocks() {
        // Before that, the note conveys precisely nothing — and a note worth nothing is what a
        // search learns to sell.
        let mut state = game_hacan_jolnar();
        let content = ContentStore::embedded();

        assert!(
            !available_notes(&state, content, &a())
                .iter()
                .any(|n| n == "an:hacan"),
            "commander still locked"
        );

        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new("hacancommander"), LeaderStatus::Unlocked);

        assert!(
            available_notes(&state, content, &a())
                .iter()
                .any(|n| n == "an:hacan")
        );
    }

    #[test]
    fn commander_ability_checks_exact_own_commander_and_faceup_alliance() {
        let mut state = game_hacan_jolnar();
        let content = ContentStore::embedded();

        assert!(!has_commander_ability(&state, &a(), "hacancommander"));
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new("hacancommander"), LeaderStatus::Unlocked);
        assert!(has_commander_ability(&state, &a(), "hacancommander"));
        assert!(!has_commander_ability(&state, &a(), "jolnarcommander"));

        // Ordinary Alliance still requires the owner's exact commander to be unlocked.
        take(&mut state, content, &a(), "an:jolnar");
        assert!(!has_commander_ability(&state, &a(), "jolnarcommander"));
        state
            .player_mut(&b())
            .unwrap()
            .leaders
            .insert(LeaderId::new("jolnarcommander"), LeaderStatus::Unlocked);
        assert!(has_commander_ability(&state, &a(), "jolnarcommander"));

        // A facedown or merely foreign held note conveys nothing.
        state.promissory_faceup.remove("an:jolnar");
        assert!(!has_commander_ability(&state, &a(), "jolnarcommander"));
        state.promissory_faceup.insert("an:jolnar".to_owned());
        state.promissory_notes.insert("an:jolnar".to_owned(), b());
        assert!(!has_commander_ability(&state, &a(), "jolnarcommander"));
    }

    #[test]
    fn direct_commander_grant_is_public_durable_and_does_not_mutate_owners() {
        use ti4_model::view::mark_visible_to;

        let mut state = game_hacan_jolnar();
        let content = ContentStore::embedded();
        let key = commander_ability_mark(&a(), "ghostcommander");
        let before_owner_leaders = state.player(&b()).unwrap().leaders.clone();

        // Ghost is not seated; a direct grant is still valid and does not unlock a borrowed seat.
        assert!(grant_commander_ability(
            &mut state,
            content,
            &a(),
            "ghostcommander"
        ));
        assert!(has_commander_ability(&state, &a(), "ghostcommander"));
        assert_eq!(state.player(&b()).unwrap().leaders, before_owner_leaders);
        assert_eq!(
            state.faction_marks.get(&key).map(String::as_str),
            Some("granted")
        );
        assert!(mark_visible_to(&key, &a()));
        assert!(mark_visible_to(&key, &b()));
        assert!(!grant_commander_ability(
            &mut state,
            content,
            &a(),
            "ghostcommander"
        ));

        let serialized = serde_json::to_vec(&state).unwrap();
        let restored: GameState = serde_json::from_slice(&serialized).unwrap();
        assert!(has_commander_ability(&restored, &a(), "ghostcommander"));
    }

    #[test]
    fn direct_commander_grants_reject_invalid_records_and_recipients() {
        let mut state = game_hacan_jolnar();
        let content = ContentStore::embedded();
        assert!(!grant_commander_ability(
            &mut state,
            content,
            &a(),
            "hacanhero"
        ));
        assert!(!grant_commander_ability(
            &mut state,
            content,
            &a(),
            "missingcommander"
        ));
        assert!(!grant_commander_ability(
            &mut state,
            content,
            &PlayerId::new("missing"),
            "ghostcommander"
        ));
        assert!(state.faction_marks.is_empty());
    }

    #[test]
    fn a_ceasefire_denies_a_move_only_where_its_holder_stands() {
        let mut state = game_hacan_jolnar();
        let content = ContentStore::embedded();
        let (system, _) = crate::fixtures::a_placed_planet();
        take(&mut state, content, &b(), "cf:hacan");

        assert!(
            !denies_movement_into(&state, &a(), &system),
            "b is not there yet"
        );

        crate::fixtures::put(&mut state, &system, "cruiser", &b(), 1);
        assert!(
            denies_movement_into(&state, &a(), &system),
            "now the holder occupies it"
        );
        assert!(
            !denies_movement_into(&state, &b(), &system),
            "and it never denies its own holder"
        );
    }

    #[test]
    fn a_spent_ceasefire_goes_home_and_stops_denying() {
        // A note is a loan. Left in the play area it would deny every future move into that
        // system for the rest of the game.
        let mut state = game_hacan_jolnar();
        let content = ContentStore::embedded();
        let (system, _) = crate::fixtures::a_placed_planet();
        take(&mut state, content, &b(), "cf:hacan");
        crate::fixtures::put(&mut state, &system, "cruiser", &b(), 1);

        assert!(use_ceasefire(&mut state, &a()));

        assert!(!denies_movement_into(&state, &a(), &system), "spent");
        assert_eq!(holder_of(&state, "cf", &a()), None, "and back with a");
        assert!(
            !use_ceasefire(&mut state, &a()),
            "and cannot be spent twice"
        );
    }

    #[test]
    fn trade_convoys_reach_the_whole_table_only_when_faceup() {
        // Trade Convoys is Hacan's card: the ability follows the note to whoever holds it faceup.
        let mut state = game_hacan_jolnar();
        let content = ContentStore::embedded();

        assert!(
            !reaches_anyone(&state, &b()),
            "a note in hand is not in play"
        );

        take(&mut state, content, &b(), "convoys:hacan");
        assert!(
            !reaches_anyone(&state, &b()),
            "ACTION: it is placed faceup by its holder, not on receipt"
        );
        assert_eq!(
            convoys_in_hand(&state, &b()).as_deref(),
            Some("convoys:hacan")
        );
        assert_eq!(convoys_in_hand(&state, &a()), None, "never its own owner");

        assert!(play_convoys(&mut state, &b()));
        assert!(reaches_anyone(&state, &b()));
        assert!(!reaches_anyone(&state, &a()), "and only for its holder");
        assert!(!play_convoys(&mut state, &b()), "nothing left to place");
    }

    #[test]
    fn trade_convoys_return_when_the_holder_activates_a_system_with_hacan_units() {
        let mut state = game_hacan_jolnar();
        let content = ContentStore::embedded();
        let (system, _) = crate::fixtures::a_placed_planet();
        take(&mut state, content, &b(), "convoys:hacan");
        assert!(play_convoys(&mut state, &b()));

        // No Hacan unit in the system: the card stays.
        crate::fixtures::put(&mut state, &system, "cruiser", &b(), 1);
        spend_support_on_activation(&mut state, &b(), &system);
        assert_eq!(holder_of(&state, "convoys", &a()), Some(b()));
        assert!(state.promissory_faceup.contains("convoys:hacan"));

        // A Hacan unit is there: it goes home, and out of the play area.
        crate::fixtures::put(&mut state, &system, "cruiser", &a(), 1);
        spend_support_on_activation(&mut state, &b(), &system);
        assert_eq!(holder_of(&state, "convoys", &a()), None);
        assert!(!state.promissory_faceup.contains("convoys:hacan"));
        assert!(!reaches_anyone(&state, &b()));
    }

    #[test]
    fn a_trade_agreement_is_worth_what_its_owner_replenishes() {
        // Six commodities for one faction against two for another: pricing them alike makes the
        // most valuable card cost what the least valuable one does. Worth now keys off the name
        // in the id — no seat lookup at all.
        let content = ContentStore::embedded();
        let state = game_hacan_jolnar();
        let rich = ti4_content::factions::catalogue(content, POK)
            .iter()
            .max_by_key(|(_, faction)| faction.commodities())
            .map(|(alias, _)| (*alias).to_owned());
        let poor = ti4_content::factions::catalogue(content, POK)
            .iter()
            .filter(|(_, faction)| faction.commodities() > 0)
            .min_by_key(|(_, faction)| faction.commodities())
            .map(|(alias, _)| (*alias).to_owned());
        let (Some(rich), Some(poor)) = (rich, poor) else {
            return;
        };

        let dear = trade_agreement_worth(&state, content, &note_id("ta", &rich));
        let cheap = trade_agreement_worth(&state, content, &note_id("ta", &poor));

        assert!(dear > cheap, "{dear} should beat {cheap}");
    }

    /// Sol (a) with Military Support lent to b, who controls one planet.
    fn military_support_fixture() -> (GameState, ti4_model::id::SystemId, ti4_model::id::PlanetId) {
        let content = ContentStore::embedded();
        let mut state = game(&["a", "b"]);
        // Military Support belongs to Sol, so seat a as sol and deal with factions known.
        state.player_mut(&a()).unwrap().faction = FactionId::new("sol");
        state.player_mut(&b()).unwrap().faction = FactionId::new("jolnar");
        deal(&mut state, content, POK);
        let (system, planet) = crate::fixtures::a_placed_planet();
        state.system_mut(&system).set_control(planet.clone(), b());
        take(&mut state, content, &b(), "ms:sol");
        (state, system, planet)
    }

    fn infantry_on(
        state: &GameState,
        system: &ti4_model::id::SystemId,
        planet: &ti4_model::id::PlanetId,
    ) -> Vec<String> {
        state
            .system_state(system)
            .planet_units
            .get(planet)
            .map(|units| {
                units
                    .iter()
                    .filter(|u| u.owner == b())
                    .map(|u| u.type_id.to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn run_turn_started(state: &mut GameState, answers: &[&str]) -> bool {
        let mut table = crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new(
            answers.iter().map(|s| (*s).to_owned()),
        )));
        crate::fixtures::with_context(state, POK, None, &mut table, |context| {
            turn_started(context, &a()).expect("a legal answer")
        })
    }

    #[test]
    fn military_support_plants_two_of_the_holders_infantry_and_the_note_goes_home() {
        let (mut state, system, planet) = military_support_fixture();
        let tokens = state
            .player(&a())
            .unwrap()
            .tokens(ti4_model::state::TokenPool::Strategic);
        assert!(tokens > 0, "the fixture gives Sol a token to lose");

        assert!(run_turn_started(
            &mut state,
            &[&format!("{system}|{planet}")]
        ));

        let landed = infantry_on(&state, &system, &planet);
        assert_eq!(landed.len(), 2, "two infantry, for the holder");
        assert!(
            landed.iter().all(|id| id.contains("infantry")),
            "{landed:?}"
        );
        assert_eq!(
            holder_of(&state, "ms", &a()),
            None,
            "and the note went home"
        );
        assert_eq!(
            state
                .player(&a())
                .unwrap()
                .tokens(ti4_model::state::TokenPool::Strategic),
            tokens - 1,
            "Sol paid a strategy token"
        );
    }

    #[test]
    fn declining_the_infantry_still_takes_the_token_and_sends_the_card_home() {
        // Only the infantry is a "may": the token removal and the return happen regardless.
        let (mut state, system, planet) = military_support_fixture();
        let tokens = state
            .player(&a())
            .unwrap()
            .tokens(ti4_model::state::TokenPool::Strategic);

        assert!(run_turn_started(&mut state, &["decline"]));

        assert!(infantry_on(&state, &system, &planet).is_empty());
        assert_eq!(
            state
                .player(&a())
                .unwrap()
                .tokens(ti4_model::state::TokenPool::Strategic),
            tokens - 1
        );
        assert_eq!(holder_of(&state, "ms", &a()), None, "the card went home");
    }

    #[test]
    fn military_support_lets_the_holder_pick_the_planet_and_uses_their_upgrade() {
        let (mut state, system, first) = military_support_fixture();
        let content = ContentStore::embedded();
        // A second controlled planet in another system.
        let (other_system, second) = ti4_content::galaxy::all_planets(content, POK)
            .iter()
            .filter(|(_, planet)| planet.system_id().is_some() && !planet.is_placed_during_play())
            .map(|(id, planet)| {
                (
                    ti4_model::id::SystemId::new(planet.system_id().unwrap()),
                    ti4_model::id::PlanetId::new(*id),
                )
            })
            .find(|(sys, planet)| *sys != system && *planet != first)
            .expect("another planet");
        state
            .system_mut(&other_system)
            .set_control(second.clone(), b());
        // b is Jol-Nar and owns the infantry upgrade, so the upgraded unit is what lands.
        state
            .player_mut(&b())
            .unwrap()
            .technologies
            .insert(ti4_model::id::TechnologyId::new("inf2"));
        let expected = crate::action_cards::placed_unit_id(&state, content, POK, &b(), "infantry")
            .expect("an infantry form")
            .to_string();

        assert!(run_turn_started(
            &mut state,
            &[&format!("{other_system}|{second}")]
        ));

        assert!(infantry_on(&state, &system, &first).is_empty());
        assert_eq!(
            infantry_on(&state, &other_system, &second),
            vec![expected.clone(), expected]
        );
    }

    #[test]
    fn without_a_planet_nothing_is_asked_but_the_card_still_resolves() {
        let (mut state, system, planet) = military_support_fixture();
        state.system_mut(&system).set_control(planet, a());

        // An empty script fails the run if anything were asked.
        assert!(run_turn_started(&mut state, &[]));

        assert_eq!(holder_of(&state, "ms", &a()), None, "the card went home");
    }

    #[test]
    fn support_for_the_throne_is_worth_a_point_to_whoever_holds_it() {
        let mut state = game_hacan_jolnar();
        let before = state.player(&b()).unwrap().victory_points;

        assert!(receive(&mut state, &b(), &support("hacan")));

        assert_eq!(state.player(&b()).unwrap().victory_points, before + 1);
        assert_eq!(state.support_holders.get(&a()), Some(&b()));
    }

    #[test]
    fn the_point_goes_home_with_the_card() {
        // Keeping it would score a player for a card they no longer hold.
        let mut state = game_hacan_jolnar();
        receive(&mut state, &b(), &support("hacan"));
        let with_it = state.player(&b()).unwrap().victory_points;

        assert!(return_support(&mut state, &a()));

        assert_eq!(state.player(&b()).unwrap().victory_points, with_it - 1);
        assert!(state.support_holders.is_empty());
    }

    #[test]
    fn nobody_scores_off_their_own_support() {
        let mut state = game_hacan_jolnar();
        let before = state.player(&a()).unwrap().victory_points;

        assert!(!receive(&mut state, &a(), &support("hacan")));

        assert_eq!(state.player(&a()).unwrap().victory_points, before);
    }

    #[test]
    fn play_area_membership_comes_from_the_corpus() {
        // Table-driven over the accepted corpus's `playArea` field: a faction record binds to
        // its owner and no other, a generic `<color>` record applies under every owner faction,
        // and unknown aliases are held in hand.
        // `is_play_area` resolves by identity without source scope (a note that exists in a
        // game always matches its own corpus record), so the table covers every record.
        let content = ContentStore::embedded();
        let mut play_area_records = 0usize;
        for record in content.records(ContentType::PromissoryNotes) {
            let alias = record.text("alias").expect("every note has an alias");
            if let Some(faction) = record.text("faction") {
                assert_eq!(
                    is_play_area(content, &note_id(alias, faction)),
                    record.flag("playArea"),
                    "{alias}:{faction}"
                );
                let other = if faction == "hacan" {
                    "jolnar"
                } else {
                    "hacan"
                };
                assert!(
                    !is_play_area(content, &note_id(alias, other)),
                    "{alias} binds to its owner {faction}, not {other}"
                );
            } else {
                let bare = alias
                    .strip_prefix(GENERIC_PREFIX)
                    .expect("a record without a faction is generic and prefixed");
                assert_eq!(
                    is_play_area(content, &note_id(bare, "hacan")),
                    record.flag("playArea"),
                    "{bare}:hacan"
                );
            }
            if record.flag("playArea") {
                play_area_records += 1;
            }
        }
        // Pins the corpus inventory: eleven play-area notes (two generic, nine faction).
        assert_eq!(play_area_records, 11);

        assert!(!is_play_area(content, "nonsense:hacan"), "unknown alias");
        assert!(!is_play_area(content, "nonsense"), "malformed key");
    }

    #[test]
    fn receipt_puts_play_area_notes_faceup_and_the_rest_in_hand() {
        let content = ContentStore::embedded();
        let mut state = game_hacan_jolnar();

        // Trade Convoys is Hacan's play-area card, but its text opens ACTION: receipt leaves it
        // in hand until the recipient places it.
        take(&mut state, content, &b(), "convoys:hacan");
        assert!(!state.promissory_faceup.contains("convoys:hacan"));
        assert!(play_convoys(&mut state, &b()));
        assert!(state.promissory_faceup.contains("convoys:hacan"));

        // Jolnar's note is not a play-area card: it stays held in hand.
        take(&mut state, content, &a(), "ra:jolnar");
        assert!(!state.promissory_faceup.contains("ra:jolnar"));

        // Giving the note back takes it out of the play area with it.
        give_back(&mut state, "convoys:hacan");
        assert!(
            !state.promissory_faceup.contains("convoys:hacan"),
            "the play area is where a lent note lives, not its home"
        );
    }

    #[test]
    fn support_for_the_throne_returns_when_its_holder_activates_the_owners_system() {
        // 69.3: "When you activate a system that contains 1 or more of the <color> player's
        // units... lose 1 victory point and return this card to the <color> player." This bug
        // report was that the note never came home; nothing in the engine called
        // `return_support` at all (grep confirms no caller outside these tests), so this pins
        // the rule the missing wiring is supposed to invoke.
        let mut state = game_hacan_jolnar();
        let (system, _) = crate::fixtures::a_placed_planet();
        let before = state.player(&b()).unwrap().victory_points;
        assert!(receive(&mut state, &b(), &support("hacan"))); // a's Support, held by b
        crate::fixtures::put(&mut state, &system, "cruiser", &a(), 1); // a (the owner) present

        let owners = spend_support_on_activation(&mut state, &b(), &system);

        assert_eq!(owners, vec![a()]);
        assert_eq!(
            state.player(&b()).unwrap().victory_points,
            before,
            "the point went home with the card"
        );
        assert!(state.support_holders.is_empty(), "and the card with it");
    }

    #[test]
    fn a_returned_support_takes_its_point_out_of_the_ledger_too() {
        // `vp_sources` and `game_cost` read the ledger, not the seat total. A return that only
        // decremented `victory_points` left every report still crediting the lost note.
        let mut state = game_hacan_jolnar();
        assert!(receive(&mut state, &b(), &support("hacan")));
        assert!(return_support(&mut state, &a()));

        let net: i32 = state
            .vp_ledger
            .iter()
            .filter(|(player, _, reason)| *player == b() && reason == "support_for_the_throne")
            .map(|(_, delta, _)| *delta)
            .sum();
        assert_eq!(net, 0, "the +1 on receipt and the -1 on return cancel");
    }

    #[test]
    fn a_trade_agreement_hands_over_the_replenished_commodities_and_goes_home() {
        let mut state = game_hacan_jolnar();
        let note = note_id("ta", "hacan");
        state.promissory_notes.insert(note.clone(), b()); // a's (Hacan's) Trade Agreement, held by b
        state.player_mut(&a()).unwrap().commodities = 6;
        let goods = state.player(&b()).unwrap().trade_goods;

        assert_eq!(trade_agreement_on_replenish(&mut state, &a()), Some(b()));

        assert_eq!(state.player(&a()).unwrap().commodities, 0);
        assert_eq!(state.player(&b()).unwrap().trade_goods, goods + 6);
        assert_eq!(
            state.promissory_notes.get(&note),
            Some(&a()),
            "the card went home"
        );
        assert_eq!(
            trade_agreement_on_replenish(&mut state, &a()),
            None,
            "once home it pays nobody"
        );
    }

    #[test]
    fn foreign_notes_are_counted_and_returned_but_a_players_own_are_not() {
        let mut state = game_hacan_jolnar();
        let theirs = note_id("ce", "jolnar");
        let mine = note_id("ce", "hacan");
        state.promissory_notes.insert(theirs.clone(), a());
        state.promissory_notes.insert(mine.clone(), a());
        assert_eq!(held_foreign(&state, &a(), "ce"), 1);

        return_all_foreign(&mut state, "ce");

        assert_eq!(state.promissory_notes.get(&theirs), Some(&b()));
        assert_eq!(state.promissory_notes.get(&mine), Some(&a()));
        assert_eq!(held_foreign(&state, &a(), "ce"), 0);
    }

    #[test]
    fn activating_a_system_without_the_owners_units_does_not_return_support() {
        // The trigger names the *owner's* units in the activated system, not any units at all
        // and not the holder's own presence there.
        let mut state = game_hacan_jolnar();
        let (system, _) = crate::fixtures::a_placed_planet();
        assert!(receive(&mut state, &b(), &support("hacan")));
        crate::fixtures::put(&mut state, &system, "cruiser", &b(), 1); // holder's own units only

        let owners = spend_support_on_activation(&mut state, &b(), &system);

        assert!(owners.is_empty());
        assert_eq!(
            state.support_holders.get(&a()),
            Some(&b()),
            "still lent out"
        );
    }

    #[test]
    fn a_support_held_by_a_different_activator_is_untouched() {
        // Support only returns to whoever gave it out, and only when the note's own owner has
        // units in the system that player just activated -- a third player activating the same
        // system must not spend somebody else's loan.
        let mut state = game(&["a", "b", "c"]);
        state.player_mut(&a()).unwrap().faction = FactionId::new("hacan");
        state.player_mut(&b()).unwrap().faction = FactionId::new("jolnar");
        let (system, _) = crate::fixtures::a_placed_planet();
        assert!(receive(&mut state, &b(), &support("hacan")));
        crate::fixtures::put(&mut state, &system, "cruiser", &a(), 1);

        let owners = spend_support_on_activation(&mut state, &PlayerId::new("c"), &system);

        assert!(owners.is_empty(), "c never held a's Support");
        assert_eq!(state.support_holders.get(&a()), Some(&b()));
    }

    #[test]
    fn one_support_cannot_be_lent_twice() {
        let mut state = game(&["a", "b", "c"]);
        state.player_mut(&a()).unwrap().faction = FactionId::new("hacan");
        state.player_mut(&b()).unwrap().faction = FactionId::new("jolnar");
        assert!(receive(&mut state, &b(), &support("hacan")));

        assert!(
            !receive(&mut state, &PlayerId::new("c"), &support("hacan")),
            "it is already in b's play area"
        );
        assert_eq!(state.player(&PlayerId::new("c")).unwrap().victory_points, 0);
    }
}
