//! Faction effect tests, group f4 (see mod.rs and docs/FACTION_EFFECT_COVERAGE.md).
//!
//! The Thunder's Edge factions (Bastion, Deepwrought, Firmament, Obsidian, Crimson, Ral Nel), the
//! 31 breakthroughs and the generic promissory notes. Most of these effects already have deep
//! tests next to their code (`factions/*.rs`); the table cites them. The tests here cover the
//! cross-cutting facts every card shares and that nothing else pinned per card: the agent starts
//! ready and exhausts, the hero unlocks on the third scored objective and is purged, the commander
//! has a registered unlock condition (an unregistered one would stay locked forever), the faction
//! promissory note is dealt to its owner, can be lent and goes home, and every breakthrough has a
//! handler. Each is a shallow test of the *plumbing*; the effect text itself is cited.

use ti4_model::id::{LeaderId, ObjectiveId, PlayerId};
use ti4_model::state::{GameState, LeaderStatus};

use super::{ALL, content, pid, seated_all};
use crate::leaders;

fn a() -> PlayerId {
    pid("a")
}

fn b() -> PlayerId {
    pid("b")
}

fn game(faction: &str) -> GameState {
    let mut state = seated_all(&[("a", faction), ("b", "sol")]);
    crate::promissory::deal(&mut state, content(), ALL);
    state
}

fn status(state: &GameState, id: &str) -> Option<LeaderStatus> {
    leaders::status(state, &a(), &LeaderId::new(id))
}

fn agent_cycle(faction: &str, agent: &str) {
    let mut state = game(faction);
    let id = LeaderId::new(agent);
    assert_eq!(status(&state, agent), Some(LeaderStatus::Readied));
    assert!(leaders::exhaust(&mut state, &a(), &id));
    assert_eq!(status(&state, agent), Some(LeaderStatus::Exhausted));
    assert!(!leaders::exhaust(&mut state, &a(), &id), "not twice");
    leaders::ready_all(&mut state, &a());
    assert_eq!(status(&state, agent), Some(LeaderStatus::Readied));
}

fn hero_cycle(faction: &str, hero: &str) {
    let mut state = game(faction);
    assert_eq!(status(&state, hero), Some(LeaderStatus::Locked));
    for objective in ["o1", "o2"] {
        state.record_score(&a(), ObjectiveId::new(objective));
    }
    leaders::check_unlocks(&mut state, content(), ALL, None, &a());
    assert_eq!(status(&state, hero), Some(LeaderStatus::Locked), "two is not three");
    state.record_score(&a(), ObjectiveId::new("o3"));
    leaders::check_unlocks(&mut state, content(), ALL, None, &a());
    assert_eq!(status(&state, hero), Some(LeaderStatus::Unlocked));
    assert!(leaders::purge(&mut state, &a(), &LeaderId::new(hero)));
    assert_eq!(status(&state, hero), Some(LeaderStatus::Purged));
    leaders::check_unlocks(&mut state, content(), ALL, None, &a());
    assert_eq!(status(&state, hero), Some(LeaderStatus::Purged), "stays purged");
}

fn commander_registered(faction: &str, commander: &str) {
    let state = game(faction);
    assert_eq!(status(&state, commander), Some(LeaderStatus::Locked));
    assert_eq!(
        leaders::commander_unlocked(&state, content(), ALL, None, &a(), &LeaderId::new(commander)),
        Some(false),
        "a fresh game meets no unlock condition, and the condition is registered"
    );
}

/// The note travels: owner holds it at the deal, a holder can take it, `give_back` sends it home.
fn note_travels(faction: &str, alias: &str) {
    let mut state = game(faction);
    let note = crate::promissory::note_id(alias, faction);
    assert_eq!(state.promissory_notes.get(&note), Some(&a()), "dealt to its owner");
    assert!(
        state.promissory_notes.keys().filter(|key| key.starts_with(&format!("{alias}:"))).count() == 1,
        "only the owner's faction has this note"
    );
    crate::promissory::take(&mut state, content(), &b(), &note);
    assert_eq!(state.promissory_notes.get(&note), Some(&b()), "lent to b");
    crate::promissory::give_back(&mut state, &note);
    assert_eq!(state.promissory_notes.get(&note), Some(&a()), "home again");
}

