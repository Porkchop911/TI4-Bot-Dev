//! The Deepwrought Scholarate's cards that act on other players' decks: Share Knowledge, Visionaria
//! Select and Ta Zern. See `deepwrought.rs` for the ocean model and `plans/evidence/BF-deepwrought.md`.
//!
//! Card texts (latest printing):
//!
//! * Share Knowledge: "ACTION: Place this card faceup in your play area and gain 1 non-faction,
//!   non-unit upgrade technology that the Deepwrought player owns; place that technology on this
//!   card. Return that technology to the deck and this card to the Deepwrought player at the end of
//!   the status phase."
//! * Visionaria Select (`deepwroughtbt`): "ACTION: Exhaust this card to allow each other player to
//!   spend 3 trade goods and give you 1 promissory note. Each player that does may research a
//!   non-faction, non-unit upgrade technology. You also gain each technology researched in this
//!   way." (The commander cannot lower the trade good cost: it reduces resource costs only.)
//! * Ta Zern (`deepwroughthero`): "ACTION: Purge this card and a non-unit upgrade technology you own
//!   or from your deck; then, purge all cards with the same name owned by other players and in
//!   other players' decks. Then, each player that purged a technology they owned researches another
//!   technology." Unlock: "Have 3 scored objectives" (the generic hero unlock).
//!
//! **Research here is "research" in the table-less sense**: prerequisites are checked, nothing is
//! paid, and only technologies whose prerequisites the researcher already meets are offered (no
//! waiver or Inheritance Systems is silently taken: `technology::research` would take the latter on
//! its own, which an unasked payment must not).

use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::{DEFAULT, SourceSet};
use ti4_model::id::{LeaderId, PlayerId, TechnologyId};
use ti4_model::state::GameState;

use super::deepwrought::{
    BREAKTHROUGH, HERO, SHARE_KNOWLEDGE, ask, deck, mark_technology_purged, technology_purged,
};
use crate::choice::ChoiceOption;
use crate::timing::{Ability, Relation, TimingContext, TimingError};

const SHARE_ACTION: &str = "faction|deepwrought|shareknowledge";
const VISIONARIA_ACTION: &str = "faction|deepwrought|visionaria";
const SHARE_PREFIX: &str = "deepwrought:share:";
const BT_EXHAUSTED_PREFIX: &str = "deepwrought:bt:exhausted:";
/// Visionaria Select's price, in trade goods.
const VISIONARIA_COST: i32 = 3;

fn share_key(note: &str) -> String {
    format!("{SHARE_PREFIX}{note}")
}

fn bt_exhausted_key(player: &PlayerId) -> String {
    format!("{BT_EXHAUSTED_PREFIX}{player}")
}

// -- Share Knowledge ----------------------------------------------------------------------------------

/// The technologies Share Knowledge could hand `holder`: non-faction, non-unit-upgrade technologies
/// the Deepwrought player owns and `holder` does not.
fn share_candidates(
    state: &GameState,
    content: &ContentStore,
    holder: &PlayerId,
    note: &str,
) -> Vec<TechnologyId> {
    let Some(owner) =
        crate::promissory::owner_of(note).and_then(|name| crate::promissory::seat_of(state, &name))
    else {
        return Vec::new();
    };
    let (Some(giver), Some(taker)) = (state.player(&owner), state.player(holder)) else {
        return Vec::new();
    };
    giver
        .technologies
        .iter()
        .filter(|tech| crate::technology::faction_of(content, tech).is_none())
        .filter(|tech| !crate::technology::is_unit_upgrade(content, tech))
        .filter(|tech| !taker.technologies.contains(*tech))
        .cloned()
        .collect()
}

/// The Share Knowledge notes `player` holds in hand that could be played now.
fn playable_notes(state: &GameState, content: &ContentStore, player: &PlayerId) -> Vec<String> {
    crate::promissory::action_notes_in_hand(state, player)
        .into_iter()
        .filter(|note| crate::promissory::alias_of(note) == SHARE_KNOWLEDGE)
        .filter(|note| !share_candidates(state, content, player, note).is_empty())
        .collect()
}

fn share_knowledge(context: &mut TimingContext<'_>, player: &PlayerId) -> bool {
    let Some(note) = playable_notes(context.state, context.content, player)
        .into_iter()
        .next()
    else {
        return false;
    };
    let candidates = share_candidates(context.state, context.content, player, &note);
    let mut options: Vec<ChoiceOption> = candidates
        .iter()
        .map(|tech| {
            ChoiceOption::labelled(
                tech.to_string(),
                "technology",
                format!("gain {}", crate::technology::name(context.content, tech)),
            )
        })
        .collect();
    options.push(ChoiceOption::decline());
    let Ok(answer) = ask(
        context,
        player,
        "Share Knowledge: gain which of the Deepwrought player's technologies".to_owned(),
        SHARE_KNOWLEDGE,
        "share_knowledge_technology",
        options,
    ) else {
        return false;
    };
    let Some(tech) = candidates
        .into_iter()
        .find(|tech| tech.as_str() == answer.id)
    else {
        return false;
    };
    if !crate::promissory::play_action_note(context.state, player, &note) {
        return false;
    }
    crate::technology::gain(context.state, context.content, context.sources, player, &tech);
    context
        .state
        .faction_marks
        .insert(share_key(&note), format!("{player}|{tech}"));
    true
}

