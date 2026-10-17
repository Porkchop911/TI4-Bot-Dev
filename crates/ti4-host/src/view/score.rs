//! Where the victory points of every seat come from: the public objectives with who scored each,
//! and every other source of a point (secret objectives, the custodians, Imperial, Support for
//! the Throne, agendas, relics).
//!
//! Nothing here decides a rule. The objectives are read from what the game recorded as scored,
//! and the other sources from the ledger of the engine (`GameState::vp_ledger`), which has a row
//! for every point that was given or taken. Every fact here is public.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_model::content_types::ContentType;
use ti4_model::id::{ObjectiveId, PlayerId};
use ti4_model::state::GameState;

/// One public objective as printed, with the seats that scored it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PublicObjective {
    pub id: String,
    pub name: String,
    /// What the objective is worth: 1 for stage I, 2 for stage II.
    pub points: i32,
    pub text: String,
    /// The seats that scored it, sorted by id.
    pub scored: Vec<PlayerId>,
}

/// What kind of thing gave a point. The client words the row by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum PointSourceKind {
    /// Scored secret objectives, one point each.
    Secrets,
    /// The custodians token of Mecatol Rex.
    Custodians,
    /// The point of the Imperial strategy card for the seat that controls Mecatol Rex.
    Imperial,
    /// Support for the Throne in the play area.
    Support,
    Agenda,
    Relic,
    ActionCard,
    Planet,
    /// A reason of the ledger that this module does not know. `name` is the reason.
    Other,
}

/// One source of points that is not a public objective, with what each seat has from it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PointSource {
    pub kind: PointSourceKind,
    /// The printed name of the agenda, relic, card or planet. Empty for the first four kinds:
    /// there is one row of each.
    pub name: String,
    /// The points of each seat, gains and losses together. A seat with none is left out.
    pub points: BTreeMap<PlayerId, i32>,
    /// The printed text of the agenda, relic, card or planet. Empty for the first four kinds.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    /// For [`PointSourceKind::Support`]: whose notes the seat has, sorted by id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub from: BTreeMap<PlayerId, Vec<PlayerId>>,
}

/// Where the points of every seat come from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ScoreFacts {
    /// The points that win the game.
    pub target: i32,
    /// The revealed public objectives in the order of the reveal, then any that a seat scored and
    /// that is no longer revealed.
    pub public: Vec<PublicObjective>,
    /// The other sources. Secret objectives, the custodians, Imperial and Support for the Throne
    /// are always there, in this order; the rest only when a seat has a point from it.
    pub sources: Vec<PointSource>,
    /// The points of a seat that no row above explains. Empty in a game that the engine played.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub unexplained: BTreeMap<PlayerId, i32>,
}

/// The source of a row of the ledger, by its reason: its kind, its printed name and its printed
/// text. `None` for the objectives: they are read from what was scored.
fn source_of(content: &ContentStore, reason: &str) -> Option<(PointSourceKind, String, String)> {
    // The name of a record, and the texts under `keys`, one line each.
    let printed = |kind: PointSourceKind, category: ContentType, id: &str, keys: &[&str]| {
        let record = content.get(category, id);
        let name = record.and_then(|record| record.text("name")).unwrap_or(id);
        let text: Vec<&str> = keys
            .iter()
            .filter_map(|key| record.and_then(|record| record.text(key)))
            .collect();
        (kind, name.to_owned(), text.join("\n"))
    };
    let fixed = |kind| (kind, String::new(), String::new());
    Some(match reason {
        "objective" | "secret_objective" => return None,
        "custodians" => fixed(PointSourceKind::Custodians),
        "imperial_primary" => fixed(PointSourceKind::Imperial),
        "support_for_the_throne" => fixed(PointSourceKind::Support),
        "imperial_rider" => printed(
            PointSourceKind::ActionCard,
            ContentType::ActionCards,
            reason,
            &["text"],
        ),
        "styx" => printed(
            PointSourceKind::Planet,
            ContentType::Planets,
            reason,
            &["legendaryAbilityText"],
        ),
        _ => {
            if let Some(agenda) = reason.strip_prefix("agenda:") {
                // An agenda has the text of each outcome.
                printed(
                    PointSourceKind::Agenda,
                    ContentType::Agendas,
                    agenda,
                    &["text1", "text2"],
                )
            } else if content.get(ContentType::Relics, reason).is_some() {
                printed(
                    PointSourceKind::Relic,
                    ContentType::Relics,
                    reason,
                    &["text"],
                )
            } else {
                (PointSourceKind::Other, reason.to_owned(), String::new())
            }
        }
    })
}