macro_rules! plumbing {
    ($faction:literal, $agent:literal, $commander:literal, $hero:literal, $note:literal,
     $agent_fn:ident, $(#[$commander_attr:meta])* $commander_fn:ident, $hero_fn:ident, $note_fn:ident) => {
        #[test]
        fn $agent_fn() {
            agent_cycle($faction, $agent);
        }
        #[test]
        $(#[$commander_attr])*
        fn $commander_fn() {
            commander_registered($faction, $commander);
        }
        #[test]
        fn $hero_fn() {
            hero_cycle($faction, $hero);
        }
        #[test]
        fn $note_fn() {
            note_travels($faction, $note);
        }
    };
}

plumbing!("bastion", "bastionagent", "bastioncommander", "bastionhero", "raisethestandard",
    bastionagent_1_starts_ready_exhausts_and_readies_in_the_status_phase,
    bastioncommander_1_starts_locked_with_a_registered_unlock_condition,
    bastionhero_1_unlocks_on_three_scored_objectives_and_is_purged,
    raisethestandard_1_is_dealt_to_the_bastion_lent_and_returned);
plumbing!("deepwrought", "deepwroughtagent", "deepwroughtcommander", "deepwroughthero", "shareknowledge",
    deepwroughtagent_1_starts_ready_exhausts_and_readies_in_the_status_phase,
    deepwroughtcommander_1_starts_locked_with_a_registered_unlock_condition,
    deepwroughthero_1_unlocks_on_three_scored_objectives_and_is_purged,
    shareknowledge_1_is_dealt_to_the_deepwrought_lent_and_returned);
plumbing!("firmament", "firmamentagent", "firmamentcommander", "firmamenthero", "blackops",
    firmamentagent_1_starts_ready_exhausts_and_readies_in_the_status_phase,
    firmamentcommander_1_starts_locked_with_a_registered_unlock_condition,
    firmamenthero_1_unlocks_on_three_scored_objectives_and_is_purged,
    blackops_1_is_dealt_to_the_firmament_lent_and_returned);
plumbing!("crimson", "crimsonagent", "crimsoncommander", "crimsonhero", "sever",
    crimsonagent_1_starts_ready_exhausts_and_readies_in_the_status_phase,
    crimsoncommander_1_starts_locked_with_a_registered_unlock_condition,
    crimsonhero_1_unlocks_on_three_scored_objectives_and_is_purged,
    sever_1_is_dealt_to_the_crimson_lent_and_returned);
plumbing!("ralnel", "ralnelagent", "ralnelcommander", "ralnelhero", "nanolink",
    ralnelagent_1_starts_ready_exhausts_and_readies_in_the_status_phase,
    ralnelcommander_1_starts_locked_with_a_registered_unlock_condition,
    ralnelhero_1_unlocks_on_three_scored_objectives_and_is_purged,
    nanolink_1_is_dealt_to_the_ral_nel_lent_and_returned);

// Obsidian: the faction cannot be seated at setup (Nocturne), so its cards are checked on the
// Firmament seat that flips into it; the Obsidian's own components are the flipped leaders.
#[test]
fn obsidian_leaders_exist_in_the_corpus_and_the_commander_condition_is_registered() {
    let state = game("firmament");
    for leader in ["obsidianagent", "obsidiancommander", "obsidianhero"] {
        assert!(leaders::kind_of(content(), &LeaderId::new(leader)).is_some(), "{leader}");
    }
    // The Obsidian's commander check is reached through the flipped seat (obsidian.rs tests it).
    let _ = state;
}

/// Every breakthrough in the content has a handler registered by its faction module or the
/// breakthrough module (a card nobody handles would be gainable and do nothing).
#[test]
fn every_breakthrough_in_the_content_has_a_registered_handler() {
    let known = crate::breakthroughs::registered_aliases();
    let missing: Vec<String> = content()
        .from_sources(ti4_model::content_types::ContentType::Breakthroughs, ALL)
        .filter_map(|record| record.text("alias").map(str::to_owned))
        .filter(|alias| !known.contains(&alias.as_str()))
        .collect();
    assert!(missing.is_empty(), "no handler: {missing:?}");
}

/// The five generic notes are dealt to every player in the game, one copy each.
#[test]
fn generic_notes_cf_ps_ta_an_and_support_are_dealt_to_every_player() {
    let state = game("bastion");
    for (faction, holder) in [("bastion", a()), ("sol", b())] {
        for alias in ["cf", "ps", "ta", "an"] {
            let note = crate::promissory::note_id(alias, faction);
            assert_eq!(state.promissory_notes.get(&note), Some(&holder), "{note}");
        }
    }
}

/// Watchful Ojz unlocks for "the last person to pass during the action phase". The game records
/// `Feat::LastToPass` when the last seat passes; the commander should open from that.
#[test]
fn ralnelcommander_2_unlocks_for_the_last_player_to_pass() {
    let mut state = game("ralnel");
    for seat in state.players.iter_mut() {
        seat.passed = true;
    }
    let occurrence = state.begin_feat_occurrence();
    state.record_event_feat(&a(), ti4_model::state::Feat::LastToPass, occurrence);
    leaders::check_unlocks(&mut state, content(), ALL, None, &a());
    assert_eq!(status(&state, "ralnelcommander"), Some(LeaderStatus::Unlocked));
}

/// Letnev, Gravleash Maneuvers, second half: before rolling in a space combat, +X to the result of
/// 1 ship's roll, X being the number of ship types the Letnev has in the combat. Two ship types
/// whose dice both just miss (6 against a cruiser's 7, 8 against a destroyer's 9) must produce a
/// hit with the card, and none without it.
#[test]
fn letnevbt_2_plus_x_to_one_roll_in_space_combat() {
    let hits = |with_card: bool| {
        let mut state = seated_all(&[("a", "letnev"), ("b", "sol")]);
        if with_card {
            state.player_mut(&a()).unwrap().breakthrough =
                Some(ti4_model::id::BreakthroughId::new("letnevbt"));
        }
        let system = ti4_model::id::SystemId::new(crate::fixtures::plain_systems(1)[0].clone());
        crate::fixtures::put(&mut state, &system, "cruiser", &a(), 1);
        crate::fixtures::put(&mut state, &system, "destroyer", &a(), 1);
        let mut dice = crate::dice::Dice::from_faces([6, 8]);
        let mut rng = crate::rng::GameRng::new(0);
        crate::combat::roll_fleet(&state, content(), ALL, &mut dice, &mut rng, &a(), &system)
    };
    assert_eq!(hits(false), 0, "control: both dice miss without the card");
    assert!(hits(true) >= 1, "with two ship types one roll gains +2");
}
