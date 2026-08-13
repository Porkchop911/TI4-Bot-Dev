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
use ti4_model::content_types::SourceSet;
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;

/// Support for the Throne is keyed by its owner rather than by an alias.
pub const SUPPORT_PREFIX: &str = "support:";

/// The notes every player owns a copy of (69.1), by alias.
///
/// Support is handled separately — it is the one whose position scores a point.
pub const GENERIC: &[&str] = &["cf", "ps", "ta", "an"];

/// Notes that live faceup in a play area rather than in hand (69.3).
pub const FACEUP: &[&str] = &["an"];

/// This owner's Support for the Throne.
#[must_use]
pub fn support(owner: &PlayerId) -> String {
    format!("{SUPPORT_PREFIX}{owner}")
}

/// A note id: the printed alias, and whose copy it is.
#[must_use]
pub fn note_id(alias: &str, owner: &PlayerId) -> String {
    format!("{alias}:{owner}")
}

/// The printed alias a note id carries.
#[must_use]
pub fn alias_of(note: &str) -> &str {
    note.split_once(':').map_or(note, |(alias, _)| alias)
}

/// Who a note belongs to, or `None` if it is not a note id at all.
#[must_use]
pub fn owner_of(note: &str) -> Option<PlayerId> {
    if let Some(owner) = note.strip_prefix(SUPPORT_PREFIX) {
        return Some(PlayerId::new(owner));
    }
    note.split_once(':').map(|(_, owner)| PlayerId::new(owner))
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
            hands.insert(note_id(&alias, &seat.id), seat.id.clone());
        }
    }
    let _ = sources;
    state.promissory_notes = hands;
}

/// Give a note to a holder, faceup if that is where the card lives (69.3).
pub fn take(state: &mut GameState, holder: &PlayerId, note: &str) {
    state
        .promissory_notes
        .insert(note.to_owned(), holder.clone());
    if FACEUP.contains(&alias_of(note)) {
        state.promissory_faceup.insert(note.to_owned());
    }
}

