//! Audit L5: "Erect a Monument -- spend 8 resources" reported as counted wrong or sticky.
//!
//! Findings these tests pin down (see the audit report for the rules citations):
//!
//! * The engine keeps NO running counter of resources spent. `monument` is a *bought* objective
//!   (61.10, `objectives::cost_of`): it is offered at a scoring window when the player can pay
//!   8 resources *right then* from readied planets plus trade goods, and the payment is taken in
//!   `award`. Nothing is accumulated across a turn, a round or the game.
//! * The "progress" shown to the web UI (`bought_progress`) is therefore *spending capacity at the
//!   moment of the query* (greatest k <= 8 payable now), not "resources spent so far". Spending
//!   earlier in the round (production, research) LOWERS it, because those planets are exhausted
//!   until the status phase readies them (after scoring).
//!
//! Tests marked `#[ignore = "known bug: ..."]` fail today and describe the defect.

use ti4_content::ContentStore;
use ti4_model::content_types::POK;
use ti4_model::id::{ObjectiveId, PlanetId, PlayerId, SystemId};
use ti4_model::state::GameState;

use crate::objectives::{Cost, CostFamily, award, bought_progress, can_afford, cost_of, scoreable};
use crate::production::Spend;

fn a() -> PlayerId {
    PlayerId::new("a")
}

fn monument() -> ObjectiveId {
    ObjectiveId::new("monument")
}

/// A two-player game where seat `a` owns no *readied* planets (home planets are exhausted away),
/// holds nothing to spend, and `monument` is on the table.
fn blank() -> GameState {
    let seats = [a(), PlayerId::new("b")];
    let mut state = crate::setup::start_game(ContentStore::embedded(), &seats, POK, None).unwrap();
    let home: Vec<PlanetId> = state
        .controlled_planets(&a())
        .into_iter()
        .map(|(_, planet)| planet.clone())
        .collect();
    for planet in home {
        state.exhaust_planet(planet);
    }
    state.player_mut(&a()).unwrap().trade_goods = 0;
    state.player_mut(&a()).unwrap().commodities = 0;
    state.revealed_objectives = vec![monument()];
    state
}

/// Non-home, placed planets with exactly this printed resource value (and, if given, influence),
/// in a stable order.
fn planets_with(resources: i64, influence: Option<i64>, count: usize) -> Vec<String> {
    let found: Vec<String> = ti4_content::galaxy::all_planets(ContentStore::embedded(), POK)
        .iter()
        .filter(|(_, p)| {
            p.homeworld_of().is_none()
                && !p.is_placed_during_play()
                && p.system_id().is_some()
                && p.resources() == resources
                && influence.is_none_or(|i| p.influence() == i)
        })
        .map(|(id, _)| (*id).to_owned())
        .take(count)
        .collect();
    assert_eq!(found.len(), count, "corpus has {count} such planets");
    found
}

fn give(state: &mut GameState, planet: &str) {
    let catalogue = ti4_content::galaxy::all_planets(ContentStore::embedded(), POK);
    let system = catalogue[planet].system_id().expect("placed planet");
    state
        .system_mut(&SystemId::new(system))
        .set_control(PlanetId::new(planet), a());
}

fn have(state: &GameState) -> i64 {
    bought_progress(state, ContentStore::embedded(), POK, &a(), &monument())
        .unwrap()
        .have
}

fn offered(state: &GameState) -> bool {
    scoreable(state, ContentStore::embedded(), POK, &a()).contains(&monument())
}

fn score(state: &mut GameState) -> bool {
    award(state, ContentStore::embedded(), POK, &a(), &monument()).is_ok()
}

/// Planets worth 3 + 3 + 2 resources: exactly 8 (no non-home planet prints more than 3).
fn give_eight(state: &mut GameState) {
    for planet in planets_with(3, None, 2)
        .iter()
        .chain(planets_with(2, None, 1).iter())
    {
        give(state, planet);
    }
}

