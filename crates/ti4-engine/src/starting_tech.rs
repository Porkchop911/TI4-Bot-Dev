//! Starting technology that a seat chooses (the "Gather Starting Components" step of setup).
//!
//! Most factions start with a fixed list, which [`crate::seating::deploy`] deals out. Four kinds
//! of seat choose instead, and only a game made with [`open`] offers the choice (the pending picks
//! are [`GameState::starting_tech_picks`]; a game made without them -- every game saved before the
//! choice existed -- keeps the fixed technologies it was seated with and asks nothing):
//!
//! * **Winnu**: any 1 technology that has no prerequisites.
//! * **Argent Flight**: 2 of Neural Motivator, Sarween Tools, Plasma Scoring (the one left behind
//!   stays in the deck).
//! * **Thunder's Edge**: one technology of two colours with no prerequisites (Last Bastion blue or
//!   yellow, Crimson Rebellion red or blue, Ral Nel red or green, Firmament green or yellow); the
//!   Deepwrought Scholarate researches twice during setup, price waived.
//! * **Council Keleres**: 2 non-faction technologies that other players own, chosen after every
//!   other seat has chosen; only as many as there are.
//!
//! The picks are asked one at a time, at the start of round 1's strategy phase before the first
//! card is drafted, in seat order with the Keleres last. A pick with nothing to choose (one option
//! left, or as many options as picks) is settled without a decision.

use ti4_content::ContentStore;
use ti4_content::factions;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{PlayerId, TechnologyId};
use ti4_model::state::{GameState, StartingTechPick, StartingTechSource};

use crate::choice::{Choice, ChoiceOption, IllegalChoice, validate};
use crate::decision_context::{DecisionContext, DecisionSource};

/// The subtype of the decision.
pub const SUBTYPE: &str = "starting_technology";
/// The kind of its options.
pub const KIND: &str = "starting_technology";
/// How many non-faction technologies of the other players the Council Keleres take.
pub const KELERES_PICKS: u8 = 2;

/// Queue the picks that the seats of `state` owe and settle those that need no decision. Call it
/// once, on a game that is being made: its seats are deployed and nothing has been played.
pub fn open(state: &mut GameState, content: &ContentStore, sources: SourceSet) {
    let mut picks = Vec::new();
    let mut last = Vec::new();
    for seat in &state.players {
        let alias = seat.faction.as_str();
        if crate::factions::keleres::VARIANTS.contains(&alias) {
            last.push(StartingTechPick {
                player: seat.id.clone(),
                of: KELERES_PICKS,
                remaining: KELERES_PICKS,
                source: StartingTechSource::OwnedByOthers,
            });
            continue;
        }
        let Some(faction) = factions::get(content, alias) else {
            continue;
        };
        let Some(amount) = faction.starting_tech_amount().filter(|n| *n > 0) else {
            continue;
        };
        let listed: Vec<TechnologyId> = faction
            .starting_tech_options()
            .iter()
            .filter_map(|t| content.resolve_id(ContentType::Technologies, t, sources))
            .map(TechnologyId::new)
            .collect();
        let source = if faction.starting_tech_options().is_empty() {
            StartingTechSource::Researchable
        } else {
            StartingTechSource::Listed(listed)
        };
        picks.push(StartingTechPick {
            player: seat.id.clone(),
            of: amount,
            remaining: amount,
            source,
        });
    }
    picks.extend(last);
    state.starting_tech_picks = picks;
    settle(state, content, sources);
}

/// The technologies a pick offers now, in a stable order.
#[must_use]
pub fn options(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    pick: &StartingTechPick,
) -> Vec<TechnologyId> {
    let Some(me) = state.player(&pick.player) else {
        return Vec::new();
    };
    let mut open: Vec<TechnologyId> = match &pick.source {
        StartingTechSource::Listed(listed) => listed.clone(),
        StartingTechSource::OwnedByOthers => {
            let active = crate::technology::active_aliases(content, sources);
            state
                .players
                .iter()
                .filter(|seat| seat.id != pick.player)
                .flat_map(|seat| seat.technologies.iter().cloned())
                .filter(|t| active.contains(t))
                .filter(|t| crate::technology::faction_of(content, t).is_none())
                .collect()
        }
        StartingTechSource::Researchable => crate::technology::active_aliases(content, sources)
            .into_iter()
            .filter(|t| {
                crate::technology::can_research(state, content, sources, &pick.player, t)
                    && crate::technology::prerequisites_met(
                        state,
                        content,
                        sources,
                        &pick.player,
                        t,
                    )
            })
            .collect(),
    };
    open.retain(|t| !me.technologies.contains(t));
    open.sort();
    open.dedup();
    open
}

