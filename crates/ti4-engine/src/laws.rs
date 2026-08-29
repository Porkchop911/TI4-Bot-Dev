//! Laws in play (LRR 8.20, and the standing rules they impose).
//!
//! Ported from the oracle's `engine/laws.py`: `in_play`, `active`, `elected`, `repeal`,
//! `fleet_pool_cap`, `action_card_limit`, `nebulae_passable`, and `unimplemented`.
//!
//! A law is *enacted* by a vote and *enforced* by whatever rule it changes. Those are separate:
//! this engine could already enact every law, and enforced none of them, so `state.laws` was a
//! list nothing read. These are the first that bite.

use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;

use crate::objectives::VICTORY_TARGET;

/// Laws currently in play.
#[must_use]
pub fn in_play(state: &GameState) -> Vec<String> {
    state.laws.keys().cloned().collect()
}

/// Whether a law is in play.
#[must_use]
pub fn active(state: &GameState, alias: &str) -> bool {
    state.laws.contains_key(alias)
}

/// What this law was elected onto — a planet or a player (8.9 to 8.11).
///
/// For a For/Against law the value is the outcome itself, which is why a caller meaning "the
/// elected planet" must check it against the board rather than trusting it blindly.
#[must_use]
pub fn elected<'a>(state: &'a GameState, alias: &str) -> Option<&'a String> {
    state.laws.get(alias)
}

/// Remove a law, resolving any printed consequence of losing its card.
///
/// Public Censure is the one with a consequence: its holder loses the victory point it gave
/// them. A repeal that only deleted the entry would leave that point behind for good.
pub fn repeal(state: &mut GameState, alias: &str) -> bool {
    let Some(owner) = state.laws.get(alias).cloned() else {
        return false;
    };
    state.laws.remove(alias);
    if alias == "censure" {
        let holder = PlayerId::new(owner);
        if let Some(seat) = state.player_mut(&holder) {
            seat.victory_points = (seat.victory_points - 1).clamp(0, VICTORY_TARGET);
        }
    }
    true
}

/// Fleet Regulations caps the fleet pool at four.
#[must_use]
pub fn fleet_pool_cap(state: &GameState, base: i32) -> i32 {
    if active(state, "regulations") {
        base.min(4)
    } else {
        base
    }
}

/// Sanctions caps the action-card hand limit at three.
#[must_use]
pub fn action_card_limit(state: &GameState, base: usize) -> usize {
    if active(state, "sanctions") {
        base.min(3)
    } else {
        base
    }
}

/// Political Censure: its elected owner cannot play action cards.
#[must_use]
pub fn action_cards_forbidden(state: &GameState, player: &PlayerId) -> bool {
    active(state, "censure") && elected(state, "censure").is_some_and(|who| who == player.as_str())
}

/// Shared Research makes nebulae passable.
#[must_use]
pub fn nebulae_passable(state: &GameState) -> bool {
    active(state, "shared_research")
}

/// Homeland Defense Act: any number of PDS on a controlled planet.
///
/// The law removes the per-planet cap rather than raising it, so this answers "is there a cap"
/// rather than "what is it".
#[must_use]
pub fn structure_cap_lifted(state: &GameState, base_type: &str) -> bool {
    base_type == "pds" && active(state, "defense_act")
}

/// Conventions of War: BOMBARDMENT cannot be used against units on cultural planets.
#[must_use]
pub fn bombardment_forbidden(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    planet: &ti4_model::id::PlanetId,
) -> bool {
    if !active(state, "conventions") {
        return false;
    }
    ti4_content::galaxy::planet(content, planet.as_str(), sources)
        .is_some_and(|record| record.has_trait("CULTURAL"))
}

/// Demilitarized Zone: nothing may land on, be produced on, or be placed on the elected planet.
#[must_use]
pub fn planet_is_demilitarized(state: &GameState, planet: &ti4_model::id::PlanetId) -> bool {
    elected(state, "demilitarized_zone").is_some_and(|elected| elected == planet.as_str())
}

/// Holy Planet of Ixth: units on the elected planet cannot use PRODUCTION.
#[must_use]
pub fn production_forbidden_on(state: &GameState, planet: &ti4_model::id::PlanetId) -> bool {
    elected(state, "holy_planet_of_ixth").is_some_and(|elected| elected == planet.as_str())
}

