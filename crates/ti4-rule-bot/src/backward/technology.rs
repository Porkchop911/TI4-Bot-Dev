//! Backward planning for the technology objectives: N unit upgrades (Develop Weaponry,
//! Revolutionize Warfare), two technologies in each of N colours (Diversify Research, Master the
//! Sciences), N technologies of one colour (Master the Laws of Physics) and N faction
//! technologies.
//!
//! Backwards from the requirement: which set of at most [`MAX_RESEARCH`] technologies, put in an
//! order the prerequisites allow, makes the requirement true (the smallest set; every such
//! set is kept so a research choice can be matched against all of them), how many researches the
//! seat has before the status phase ([`sources`]: the Technology primary it holds, its paid second
//! research, the secondary of a Technology card another seat is still to play), and with what
//! probability they all happen. The plan's steer is the Technology card; [`research_pick`] steers
//! the research choice itself toward the technologies on the routes of the goals in view.
//!
//! The prerequisite model is the printed colours only (technology skips from planets and faction
//! waivers are ignored, which only makes the planner cautious); the options a research offers are
//! the engine's own, so the model can only rank them, never offer one.
//!
//! # Objectives (exactly; checked by `covers_exactly_the_documented_objectives`)
//!
//! | alias | name | kind | points | family |
//! |---|---|---|---|---|
//! | `develop` | Develop Weaponry | public | 1 | `unit_upgrades` |
//! | `diversify` | Diversify Research | public | 1 | `colours_2` |
//! | `revolutionize` | Revolutionize Warfare | public | 2 | `unit_upgrades` |
//! | `master_science` | Master the Sciences | public | 2 | `colours_2` |
//! | `ans` | Adapt New Strategies | secret | 1 | `faction_technologies` |
//! | `mlp` | Master the Laws of Physics | secret | 1 | `same_colour_technologies` |

use std::collections::BTreeSet;

use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, Observed, SeatObservation};
use ti4_model::content_types::ContentType;
use ti4_model::id::{PlayerId, TechnologyId};

use super::{Cost, ObjectivePlanner, Plan, PlanCtx, Steer};
use crate::objectives::Goal;

/// The family tokens this group plans.
pub const IDS: &[&str] = &[
    "unit_upgrades",
    "colours_2",
    "same_colour_technologies",
    "faction_technologies",
];

/// Most researches in one route: the seat has no more than this before the status phase.
pub const MAX_RESEARCH: usize = 3;
/// Probability that the Technology primary of the seat's own card is played this round.
pub const OWN_CARD_P: f64 = 0.95;
/// Probability that the paid second research of the primary (6 resources) goes through.
pub const SECOND_P: f64 = 0.9;
/// Probability that another seat plays its Technology card before the round ends (and the
/// secondary is open to the seat).
pub const SECONDARY_P: f64 = 0.7;
/// Resources of the primary's second research and of the secondary.
pub const SECOND_COST: i64 = 6;
pub const SECONDARY_COST: i64 = 4;

const COLOURS: [&str; 4] = ["BIOTIC", "CYBERNETIC", "PROPULSION", "WARFARE"];

/// What a technology is to the objectives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Colour(usize),
    Upgrade,
    Other,
}

/// One technology, reduced to what the objectives and the prerequisites see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tech {
    pub id: String,
    pub kind: Kind,
    /// Prerequisites per colour.
    pub needs: [usize; 4],
    pub faction: bool,
}

/// Which requirement a goal states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metric {
    UnitUpgrades,
    /// Colours with at least two technologies.
    Colours,
    /// Most technologies in one colour.
    SameColour,
    FactionTechnologies,
}

impl Metric {
    #[must_use]
    pub fn of(family: &str) -> Option<Self> {
        match family {
            "unit_upgrades" => Some(Self::UnitUpgrades),
            "colours_2" => Some(Self::Colours),
            "same_colour_technologies" => Some(Self::SameColour),
            "faction_technologies" => Some(Self::FactionTechnologies),
            _ => None,
        }
    }

