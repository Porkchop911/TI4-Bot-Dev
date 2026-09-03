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
    if is_play_area(content, note) {
        state.promissory_faceup.insert(note.to_owned());
    }
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
    match state.promissory_notes.get(&note) {
        Some(holder) if holder != owner => Some(holder.clone()),
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
    if let Some(seat) = state.player_mut(holder) {
        seat.victory_points = (seat.victory_points + 1).min(crate::objectives::VICTORY_TARGET);
    }
    state.note_vp(holder, 1, "support_for_the_throne");
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
    if let Some(seat) = state.player_mut(&holder) {
        seat.victory_points = (seat.victory_points - 1).max(0);
    }
    true
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
        .is_some_and(|holder| holder != mover && present.contains(holder))
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
        .is_some_and(|holder| holder != mover);
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

/// Military Support: the holder plants two infantry when the owner's turn begins.
///
/// The note is returned whether or not the troops land: it was spent on the turn starting, and a
/// holder with nowhere to put them has still used the card.
pub fn turn_started(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> bool {
    let Some(holder) = holder_of(state, "ms", player) else {
        return false;
    };
    if let Some(seat) = state.player_mut(player)
        && seat.tokens(ti4_model::state::TokenPool::Strategic) > 0
    {
        seat.gain_token_uncapped(ti4_model::state::TokenPool::Strategic, -1);
    }
    let spot = state
        .controlled_planets(&holder)
        .first()
        .map(|(system, planet)| ((*system).clone(), (*planet).clone()));
    if let Some((system, planet)) = spot {
        let generic = ti4_content::units::catalogue(content, sources)
            .get("infantry")
            .map(|unit| unit.id().to_owned());
        let faction = state
            .player(&holder)
            .map(|seat| seat.faction.to_string())
            .unwrap_or_default();
        if let Some(id) = ti4_content::units::faction_unit(content, &faction, "infantry", sources)
            .map(|unit| unit.id().to_owned())
            .or(generic)
        {
            let unit =
                ti4_model::units::Unit::new(ti4_model::id::UnitTypeId::new(id), holder.clone());
            let troops = state
                .system_mut(&system)
                .planet_units
                .entry(planet)
                .or_default();
            troops.push(unit.clone());
            troops.push(unit);
        }
    }
    let name = faction_name(state, player);
    give_back(state, &note_id("ms", &name));
    true
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

        // Trade Convoys is Hacan's card: lent to b, it sits faceup in b's play area. (A key like
        // `convoys:jolnar` cannot exist — Jolnar owns no such note.)
        take(&mut state, content, &b(), "convoys:hacan");
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

        take(&mut state, content, &b(), "convoys:hacan"); // lent to b: faceup from then on
        assert!(reaches_anyone(&state, &b()));
        assert!(!reaches_anyone(&state, &a()), "and only for its holder");
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

    #[test]
    fn military_support_plants_two_and_the_note_goes_home() {
        let content = ContentStore::embedded();
        let mut state = game(&["a", "b"]);
        // Military Support belongs to Sol, so seat a as sol and deal with factions known.
        state.player_mut(&a()).unwrap().faction = FactionId::new("sol");
        state.player_mut(&b()).unwrap().faction = FactionId::new("jolnar");
        deal(&mut state, content, POK);
        let (system, planet) = crate::fixtures::a_placed_planet();
        state.system_mut(&system).set_control(planet.clone(), b());
        take(&mut state, content, &b(), "ms:sol");

        assert!(turn_started(&mut state, content, POK, &a()));

        let landed = state
            .system_state(&system)
            .planet_units
            .get(&planet)
            .map_or(0, |units| units.iter().filter(|u| u.owner == b()).count());
        assert_eq!(landed, 2, "two infantry, for the holder");
        assert_eq!(
            holder_of(&state, "ms", &a()),
            None,
            "and the note went home"
        );
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

        // Trade Convoys is Hacan's play-area card: receipt puts it faceup in the recipient's
        // play area.
        take(&mut state, content, &b(), "convoys:hacan");
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