/// The card's tail: "Return that technology to the deck and this card to the Deepwrought player at
/// the end of the status phase."
fn share_returns(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_seat) = (seat.clone(), seat.clone());
    let pending = |state: &GameState, who: &PlayerId| -> Vec<(String, TechnologyId)> {
        state
            .faction_marks
            .range(SHARE_PREFIX.to_owned()..)
            .take_while(|(key, _)| key.starts_with(SHARE_PREFIX))
            .filter_map(|(key, value)| {
                let (holder, tech) = value.split_once('|')?;
                (holder == who.as_str()).then(|| {
                    (
                        key[SHARE_PREFIX.len()..].to_owned(),
                        TechnologyId::new(tech),
                    )
                })
            })
            .collect()
    };
    Ability::stateful(
        format!("promissory:{owner_name}:share_knowledge_return:STATUS_PHASE_ENDED:after"),
        seat.clone(),
        "STATUS_PHASE_ENDED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            for (note, tech) in pending(context.state, &owner) {
                if let Some(seat) = context.state.player_mut(&owner) {
                    seat.technologies.remove(&tech);
                    seat.exhausted_technologies.remove(&tech);
                }
                crate::promissory::give_back(context.state, &note);
                context.state.faction_marks.remove(&share_key(&note));
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        !pending(context.state, &condition_seat).is_empty()
    }))
}

// -- Visionaria Select -------------------------------------------------------------------------------------

/// The notes `giver` could give the holder: in hand, receivable, and not a Support for the Throne
/// (whose point travels with the card through its own bookkeeping).
fn giveable_notes(
    state: &GameState,
    content: &ContentStore,
    giver: &PlayerId,
    holder: &PlayerId,
) -> Vec<String> {
    crate::promissory::available_notes(state, content, giver)
        .into_iter()
        .filter(|note| crate::promissory::may_receive(state, holder, note))
        .filter(|note| {
            crate::promissory::owner_of(note)
                .is_none_or(|name| *note != crate::promissory::support(&name))
        })
        .collect()
}

/// The seats Visionaria Select could ask: another player with 3 trade goods in reach and a note to
/// give, in seating order after the holder.
fn visionaria_players(
    state: &GameState,
    content: &ContentStore,
    holder: &PlayerId,
) -> Vec<PlayerId> {
    let order = &state.seating_order;
    let start = order.iter().position(|seat| seat == holder).unwrap_or(0);
    order[start..]
        .iter()
        .chain(&order[..start])
        .filter(|seat| *seat != holder)
        .filter(|seat| {
            crate::supply::potential_goods(state, seat) >= i64::from(VISIONARIA_COST)
                && !giveable_notes(state, content, seat, holder).is_empty()
        })
        .cloned()
        .collect()
}

fn visionaria_ready(state: &GameState, player: &PlayerId) -> bool {
    crate::breakthroughs::holds(state, player, BREAKTHROUGH)
        && !state.faction_marks.contains_key(&bt_exhausted_key(player))
}