/// How much a law adds to a planet's resources or influence.
///
/// Separate from [`crate::production::planet_value`], which stays the *printed* value: three laws
/// attach to a planet card and change what it is worth now, and conflating "printed" with "current"
/// is how an attachment silently stops applying somewhere.
///
/// * Core Mining — resources +2
/// * Senate Sanctuary — influence +2
/// * Terraforming Initiative — resources +1 and influence +1
#[must_use]
pub fn planet_value_bonus(
    state: &GameState,
    planet: &ti4_model::id::PlanetId,
    kind: crate::production::Spend,
) -> i64 {
    let attached = |alias: &str| -> bool {
        elected(state, alias).is_some_and(|elected| elected == planet.as_str())
    };
    let mut bonus = 0;
    match kind {
        crate::production::Spend::Resources => {
            if attached("core_mining") {
                bonus += 2;
            }
            if attached("terraforming_initiative") {
                bonus += 1;
            }
        }
        crate::production::Spend::Influence => {
            if attached("senate_sanctuary") {
                bonus += 2;
            }
            if attached("terraforming_initiative") {
                bonus += 1;
            }
        }
    }
    bonus
}

/// Regulated Conscription: a fighter or infantry costs its price for one unit, not two.
#[must_use]
pub fn single_unit_production(state: &GameState) -> bool {
    active(state, "conscription")
}

/// Research Teams: an exhausted research-team planet ignores one prerequisite of its colour.
///
/// Returns how many prerequisites of `colour` this player may waive. The card is attached to a
/// planet and exhausted to use, so the player must control it and it must be readied.
#[must_use]
pub fn research_team_waivers(state: &GameState, player: &PlayerId, colour: &str) -> usize {
    let alias = match colour {
        "BIOTIC" => "rt_biotic",
        "CYBERNETIC" => "rt_cybernetic",
        "PROPULSION" => "rt_propulsion",
        "WARFARE" => "rt_warfare",
        _ => return 0,
    };
    let Some(planet) = elected(state, alias) else {
        return 0;
    };
    let planet = ti4_model::id::PlanetId::new(planet.clone());
    if state.exhausted_planets.contains(&planet) {
        return 0;
    }
    usize::from(
        state
            .controlled_planets(player)
            .into_iter()
            .any(|(_, held)| *held == planet),
    )
}

/// Representative Government: one vote each, and planets are not exhausted to vote.
///
/// Both printings say the same thing; the corpus carries them as two aliases.
#[must_use]
pub fn flat_votes(state: &GameState) -> bool {
    active(state, "rep_govt") || active(state, "representative_government")
}

/// Enforced Travel Ban: alpha and beta wormholes have no effect during movement.
#[must_use]
pub fn wormholes_suppressed(state: &GameState) -> bool {
    active(state, "travel_ban")
}

/// Wormhole Reconstruction: every system with an alpha or beta wormhole is adjacent to every other.
#[must_use]
pub fn wormholes_all_connected(state: &GameState) -> bool {
    active(state, "wormhole_recon")
}

/// Minister of Commerce/Exploration and friends: who holds this ministry, if anyone.
///
/// A ministry is a law whose elected value is its owner, so "does this player own it" is one
/// question asked of several cards rather than a predicate each.
#[must_use]
pub fn ministry_owner<'a>(state: &'a GameState, alias: &str) -> Option<&'a String> {
    elected(state, alias)
}

/// Whether this player holds a named ministry.
#[must_use]
pub fn holds_ministry(state: &GameState, player: &PlayerId, alias: &str) -> bool {
    ministry_owner(state, alias).is_some_and(|owner| owner == player.as_str())
}

/// Minister of Exploration: gaining control of a planet pays its owner a trade good.
///
/// Called where control changes hands, beside the gain-control breakthrough, so the two cannot be
/// honoured in one path and forgotten in another. Returns how many goods were paid.
pub fn on_gain_control(state: &mut GameState, player: &PlayerId) -> i32 {
    if !holds_ministry(state, player, "minister_exploration") {
        return 0;
    }
    if let Some(seat) = state.player_mut(player) {
        seat.trade_goods += 1;
    }
    1
}

/// Minister of Policy: its owner draws an action card at the end of the status phase.
#[must_use]
pub fn draws_at_status_end(state: &GameState, player: &PlayerId) -> bool {
    holds_ministry(state, player, "minister_policy")
}

/// Articles of War: mechs lose their printed abilities, SUSTAIN DAMAGE excepted.
///
/// The same shape as an entropic scar suppressing unit abilities, and asked the same way — of the
/// unit, at the point the ability would be used.
#[must_use]
pub fn mech_abilities_suppressed(state: &GameState, base_type: &str, ability: &str) -> bool {
    base_type == "mech"
        && active(state, "articles_war")
        && !ability.eq_ignore_ascii_case("sustain")
}