#[test]
fn the_card_is_a_bought_objective_priced_at_eight_resources() {
    assert_eq!(
        cost_of(&monument()),
        Some(Cost::Spend {
            amount: 8,
            kind: Spend::Resources
        })
    );
    let progress =
        bought_progress(&blank(), ContentStore::embedded(), POK, &a(), &monument()).unwrap();
    assert_eq!(progress.family, CostFamily::Spend(Spend::Resources));
    assert_eq!((progress.have, progress.target), (0, 8));
}

#[test]
fn threshold_exactly_eight_scores_and_seven_does_not() {
    let mut state = blank();
    give_eight(&mut state);
    assert_eq!(have(&state), 8);
    assert!(offered(&state));

    // 3 + 3 + 1 = 7 resources.
    let mut seven = blank();
    for planet in planets_with(3, None, 2)
        .iter()
        .chain(planets_with(1, None, 1).iter())
    {
        give(&mut seven, planet);
    }
    assert_eq!(have(&seven), 7);
    assert!(!offered(&seven));
    let before = seven.clone();
    assert!(!score(&mut seven), "7 resources cannot buy it");
    assert!(
        seven.identical(&before),
        "a failed purchase changes nothing"
    );

    assert!(score(&mut state));
    assert!(
        state.exhausted_planets.len() >= 2,
        "both planets were exhausted to pay"
    );
}

#[test]
fn partial_spending_earlier_in_the_turn_lowers_capacity_and_is_not_credited() {
    // Rules: you spend the 8 when you score; resources spent on production earlier do not count
    // toward it. Engine agrees: exhausting one 3-resource planet earlier leaves 5 payable.
    let mut state = blank();
    give_eight(&mut state);
    assert_eq!(have(&state), 8);

    let first = planets_with(3, None, 2).remove(0);
    state.exhaust_planet(PlanetId::new(first.as_str()));
    assert_eq!(
        have(&state),
        5,
        "capacity, not 'spent so far': drops after spending"
    );
    assert!(!offered(&state));
}

#[test]
fn a_production_or_research_purchase_is_just_an_exhausted_planet() {
    // Production, tech costs and strategy-card payments all go through `payment::apply` /
    // `exhaust_planet`; none touches an objective counter (there is none). Spending 8 elsewhere
    // never "pre-pays" the monument: after exhausting everything, 0/8.
    let mut state = blank();
    give_eight(&mut state);
    let plan = crate::payment::plans(
        &state,
        ContentStore::embedded(),
        POK,
        &a(),
        8,
        Spend::Resources,
    )
    .into_iter()
    .next()
    .unwrap();
    assert!(
        crate::payment::apply(&mut state, &a(), &plan),
        "spent 8 on something else"
    );
    assert_eq!(have(&state), 0);
    assert!(
        !offered(&state),
        "8 resources spent in the action phase do not score the monument"
    );
}

#[test]
fn trade_goods_count_as_resources_and_planets_are_used_first() {
    // 75: trade goods may be spent as resources or influence, 1 : 1.
    let mut state = blank();
    let five = planets_with(3, None, 1).remove(0);
    give(&mut state, &five);
    state.player_mut(&a()).unwrap().trade_goods = 5;
    assert_eq!(have(&state), 8);
    assert!(score(&mut state));
    assert_eq!(
        state.player(&a()).unwrap().trade_goods,
        0,
        "5 goods made up the shortfall"
    );
    assert!(
        state
            .exhausted_planets
            .contains(&PlanetId::new(five.as_str()))
    );

    // Trade goods alone also pay (8 goods, no planets).
    let mut goods = blank();
    goods.player_mut(&a()).unwrap().trade_goods = 8;
    assert!(offered(&goods));
    assert!(score(&mut goods));
    assert_eq!(goods.player(&a()).unwrap().trade_goods, 0);

    // 7 goods are not enough.
    let mut seven = blank();
    seven.player_mut(&a()).unwrap().trade_goods = 7;
    assert!(!offered(&seven));
}

