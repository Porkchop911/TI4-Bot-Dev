//! Which planner covers which objective of the corpus: the 40 public and 40 secret objectives,
//! each mapped to its goal family by the code path the bot uses ([`crate::objectives::goals`]) and
//! matched against the registry. Test support, plus `print_coverage_table` (ignored; run with
//! `cargo test -p ti4-rule-bot print_coverage_table -- --ignored --nocapture`) which prints the
//! table kept in `docs/BACKWARD_PLANNING_B.md`.
#![cfg(test)]

use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_engine::fixtures::{game, plain_hub};
use ti4_model::POK;
use ti4_model::content_types::ContentType;
use ti4_model::id::{ObjectiveId, PlayerId, SecretObjectiveId};

use super::{ObjectivePlanner, registry};
use crate::objectives::Goal;

/// One objective of the corpus.
#[derive(Debug, Clone)]
pub struct Row {
    pub alias: String,
    pub name: String,
    pub public: bool,
    pub points: i64,
    /// The goal family the bot sees, or `None` when the engine reports no progress for it.
    pub family: Option<String>,
}

impl Row {
    pub fn goal(&self) -> Goal {
        Goal {
            alias: self.alias.clone(),
            secret: !self.public,
            points: 1.0,
            bought: false,
            have: 0.0,
            threshold: 1.0,
            satisfied: false,
            family: self.family.clone().unwrap_or_default(),
        }
    }
}

struct Probe(Vec<Goal>);
impl Decider for Probe {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        Ok(choice.options[0].clone())
    }
    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        self.0 = crate::objectives::goals(
            seen,
            &PlayerId::new("a"),
            &ti4_engine::objectives::Imagined::NONE,
        );
        Ok(choice.options[0].clone())
    }
}

/// All 80 objectives with their goal family.
pub fn rows() -> Vec<Row> {
    let content = ContentStore::embedded();
    let hub = plain_hub();
    let mut state = game(&["a", "b"]);
    let records = |kind: ContentType| -> Vec<(String, String, i64)> {
        content
            .records(kind)
            .iter()
            .map(|r| {
                (
                    r.text("alias").unwrap_or_default().to_owned(),
                    r.text("name").unwrap_or_default().to_owned(),
                    r.int("points").unwrap_or(1),
                )
            })
            .collect()
    };
    let publics = records(ContentType::PublicObjectives);
    let secrets = records(ContentType::SecretObjectives);
    state.revealed_objectives = publics
        .iter()
        .map(|(a, _, _)| ObjectiveId::new(a))
        .collect();
    state
        .player_mut(&PlayerId::new("a"))
        .expect("seat")
        .secret_objectives = secrets
        .iter()
        .map(|(a, _, _)| SecretObjectiveId::new(a))
        .collect();
    let mut probe = Probe(Vec::new());
    let choice = Choice::new(
        PlayerId::new("a"),
        "x",
        vec![ChoiceOption::labelled("o", "o", "o")],
    );
    ti4_engine::choice::ask_private(&choice, &state, content, POK, Some(&hub.galaxy), &mut probe)
        .expect("legal");
    let family = |alias: &str, secret: bool| {
        probe
            .0
            .iter()
            .find(|g| g.alias == alias && g.secret == secret)
            .map(|g| g.family.clone())
    };
    let mut out = Vec::new();
    for (alias, name, points) in publics {
        let f = family(&alias, false);
        out.push(Row {
            alias,
            name,
            public: true,
            points,
            family: f,
        });
    }
    for (alias, name, points) in secrets {
        let f = family(&alias, true);
        out.push(Row {
            alias,
            name,
            public: false,
            points,
            family: f,
        });
    }
    out
}