/// Point the map at what the wormhole laws say.
///
/// `Galaxy` already carries both switches — `wormholes_off` for "alpha and beta wormholes have no
/// effect during movement", and `wormholes_all_linked` for "all systems that contain either an
/// alpha or beta wormhole are adjacent to each other", complete with the ALPHA/BETA restriction
/// both laws share. Neither was ever set from a law, so both laws were inert.
///
/// Applied to the owned map rather than threaded through movement: every route query already reads
/// the galaxy, so setting it here means no movement path can consult the wrong one.
pub fn apply_to_galaxy(state: &GameState, galaxy: &mut ti4_content::galaxy::Galaxy) {
    galaxy.wormholes_off = wormholes_suppressed(state);
    galaxy.wormholes_all_linked = wormholes_all_connected(state);
}

/// Laws this engine can enact but not enforce — the honest coverage gap.
#[must_use]
pub fn enforced_aliases() -> Vec<&'static str> {
    vec![
        "censure",
        "conscription",
        "conventions",
        "core_mining",
        "defense_act",
        "demilitarized_zone",
        "holy_planet_of_ixth",
        "regulations",
        "rep_govt",
        "representative_government",
        "rt_biotic",
        "rt_cybernetic",
        "rt_propulsion",
        "rt_warfare",
        "sanctions",
        "senate_sanctuary",
        "shared_research",
        "terraforming_initiative",
        "travel_ban",
        "wormhole_recon",
        "articles_war",
        "minister_exploration",
        "minister_policy",
    ]
}