#[test]
fn commodities_never_count_toward_resources() {
    let mut state = blank();
    state.player_mut(&a()).unwrap().commodities = 8;
    assert_eq!(have(&state), 0);
    assert!(!offered(&state));
}

#[test]
fn influence_does_not_pay_for_resources() {
    // Planets with 0 resources and 3 influence are worth nothing toward 8 resources.
    let mut state = blank();
    for planet in planets_with(0, Some(3), 3) {
        give(&mut state, &planet);
    }
    assert_eq!(have(&state), 0);
    assert!(!can_afford(
        &state,
        ContentStore::embedded(),
        POK,
        &a(),
        Cost::Spend {
            amount: 8,
            kind: Spend::Resources
        }
    ));
    // ...while 8 influence is payable from the same planets (9 available).
    assert!(can_afford(
        &state,
        ContentStore::embedded(),
        POK,
        &a(),
        Cost::Spend {
            amount: 8,
            kind: Spend::Influence
        }
    ));
}

#[test]
fn scoring_exhausts_what_it_spent_so_capacity_is_gone_afterwards() {
    let mut state = blank();
    give_eight(&mut state);
    assert!(score(&mut state));
    assert_eq!(have(&state), 0, "everything was exhausted paying");
}

#[test]
fn scoring_is_once_per_game_and_progress_is_not_a_sticky_total() {
    let mut state = blank();
    give_eight(&mut state);
    assert!(score(&mut state));
    assert!(state.scored_by(&a()).contains(&monument()));
    // Round boundary: readying restores capacity to 8/8, but the card is already scored, so it is
    // never offered (nor charged) again.
    state.ready_all_planets();
    assert_eq!(
        have(&state),
        8,
        "recomputed from readied planets each query"
    );
    assert!(!offered(&state), "already scored: not offered again (61.8)");
}

#[test]
fn round_boundary_resets_capacity_because_nothing_is_stored() {
    let mut state = blank();
    give_eight(&mut state);
    let all: Vec<PlanetId> = state
        .controlled_planets(&a())
        .into_iter()
        .map(|(_, p)| p.clone())
        .collect();
    for planet in all {
        state.exhaust_planet(planet);
    }
    assert_eq!(have(&state), 0);
    state.phase = ti4_model::state::Phase::Status;
    crate::status::resolve_status_phase(&mut state).unwrap();
    assert_eq!(
        have(&state),
        8,
        "no counter carries over; readying restores the full capacity"
    );
}

#[test]
fn a_failed_purchase_is_atomic_and_replay_is_deterministic() {
    // The engine has no refund path to test: scoring is all-or-nothing (`pay_for` checks
    // `can_afford` first and `payment::apply` re-checks). Server undo works by replaying the log.
    let mut state = blank();
    state.player_mut(&a()).unwrap().trade_goods = 7;
    let before = state.clone();
    assert!(!score(&mut state));
    assert!(state.identical(&before));

    let mut replay = before.clone();
    state.player_mut(&a()).unwrap().trade_goods = 8;
    replay.player_mut(&a()).unwrap().trade_goods = 8;
    assert!(score(&mut state) && score(&mut replay));
    assert!(state.identical(&replay));
}

#[test]
#[ignore = "known bug: payment::plans keeps only the 6 most valuable planets \
            (MAX_PLANETS_PER_PLAN), so 8 readied 1-resource planets (= 8 resources) report 6/8 \
            and the monument is not offered"]
fn eight_one_resource_planets_can_pay_eight() {
    let mut state = blank();
    for planet in planets_with(1, None, 8) {
        give(&mut state, &planet);
    }
    assert_eq!(
        have(&state),
        8,
        "8 planets x 1 resource = 8 resources are readied"
    );
    assert!(offered(&state));
}