    /// The requirement's count for the technologies `held`.
    #[must_use]
    pub fn value(self, held: &[&Tech]) -> usize {
        let mut per = [0usize; 4];
        let mut upgrades = 0;
        let mut factions = 0;
        for t in held {
            match t.kind {
                Kind::Colour(c) => per[c] += 1,
                Kind::Upgrade => upgrades += 1,
                Kind::Other => {}
            }
            factions += usize::from(t.faction);
        }
        match self {
            Self::UnitUpgrades => upgrades,
            Self::Colours => per.iter().filter(|&&n| n >= 2).count(),
            Self::SameColour => per.iter().copied().max().unwrap_or(0),
            Self::FactionTechnologies => factions,
        }
    }
}

fn colours_of(owned: &[&Tech]) -> [usize; 4] {
    let mut per = [0usize; 4];
    for t in owned {
        if let Kind::Colour(c) = t.kind {
            per[c] += 1;
        }
    }
    per
}

/// Whether the technologies of `combo` can be researched one after the other from `owned`.
fn orderable(owned: &[Tech], combo: &[&Tech]) -> bool {
    let mut have = colours_of(&owned.iter().collect::<Vec<_>>());
    let mut left: Vec<&Tech> = combo.to_vec();
    while !left.is_empty() {
        let Some(i) = left
            .iter()
            .position(|t| (0..4).all(|c| have[c] >= t.needs[c]))
        else {
            return false;
        };
        let t = left.remove(i);
        if let Kind::Colour(c) = t.kind {
            have[c] += 1;
        }
    }
    true
}

/// The smallest sets (indices into `cands`) of at most `max` technologies that bring `metric` to
/// `threshold` from `owned`, with their size. `None` when none does. Sets are capped at 64.
#[must_use]
pub fn min_sets(
    owned: &[Tech],
    cands: &[Tech],
    metric: Metric,
    threshold: usize,
    max: usize,
) -> Option<(usize, Vec<Vec<usize>>)> {
    let base: Vec<&Tech> = owned.iter().collect();
    if metric.value(&base) >= threshold {
        return Some((0, vec![Vec::new()]));
    }
    for size in 1..=max.min(cands.len()) {
        let mut found: Vec<Vec<usize>> = Vec::new();
        let mut idx: Vec<usize> = (0..size).collect();
        loop {
            let combo: Vec<&Tech> = idx.iter().map(|&i| &cands[i]).collect();
            let mut all = base.clone();
            all.extend(combo.iter().copied());
            if metric.value(&all) >= threshold && orderable(owned, &combo) {
                found.push(idx.clone());
                if found.len() >= 64 {
                    break;
                }
            }
            // next combination
            let mut k = size;
            while k > 0 && idx[k - 1] == cands.len() - size + k - 1 {
                k -= 1;
            }
            if k == 0 {
                break;
            }
            idx[k - 1] += 1;
            for j in k..size {
                idx[j] = idx[j - 1] + 1;
            }
        }
        if !found.is_empty() {
            return Some((size, found));
        }
    }
    None
}

/// One research the seat can still make before the status phase.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Source {
    pub probability: f64,
    pub resources: i64,
    /// A strategy token (the secondary).
    pub tokens: i32,
    /// The seat's own Technology card is used.
    pub own_card: bool,
}

/// The researches open to the seat, best first.
#[must_use]
pub fn sources(
    own_card_open: bool,
    rival_cards_open: usize,
    resources: i64,
    strategic_tokens: i32,
) -> Vec<Source> {
    let mut out = Vec::new();
    let mut left = resources;
    if own_card_open {
        out.push(Source {
            probability: OWN_CARD_P,
            resources: 0,
            tokens: 0,
            own_card: true,
        });
        if left >= SECOND_COST {
            left -= SECOND_COST;
            out.push(Source {
                probability: SECOND_P,
                resources: SECOND_COST,
                tokens: 0,
                own_card: true,
            });
        }
    }
    let mut tokens = strategic_tokens;
    for _ in 0..rival_cards_open {
        if tokens >= 1 && left >= SECONDARY_COST {
            tokens -= 1;
            left -= SECONDARY_COST;
            out.push(Source {
                probability: SECONDARY_P,
                resources: SECONDARY_COST,
                tokens: 1,
                own_card: false,
            });
        }
    }
    out.sort_by(|a, b| b.probability.total_cmp(&a.probability));
    out
}