/// Return a note to its owner once it has done its work.
///
/// Every note in the game says "then, return this card". A note is a loan, not a sale, which is
/// why parting with one is worth less than receiving one.
pub fn give_back(state: &mut GameState, note: &str) {
    let Some(owner) = owner_of(note) else {
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
    let note = note_id(alias, owner);
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

/// Notes this player could put on the table: their own, still in their hand.
///
/// A note already lent out is not theirs to offer, and somebody else's note is not theirs to
/// sell — both would let one card be traded twice.
#[must_use]
pub fn available_notes(state: &GameState, player: &PlayerId) -> Vec<String> {
    held_by(state, player)
        .into_iter()
        .filter(|note| owner_of(note).as_ref() == Some(player))
        .collect()
}

/// This player's Support for the Throne, if they still hold it.
#[must_use]
pub fn available_support(state: &GameState, player: &PlayerId) -> Option<String> {
    (!state.support_holders.contains_key(player)).then(|| support(player))
}

/// Put another player's Support faceup in a play area and award its point.
///
/// Returns `false` when the note cannot move: nobody may hold their own Support, and a Support
/// already lent out cannot be lent again.
pub fn receive(state: &mut GameState, holder: &PlayerId, note: &str) -> bool {
    let Some(owner) = owner_of(note) else {
        return false;
    };
    if &owner == holder || state.support_holders.contains_key(&owner) {
        return false;
    }
    state.support_holders.insert(owner, holder.clone());
    if let Some(seat) = state.player_mut(holder) {
        seat.victory_points = (seat.victory_points + 1).min(crate::objectives::VICTORY_TARGET);
    }
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

    let note = note_id("cf", mover);
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
    let note = note_id("cf", mover);
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
        .filter_map(|(note, _)| owner_of(note))
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
        seat.gain_token(ti4_model::state::TokenPool::Strategic, -1);
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
    give_back(state, &note_id("ms", player));
    true
}

/// What a particular Trade Agreement is worth: its owner's commodity value.
///
/// The card is generic only on paper. It hands over everything its owner replenishes — six
/// commodities for Hacan against two for Letnev — so pricing them alike would make the single
/// most valuable card a trading faction owns cost what the least valuable one does.
#[must_use]
pub fn trade_agreement_worth(state: &GameState, content: &ContentStore, note: &str) -> f64 {
    let default = 2.5;
    let Some(owner) = owner_of(note) else {
        return default;
    };
    let Some(seat) = state.player(&owner) else {
        return default;
    };
    let Some(faction) = ti4_content::factions::get(content, seat.faction.as_str()) else {
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

    fn a() -> PlayerId {
        PlayerId::new("a")
    }
    fn b() -> PlayerId {
        PlayerId::new("b")
    }

    #[test]
    fn a_note_id_carries_whose_copy_it_is() {
        // Five players each own a Ceasefire. Without the owner in the id they are one card, and
        // returning one would return them all.
        assert_eq!(note_id("cf", &a()), "cf:a");
        assert_eq!(alias_of("cf:a"), "cf");
        assert_eq!(owner_of("cf:a"), Some(a()));
        assert_eq!(owner_of(&support(&b())), Some(b()));
        assert_eq!(owner_of("nonsense"), None);
    }

    #[test]
    fn setup_deals_every_player_their_own_notes() {
        let mut state = game(&["a", "b"]);
        deal(&mut state, ContentStore::embedded(), POK);

        let mine = available_notes(&state, &a());
        assert!(mine.len() >= GENERIC.len(), "at least the generic ones");
        assert!(
            mine.iter()
                .all(|note| owner_of(note).as_ref() == Some(&a())),
            "and only your own: {mine:?}"
        );
        assert!(mine.contains(&"cf:a".to_owned()));
        assert!(!mine.contains(&"cf:b".to_owned()));
    }

    #[test]
    fn a_note_lent_out_is_no_longer_yours_to_offer() {
        // Otherwise one card is traded twice.
        let mut state = game(&["a", "b"]);
        deal(&mut state, ContentStore::embedded(), POK);
        assert!(available_notes(&state, &a()).contains(&"cf:a".to_owned()));

        take(&mut state, &b(), "cf:a");

        assert!(!available_notes(&state, &a()).contains(&"cf:a".to_owned()));
        assert_eq!(holder_of(&state, "cf", &a()), Some(b()));
        assert!(
            !available_notes(&state, &b()).contains(&"cf:a".to_owned()),
            "and holding somebody else's note is not owning it"
        );
    }

    #[test]
    fn a_note_is_a_loan_and_goes_home() {
        let mut state = game(&["a", "b"]);
        deal(&mut state, ContentStore::embedded(), POK);
        take(&mut state, &b(), "cf:a");

        give_back(&mut state, "cf:a");

        assert_eq!(holder_of(&state, "cf", &a()), None, "back with its owner");
        assert!(available_notes(&state, &a()).contains(&"cf:a".to_owned()));
    }

    #[test]
    fn a_faceup_note_sits_in_the_play_area() {
        // 69.3: Alliance is played faceup, and a note in a play area is public information.
        let mut state = game(&["a", "b"]);
        deal(&mut state, ContentStore::embedded(), POK);

        take(&mut state, &b(), "an:a");
        assert!(state.promissory_faceup.contains("an:a"));

        take(&mut state, &b(), "cf:a");
        assert!(
            !state.promissory_faceup.contains("cf:a"),
            "Ceasefire is held"
        );

        give_back(&mut state, "an:a");
        assert!(
            !state.promissory_faceup.contains("an:a"),
            "and it leaves the play area when it goes home"
        );
    }

    #[test]
    fn a_ceasefire_denies_a_move_only_where_its_holder_stands() {
        let mut state = game(&["a", "b"]);
        deal(&mut state, ContentStore::embedded(), POK);
        let (system, _) = crate::fixtures::a_placed_planet();
        take(&mut state, &b(), &note_id("cf", &a()));

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
        let mut state = game(&["a", "b"]);
        deal(&mut state, ContentStore::embedded(), POK);
        let (system, _) = crate::fixtures::a_placed_planet();
        take(&mut state, &b(), &note_id("cf", &a()));
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
        let mut state = game(&["a", "b"]);
        let note = note_id("convoys", &b());
        state.promissory_notes.insert(note.clone(), a());

        assert!(
            !reaches_anyone(&state, &a()),
            "a note in hand is not in play"
        );

        state.promissory_faceup.insert(note);
        assert!(reaches_anyone(&state, &a()));
        assert!(!reaches_anyone(&state, &b()), "and only for its holder");
    }

    #[test]
    fn a_trade_agreement_is_worth_what_its_owner_replenishes() {
        // Six commodities for one faction against two for another: pricing them alike makes the
        // most valuable card a trading faction owns cost what the least valuable does.
        let content = ContentStore::embedded();
        let mut state = game(&["a", "b"]);
        let rich = ti4_content::factions::catalogue(content, POK)
            .iter()
            .max_by_key(|(_, faction)| faction.commodities())
            .map(|(alias, faction)| ((*alias).to_owned(), faction.commodities()));
        let poor = ti4_content::factions::catalogue(content, POK)
            .iter()
            .filter(|(_, faction)| faction.commodities() > 0)
            .min_by_key(|(_, faction)| faction.commodities())
            .map(|(alias, faction)| ((*alias).to_owned(), faction.commodities()));
        let (Some((rich, high)), Some((poor, low))) = (rich, poor) else {
            return;
        };
        if high == low {
            return; // this corpus prices every faction alike
        }
        state.player_mut(&a()).unwrap().faction = ti4_model::id::FactionId::new(rich);
        state.player_mut(&b()).unwrap().faction = ti4_model::id::FactionId::new(poor);

        let dear = trade_agreement_worth(&state, content, &note_id("ta", &a()));
        let cheap = trade_agreement_worth(&state, content, &note_id("ta", &b()));

        assert!(dear > cheap, "{dear} should beat {cheap}");
    }

    #[test]
    fn military_support_plants_two_and_the_note_goes_home() {
        let content = ContentStore::embedded();
        let mut state = game(&["a", "b"]);
        deal(&mut state, content, POK);
        let (system, planet) = crate::fixtures::a_placed_planet();
        state.system_mut(&system).set_control(planet.clone(), b());
        take(&mut state, &b(), &note_id("ms", &a()));

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
        let mut state = game(&["a", "b"]);
        let before = state.player(&b()).unwrap().victory_points;

        assert!(receive(&mut state, &b(), &support(&a())));

        assert_eq!(state.player(&b()).unwrap().victory_points, before + 1);
        assert_eq!(state.support_holders.get(&a()), Some(&b()));
    }

    #[test]
    fn the_point_goes_home_with_the_card() {
        // Keeping it would score a player for a card they no longer hold.
        let mut state = game(&["a", "b"]);
        receive(&mut state, &b(), &support(&a()));
        let with_it = state.player(&b()).unwrap().victory_points;

        assert!(return_support(&mut state, &a()));

        assert_eq!(state.player(&b()).unwrap().victory_points, with_it - 1);
        assert!(state.support_holders.is_empty());
    }

    #[test]
    fn nobody_scores_off_their_own_support() {
        let mut state = game(&["a", "b"]);
        let before = state.player(&a()).unwrap().victory_points;

        assert!(!receive(&mut state, &a(), &support(&a())));

        assert_eq!(state.player(&a()).unwrap().victory_points, before);
    }

    #[test]
    fn one_support_cannot_be_lent_twice() {
        let mut state = game(&["a", "b", "c"]);
        assert!(receive(&mut state, &b(), &support(&a())));

        assert!(
            !receive(&mut state, &PlayerId::new("c"), &support(&a())),
            "it is already in b's play area"
        );
        assert_eq!(state.player(&PlayerId::new("c")).unwrap().victory_points, 0);
    }
}