/// Non-faction, non-unit-upgrade technologies `player` could research now with their prerequisites
/// met (see the module note on table-less research).
fn researchable_plain(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<TechnologyId> {
    crate::technology::researchable(state, content, sources, player)
        .into_iter()
        .filter(|tech| crate::technology::faction_of(content, tech).is_none())
        .filter(|tech| !crate::technology::is_unit_upgrade(content, tech))
        .filter(|tech| crate::technology::prerequisites_met(state, content, sources, player, tech))
        .collect()
}

fn visionaria(context: &mut TimingContext<'_>, holder: &PlayerId) -> bool {
    if !visionaria_ready(context.state, holder)
        || visionaria_players(context.state, context.content, holder).is_empty()
    {
        return false;
    }
    let before = context.state.clone();
    context
        .state
        .faction_marks
        .insert(bt_exhausted_key(holder), "1".to_owned());
    match visionaria_round(context, holder) {
        Ok(()) => true,
        Err(_) => {
            *context.state = before;
            false
        }
    }
}

fn visionaria_round(context: &mut TimingContext<'_>, holder: &PlayerId) -> Result<(), TimingError> {
    for other in visionaria_players(context.state, context.content, holder) {
        let notes = giveable_notes(context.state, context.content, &other, holder);
        if notes.is_empty()
            || crate::supply::potential_goods(context.state, &other) < i64::from(VISIONARIA_COST)
        {
            continue; // an earlier step of this very action changed what they hold
        }
        let answer = ask(
            context,
            &other,
            format!(
                "Visionaria Select: spend {VISIONARIA_COST} trade goods and give {holder} a promissory note"
            ),
            BREAKTHROUGH,
            "visionaria_accept",
            vec![
                ChoiceOption::labelled(
                    "accept",
                    "visionaria",
                    "spend the trade goods and give a note",
                ),
                ChoiceOption::decline(),
            ],
        )?;
        if answer.is_decline() {
            continue;
        }
        let given = ask(
            context,
            &other,
            format!("Visionaria Select: which promissory note to give {holder}"),
            BREAKTHROUGH,
            "visionaria_note",
            notes
                .iter()
                .map(|note| ChoiceOption::labelled(note.clone(), "promissory", note.clone()))
                .collect(),
        )?;
        let Some(note) = notes.into_iter().find(|note| *note == given.id) else {
            continue;
        };
        let paid = crate::supply::with_goods_window(
            context,
            &other,
            i64::from(VISIONARIA_COST),
            |inner| crate::supply::spend_goods(inner.state, &other, VISIONARIA_COST),
        );
        if paid != Some(true) {
            continue;
        }
        context.state.promissory_notes.insert(note, holder.clone());
        // Doctor Carrina's window: the player's research is a research (`deepwrought_research.rs`).
        let window = super::deepwrought_research::open(
            &mut super::deepwrought_research::Host::of(context),
            &other,
            &|content, tech| {
                crate::technology::faction_of(content, tech).is_none()
                    && !crate::technology::is_unit_upgrade(content, tech)
            },
        )
        .map_err(TimingError::IllegalChoice)?;
        let result = visionaria_research(context, holder, &other);
        super::deepwrought_research::settle(
            &mut super::deepwrought_research::Host::of(context),
            &other,
            window,
            result.is_ok(),
        )
        .map_err(TimingError::IllegalChoice)?;
        result?;
    }
    Ok(())
}

/// The research a player may take after paying for Visionaria Select; the holder gains it too.
fn visionaria_research(
    context: &mut TimingContext<'_>,
    holder: &PlayerId,
    other: &PlayerId,
) -> Result<(), TimingError> {
    let options = researchable_plain(context.state, context.content, context.sources, other);
    if options.is_empty() {
        return Ok(());
    }
    let mut offered: Vec<ChoiceOption> = options
        .iter()
        .map(|tech| {
            ChoiceOption::labelled(
                tech.to_string(),
                crate::strategy_cards::RESEARCH_KIND,
                crate::technology::name(context.content, tech),
            )
        })
        .collect();
    offered.push(ChoiceOption::decline());
    let chosen = ask(
        context,
        other,
        "Visionaria Select: research a non-faction, non-unit upgrade technology".to_owned(),
        BREAKTHROUGH,
        "visionaria_research",
        offered,
    )?;
    let Some(tech) = options.into_iter().find(|tech| tech.as_str() == chosen.id) else {
        return Ok(());
    };
    if crate::technology::research(
        context.state,
        context.content,
        context.sources,
        other,
        &tech,
    ) && context
        .state
        .player(holder)
        .is_some_and(|seat| !seat.technologies.contains(&tech))
    {
        crate::technology::gain(context.state, context.content, context.sources, holder, &tech);
    }
    Ok(())
}

/// Visionaria Select readies with everything else at the end of the status phase.
fn visionaria_readies(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("breakthrough:{owner_name}:{BREAKTHROUGH}_ready:STATUS_PHASE_ENDED:after"),
        seat.clone(),
        "STATUS_PHASE_ENDED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            context
                .state
                .faction_marks
                .remove(&bt_exhausted_key(&owner));
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        context
            .state
            .faction_marks
            .contains_key(&bt_exhausted_key(&condition_owner))
    }))
}

// -- Ta Zern ---------------------------------------------------------------------------------------------------