/// Give out every pick that has nothing to choose, from the front of the line, until one is left
/// that needs an answer. Deterministic from the state, so a replay settles the same picks.
pub fn settle(state: &mut GameState, content: &ContentStore, sources: SourceSet) {
    while let Some(pick) = state.starting_tech_picks.first().cloned() {
        let open = options(state, content, sources, &pick);
        let forced: Vec<TechnologyId> = match (&pick.source, open.len()) {
            (_, 0) => Vec::new(),
            (StartingTechSource::Researchable, 1) => open.clone(),
            (StartingTechSource::Researchable, _) => break,
            (_, n) if n <= usize::from(pick.remaining) => open.clone(),
            _ => break,
        };
        for technology in &forced {
            crate::technology::gain(state, content, sources, &pick.player, technology);
        }
        let given = u8::try_from(forced.len()).unwrap_or(u8::MAX);
        let left = if forced.is_empty() {
            0
        } else {
            pick.remaining.saturating_sub(given)
        };
        if left == 0 {
            state.starting_tech_picks.remove(0);
        } else if let Some(front) = state.starting_tech_picks.first_mut() {
            front.remaining = left;
        }
    }
}

/// The pick that is asked now, if any. `open` and `apply` leave the front pick one that needs an
/// answer.
#[must_use]
pub fn choice(state: &GameState, content: &ContentStore, sources: SourceSet) -> Option<Choice> {
    let pick = state.starting_tech_picks.first()?;
    let open = options(state, content, sources, pick);
    if open.is_empty() {
        return None;
    }
    let number = pick.of - pick.remaining + 1;
    let mut options: Vec<ChoiceOption> = open
        .iter()
        .map(|id| {
            let mut option =
                ChoiceOption::labelled(id.to_string(), KIND, crate::technology::name(content, id));
            if let Some(colour) = crate::technology::colour_type(content, id) {
                option = option.with("colour", colour);
            }
            // Whose card it is, for the Keleres: the seats that hold it.
            let owners: Vec<String> = state
                .players
                .iter()
                .filter(|seat| seat.id != pick.player && seat.technologies.contains(id))
                .map(|seat| seat.id.to_string())
                .collect();
            if !owners.is_empty() {
                option = option.with("owners", owners);
            }
            option.with(
                "prerequisites",
                crate::technology::prerequisites(content, id)
                    .values()
                    .sum::<usize>() as u64,
            )
        })
        .collect();
    options.sort_by(|a, b| a.id.cmp(&b.id));
    let prompt = if pick.of > 1 {
        format!("choose a starting technology ({number} of {})", pick.of)
    } else {
        "choose a starting technology".to_owned()
    };
    let rule = match pick.source {
        StartingTechSource::Listed(_) => "listed",
        StartingTechSource::OwnedByOthers => "owned_by_others",
        StartingTechSource::Researchable => "researchable",
    };
    let faction = state
        .player(&pick.player)
        .map(|seat| seat.faction.to_string())
        .unwrap_or_default();
    Some(
        Choice::new(pick.player.clone(), prompt, options)
            .detailed("pick", u64::from(number))
            .detailed("of", u64::from(pick.of))
            .detailed("rule", rule)
            .detailed("faction", faction)
            .detailed(
                "held",
                state
                    .player(&pick.player)
                    .map(|seat| {
                        seat.technologies
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
            )
            .contextualized(DecisionContext::new(
                pick.player.clone(),
                DecisionSource::Content("starting_technology".to_owned()),
                SUBTYPE,
                state.phase,
                state.round,
            )),
    )
}

/// Take one offered technology for the seat whose pick is asked.
///
/// # Errors
/// [`IllegalChoice`] when `answer` was not offered (or nothing is asked).
pub fn apply(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    answer: ChoiceOption,
) -> Result<PlayerId, IllegalChoice> {
    let asked = choice(state, content, sources).ok_or_else(|| IllegalChoice::NotOffered {
        player: PlayerId::new("-"),
        chosen: answer.id.clone(),
        offered: Vec::new(),
    })?;
    let answer = validate(&asked, answer)?;
    let player = asked.player;
    let technology = TechnologyId::new(answer.id);
    crate::technology::gain(state, content, sources, &player, &technology);
    if let Some(front) = state.starting_tech_picks.first_mut() {
        front.remaining = front.remaining.saturating_sub(1);
        if front.remaining == 0 {
            state.starting_tech_picks.remove(0);
        }
    }
    settle(state, content, sources);
    Ok(player)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Game;
    use crate::seating::deploy;
    use crate::setup::start_game;
    use ti4_model::content_types::{BASE, POK};
    use ti4_model::id::FactionId;

    fn content() -> &'static ContentStore {
        ContentStore::embedded()
    }

    fn table(pairs: &[(&str, &str)], sources: SourceSet) -> GameState {
        let ids: Vec<PlayerId> = pairs.iter().map(|(p, _)| PlayerId::new(*p)).collect();
        let mut state = start_game(content(), &ids, sources, None).unwrap();
        for (player, alias) in pairs {
            deploy(
                &mut state,
                content(),
                &PlayerId::new(*player),
                &FactionId::new(*alias),
                sources,
            )
            .unwrap();
        }
        open(&mut state, content(), sources);
        state
    }

    fn ask(state: &GameState, sources: SourceSet) -> Choice {
        choice(state, content(), sources).expect("a pick is asked")
    }

    fn answer(state: &mut GameState, sources: SourceSet, id: &str) -> PlayerId {
        let offered = ask(state, sources);
        let option = offered.option(id).unwrap_or_else(|| {
            panic!("{id} is not offered: {:?}", offered.ids());
        });
        apply(state, content(), sources, option.clone()).unwrap()
    }

    fn owns(state: &GameState, player: &str, tech: &str) -> bool {
        state
            .player(&PlayerId::new(player))
            .unwrap()
            .technologies
            .contains(&TechnologyId::new(tech))
    }

    #[test]
    fn fixed_factions_ask_nothing() {
        let state = table(&[("a", "sol"), ("b", "hacan"), ("c", "sardakk")], POK);
        assert!(state.starting_tech_picks.is_empty());
        assert!(choice(&state, content(), POK).is_none());
        assert!(
            state
                .player(&PlayerId::new("c"))
                .unwrap()
                .technologies
                .is_empty(),
            "the Sardakk N'orr start with no technology, as printed"
        );
    }

    #[test]
    fn the_winnu_pick_any_one_technology_without_prerequisites() {
        let mut state = table(&[("a", "winnu"), ("b", "sol")], POK);
        let offered = ask(&state, POK);
        assert_eq!(offered.player, PlayerId::new("a"));
        assert_eq!(
            offered.ids(),
            vec!["aida", "amd", "det", "nm", "pa", "ps", "sdn", "st"]
        );
        assert!(
            offered
                .options
                .iter()
                .all(|o| o.payload["prerequisites"] == 0)
        );
        assert!(
            state
                .player(&PlayerId::new("a"))
                .unwrap()
                .technologies
                .is_empty()
        );
        answer(&mut state, POK, "det");
        assert!(owns(&state, "a", "det"));
        assert_eq!(
            state
                .player(&PlayerId::new("a"))
                .unwrap()
                .technologies
                .len(),
            1
        );
        assert!(state.starting_tech_picks.is_empty());
    }

    #[test]
    fn without_the_prophecy_of_kings_the_winnu_choose_among_the_four_base_technologies() {
        let state = table(&[("a", "winnu"), ("b", "sol")], BASE);
        assert_eq!(ask(&state, BASE).ids(), vec!["amd", "nm", "ps", "st"]);
    }

    #[test]
    fn the_argent_flight_pick_two_of_three_and_the_third_stays_in_the_deck() {
        let mut state = table(&[("a", "argent"), ("b", "sol")], POK);
        let first = ask(&state, POK);
        assert_eq!(first.ids(), vec!["nm", "ps", "st"]);
        assert_eq!(first.details["pick"], 1);
        assert_eq!(first.details["of"], 2);
        answer(&mut state, POK, "st");
        let second = ask(&state, POK);
        assert_eq!(second.ids(), vec!["nm", "ps"]);
        assert_eq!(second.details["pick"], 2);
        answer(&mut state, POK, "nm");
        assert!(owns(&state, "a", "st") && owns(&state, "a", "nm"));
        assert!(
            !owns(&state, "a", "ps"),
            "the one left behind stays in the deck"
        );
        assert!(state.starting_tech_picks.is_empty());
    }

    #[test]
    fn the_keleres_choose_last_among_the_technologies_of_the_others() {
        let mut state = table(&[("k", "keleresm"), ("a", "argent"), ("b", "hacan")], POK);
        // Hacan: amd, st. The Argent pick before the Keleres.
        assert_eq!(ask(&state, POK).player, PlayerId::new("a"));
        answer(&mut state, POK, "nm");
        answer(&mut state, POK, "ps");
        let keleres = ask(&state, POK);
        assert_eq!(keleres.player, PlayerId::new("k"));
        assert_eq!(keleres.ids(), vec!["amd", "nm", "ps", "st"]);
        answer(&mut state, POK, "amd");
        assert_eq!(ask(&state, POK).ids(), vec!["nm", "ps", "st"]);
        answer(&mut state, POK, "ps");
        assert!(owns(&state, "k", "amd") && owns(&state, "k", "ps"));
    }

    #[test]
    fn the_keleres_take_only_as_many_as_the_others_hold_and_never_a_faction_technology() {
        // Sardakk hold nothing; Nekro hold the Valefar Assimilators (faction technologies) and
        // Dacxive Animators, the only technology on offer: taken without a question.
        let state = table(&[("k", "keleresx"), ("n", "nekro"), ("s", "sardakk")], POK);
        assert!(state.starting_tech_picks.is_empty());
        assert!(owns(&state, "k", "dxa"));
        assert!(!owns(&state, "k", "vax") && !owns(&state, "k", "vay"));
        assert_eq!(
            state
                .player(&PlayerId::new("k"))
                .unwrap()
                .technologies
                .len(),
            1
        );
    }

    #[test]
    fn a_pick_with_as_many_options_as_picks_is_not_asked() {
        // Two Argent seats cannot both... each still picks from its own three; one Keleres facing
        // exactly two distinct technologies takes both.
        let state = table(&[("k", "keleresa"), ("s", "sol")], POK);
        assert!(state.starting_tech_picks.is_empty());
        assert!(owns(&state, "k", "amd") && owns(&state, "k", "nm"));
    }

    #[test]
    fn a_game_asks_the_picks_before_the_first_strategy_card() {
        let mut state = table(&[("a", "winnu"), ("b", "argent")], POK);
        state.speaker = PlayerId::new("a");
        let mut game = Game::with_seeded_random(state, content(), 7).with_sources(POK);
        let first = game.legal_options().expect("a question");
        assert_eq!(first.context.as_ref().unwrap().subtype, SUBTYPE);
        let mut asked = 0;
        while game
            .legal_options()
            .is_some_and(|c| c.context.is_some_and(|x| x.subtype == SUBTYPE))
        {
            assert!(game.step().resolved_choice);
            asked += 1;
            assert!(asked < 10);
        }
        assert_eq!(asked, 3, "one Winnu pick, two Argent picks");
        assert!(game.state.starting_tech_picks.is_empty());
        let next = game.legal_options().expect("then the draft");
        assert_eq!(next.context.unwrap().subtype, "draft_strategy_card");
    }

    #[test]
    fn the_deepwrought_research_twice_and_the_second_may_use_the_first() {
        use ti4_model::content_types::FULL;
        let mut state = table(&[("d", "deepwrought"), ("s", "sol")], FULL);
        let first = ask(&state, FULL);
        assert_eq!(first.player, PlayerId::new("d"));
        assert!(
            first
                .options
                .iter()
                .all(|o| o.payload["prerequisites"] == 0)
        );
        assert!(first.option("nm").is_some() && first.option("dxa").is_none());
        answer(&mut state, FULL, "nm");
        let second = ask(&state, FULL);
        assert_eq!(second.details["pick"], 2);
        assert!(
            second.option("dxa").is_some(),
            "Neural Motivator opens the green track"
        );
        assert!(second.option("nm").is_none());
        answer(&mut state, FULL, "dxa");
        assert!(owns(&state, "d", "nm") && owns(&state, "d", "dxa"));
        assert!(state.starting_tech_picks.is_empty());
    }

    #[test]
    fn a_state_without_picks_serializes_without_the_key() {
        let state = table(&[("a", "sol")], POK);
        let text = serde_json::to_string(&state).unwrap();
        assert!(!text.contains("starting_tech_picks"));
    }
}