/// A route of `k` researches: probability, cost and whether the own card is used.
#[must_use]
pub fn route_of(sources: &[Source], k: usize) -> Option<(f64, Cost, bool)> {
    if k == 0 || k > sources.len() {
        return None;
    }
    let used = &sources[..k];
    Some((
        used.iter().map(|s| s.probability).product(),
        Cost {
            tokens: used.iter().map(|s| s.tokens).sum(),
            resources: used.iter().map(|s| s.resources).sum(),
            activations: 0,
        },
        used.iter().any(|s| s.own_card),
    ))
}

// -- reading the position --------------------------------------------------------------------------

fn tech_of(content: &ContentStore, alias: &str) -> Tech {
    let id = TechnologyId::new(alias);
    let needs_map = ti4_engine::technology::prerequisites(content, &id);
    let mut needs = [0usize; 4];
    for (c, name) in COLOURS.iter().enumerate() {
        needs[c] = needs_map.get(name).copied().unwrap_or(0);
    }
    let kind = if ti4_engine::technology::is_unit_upgrade(content, &id) {
        Kind::Upgrade
    } else {
        ti4_engine::technology::colour_type(content, &id)
            .and_then(|name| COLOURS.iter().position(|c| *c == name))
            .map_or(Kind::Other, Kind::Colour)
    };
    Tech {
        id: alias.to_owned(),
        kind,
        needs,
        faction: ti4_engine::technology::faction_of(content, &id).is_some(),
    }
}

/// The seat's technologies and those it may still research (faction and replacement rules; the
/// prerequisites are the route's business).
#[must_use]
pub fn model(seen: &Observed<'_>, me: &PlayerId) -> Option<(Vec<Tech>, Vec<Tech>)> {
    let seat = seen.seat(me)?;
    let content = seen.content();
    let owned: Vec<Tech> = seat
        .technologies
        .iter()
        .map(|t| tech_of(content, t.as_str()))
        .collect();
    let have: BTreeSet<&str> = seat.technologies.iter().map(TechnologyId::as_str).collect();
    let faction = seat.faction.as_str();
    let active = ti4_engine::technology::active_aliases(content, seen.sources());
    let mut cands: Vec<Tech> = content
        .records(ContentType::Technologies)
        .iter()
        .filter_map(|r| r.text("alias"))
        .filter(|a| active.contains(&TechnologyId::new(*a)) && !have.contains(a))
        .filter(|a| {
            let id = TechnologyId::new(*a);
            ti4_engine::technology::faction_of(content, &id).is_none_or(|f| f == faction)
                && !ti4_engine::technology::replaced_for_faction(content, faction, &id)
        })
        .map(|a| tech_of(content, a))
        .filter(|t| t.kind != Kind::Other)
        .collect();
    cands.sort_by(|a, b| a.id.cmp(&b.id));
    Some((owned, cands))
}

fn technology_card(seen: &Observed<'_>, card: &ti4_model::id::StrategyCardId) -> bool {
    ti4_engine::strategy_cards::card_name(seen.content(), card.as_str()).as_deref()
        == Some("Technology")
}