/// The aliases (sorted) of the rows a planner matches by alias or family.
pub fn matched(planner: &dyn ObjectivePlanner) -> Vec<String> {
    let mut out: Vec<String> = rows()
        .iter()
        .filter(|r| r.goal().matches_any(planner.objective_ids()))
        .map(|r| r.alias.clone())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// A planner's documented alias list, sorted.
pub fn sorted(covers: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = covers.iter().map(|s| (*s).to_owned()).collect();
    v.sort();
    v
}

/// Every planner with its file name, in dispatcher order.
pub fn planners() -> Vec<(&'static str, Box<dyn ObjectivePlanner>)> {
    use super::{
        abilities, agenda, combat, fleet, fleet_shape, holdings, payment, production, structures,
        technology, territory, ttfd,
    };
    vec![
        ("territory", Box::new(territory::Territory)),
        ("fleet", Box::new(fleet::Fleet)),
        ("fleet_shape", Box::new(fleet_shape::FleetShape)),
        ("payment", Box::new(payment::Payment)),
        ("technology", Box::new(technology::Technology)),
        ("combat", Box::new(combat::Combat)),
        ("structures", Box::new(structures::Structures)),
        ("abilities", Box::new(abilities::Abilities)),
        ("agenda", Box::new(agenda::Agenda)),
        ("holdings", Box::new(holdings::Holdings)),
        ("ttfd", Box::new(ttfd::Ttfd)),
        ("production", Box::new(production::Production)),
    ]
}

/// The first planner (dispatcher order) that matches, by file.
pub fn covered_by(row: &Row) -> Option<&'static str> {
    planners()
        .iter()
        .find(|(_, p)| row.goal().matches_any(p.objective_ids()))
        .map(|(name, _)| *name)
}

#[test]
fn the_registry_is_the_planner_list() {
    let mut all = crate::tuning::Tuning::default();
    all.bw_production = true;
    all.bw_combat_abilities = true;
    all.bw_agenda = true;
    all.bw_holdings = true;
    all.bw_ttfd = true;
    all.bw_fleet_shape = true;
    assert_eq!(
        crate::tuning::with(all, || registry().len()),
        planners().len()
    );
}

#[test]
fn the_corpus_has_forty_publics_and_forty_secrets() {
    let rows = rows();
    assert_eq!(rows.iter().filter(|r| r.public).count(), 40);
    assert_eq!(rows.iter().filter(|r| !r.public).count(), 40);
}

#[test]
#[ignore = "prints the coverage table"]
fn print_coverage_table() {
    println!("| alias | name | kind | pts | family | planner |\n|---|---|---|---|---|---|");
    for r in rows() {
        println!(
            "| `{}` | {} | {} | {} | {} | {} |",
            r.alias,
            r.name,
            if r.public { "public" } else { "secret" },
            r.points,
            r.family.as_deref().unwrap_or("(no progress)"),
            covered_by(&r).map_or_else(
                || {
                    crate::backward::unsteerable::UNSTEERABLE
                        .iter()
                        .find(|(a, _)| *a == r.alias)
                        .map_or("NONE".to_owned(), |(_, why)| {
                            format!("NONE: unsteerable ({why})")
                        })
                },
                str::to_owned,
            ),
        );
    }
}

struct PlanProbe<'p>(&'p dyn ObjectivePlanner, Vec<super::Plan>);
impl Decider for PlanProbe<'_> {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        Ok(choice.options[0].clone())
    }
    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let mut ctx = super::PlanCtx::new(seen, &PlayerId::new("a"));
        for goal in ctx.goals.clone() {
            ctx.goal = goal;
            if let Some(plan) = self.0.plan(seen, &ctx) {
                self.1.push(plan);
            }
        }
        Ok(choice.options[0].clone())
    }
}

/// The plans `planner` makes for every goal of seat `a` in `state`.
pub fn plans_of(
    planner: &dyn ObjectivePlanner,
    state: &ti4_model::state::GameState,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
) -> Vec<super::Plan> {
    let mut probe = PlanProbe(planner, Vec::new());
    let choice = Choice::new(
        PlayerId::new("a"),
        "x",
        vec![ChoiceOption::labelled("o", "o", "o")],
    );
    ti4_engine::choice::ask_private(
        &choice,
        state,
        ContentStore::embedded(),
        POK,
        galaxy,
        &mut probe,
    )
    .expect("legal");
    probe.1
}