/// Projects where the points of every seat of `state` come from.
#[must_use]
pub fn project_score_facts(state: &GameState, content: &ContentStore) -> ScoreFacts {
    let is_secret = |id: &ObjectiveId| {
        content
            .get(ContentType::SecretObjectives, id.as_str())
            .is_some()
    };
    let revealed: BTreeSet<&ObjectiveId> = state.revealed_objectives.iter().collect();
    // An objective that was scored and is not secret was public when it was scored.
    let gone: BTreeSet<&ObjectiveId> = state
        .scored_objectives
        .values()
        .flatten()
        .filter(|id| !revealed.contains(id) && !is_secret(id))
        .collect();
    let public: Vec<PublicObjective> = state
        .revealed_objectives
        .iter()
        .chain(gone)
        .map(|id| {
            // Classified Document Leaks makes a secret objective public.
            let record = [ContentType::PublicObjectives, ContentType::SecretObjectives]
                .into_iter()
                .find_map(|category| content.get(category, id.as_str()));
            let printed = |key: &str| record.and_then(|record| record.text(key));
            PublicObjective {
                id: id.as_str().to_owned(),
                name: printed("name").unwrap_or(id.as_str()).to_owned(),
                points: ti4_engine::objectives::points_for(content, id).unwrap_or(1),
                text: printed("text").unwrap_or_default().to_owned(),
                scored: state
                    .players
                    .iter()
                    .filter(|player| state.scored_by(&player.id).contains(id))
                    .map(|player| player.id.clone())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
            }
        })
        .collect();

    let fixed = |kind| PointSource {
        kind,
        name: String::new(),
        text: String::new(),
        points: BTreeMap::new(),
        from: BTreeMap::new(),
    };
    let mut secrets = fixed(PointSourceKind::Secrets);
    for player in &state.players {
        // The same test as `crate::view::secret_objectives`, without an objective that is public now.
        let scored: BTreeSet<&str> = state
            .scored_objectives
            .get(&player.id)
            .into_iter()
            .flatten()
            .filter(|id| is_secret(id) && !revealed.contains(id))
            .map(ObjectiveId::as_str)
            .chain(player.plot_objectives.iter().map(|id| id.as_str()))
            .collect();
        if let Ok(count @ 1..) = i32::try_from(scored.len()) {
            secrets.points.insert(player.id.clone(), count);
        }
    }

    // By kind and name, with the printed text of the source.
    let mut ledger: BTreeMap<(PointSourceKind, String), (String, BTreeMap<PlayerId, i32>)> =
        BTreeMap::new();
    for kind in [
        PointSourceKind::Custodians,
        PointSourceKind::Imperial,
        PointSourceKind::Support,
    ] {
        ledger.insert((kind, String::new()), Default::default());
    }
    for (player, delta, reason) in &state.vp_ledger {
        if let Some((kind, name, text)) = source_of(content, reason) {
            let row = ledger.entry((kind, name)).or_default();
            row.0 = text;
            *row.1.entry(player.clone()).or_default() += delta;
        }
    }
    let mut sources = vec![secrets];
    for ((kind, name), (text, mut points)) in ledger {
        points.retain(|_, points| *points != 0);
        let always = matches!(
            kind,
            PointSourceKind::Custodians | PointSourceKind::Imperial | PointSourceKind::Support
        );
        if points.is_empty() && !always {
            continue;
        }
        let mut from: BTreeMap<PlayerId, Vec<PlayerId>> = BTreeMap::new();
        if kind == PointSourceKind::Support {
            // Sorted by the owner, as the map is.
            for (owner, holder) in &state.support_holders {
                from.entry(holder.clone()).or_default().push(owner.clone());
            }
        }
        sources.push(PointSource {
            kind,
            name,
            text,
            points,
            from,
        });
    }

    let unexplained = state
        .players
        .iter()
        .filter_map(|player| {
            let explained: i32 = public
                .iter()
                .filter(|objective| objective.scored.contains(&player.id))
                .map(|objective| objective.points)
                .chain(
                    sources
                        .iter()
                        .filter_map(|source| source.points.get(&player.id).copied()),
                )
                .sum();
            let rest = player.victory_points - explained;
            (rest != 0).then(|| (player.id.clone(), rest))
        })
        .collect();
    ScoreFacts {
        target: ti4_engine::objectives::VICTORY_TARGET,
        public,
        sources,
        unexplained,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_engine::objectives::adjust_victory_points;
    use ti4_model::content_types::POK;

    fn game() -> GameState {
        let content = ContentStore::embedded();
        let ids: Vec<PlayerId> = ["a", "b", "c", "d", "e", "f"]
            .iter()
            .map(|name| PlayerId::new(*name))
            .collect();
        let loader = crate::maps::TemplateLoader::load().expect("the templates load");
        let template =
            crate::maps::default_template_for(content, &loader, 6, POK).expect("a template");
        crate::map::create_game_with_template(content, &ids, 42, Some(&template))
            .expect("a game")
            .0
    }

    fn source<'a>(facts: &'a ScoreFacts, kind: PointSourceKind, name: &str) -> &'a PointSource {
        facts
            .sources
            .iter()
            .find(|source| source.kind == kind && source.name == name)
            .unwrap_or_else(|| panic!("no source {kind:?} {name:?}"))
    }

    /// The points of a seat as the rows of the facts say.
    fn sum(facts: &ScoreFacts, seat: &PlayerId) -> i32 {
        let public: i32 = facts
            .public
            .iter()
            .filter(|objective| objective.scored.contains(seat))
            .map(|objective| objective.points)
            .sum();
        let other: i32 = facts
            .sources
            .iter()
            .filter_map(|source| source.points.get(seat))
            .sum();
        public + other + facts.unexplained.get(seat).copied().unwrap_or(0)
    }

    #[test]
    fn a_new_game_has_the_four_fixed_sources_and_no_points() {
        let facts = project_score_facts(&game(), ContentStore::embedded());

        assert_eq!(facts.target, 10);
        let kinds: Vec<_> = facts.sources.iter().map(|source| source.kind).collect();
        assert_eq!(
            kinds,
            [
                PointSourceKind::Secrets,
                PointSourceKind::Custodians,
                PointSourceKind::Imperial,
                PointSourceKind::Support,
            ]
        );
        assert!(facts.sources.iter().all(|source| source.points.is_empty()));
        assert!(facts.public.iter().all(|public| public.scored.is_empty()));
        assert!(facts.unexplained.is_empty());
    }

    #[test]
    fn every_point_is_in_the_row_of_its_source() {
        let content = ContentStore::embedded();
        let mut state = game();
        let [a, b, c] = ["a", "b", "c"].map(PlayerId::new);
        let score = |state: &mut GameState, seat: &PlayerId, id: &str, reason: &str| {
            let id = ObjectiveId::new(id);
            let points = ti4_engine::objectives::points_for(content, &id).expect("printed");
            state.record_score(seat, id);
            adjust_victory_points(state, seat, points, reason);
        };
        state.revealed_objectives =
            vec![ObjectiveId::new("expand_borders"), ObjectiveId::new("lead")];
        let stage_two = content
            .records(ContentType::PublicObjectives)
            .iter()
            .find(|record| record.int("points") == Some(2))
            .and_then(|record| record.text("alias"))
            .expect("a stage II objective")
            .to_owned();
        state
            .revealed_objectives
            .push(ObjectiveId::new(stage_two.as_str()));
        score(&mut state, &a, "expand_borders", "objective");
        score(&mut state, &b, "expand_borders", "objective");
        score(&mut state, &a, &stage_two, "objective");
        let secret = state.secret_deck[0].clone();
        score(&mut state, &a, secret.as_str(), "secret_objective");
        adjust_victory_points(&mut state, &b, 1, "custodians");
        adjust_victory_points(&mut state, &b, 1, "imperial_primary");
        adjust_victory_points(&mut state, &b, 1, "imperial_primary");
        state.support_holders.insert(c.clone(), a.clone());
        adjust_victory_points(&mut state, &a, 1, "support_for_the_throne");
        adjust_victory_points(&mut state, &a, 1, "agenda:mutiny");
        adjust_victory_points(&mut state, &b, 1, "agenda:mutiny");
        adjust_victory_points(&mut state, &b, -1, "agenda:mutiny");
        adjust_victory_points(&mut state, &c, 1, "shard");
        adjust_victory_points(&mut state, &c, 1, "a reason of tomorrow");
        let facts = project_score_facts(&state, content);

        assert_eq!(facts.public.len(), 3);
        assert_eq!(facts.public[0].name, "Expand Borders");
        assert_eq!(
            facts.public[0].text,
            "Control 6 planets in non-home systems."
        );
        assert_eq!(facts.public[0].scored, [a.clone(), b.clone()]);
        assert_eq!(facts.public[2].points, 2);
        assert_eq!(facts.public[2].scored, std::slice::from_ref(&a));
        let points = |kind, name: &str| source(&facts, kind, name).points.clone();
        assert_eq!(
            points(PointSourceKind::Secrets, ""),
            BTreeMap::from([(a.clone(), 1)])
        );
        assert_eq!(
            points(PointSourceKind::Custodians, ""),
            BTreeMap::from([(b.clone(), 1)])
        );
        assert_eq!(
            points(PointSourceKind::Imperial, ""),
            BTreeMap::from([(b.clone(), 2)])
        );
        let support = source(&facts, PointSourceKind::Support, "");
        assert_eq!(support.points, BTreeMap::from([(a.clone(), 1)]));
        assert_eq!(support.from, BTreeMap::from([(a.clone(), vec![c.clone()])]));
        // The gain and the loss of seat b are one cell, and a cell of zero is left out.
        assert_eq!(
            points(PointSourceKind::Agenda, "Mutiny"),
            BTreeMap::from([(a.clone(), 1)])
        );
        assert_eq!(
            points(PointSourceKind::Relic, "Shard of the Throne"),
            BTreeMap::from([(c.clone(), 1)])
        );
        // A source with a printed card has its text: an agenda has both outcomes.
        let mutiny = &source(&facts, PointSourceKind::Agenda, "Mutiny").text;
        assert!(
            mutiny.starts_with("For: Each player that voted"),
            "{mutiny}"
        );
        assert!(mutiny.contains("\nAgainst: "), "{mutiny}");
        assert!(
            source(&facts, PointSourceKind::Relic, "Shard of the Throne")
                .text
                .starts_with("When you gain this card, gain 1 victory point")
        );
        assert_eq!(support.text, "");
        assert_eq!(
            points(PointSourceKind::Other, "a reason of tomorrow"),
            BTreeMap::from([(c.clone(), 1)])
        );
        assert!(facts.unexplained.is_empty());
        for player in &state.players {
            assert_eq!(sum(&facts, &player.id), player.victory_points);
        }
    }

    #[test]
    fn a_point_with_no_row_of_the_ledger_is_unexplained() {
        let mut state = game();
        state.players[0].victory_points = 2;
        let facts = project_score_facts(&state, ContentStore::embedded());

        assert_eq!(
            facts.unexplained,
            BTreeMap::from([(state.players[0].id.clone(), 2)])
        );
    }

    #[test]
    fn the_kind_of_a_source_is_a_word_on_the_wire() {
        let json = serde_json::to_value(PointSourceKind::ActionCard).expect("json");
        assert_eq!(json, "action_card");
    }
}