/// The researches before the status phase for the seat.
pub fn seat_sources(seen: &Observed<'_>, me: &PlayerId) -> Vec<Source> {
    let Some(seat) = seen.seat(me) else {
        return Vec::new();
    };
    let own_open = seat
        .strategy_cards
        .iter()
        .any(|c| technology_card(seen, c) && !seat.exhausted_strategy_cards.contains(c));
    let rivals = seen
        .players()
        .into_iter()
        .filter(|p| *p != me)
        .filter_map(|p| seen.seat(p))
        .filter(|s| !s.passed)
        .filter(|s| {
            s.strategy_cards
                .iter()
                .any(|c| technology_card(seen, c) && !s.exhausted_strategy_cards.contains(c))
        })
        .count();
    sources(
        own_open,
        rivals,
        seen.available_spend(me, ti4_engine::production::Spend::Resources),
        seat.strategic_tokens,
    )
}

/// Researches (at most [`MAX_RESEARCH`]) after which the seat owns every technology of `wanted`,
/// prerequisites included, with their probability and cost: the prep step of a plan that needs a
/// technology for reach (Gravity Drive) or strength (a unit upgrade). `Some((0, 1.0, ..))` when
/// it owns them all, `None` when out of reach before the status phase.
#[must_use]
pub fn reach_by_research(
    seen: &Observed<'_>,
    me: &PlayerId,
    wanted: &[&str],
) -> Option<(usize, f64, Cost)> {
    let (owned, cands) = model(seen, me)?;
    let have: BTreeSet<&str> = owned.iter().map(|t| t.id.as_str()).collect();
    let missing: Vec<&str> = wanted
        .iter()
        .copied()
        .filter(|w| !have.contains(w))
        .collect();
    if missing.is_empty() {
        return Some((0, 1.0, Cost::default()));
    }
    let srcs = seat_sources(seen, me);
    let k = steps_to_own(&owned, &cands, &missing, srcs.len().min(MAX_RESEARCH))?;
    let (p, cost, _) = route_of(&srcs, k)?;
    Some((k, p, cost))
}

/// The fewest researches (at most `max`) after which every id of `missing` is owned.
#[must_use]
pub fn steps_to_own(owned: &[Tech], cands: &[Tech], missing: &[&str], max: usize) -> Option<usize> {
    let targets: Vec<usize> = missing
        .iter()
        .map(|id| cands.iter().position(|t| t.id == *id))
        .collect::<Option<Vec<_>>>()?;
    // Smallest superset of the targets (enablers from the candidates) that can be ordered.
    for size in targets.len()..=max {
        let extra = size - targets.len();
        let others: Vec<usize> = (0..cands.len()).filter(|i| !targets.contains(i)).collect();
        let mut found = false;
        let mut idx: Vec<usize> = (0..extra).collect();
        loop {
            if extra <= others.len() {
                let mut combo: Vec<&Tech> = targets.iter().map(|&i| &cands[i]).collect();
                combo.extend(idx.iter().map(|&j| &cands[others[j]]));
                if orderable(owned, &combo) {
                    found = true;
                }
            }
            if found || extra == 0 || extra > others.len() {
                break;
            }
            let mut k = extra;
            while k > 0 && idx[k - 1] == others.len() - extra + k - 1 {
                k -= 1;
            }
            if k == 0 {
                break;
            }
            idx[k - 1] += 1;
            for j in k..extra {
                idx[j] = idx[j - 1] + 1;
            }
        }
        if found {
            return Some(size);
        }
    }
    None
}

fn threshold(goal: &Goal) -> usize {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a small count"
    )]
    let t = goal.threshold.max(0.0) as usize;
    t
}

/// The aliases this group plans, as documented at the top of the file.
pub const COVERS: &[&str] = &[
    "develop",
    "diversify",
    "revolutionize",
    "master_science",
    "ans",
    "mlp",
];

/// The technology group.
pub struct Technology;

impl ObjectivePlanner for Technology {
    fn objective_ids(&self) -> &[&str] {
        IDS
    }