/// Laws in the corpus that nothing here consults.
#[must_use]
pub fn unimplemented(content: &ContentStore, sources: SourceSet) -> Vec<String> {
    let enforced = enforced_aliases();
    content
        .from_sources(ContentType::Agendas, sources)
        .filter(|record| record.text("type") == Some("Law"))
        .filter_map(|record| record.text("alias"))
        .filter(|alias| !enforced.contains(alias))
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use ti4_model::content_types::DEFAULT as ALL_SOURCES;

    /// Every law claimed as enforced must change something observable.
    ///
    /// The reason this exists: four laws were listed in `enforced_aliases` with a predicate written
    /// and no caller — Regulated Conscription, both printings of Representative Government,
    /// Enforced Travel Ban and Wormhole Reconstruction. Each read as enforced and did nothing. A
    /// list of claims is not evidence; this drives the engine and watches it move.
    #[test]
    fn regulated_conscription_halves_the_yield_without_changing_the_price() {
        let content = ti4_content::ContentStore::embedded();
        let types = ti4_content::units::catalogue(content, ALL_SOURCES);
        let fighter = types.get("fighter").expect("in the corpus");

        let quiet = crate::fixtures::game(&["a"]);
        assert_eq!(
            crate::production::price_of_under(Some(&quiet), fighter),
            (1, 2),
            "ordinarily one resource buys two"
        );

        let state = enacted("conscription", "For");
        assert_eq!(
            crate::production::price_of_under(Some(&state), fighter),
            (1, 1),
            "the law halves the yield; the cost is unchanged"
        );

        // A full-price unit is untouched either way.
        let cruiser = types.get("cruiser").expect("in the corpus");
        assert_eq!(
            crate::production::price_of_under(Some(&state), cruiser),
            crate::production::price_of_under(Some(&quiet), cruiser)
        );
    }

    #[test]
    fn representative_government_replaces_influence_with_one_vote() {
        assert_eq!(crate::vote::flat_vote_amount(&enacted("rep_govt", "For")), Some(1));
        assert_eq!(
            crate::vote::flat_vote_amount(&enacted("representative_government", "For")),
            Some(1)
        );
        assert_eq!(
            crate::vote::flat_vote_amount(&crate::fixtures::game(&["a"])),
            None,
            "without the law, votes come from influence as usual"
        );
    }

    #[test]
    fn the_wormhole_laws_reach_the_map() {
        let mut galaxy = crate::fixtures::plain_hub().galaxy;

        apply_to_galaxy(&enacted("travel_ban", "For"), &mut galaxy);
        assert!(galaxy.wormholes_off, "Enforced Travel Ban closes them");
        assert!(!galaxy.wormholes_all_linked);

        apply_to_galaxy(&enacted("wormhole_recon", "For"), &mut galaxy);
        assert!(galaxy.wormholes_all_linked, "Wormhole Reconstruction joins them");
        assert!(!galaxy.wormholes_off, "and re-opens what the ban had shut");

        apply_to_galaxy(&crate::fixtures::game(&["a"]), &mut galaxy);
        assert!(!galaxy.wormholes_off && !galaxy.wormholes_all_linked);
    }

    fn enacted(alias: &str, elected: &str) -> GameState {
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.laws.insert(alias.to_owned(), elected.to_owned());
        state
    }

    /// Core Mining, Senate Sanctuary and Terraforming Initiative change what a planet can pay.
    ///
    /// Checked through `planet_value_now`, the function every spending path reads, rather than
    /// through the bonus itself — an attachment that changed a number nothing consults would pass a
    /// test written the other way round.
    #[test]
    fn an_attached_law_changes_what_a_planet_can_pay() {
        let content = ti4_content::ContentStore::embedded();
        let planet = ti4_model::id::PlanetId::new("bellatrix");
        let printed_resources =
            crate::production::planet_value(content, ALL_SOURCES, &planet, crate::production::Spend::Resources);
        let printed_influence =
            crate::production::planet_value(content, ALL_SOURCES, &planet, crate::production::Spend::Influence);

        let state = enacted("core_mining", "bellatrix");
        assert_eq!(
            crate::production::planet_value_now(
                &state, content, ALL_SOURCES, &planet, crate::production::Spend::Resources
            ),
            printed_resources + 2,
            "Core Mining adds two resources"
        );

        let state = enacted("senate_sanctuary", "bellatrix");
        assert_eq!(
            crate::production::planet_value_now(
                &state, content, ALL_SOURCES, &planet, crate::production::Spend::Influence
            ),
            printed_influence + 2,
            "Senate Sanctuary adds two influence"
        );

        let state = enacted("terraforming_initiative", "bellatrix");
        assert_eq!(
            (
                crate::production::planet_value_now(
                    &state, content, ALL_SOURCES, &planet, crate::production::Spend::Resources
                ),
                crate::production::planet_value_now(
                    &state, content, ALL_SOURCES, &planet, crate::production::Spend::Influence
                )
            ),
            (printed_resources + 1, printed_influence + 1),
            "Terraforming Initiative adds one of each"
        );

        // A law attached elsewhere leaves this planet alone.
        let state = enacted("core_mining", "somewhere_else");
        assert_eq!(
            crate::production::planet_value_now(
                &state, content, ALL_SOURCES, &planet, crate::production::Spend::Resources
            ),
            printed_resources
        );
    }

    /// Conventions of War: no BOMBARDMENT against units on a cultural planet.
    #[test]
    fn conventions_of_war_protects_cultural_planets() {
        let content = ti4_content::ContentStore::embedded();
        let cultural = ti4_content::galaxy::all_planets(content, ALL_SOURCES)
            .into_iter()
            .find(|(_, record)| record.has_trait("CULTURAL"))
            .map(|(name, _)| ti4_model::id::PlanetId::new(name.to_owned()))
            .expect("the corpus has cultural planets");
        let hazardous = ti4_content::galaxy::all_planets(content, ALL_SOURCES)
            .into_iter()
            .find(|(_, record)| record.has_trait("HAZARDOUS"))
            .map(|(name, _)| ti4_model::id::PlanetId::new(name.to_owned()))
            .expect("the corpus has hazardous planets");

        let state = enacted("conventions", "For");
        assert!(bombardment_forbidden(&state, content, ALL_SOURCES, &cultural));
        assert!(
            !bombardment_forbidden(&state, content, ALL_SOURCES, &hazardous),
            "only cultural planets are protected"
        );

        let quiet = crate::fixtures::game(&["a"]);
        assert!(!bombardment_forbidden(&quiet, content, ALL_SOURCES, &cultural));
    }

    /// Homeland Defense Act removes the PDS cap; Demilitarized Zone bars the planet entirely.
    #[test]
    fn structure_laws_lift_and_bar() {
        let state = enacted("defense_act", "For");
        assert!(structure_cap_lifted(&state, "pds"));
        assert!(
            !structure_cap_lifted(&state, "spacedock"),
            "the law names PDS only"
        );

        let state = enacted("demilitarized_zone", "bellatrix");
        assert!(planet_is_demilitarized(
            &state,
            &ti4_model::id::PlanetId::new("bellatrix")
        ));
        assert!(!planet_is_demilitarized(
            &state,
            &ti4_model::id::PlanetId::new("somewhere_else")
        ));
    }

    /// A Research Team waives one prerequisite of its colour, and only while readied and held.
    #[test]
    fn a_research_team_waives_one_prerequisite_of_its_colour() {
        let player = PlayerId::new("a");
        let planet = ti4_model::id::PlanetId::new("bellatrix");
        let mut state = enacted("rt_biotic", "bellatrix");
        state
            .system_mut(&ti4_model::id::SystemId::new("109"))
            .set_control(planet.clone(), player.clone());

        assert_eq!(research_team_waivers(&state, &player, "BIOTIC"), 1);
        assert_eq!(
            research_team_waivers(&state, &player, "WARFARE"),
            0,
            "each team covers one colour"
        );
        assert_eq!(
            research_team_waivers(&state, &PlayerId::new("b"), "BIOTIC"),
            0,
            "and only for the player holding the planet"
        );

        state.exhausted_planets.insert(planet);
        assert_eq!(
            research_team_waivers(&state, &player, "BIOTIC"),
            0,
            "the card is exhausted to use it"
        );
    }

    /// Both printings of Representative Government mean the same thing.
    #[test]
    fn representative_government_flattens_votes_under_either_alias() {
        assert!(flat_votes(&enacted("rep_govt", "For")));
        assert!(flat_votes(&enacted("representative_government", "For")));
        assert!(!flat_votes(&crate::fixtures::game(&["a"])));
    }


    use ti4_model::content_types::POK;

    use super::*;
    use crate::fixtures::game;

    fn player() -> PlayerId {
        PlayerId::new("a")
    }

    #[test]
    fn a_law_is_in_play_once_enacted() {
        let mut state = game(&["a"]);
        assert!(!active(&state, "regulations"));

        state.enact_law("regulations", "for");

        assert!(active(&state, "regulations"));
        assert_eq!(in_play(&state), vec!["regulations".to_owned()]);
        assert_eq!(elected(&state, "regulations"), Some(&"for".to_owned()));
    }

    #[test]
    fn fleet_regulations_caps_the_pool_at_four() {
        let mut state = game(&["a"]);
        assert_eq!(fleet_pool_cap(&state, 6), 6, "uncapped without the law");

        state.enact_law("regulations", "for");
        assert_eq!(fleet_pool_cap(&state, 6), 4);
        assert_eq!(fleet_pool_cap(&state, 3), 3, "a cap never raises anything");
    }

    #[test]
    fn sanctions_caps_the_action_card_hand() {
        let mut state = game(&["a"]);
        assert_eq!(action_card_limit(&state, 7), 7);

        state.enact_law("sanctions", "for");
        assert_eq!(action_card_limit(&state, 7), 3);
    }

    #[test]
    fn shared_research_opens_the_nebulae() {
        let mut state = game(&["a"]);
        assert!(!nebulae_passable(&state));
        state.enact_law("shared_research", "for");
        assert!(nebulae_passable(&state));
    }

    #[test]
    fn repealing_public_censure_takes_its_victory_point_back() {
        // A repeal that only deleted the entry would leave the point behind for good.
        let mut state = game(&["a"]);
        state.enact_law("censure", "a");
        state.player_mut(&player()).unwrap().victory_points = 1;

        assert!(repeal(&mut state, "censure"));

        assert!(!active(&state, "censure"));
        assert_eq!(state.player(&player()).unwrap().victory_points, 0);
    }

    #[test]
    fn a_censure_repeal_cannot_take_a_player_below_zero() {
        let mut state = game(&["a"]);
        state.enact_law("censure", "a");
        repeal(&mut state, "censure");
        assert_eq!(state.player(&player()).unwrap().victory_points, 0);
    }

    #[test]
    fn repealing_a_law_that_is_not_in_play_does_nothing() {
        let mut state = game(&["a"]);
        let before = state.clone();
        assert!(!repeal(&mut state, "regulations"));
        assert!(state.identical(&before));
    }

    #[test]
    fn the_unenforced_laws_are_reported() {
        // Enacting a law nothing reads is the failure this module exists to make visible.
        let unenforced = unimplemented(ContentStore::embedded(), POK);
        assert!(!unenforced.is_empty(), "most laws are still unenforced");
        for alias in enforced_aliases() {
            assert!(
                !unenforced.contains(&alias.to_owned()),
                "{alias} is enforced and should not be listed"
            );
        }
    }

    #[test]
    fn every_enforced_alias_is_a_real_law() {
        for alias in enforced_aliases() {
            let record = ContentStore::embedded().get(ContentType::Agendas, alias);
            assert!(
                record.is_some(),
                "{alias} is not an agenda the corpus knows"
            );
            assert_eq!(
                record.and_then(|r| r.text("type")),
                Some("Law"),
                "{alias} is not a law"
            );
        }
    }
}