/// The non-unit-upgrade technologies Ta Zern may purge: owned, or in the owner's deck.
fn hero_candidates(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<TechnologyId> {
    let mut found: std::collections::BTreeSet<TechnologyId> =
        deck(state, content, sources, player).into_iter().collect();
    if let Some(seat) = state.player(player) {
        found.extend(seat.technologies.iter().cloned());
    }
    found
        .into_iter()
        .filter(|tech| !crate::technology::is_unit_upgrade(content, tech))
        .filter(|tech| !technology_purged(state, tech))
        .collect()
}

pub(crate) fn leader_action(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    (leader.as_str() == HERO).then(|| !hero_candidates(state, content, DEFAULT, player).is_empty())
}

pub(crate) fn use_leader(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    (leader.as_str() == HERO).then(|| hero(context, player))
}

/// Wave Function Collapse: choose, purge, then each affected player researches again.
fn hero(context: &mut TimingContext<'_>, player: &PlayerId) -> bool {
    let candidates = hero_candidates(context.state, context.content, context.sources, player);
    if candidates.is_empty() {
        return false;
    }
    let mut options: Vec<ChoiceOption> = candidates
        .iter()
        .map(|tech| {
            ChoiceOption::labelled(
                tech.to_string(),
                "technology",
                format!("purge {}", crate::technology::name(context.content, tech)),
            )
        })
        .collect();
    options.push(ChoiceOption::decline());
    let Ok(answer) = ask(
        context,
        player,
        "Ta Zern: purge which technology".to_owned(),
        HERO,
        "ta_zern_purge",
        options,
    ) else {
        return false;
    };
    let Some(tech) = candidates
        .into_iter()
        .find(|tech| tech.as_str() == answer.id)
    else {
        return false;
    };
    let before = context.state.clone();
    match hero_resolve(context, player, &tech) {
        Ok(()) => true,
        Err(_) => {
            *context.state = before;
            false
        }
    }
}

fn hero_resolve(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    tech: &TechnologyId,
) -> Result<(), TimingError> {
    // The hero's owner first, then the other seats in seating order from them.
    let order = context.state.seating_order.clone();
    let start = order.iter().position(|seat| seat == player).unwrap_or(0);
    let order: Vec<PlayerId> = order[start..]
        .iter()
        .chain(&order[..start])
        .cloned()
        .collect();
    let mut purged_owned = Vec::new();
    for seat in &order {
        if crate::technology::purge(context.state, context.content, context.sources, seat, tech) {
            purged_owned.push(seat.clone());
        }
    }
    mark_technology_purged(context.state, tech);
    for seat in purged_owned {
        // Doctor Carrina's window: each loser's research is a research (`deepwrought_research.rs`).
        let window = super::deepwrought_research::open(
            &mut super::deepwrought_research::Host::of(context),
            &seat,
            &super::deepwrought_research::any,
        )
        .map_err(TimingError::IllegalChoice)?;
        let result = ta_zern_research(context, &seat);
        super::deepwrought_research::settle(
            &mut super::deepwrought_research::Host::of(context),
            &seat,
            window,
            result.is_ok(),
        )
        .map_err(TimingError::IllegalChoice)?;
        result?;
    }
    Ok(())
}

/// One loser's "research another technology".
fn ta_zern_research(context: &mut TimingContext<'_>, seat: &PlayerId) -> Result<(), TimingError> {
    let options = researchable_with_met_prerequisites(context, seat);
    if options.is_empty() {
        return Ok(());
    }
    let chosen = ask(
        context,
        seat,
        "Ta Zern: research another technology".to_owned(),
        HERO,
        "ta_zern_research",
        options
            .iter()
            .map(|alias| {
                ChoiceOption::labelled(
                    alias.to_string(),
                    crate::strategy_cards::RESEARCH_KIND,
                    crate::technology::name(context.content, alias),
                )
            })
            .collect(),
    )?;
    if let Some(alias) = options
        .into_iter()
        .find(|alias| alias.as_str() == chosen.id)
    {
        crate::technology::research(
            context.state,
            context.content,
            context.sources,
            seat,
            &alias,
        );
    }
    Ok(())
}

fn researchable_with_met_prerequisites(
    context: &TimingContext<'_>,
    player: &PlayerId,
) -> Vec<TechnologyId> {
    crate::technology::researchable(context.state, context.content, context.sources, player)
        .into_iter()
        .filter(|tech| {
            crate::technology::prerequisites_met(
                context.state,
                context.content,
                context.sources,
                player,
                tech,
            )
        })
        .collect()
}

// -- hooks ---------------------------------------------------------------------------------------------------------

pub(crate) fn timing_abilities(owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    vec![
        share_returns(owner_name, seat),
        visionaria_readies(owner_name, seat),
    ]
}

pub(crate) fn component_actions(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
) -> Vec<ChoiceOption> {
    let mut actions = Vec::new();
    if !playable_notes(state, content, player).is_empty() {
        actions.push(ChoiceOption::labelled(
            SHARE_ACTION,
            crate::faction_abilities::ACTION_KIND,
            "Share Knowledge: place it faceup and gain a technology",
        ));
    }
    if visionaria_ready(state, player) && !visionaria_players(state, content, player).is_empty() {
        actions.push(ChoiceOption::labelled(
            VISIONARIA_ACTION,
            crate::faction_abilities::ACTION_KIND,
            "Visionaria Select: exhaust to let each other player spend 3 trade goods for a technology",
        ));
    }
    actions
}

pub(crate) fn perform_component(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    option: &ChoiceOption,
) -> bool {
    match option.id.as_str() {
        SHARE_ACTION => share_knowledge(context, player),
        VISIONARIA_ACTION => visionaria(context, player),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use ti4_model::id::{BreakthroughId, LeaderId, PlanetId, PlayerId, TechnologyId};
    use ti4_model::state::{GameState, LeaderStatus};

    use super::super::deepwrought::testkit::*;
    use super::super::deepwrought::{AGENT, oceans};
    use super::*;

    fn clear(state: &mut GameState, who: &PlayerId) {
        state.player_mut(who).expect("a seat").technologies.clear();
    }
    fn give(state: &mut GameState, who: &PlayerId, techs: &[&str]) {
        let seat = state.player_mut(who).expect("a seat");
        for tech in techs {
            seat.technologies.insert(TechnologyId::new(*tech));
        }
    }
    fn agent_status(state: &GameState) -> Option<LeaderStatus> {
        state
            .player(&a())
            .and_then(|seat| seat.leaders.get(&LeaderId::new(AGENT)).copied())
    }
    fn component(state: &GameState, who: &PlayerId) -> Vec<String> {
        crate::factions::component_actions(state, content(), who)
            .into_iter()
            .map(|option| option.id)
            .collect()
    }
    fn perform(state: &mut GameState, who: &PlayerId, id: &str, answers: &[&str]) -> bool {
        let mut table = scripted(answers);
        let option = crate::factions::component_actions(state, content(), who)
            .into_iter()
            .find(|option| option.id == id)
            .expect("the action is offered");
        crate::fixtures::with_context(
            state,
            ti4_model::content_types::DEFAULT,
            None,
            &mut table,
            |context| crate::factions::perform_component(context, who, &option),
        )
    }

    // -- Doctor Carrina -----------------------------------------------------------------------------

    /// `b` has nothing researched and controls a plain planet where it has an infantry.
    fn carrina_setup() -> (GameState, ti4_model::id::SystemId, PlanetId) {
        let mut state = game();
        clear(&mut state, &b());
        let (system, planet) = plain_planet();
        state.system_mut(&system).set_control(planet.clone(), b());
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &b(), 1);
        (state, system, planet)
    }

    #[test]
    fn doctor_carrina_lets_a_researcher_ignore_a_prerequisite_and_places_infantry_into_coexistence()
    {
        let (mut state, system, planet) = carrina_setup();
        let place = format!("{system}|{planet}");
        // Exhaust the agent, research a one-prerequisite technology with nothing owned, place.
        // The Technology primary's optional second research is then declined.
        let mut table = scripted(&["use", "dxa", place.as_str(), "decline"]);
        crate::strategy_cards::primary(
            &mut state,
            content(),
            ti4_model::content_types::DEFAULT,
            None,
            &mut table,
            &b(),
            "pok7technology",
        )
        .expect("resolves");
        assert!(owned(&state, &b(), "dxa"), "ignored the prerequisite");
        assert_eq!(agent_status(&state), Some(LeaderStatus::Exhausted));
        assert!(crate::coexistence::is_coexisting(
            &state,
            &system,
            &planet,
            &a()
        ));
        assert_eq!(
            state.system_state(&system).planet_control.get(&planet),
            Some(&b())
        );
        assert_eq!(infantry_on(&state, &system, &planet, &a()), 1);
        assert_eq!(infantry_on(&state, &system, &planet, &b()), 1);
        assert_eq!(
            oceans(&state, &a()),
            vec![PlanetId::new("ocean1")],
            "Oceanbound answered"
        );
        assert!(
            state
                .faction_marks
                .keys()
                .all(|key| !key.starts_with("deepwrought:carrina:")),
            "the waiver ended with the research"
        );
    }

    #[test]
    fn doctor_carrina_is_a_may_and_only_one_waiver_is_given() {
        let (state, _, _) = carrina_setup();
        let mut declined = state.clone();
        let mut table = scripted(&["decline", "dxa"]);
        // Declined: the one-prerequisite technology is closed to b, so the script cannot take it.
        let result = crate::strategy_cards::primary(
            &mut declined,
            content(),
            ti4_model::content_types::DEFAULT,
            None,
            &mut table,
            &b(),
            "pok7technology",
        );
        assert!(result.is_err(), "dxa was not offered without the waiver");
        assert_eq!(agent_status(&declined), Some(LeaderStatus::Readied));
        // A two-prerequisite technology is closed even with the waiver.
        let mut table = scripted(&["use", "hm"]);
        let mut twice = state.clone();
        let result = crate::strategy_cards::primary(
            &mut twice,
            content(),
            ti4_model::content_types::DEFAULT,
            None,
            &mut table,
            &b(),
            "pok7technology",
        );
        assert!(result.is_err(), "the waiver ignores only 1 prerequisite");
    }

    #[test]
    fn doctor_carrina_is_readied_again_when_the_research_gains_nothing() {
        let (mut state, ..) = carrina_setup();
        let mut table = scripted(&["use", "decline"]);
        crate::strategy_cards::secondary(
            &mut state,
            content(),
            ti4_model::content_types::DEFAULT,
            None,
            &mut table,
            &b(),
            "pok7technology",
        )
        .expect("resolves");
        assert_eq!(
            agent_status(&state),
            Some(LeaderStatus::Readied),
            "nothing was researched"
        );
        assert!(
            state
                .faction_marks
                .keys()
                .all(|key| !key.starts_with("deepwrought:carrina:"))
        );
    }

    #[test]
    fn doctor_carrina_does_not_ask_for_its_owners_own_research() {
        let mut state = game();
        clear(&mut state, &a());
        let mut table = scripted(&[]);
        let before = state.clone();
        crate::strategy_cards::primary(
            &mut state,
            content(),
            ti4_model::content_types::DEFAULT,
            None,
            &mut table,
            &a(),
            "pok7technology",
        )
        .expect("resolves");
        assert_eq!(
            agent_status(&state),
            agent_status(&before),
            "another player's research only"
        );
    }

    /// Operator ruling 2026-10-07: "if they do" means the researcher actually needed the ignored
    /// prerequisite. A technology they could take anyway gives the holder no placement.
    #[test]
    fn doctor_carrina_places_nothing_when_the_researcher_already_met_the_prerequisite() {
        let (mut state, system, planet) = carrina_setup();
        give(&mut state, &b(), &["nm"]); // green: Dacxive Animators' prerequisite is met
        let (mut table, seen) = steer(&["use", "dxa", "decline"]);
        crate::strategy_cards::primary(
            &mut state,
            content(),
            ti4_model::content_types::DEFAULT,
            None,
            &mut table,
            &b(),
            "pok7technology",
        )
        .expect("resolves");
        let prompts = seen.borrow().clone();
        assert!(
            prompts
                .iter()
                .any(|prompt| prompt.contains("Doctor Carrina: exhaust")),
            "a waiver would still open other technologies, so she is offered: {prompts:?}"
        );
        assert!(owned(&state, &b(), "dxa"));
        assert!(
            prompts
                .iter()
                .all(|prompt| !prompt.contains("place an infantry")),
            "no prerequisite was needed, so there is no placement: {prompts:?}"
        );
        assert!(!crate::coexistence::is_coexisting(
            &state,
            &system,
            &planet,
            &a()
        ));
        assert!(oceans(&state, &a()).is_empty());
    }

    #[test]
    fn doctor_carrina_is_not_offered_when_ignoring_a_prerequisite_opens_nothing() {
        let (mut state, ..) = carrina_setup();
        // b owns every generic technology but dxa: every technology they can take is already open.
        let everything: Vec<TechnologyId> =
            crate::technology::active_aliases(content(), ti4_model::content_types::DEFAULT)
                .into_iter()
                .filter(|tech| crate::technology::faction_of(content(), tech).is_none())
                .filter(|tech| tech.as_str() != "dxa")
                .collect();
        for tech in everything {
            state.player_mut(&b()).unwrap().technologies.insert(tech);
        }
        let (mut table, seen) = steer(&["use", "dxa", "decline"]);
        crate::strategy_cards::primary(
            &mut state,
            content(),
            ti4_model::content_types::DEFAULT,
            None,
            &mut table,
            &b(),
            "pok7technology",
        )
        .expect("resolves");
        assert!(
            seen.borrow()
                .iter()
                .all(|prompt| !prompt.contains("Doctor Carrina")),
            "{:?}",
            seen.borrow()
        );
        assert_eq!(agent_status(&state), Some(LeaderStatus::Readied));
    }

    /// A research route that is not a strategy card: Visionaria Select's research opens the window
    /// too (the Deepwrought holder of Carrina is "another player" to the one researching).
    #[test]
    fn doctor_carrina_also_reaches_a_research_made_through_visionaria_select() {
        let mut state = visionaria_setup();
        let (system, planet) = plain_planet();
        state.system_mut(&system).set_control(planet.clone(), b());
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &b(), 1);
        let given = giveable_notes(&state, content(), &b(), &a())[0].clone();
        let place = format!("{system}|{planet}");
        assert!(perform(
            &mut state,
            &a(),
            VISIONARIA_ACTION,
            &["accept", given.as_str(), "use", "dxa", place.as_str()]
        ));
        assert!(owned(&state, &b(), "dxa"), "b ignored the prerequisite");
        assert_eq!(agent_status(&state), Some(LeaderStatus::Exhausted));
        assert!(crate::coexistence::is_coexisting(
            &state,
            &system,
            &planet,
            &a()
        ));
        assert_eq!(infantry_on(&state, &system, &planet, &a()), 1);
    }

    // -- Share Knowledge ------------------------------------------------------------------------------

    fn share_setup() -> (GameState, String) {
        let mut state = game();
        clear(&mut state, &a());
        clear(&mut state, &b());
        give(&mut state, &a(), &["nm", "dxa", "hydrothermal", "cv2"]);
        let note = crate::promissory::note_id(SHARE_KNOWLEDGE, "deepwrought");
        state.promissory_notes.insert(note.clone(), b());
        (state, note)
    }

    #[test]
    fn share_knowledge_lends_a_non_faction_non_unit_upgrade_technology_until_the_status_phase_ends()
    {
        let (mut state, note) = share_setup();
        assert!(component(&state, &b()).contains(&SHARE_ACTION.to_owned()));
        assert!(
            !component(&state, &a()).contains(&SHARE_ACTION.to_owned()),
            "the owner has nothing to play"
        );
        // Only a generic technology the Deepwrought owns can be taken: a faction card is refused.
        let mut refused = state.clone();
        assert!(!perform(
            &mut refused,
            &b(),
            SHARE_ACTION,
            &["hydrothermal"]
        ));
        assert_eq!(refused, state, "nothing changed");
        assert!(perform(&mut state, &b(), SHARE_ACTION, &["dxa"]));
        assert!(owned(&state, &b(), "dxa"));
        assert!(owned(&state, &a(), "dxa"), "the Deepwrought keeps theirs");
        assert!(
            state.promissory_faceup.contains(&note),
            "faceup in b's play area"
        );
        assert_eq!(state.promissory_notes.get(&note), Some(&b()));
        assert!(
            !component(&state, &b()).contains(&SHARE_ACTION.to_owned()),
            "already played"
        );
        // At the end of the status phase the technology and the card go home.
        emit(&mut state, &[], "STATUS_PHASE_ENDED", &[]);
        assert!(!owned(&state, &b(), "dxa"));
        assert!(owned(&state, &a(), "dxa"));
        assert_eq!(state.promissory_notes.get(&note), Some(&a()));
        assert!(!state.promissory_faceup.contains(&note));
        assert!(
            state
                .faction_marks
                .keys()
                .all(|key| !key.starts_with(SHARE_PREFIX))
        );
    }

    #[test]
    fn share_knowledge_offers_nothing_a_holder_already_owns() {
        let (mut state, _) = share_setup();
        give(&mut state, &b(), &["nm", "dxa"]);
        assert!(!component(&state, &b()).contains(&SHARE_ACTION.to_owned()));
    }

    // -- Visionaria Select ----------------------------------------------------------------------------

    fn visionaria_setup() -> GameState {
        let mut state = game();
        crate::promissory::deal(&mut state, content(), ti4_model::content_types::DEFAULT);
        state.player_mut(&a()).unwrap().breakthrough = Some(BreakthroughId::new(BREAKTHROUGH));
        clear(&mut state, &a());
        clear(&mut state, &b());
        state.player_mut(&b()).unwrap().trade_goods = 3;
        state.player_mut(&c()).unwrap().trade_goods = 0;
        state
    }

    #[test]
    fn visionaria_select_sells_research_for_trade_goods_and_a_promissory_note() {
        let mut state = visionaria_setup();
        assert!(component(&state, &a()).contains(&VISIONARIA_ACTION.to_owned()));
        let given = giveable_notes(&state, content(), &b(), &a())[0].clone();
        assert!(perform(
            &mut state,
            &a(),
            VISIONARIA_ACTION,
            &["accept", given.as_str(), "decline", "nm"]
        ));
        assert_eq!(
            state.player(&b()).unwrap().trade_goods,
            0,
            "3 trade goods went to the supply"
        );
        assert_eq!(
            state.promissory_notes.get(&given),
            Some(&a()),
            "a holds the note now"
        );
        assert!(owned(&state, &b(), "nm"), "b researched");
        assert!(
            owned(&state, &a(), "nm"),
            "the Deepwrought gains it as well"
        );
        assert!(state.faction_marks.contains_key(&bt_exhausted_key(&a())));
        assert!(
            !component(&state, &a()).contains(&VISIONARIA_ACTION.to_owned()),
            "exhausted"
        );
        emit(&mut state, &[], "STATUS_PHASE_ENDED", &[]);
        assert!(
            !state.faction_marks.contains_key(&bt_exhausted_key(&a())),
            "readied"
        );
    }

    #[test]
    fn visionaria_select_is_a_may_for_each_player_and_researches_only_plain_technologies() {
        let mut state = visionaria_setup();
        assert!(perform(&mut state, &a(), VISIONARIA_ACTION, &["decline"]));
        assert_eq!(
            state.player(&b()).unwrap().trade_goods,
            3,
            "declined: nothing spent"
        );
        assert!(
            state.faction_marks.contains_key(&bt_exhausted_key(&a())),
            "the card is exhausted anyway"
        );
        // A player that pays may still decline the research.
        let mut state = visionaria_setup();
        let given = giveable_notes(&state, content(), &b(), &a())[0].clone();
        assert!(perform(
            &mut state,
            &a(),
            VISIONARIA_ACTION,
            &["accept", given.as_str(), "decline", "decline"]
        ));
        assert_eq!(state.player(&b()).unwrap().trade_goods, 0);
        assert!(!owned(&state, &a(), "nm"));
        // Faction and unit upgrade technologies are never on offer.
        let options =
            researchable_plain(&state, content(), ti4_model::content_types::DEFAULT, &b());
        assert!(
            options
                .iter()
                .all(|tech| crate::technology::faction_of(content(), tech).is_none())
        );
        assert!(
            options
                .iter()
                .all(|tech| !crate::technology::is_unit_upgrade(content(), tech))
        );
    }

    #[test]
    fn visionaria_select_needs_a_player_who_can_pay() {
        let mut state = visionaria_setup();
        state.player_mut(&b()).unwrap().trade_goods = 2;
        assert!(!component(&state, &a()).contains(&VISIONARIA_ACTION.to_owned()));
        let mut bare = game();
        assert!(
            !component(&bare, &a()).contains(&VISIONARIA_ACTION.to_owned()),
            "no breakthrough"
        );
        bare.player_mut(&a()).unwrap().breakthrough = Some(BreakthroughId::new(BREAKTHROUGH));
        bare.player_mut(&b()).unwrap().trade_goods = 0;
        bare.player_mut(&c()).unwrap().trade_goods = 0;
        assert!(!component(&bare, &a()).contains(&VISIONARIA_ACTION.to_owned()));
    }

    // -- Ta Zern --------------------------------------------------------------------------------------

    fn hero_setup() -> GameState {
        let mut state = game();
        for who in [a(), b(), c()] {
            clear(&mut state, &who);
        }
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new(HERO), LeaderStatus::Unlocked);
        state
    }

    fn use_hero(state: &mut GameState, answers: &[&str]) -> bool {
        let mut table = scripted(answers);
        crate::fixtures::with_context(
            state,
            ti4_model::content_types::DEFAULT,
            None,
            &mut table,
            |context| crate::leaders::use_leader(context, &a(), &LeaderId::new(HERO)),
        )
    }

    #[test]
    fn ta_zern_purges_a_technology_everywhere_and_each_loser_researches_another() {
        let mut state = hero_setup();
        give(&mut state, &a(), &["nm"]);
        give(&mut state, &b(), &["nm"]);
        assert!(
            crate::leaders::component_actions(&state, content(), &a())
                .iter()
                .any(|option| option.id == "component|leader|deepwroughthero")
        );
        assert!(use_hero(&mut state, &["nm", "pa", "decline", "pa"]));
        assert!(
            !owned(&state, &a(), "nm") && !owned(&state, &b(), "nm"),
            "purged from both"
        );
        assert!(
            owned(&state, &a(), "pa") && owned(&state, &b(), "pa"),
            "each researched another"
        );
        assert!(technology_purged(&state, &TechnologyId::new("nm")));
        assert!(
            !crate::technology::researchable(
                &state,
                content(),
                ti4_model::content_types::DEFAULT,
                &c()
            )
            .contains(&TechnologyId::new("nm")),
            "out of every deck"
        );
        assert_eq!(
            state
                .player(&a())
                .unwrap()
                .leaders
                .get(&LeaderId::new(HERO)),
            Some(&LeaderStatus::Purged)
        );
    }

    #[test]
    fn ta_zern_from_the_deck_only_makes_the_owners_of_it_research() {
        let mut state = hero_setup();
        give(&mut state, &b(), &["dxa"]);
        assert!(use_hero(&mut state, &["dxa", "decline", "nm"]));
        assert!(!owned(&state, &b(), "dxa"));
        assert!(owned(&state, &b(), "nm"), "b researched another");
        assert!(
            state.player(&a()).unwrap().technologies.is_empty(),
            "a owned nothing: no research"
        );
        assert!(technology_purged(&state, &TechnologyId::new("dxa")));
        // A unit upgrade is not a candidate.
        let candidates =
            hero_candidates(&state, content(), ti4_model::content_types::DEFAULT, &a());
        assert!(
            candidates
                .iter()
                .all(|tech| !crate::technology::is_unit_upgrade(content(), tech))
        );
        assert!(
            !candidates.contains(&TechnologyId::new("dxa")),
            "already purged"
        );
    }

    #[test]
    fn ta_zern_is_a_may_and_needs_to_be_unlocked() {
        let mut state = hero_setup();
        give(&mut state, &a(), &["nm"]);
        let before = state.clone();
        assert!(!use_hero(&mut state, &["decline"]));
        assert_eq!(
            state, before,
            "declining the technology purges nothing, not even the card"
        );
        let mut locked = hero_setup();
        locked
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new(HERO), LeaderStatus::Locked);
        assert!(!use_hero(&mut locked, &["nm"]));
        assert!(
            !crate::leaders::component_actions(&locked, content(), &a())
                .iter()
                .any(|option| option.id == "component|leader|deepwroughthero")
        );
    }
}