    fn plan(&self, obs: &SeatObservation<'_>, ctx: &PlanCtx) -> Option<Plan> {
        let observed = obs.observed();
        let metric = Metric::of(&ctx.goal.family)?;
        let (owned, cands) = model(observed, &ctx.me)?;
        let srcs = seat_sources(observed, &ctx.me);
        if srcs.is_empty() {
            return None;
        }
        let (k, sets) = min_sets(
            &owned,
            &cands,
            metric,
            threshold(&ctx.goal),
            srcs.len().min(MAX_RESEARCH),
        )?;
        let (probability, cost, own_card) = route_of(&srcs, k)?;
        let names: Vec<&str> = sets[0].iter().map(|&i| cands[i].id.as_str()).collect();
        Some(Plan {
            objective: ctx.goal.alias.clone(),
            points: ctx.goal.points,
            probability,
            cost,
            steer: Steer {
                cards: if own_card {
                    vec!["Technology".to_owned()]
                } else {
                    Vec::new()
                },
                ..Steer::default()
            },
            reason: format!("{k} research(es): {}", names.join(", ")),
            payment: false,
            secret: ctx.goal.secret,
        })
    }
}

/// The research choice steered by the goals in view: an offered technology scores
/// `points / k` for every unmet technology goal with a route of `k <= `[`MAX_RESEARCH`] researches
/// that contains it; the best score wins, a tie goes to the seat's own order (`tie`). `None` when
/// no offered technology is on a route.
pub fn research_pick(
    choice: &Choice,
    seen: &SeatObservation<'_>,
    me: &PlayerId,
    goals: &[Goal],
    tie: impl Fn(&Choice) -> Option<usize>,
) -> Option<usize> {
    let observed = seen.observed();
    let (owned, cands) = model(observed, me)?;
    let mut score: Vec<f64> = vec![0.0; choice.options.len()];
    for goal in goals.iter().filter(|g| !g.satisfied && !g.bought) {
        let Some(metric) = Metric::of(&goal.family) else {
            continue;
        };
        let Some((k, sets)) = min_sets(&owned, &cands, metric, threshold(goal), MAX_RESEARCH)
        else {
            continue;
        };
        if k == 0 {
            continue;
        }
        #[expect(clippy::cast_precision_loss, reason = "tiny")]
        let share = goal.points / k as f64;
        let on_route: BTreeSet<&str> = sets
            .iter()
            .flatten()
            .map(|&i| cands[i].id.as_str())
            .collect();
        for (i, o) in choice.options.iter().enumerate() {
            if on_route.contains(o.id.as_str()) {
                score[i] += share;
            }
        }
    }
    let best = score.iter().copied().fold(0.0, f64::max);
    if best <= 0.0 {
        return None;
    }
    let mut narrowed = choice.clone();
    let keep: Vec<usize> = (0..score.len())
        .filter(|&i| score[i] >= best - 1e-9)
        .collect();
    narrowed.options = keep.iter().map(|&i| choice.options[i].clone()).collect();
    tie(&narrowed)
        .and_then(|i| keep.get(i).copied())
        .or_else(|| keep.first().copied())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn colour(id: &str, c: usize, needs: [usize; 4]) -> Tech {
        Tech {
            id: id.to_owned(),
            kind: Kind::Colour(c),
            needs,
            faction: false,
        }
    }
    fn upgrade(id: &str, needs: [usize; 4]) -> Tech {
        Tech {
            id: id.to_owned(),
            kind: Kind::Upgrade,
            needs,
            faction: false,
        }
    }

    #[test]
    fn two_upgrades_need_the_prerequisite_colours_first() {
        // Owns one red; both upgrades need two reds: one red tech first, then the upgrades.
        let owned = vec![colour("r1", 3, [0; 4])];
        let cands = vec![
            upgrade("dd2", [0, 0, 0, 2]),
            upgrade("dn2", [0, 0, 0, 2]),
            colour("r2", 3, [0; 4]),
        ];
        let (k, sets) = min_sets(&owned, &cands, Metric::UnitUpgrades, 2, 3).unwrap();
        assert_eq!(k, 3);
        assert_eq!(sets, vec![vec![0, 1, 2]]);
        // With two reds owned: just the two upgrades.
        let owned = vec![colour("r1", 3, [0; 4]), colour("r2", 3, [0; 4])];
        let cands = vec![upgrade("dd2", [0, 0, 0, 2]), upgrade("dn2", [0, 0, 0, 2])];
        assert_eq!(
            min_sets(&owned, &cands, Metric::UnitUpgrades, 2, 3)
                .unwrap()
                .0,
            2
        );
        // Not reachable inside the limit.
        assert!(min_sets(&owned, &cands, Metric::UnitUpgrades, 4, 3).is_none());
    }

    #[test]
    fn colour_pairs_and_one_colour_runs() {
        let owned = vec![colour("b1", 0, [0; 4]), colour("y1", 1, [0; 4])];
        let cands = vec![
            colour("b2", 0, [0; 4]),
            colour("y2", 1, [0; 4]),
            colour("p1", 2, [0; 4]),
        ];
        // Diversify (two colours with two): one more blue and one more yellow.
        let (k, sets) = min_sets(&owned, &cands, Metric::Colours, 2, 3).unwrap();
        assert_eq!((k, sets), (2, vec![vec![0, 1]]));
        // Four of a colour from one owned needs three more.
        let owned = vec![colour("b1", 0, [0; 4])];
        let cands = vec![
            colour("b2", 0, [0; 4]),
            colour("b3", 0, [0; 4]),
            colour("b4", 0, [0; 4]),
        ];
        assert_eq!(
            min_sets(&owned, &cands, Metric::SameColour, 4, 3)
                .unwrap()
                .0,
            3
        );
        assert!(min_sets(&owned, &cands, Metric::SameColour, 4, 2).is_none());
    }

    #[test]
    fn a_met_requirement_needs_no_research() {
        let owned = vec![upgrade("a", [0; 4]), upgrade("b", [0; 4])];
        assert_eq!(
            min_sets(&owned, &[], Metric::UnitUpgrades, 2, 3).unwrap().0,
            0
        );
    }

    #[test]
    fn reaching_a_technology_counts_its_enablers() {
        // Gravity-Drive-like: needs two blues; one owned, one more blue tech is the enabler.
        let owned = vec![colour("b1", 2, [0; 4])];
        let cands = vec![
            colour("gd", 2, [0, 0, 2, 0]),
            colour("b2", 2, [0; 4]),
            colour("r1", 3, [0; 4]),
        ];
        assert_eq!(steps_to_own(&owned, &cands, &["gd"], 3), Some(2));
        let owned2 = vec![colour("b1", 2, [0; 4]), colour("b2", 2, [0; 4])];
        let cands2 = vec![colour("gd", 2, [0, 0, 2, 0])];
        assert_eq!(steps_to_own(&owned2, &cands2, &["gd"], 3), Some(1));
        assert_eq!(steps_to_own(&owned, &cands, &["gd"], 1), None);
        assert_eq!(steps_to_own(&owned, &cands, &["nope"], 3), None);
    }

    #[test]
    fn sources_and_routes() {
        // Own card open, 6 resources: two researches; the second is the paid one.
        let s = sources(true, 0, 6, 0);
        assert_eq!(s.len(), 2);
        let (p, cost, own) = route_of(&s, 2).unwrap();
        assert!((p - 0.95 * 0.9).abs() < 1e-9 && own && cost.resources == 6);
        // Without the card: only a rival's secondary, with a token and four resources.
        assert!(sources(false, 1, 3, 2).is_empty());
        let s = sources(false, 1, 4, 1);
        let (p, cost, own) = route_of(&s, 1).unwrap();
        assert!((p - 0.7).abs() < 1e-9 && !own && cost.tokens == 1);
        // Three researches need three sources.
        assert!(route_of(&sources(true, 0, 5, 0), 2).is_none());
    }

    #[test]
    fn covers_exactly_the_documented_objectives() {
        assert_eq!(
            crate::backward::coverage::matched(&Technology),
            crate::backward::coverage::sorted(COVERS)
        );
    }
}
