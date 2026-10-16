//! Sol round-1 plan pilot: five arms, many dice seeds per origin. Diagnostic, not training labels.
//! Arms: control | tech-elsewhere (a fixed other seat must draft Technology) | tech-gd (same, and
//! Sol keeps 1 token + 4 resources until it follows Technology for Gravity Drive) | trade (Sol
//! drafts Trade) | military-support (with an empty strategy pool Sol offers Military Support to
//! the richest seat for 1 trade good). Every seat's MLP is still asked at every choice.
//! --bundle <checkpoint> --out <new report.json> [--origins 10] [--dice 16] [--workers 16]
//! [--seed-base 610170001] [--map-pool <Train-role pool>] [--rounds 4] [--arms a,b]

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{cell::RefCell, collections::BTreeMap, io::Write, path::Path, rc::Rc, sync::Arc};
use ti4_content::ContentStore;
use ti4_engine::{
    choice::{Choice, ChoiceOption, Decider, IllegalChoice, Observed, SeatObservation},
    game::Game,
    production::Spend,
};
use ti4_mlp::{
    Actor, FactionRow,
    bot::{InferenceStatus, MlpBot},
};
use ti4_model::{
    content_types::DEFAULT as SCOPE,
    id::{FactionId, PlayerId},
    state::Phase,
};
use ti4_policy::{progress, vocabulary::Vocabulary};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
/// Chance streams moved per dice seed. Map, galaxy and speaker stay fixed, so the origin is fixed.
const CHANCE: [&str; 7] = [
    "dice",
    "deck:objectives",
    "deck:agendas",
    "deck:action_cards",
    "deck:secrets",
    "deck:relics",
    "deck:exploration",
];
const RESERVE_RESOURCES: i64 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Arm {
    Control,
    TechElsewhere,
    TechGd,
    Trade,
    MilitarySupport,
    /// Sol builds fighters and infantry to fill capacity; no dreadnoughts or destroyers.
    CarrierDoctrine,
    /// Instead of passing with ready planets, Sol activates a dock system and builds with them.
    SpendDown,
    /// Instead of passing while a revealed planet/system objective is one step short, Sol takes
    /// one more tactical action (the MLP picks where). At most once per objective per round.
    NoEarlyPass,
}
const ARMS: [Arm; 5] = [
    Arm::Control,
    Arm::TechElsewhere,
    Arm::TechGd,
    Arm::Trade,
    Arm::MilitarySupport,
];

fn hash(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// The other seat forced onto Technology: fixed per origin, uniform over seats 1..=5.
fn tech_seat(origin: u64) -> usize {
    let mut z = origin.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    1 + usize::try_from((z ^ (z >> 31)) % 5).expect("small")
}

fn unit_of(o: &ChoiceOption) -> &str {
    o.payload.get("unit").and_then(Value::as_str).unwrap_or("")
}

/// Carrier doctrine at Sol's production: a dreadnought or destroyer becomes a fighter that fits
/// the carriers (else infantry, else a carrier), and stopping with room left buys another fighter.
fn doctrine(c: &Choice, sub: &str, proposed: &ChoiceOption) -> Option<String> {
    let produce = |o: &&ChoiceOption| o.kind == "produce";
    let banned = |o: &ChoiceOption| {
        o.kind == "produce" && ["dreadnought", "destroyer"].iter().any(|b| unit_of(o).starts_with(b))
    };
    let fits = |o: &&ChoiceOption| {
        o.payload
            .get("capacity_excess_after")
            .and_then(Value::as_i64)
            .is_none_or(|x| x <= 0)
    };
    let find = |want: &str| {
        c.options
            .iter()
            .filter(produce)
            .find(|o| unit_of(o).contains(want) && (want != "fighter" || fits(o)))
    };
    let stops = |o: &ChoiceOption| o.is_decline() || o.kind == "decline";
    let pick = match sub {
        "produce_unit" if banned(proposed) => find("fighter")
            .or_else(|| find("infantry"))
            .or_else(|| find("carrier"))
            .or_else(|| c.options.iter().find(|o| stops(o))),
        "produce_unit" if stops(proposed) => find("fighter"),
        "produce_one_unit" if banned(proposed) => find("carrier").or_else(|| find("fighter")),
        _ => None,
    };
    pick.map(|o| o.id.clone())
}

/// Ready planet resources a seat can still spend this round, after holding back what a
/// satisfied "spend" public objective will cost at the status phase.
fn spare_planet_resources(seen: &SeatObservation<'_>) -> i64 {
    let me = seen.bound_seat();
    let tg = seen.seat(me).map_or(0, |s| i64::from(s.trade_goods));
    let ready = seen.available_spend(me, Spend::Resources) - tg;
    let hold = seen
        .revealed_objective_progress(me)
        .iter()
        .filter(|p| p.satisfied && ["cost_spend_resources", "cost_all_three"].contains(&p.family_token.as_str()))
        .map(|p| p.threshold as i64)
        .max()
        .unwrap_or(0);
    ready - hold
}

/// Never pass with ready planets: take a tactical action at a system with Sol's space dock,
/// move nothing and build fighters (if they fit) or infantry with the spare planet resources.
fn spend_down(s: &mut Shared, c: &Choice, seen: &SeatObservation<'_>, proposed: &ChoiceOption) -> Option<String> {
    let sub = subtype(c);
    let menu = c.details.get("kind").and_then(Value::as_str) == Some("turn_menu");
    if menu || sub == "end_turn" {
        s.spend = None;
    }
    if menu && proposed.id == "pass" && c.option("tactical").is_some() {
        let spare = spare_planet_resources(seen);
        if spare >= 2 {
            s.spend = Some(SpendStage::Activating);
            s.spend_log.push(json!({"round":seen.round(),"spare":spare}));
            return Some("tactical".into());
        }
        return None;
    }
    let me = seen.bound_seat();
    match (s.spend?, sub) {
        (SpendStage::Activating, "activate_system") => {
            let dock = |id: &str| {
                seen.board().iter().any(|(system, st)| {
                    system.as_str() == id
                        && st.planet_units.values().flatten().chain(&st.units).any(|u| {
                            u.owner == *me && u.type_id.as_str().contains("spacedock")
                        })
                })
            };
            let pick = c.options.iter().find(|o| dock(&o.id)).map(|o| o.id.clone());
            s.spend = pick.as_ref().map(|_| SpendStage::Moving);
            if pick.is_none() {
                s.spend_log.push(json!({"round":seen.round(),"aborted":"no dock system activatable"}));
            }
            pick
        }
        (SpendStage::Moving, "movement_step") => {
            s.spend = Some(SpendStage::Producing);
            c.option("done_moving").map(|o| o.id.clone())
        }
        (SpendStage::Producing, "produce_unit") if proposed.kind == "decline" => {
            let spare = spare_planet_resources(seen);
            let affordable = |o: &&ChoiceOption| {
                o.kind == "produce"
                    && o.payload.get("owed").and_then(Value::as_i64).unwrap_or(99) <= spare
            };
            let fits = |o: &&ChoiceOption| {
                o.payload.get("capacity_excess_after").and_then(Value::as_i64).is_none_or(|x| x <= 0)
            };
            let pick = c
                .options
                .iter()
                .filter(affordable)
                .find(|o| unit_of(o).starts_with("fighter") && fits(o))
                .or_else(|| c.options.iter().filter(affordable).find(|o| unit_of(o).contains("infantry")));
            pick.map(|o| o.id.clone())
        }
        _ => None,
    }
}

/// Objective families one more move or landing can advance by one.
const MOVE_FAMILIES: [&str; 16] = [
    "non_home",
    "on_the_rim",
    "same_trait",
    "legendary_planets",
    "attached_planets",
    "planetless_systems",
    "ships_in_systems",
    "neighbours",
    "ship_systems_beside_anomaly",
    "ship_systems_beside_rival_home",
    "ships_adjacent_to_mecatol",
    "shared_planet_systems",
    "notable_systems",
    "fleet_in_one_system",
    "ground_forces_on_one_planet",
    "mecatol_ships_while_controlling",
];

fn no_early_pass(s: &mut Shared, c: &Choice, seen: &SeatObservation<'_>, proposed: &ChoiceOption) -> Option<String> {
    let me = seen.bound_seat();
    let menu = c.details.get("kind").and_then(Value::as_str) == Some("turn_menu");
    let progress = seen.revealed_objective_progress(me);
    // Settle the last forced action: did it move its objective?
    if menu || subtype(c) == "end_turn" {
        if let Some((alias, before)) = s.push_pending.take() {
            let after = progress.iter().find(|p| p.alias == alias).map_or(before, |p| p.have);
            s.push_log.push(json!({"round":seen.round(),"alias":alias,"before":before,"after":after,"gained":after > before}));
        }
    }
    if !(menu && proposed.id == "pass" && c.option("tactical").is_some()) {
        return None;
    }
    let scored = seen.scored_by(me);
    let round = seen.round();
    let target = progress.iter().find(|p| {
        !p.satisfied
            && p.threshold - p.have <= 1.0
            && MOVE_FAMILIES.contains(&p.family_token.as_str())
            && !scored.iter().any(|o| o.as_str() == p.alias)
            && !s.push_tried.contains(&(round, p.alias.clone()))
    })?;
    s.push_tried.push((round, target.alias.clone()));
    s.push_pending = Some((target.alias.clone(), target.have));
    Some("tactical".into())
}

fn card_named(id: &str, name: &str) -> bool {
    ti4_engine::strategy_cards::card_name(ContentStore::embedded(), id).as_deref() == Some(name)
}

fn subtype(choice: &Choice) -> &str {
    choice.context.as_ref().map_or("", |c| c.subtype.as_str())
}

/// Sol's forced Military Support deal, built one builder step at a time.
#[derive(Default, Debug)]
struct MsDeal {
    note: bool,
    ask: bool,
    amount: bool,
}

#[derive(Default)]
struct Shared {
    reserve: bool,
    followed_tech: Option<Value>,
    gd_forced: bool,
    unavailable: Option<String>,
    overrides: Vec<Value>,
    leaks: Vec<Value>,
    ms: Option<MsDeal>,
    ms_awaiting: bool,
    ms_log: Vec<Value>,
    sol_built: Vec<Value>,
    spend: Option<SpendStage>,
    spend_log: Vec<Value>,
    push_tried: Vec<(u32, String)>,
    push_pending: Option<(String, f64)>,
    push_log: Vec<Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SpendStage {
    Activating,
    Moving,
    Producing,
}

struct Force {
    inner: Box<dyn Decider>,
    seat: usize,
    arm: Arm,
    tech_seat: usize,
    shared: Rc<RefCell<Shared>>,
}

impl Force {
    fn steer(
        &self,
        s: &mut Shared,
        c: &Choice,
        seen: &SeatObservation<'_>,
        proposed: &ChoiceOption,
    ) -> Option<String> {
        let round = seen.round();
        let sub = subtype(c);
        if round == 1 && sub == "draft_strategy_card" {
            let want = match self.arm {
                Arm::TechElsewhere | Arm::TechGd if self.seat == self.tech_seat => "Technology",
                Arm::Trade if self.seat == 0 => "Trade",
                _ => return None,
            };
            let pick = c.options.iter().find(|o| card_named(&o.id, want));
            if pick.is_none() {
                s.unavailable = Some(format!("{want} not offered to seat{}", self.seat));
            }
            return pick.map(|o| o.id.clone());
        }
        if self.seat != 0 {
            if sub == "diplomacy_response" && s.ms_awaiting {
                s.ms_awaiting = false;
                s.ms_log
                    .push(json!({"round":round,"seat":self.seat,"response":proposed.id}));
            }
            return None;
        }
        if self.arm == Arm::CarrierDoctrine {
            return doctrine(c, sub, proposed);
        }
        if self.arm == Arm::SpendDown {
            return spend_down(s, c, seen, proposed);
        }
        if self.arm == Arm::NoEarlyPass {
            return no_early_pass(s, c, seen, proposed);
        }
        let me = seen.bound_seat();
        let sol = seen.seat(me)?;
        let resources = seen.available_spend(me, Spend::Resources);
        if self.arm == Arm::TechGd && round == 1 {
            let kind = c.details.get("kind").and_then(Value::as_str);
            let card = c.details.get("card").and_then(Value::as_str).unwrap_or("");
            if s.reserve && kind == Some("strategy_secondary") {
                if card_named(card, "Technology") {
                    s.reserve = false;
                    let yes = c.option("yes").is_some();
                    s.followed_tech = Some(
                        json!({"offered_yes":yes,"tokens":sol.strategic_tokens,"resources":resources}),
                    );
                    return yes.then(|| "yes".into());
                }
                let costs = c.details.get("costs_token").and_then(Value::as_bool) == Some(true);
                if costs && proposed.id == "yes" && sol.strategic_tokens <= 1 {
                    return Some("no".into());
                }
            }
            if s.reserve && sub == "buy_token_with_influence" && proposed.id == "yes" {
                // shortcut: planets exhausted for influence also stop counting as resources.
                return c.option("no").map(|o| o.id.clone());
            }
            if s.reserve
                && matches!(sub, "produce_unit" | "produce_one_unit")
                && proposed.kind == "produce"
            {
                let fits = |o: &ChoiceOption| {
                    let avail = o
                        .payload
                        .get("available_resources")
                        .and_then(Value::as_i64)
                        .unwrap_or(resources);
                    let owed = o
                        .payload
                        .get("owed")
                        .or(o.payload.get("cost"))
                        .and_then(Value::as_i64)
                        .unwrap_or(0);
                    o.kind != "produce" || avail - owed >= RESERVE_RESOURCES
                };
                if !fits(proposed) {
                    let alt = c
                        .options
                        .iter()
                        .find(|o| o.is_decline())
                        .or_else(|| c.options.iter().find(|o| fits(o)));
                    if alt.is_none() {
                        s.leaks.push(json!({"subtype":sub,"proposed":proposed.id}));
                    }
                    return alt.map(|o| o.id.clone());
                }
            }
            if s.reserve && sub.starts_with("pay_") {
                // shortcut: payments have no decline; leaks are recorded, not prevented.
                s.leaks
                    .push(json!({"subtype":sub,"proposed":proposed.id,"resources":resources}));
            }
            if !s.gd_forced && s.followed_tech.is_some() && sub == "research_technology" {
                s.gd_forced = true;
                return c.option("gd").map(|o| o.id.clone());
            }
        }
        if self.arm == Arm::MilitarySupport {
            let opens: Vec<_> = c
                .options
                .iter()
                .filter(|o| o.kind == "open_diplomacy")
                .collect();
            if !opens.is_empty() {
                s.ms = None;
                if sol.strategic_tokens != 0 {
                    return None;
                }
                let richest = seen
                    .players()
                    .into_iter()
                    .filter(|p| *p != me)
                    .filter_map(|p| seen.seat(p))
                    .filter(|p| p.trade_goods > 0)
                    .max_by_key(|p| p.trade_goods)?;
                let name = richest.faction.as_str().to_owned();
                let open = opens.iter().find(|o| o.label.ends_with(&name))?;
                s.ms = Some(MsDeal::default());
                s.ms_log.push(
                    json!({"round":round,"to":name,"partner_tg":richest.trade_goods,"opened":true}),
                );
                return Some(open.id.clone());
            }
            if let Some(deal) = s.ms.as_mut() {
                use ti4_engine::diplomacy::builder::{CANCEL_ID, DONE_ID, PROPOSE_ID};
                let has = |id: &str| c.option(id).is_some();
                let step = match sub {
                    "diplomacy_offer_item" if !deal.note && has("diplomacy|note|ms:sol") => {
                        deal.note = true;
                        Some("diplomacy|note|ms:sol".to_owned())
                    }
                    "diplomacy_offer_item" if deal.note => Some(DONE_ID.to_owned()),
                    "diplomacy_ask_item" if !deal.ask && has("diplomacy|now|tg") => {
                        deal.ask = true;
                        Some("diplomacy|now|tg".to_owned())
                    }
                    "diplomacy_ask_item" if deal.ask && deal.amount => Some(DONE_ID.to_owned()),
                    "diplomacy_amount" if deal.ask && !deal.amount => {
                        deal.amount = true;
                        Some("diplomacy|amount|1".to_owned())
                    }
                    "diplomacy_review" if deal.note && deal.amount => {
                        Some(PROPOSE_ID.to_owned())
                    }
                    other if other.starts_with("diplomacy_") => {
                        has(CANCEL_ID).then(|| CANCEL_ID.to_owned())
                    }
                    _ => None,
                };
                let ends = step
                    .as_deref()
                    .is_none_or(|id| id == PROPOSE_ID || id == CANCEL_ID);
                s.ms_awaiting = step.as_deref() == Some(PROPOSE_ID);
                if ends {
                    let end = step.clone().unwrap_or_else(|| format!("natural at {sub}"));
                    s.ms_log
                        .push(json!({"round":round,"end":end,"deal":format!("{deal:?}")}));
                    s.ms = None;
                }
                return step.filter(|id| has(id));
            }
        }
        None
    }
}

impl Decider for Force {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.inner.choose(choice)
    }
    fn choose_seeing(
        &mut self,
        c: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        // Asked first and always, so the seat's own stream advances exactly as when unforced.
        let proposed = self.inner.choose_seeing(c, seen)?;
        let mut s = self.shared.borrow_mut();
        let executed = match self.steer(&mut s, c, seen, &proposed) {
            None => proposed.clone(),
            Some(id) => c
                .option(&id)
                .cloned()
                .ok_or_else(|| IllegalChoice::DeciderFailed {
                    player: c.player.clone(),
                    prompt: c.prompt.clone(),
                    reason: format!("forced {id} not offered"),
                })?,
        };
        if self.seat == 0 && executed.kind == "produce" {
            let field = |k: &str| executed.payload.get(k).cloned().unwrap_or(Value::Null);
            s.sol_built.push(json!({"round":seen.round(),"subtype":subtype(c),"unit":field("unit"),
                "count":field("count"),"placed":field("placed"),"cost":field("cost")}));
        }
        if executed.id != proposed.id {
            s.overrides.push(json!({"seat":self.seat,"round":seen.round(),"subtype":subtype(c),
                "proposed":proposed.id,"executed":executed.id}));
        }
        Ok(executed)
    }
    fn stage_scores(&mut self, scores: Vec<(Option<f64>, Option<f64>)>) {
        self.inner.stage_scores(scores);
    }
}

/// One seat's forces: unit counts by type in space and on planets, how spread its ships are,
/// its biggest stack and how many of its command tokens are on the board.
fn census(state: &ti4_model::state::GameState, p: &PlayerId) -> Value {
    let mut space: BTreeMap<String, u32> = BTreeMap::new();
    let mut ground: BTreeMap<String, u32> = BTreeMap::new();
    let (mut systems, mut largest, mut tokens) = (0, 0, 0);
    for s in state.board.values() {
        let ships = s.units.iter().filter(|u| u.owner == *p).count();
        if ships > 0 {
            systems += 1;
            largest = largest.max(ships);
        }
        tokens += usize::from(s.command_tokens.contains(p));
        for u in s.units.iter().filter(|u| u.owner == *p) {
            *space.entry(u.type_id.to_string()).or_default() += 1;
        }
        for u in s.planet_units.values().flatten().filter(|u| u.owner == *p) {
            *ground.entry(u.type_id.to_string()).or_default() += 1;
        }
    }
    json!({"space":space,"ground":ground,"ship_systems":systems,"largest_stack":largest,"board_tokens":tokens})
}

fn card_progress(cards: Vec<ti4_engine::objectives::CardProgress>) -> Value {
    json!(cards.into_iter().map(|p| json!({"alias":p.alias,"family":p.family_token,"have":p.have,"threshold":p.threshold,"satisfied":p.satisfied})).collect::<Vec<_>>())
}

fn checkpoint(game: &Game<'_>, baseline: progress::Baseline, label: &str) -> Value {
    let p = PlayerId::new("seat0");
    let seen = Observed::new(&game.state, ContentStore::embedded(), SCOPE, game.galaxy());
    let seat = game.state.player(&p).expect("Sol seated");
    let seats: Vec<_> = game
        .state
        .players
        .iter()
        .map(|s| {
            json!({"id":s.id,"faction":s.faction,"vp":s.victory_points,"tg":s.trade_goods,"cards":s.strategy_cards,
                "technologies":s.technologies,"breakthrough":s.breakthrough,"leaders":s.leaders,
                "fleet_tokens":s.fleet_tokens,"tactic_tokens":s.tactic_tokens,"fleet":census(&game.state,&s.id),
                "ready_resources":seen.available_spend(&s.id,Spend::Resources)-i64::from(s.trade_goods)})
        })
        .collect();
    json!({"label":label,"round":game.state.round,"phase":format!("{:?}",game.state.phase),
        "vp":seat.victory_points,"tg":seat.trade_goods,"strategic_tokens":seat.strategic_tokens,
        "resources":seen.available_spend(&p,Spend::Resources),"planets":seen.controlled_planets(&p).len(),
        "has_gd":seat.technologies.iter().any(|t|t.as_str()=="gd"),"technologies":seat.technologies,
        "scored_public":game.state.scored_by(&p),"progress":progress::measure(&seen,&p,baseline),"seats":seats,
        "objectives":card_progress(seen.revealed_objective_progress(&p)),
        "secrets":card_progress(ti4_engine::choice::held_secret_progress(&game.state,ContentStore::embedded(),SCOPE,game.galaxy(),&p))})
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Job {
    origin: u64,
    arm: Arm,
    dice: u64,
    repeat: bool,
}

fn play(
    actor: &Rc<Actor>,
    vocabulary: &Vocabulary,
    pool: &Arc<ti4_sim::MapPool>,
    rounds: u32,
    job: &Job,
) -> Value {
    let store = ContentStore::embedded();
    let players: Vec<_> = (0..6).map(|i| PlayerId::new(format!("seat{i}"))).collect();
    let factions = players
        .iter()
        .cloned()
        .zip(FACTIONS.map(FactionId::new))
        .collect();
    let shared = Rc::new(RefCell::new(Shared {
        reserve: job.arm == Arm::TechGd,
        ..Shared::default()
    }));
    let tech = tech_seat(job.origin);
    let mut statuses: Vec<InferenceStatus> = Vec::new();
    let mut sol_baseline = progress::Baseline::default();
    let head = json!({"origin":job.origin,"arm":job.arm,"dice":job.dice,"repeat":job.repeat,"tech_seat":tech});
    let game = ti4_training::rollout::setup_game_with_capabilities_and_decider_factory(
        store,
        &players,
        &factions,
        SCOPE,
        job.origin,
        &ti4_training::rollout::OpeningMap::PythonPool {
            pool: Arc::clone(pool),
            tile_seed_offset: 0,
        },
        ti4_training::rollout::SimulationCapabilities { diplomacy: true },
        |baselines| {
            sol_baseline = baselines[&players[0]];
            let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
            for (i, p) in players.iter().enumerate() {
                let row = FactionRow::of(FACTIONS[i]).map_err(|e| e.to_string())?;
                let bot = MlpBot::sharing(
                    actor,
                    vocabulary.clone(),
                    row,
                    job.origin.wrapping_mul(1_000_003).wrapping_add(i as u64),
                )
                .at_temperature(0.001)
                .from_setup(baselines[p]);
                let (inner, status) = bot.seat();
                statuses.push(status);
                deciders.insert(
                    p.clone(),
                    Box::new(Force {
                        inner,
                        seat: i,
                        arm: job.arm,
                        tech_seat: tech,
                        shared: Rc::clone(&shared),
                    }),
                );
            }
            Ok(deciders)
        },
    );
    let mut game = match game {
        Ok(g) => g,
        Err(e) => return json!({"job":head,"error":e,"stage":"setup"}),
    };
    if job.dice > 0 {
        let mut at = game.rng_positions();
        for domain in CHANCE {
            *at.entry(domain.to_owned()).or_default() += u128::from(job.dice) << 48;
        }
        game.set_rng_positions(&at);
    }
    let mut snapshots = vec![checkpoint(&game, sol_baseline, "initial")];
    let mut error = None;
    let started = std::time::Instant::now();
    let mut steps = 0;
    while game.state.round <= rounds && !game.state.finished {
        if shared.borrow().unavailable.is_some() {
            break;
        }
        if steps >= 40_000 || started.elapsed().as_secs() >= 120 {
            error = Some("step/time bound reached".to_owned());
            break;
        }
        let phase = game.state.phase;
        let result = game.step();
        steps += 1;
        if let Some(e) = result.error {
            error = Some(e.to_string());
            break;
        }
        if phase == Phase::Action && game.state.phase == Phase::Status {
            snapshots.push(checkpoint(&game, sol_baseline, "end-action"));
        }
    }
    let inference: Vec<_> = statuses
        .into_iter()
        .map(|s| s.into_result().map_err(|e| e.to_string()))
        .collect();
    if inference.iter().any(Result::is_err) {
        error = Some(format!("inference status {inference:?}"));
    }
    let s = shared.borrow();
    if error.is_none()
        && s.unavailable.is_none()
        && !(game.state.round > rounds || game.state.finished)
    {
        error = Some("horizon not reached".into());
    }
    snapshots.push(checkpoint(&game, sol_baseline, "final"));
    let state_hash = hash(serde_json::to_vec(&game.state).expect("state serializes"));
    json!({"job":head,"error":error,"unavailable":s.unavailable,"steps":steps,"state_hash":state_hash,
        "followed_tech":s.followed_tech,"gd_forced":s.gd_forced,"overrides":s.overrides,"leaks":s.leaks,
        "ms_log":s.ms_log,"sol_built":s.sol_built,"spend_log":s.spend_log,"push_log":s.push_log,"snapshots":snapshots})
}

fn execute() -> Result<(), String> {
    let mut args = BTreeMap::new();
    let mut input = std::env::args().skip(1);
    let known = [
        "--bundle",
        "--out",
        "--origins",
        "--dice",
        "--seed-base",
        "--workers",
        "--map-pool",
        "--rounds",
        "--arms",
    ];
    while let Some(flag) = input.next() {
        if !known.contains(&flag.as_str()) || args.contains_key(&flag) {
            return Err(format!("unknown/duplicate {flag}"));
        }
        args.insert(
            flag.clone(),
            input.next().ok_or(format!("missing value {flag}"))?,
        );
    }
    let number = |k: &str, d: u64| -> Result<u64, String> {
        args.get(k)
            .map_or(Ok(d), |v| v.parse().map_err(|_| format!("invalid {k}")))
    };
    let origins = number("--origins", 10)?;
    let dice = number("--dice", 16)?;
    let base = number("--seed-base", 610_170_001)?;
    let workers = usize::try_from(number("--workers", 16)?).map_err(|_| "workers overflow")?;
    let rounds = u32::try_from(number("--rounds", 4)?).map_err(|_| "rounds overflow")?;
    if !(1..=64).contains(&origins)
        || !(1..=64).contains(&dice)
        || !(1..=32).contains(&workers)
        || !(1..=4).contains(&rounds)
    {
        return Err("origins/dice 1..64; workers 1..32; rounds 1..4".into());
    }
    let arms: Vec<Arm> = match args.get("--arms") {
        None => ARMS.to_vec(),
        Some(list) => list
            .split(',')
            .map(|a| serde_json::from_value(json!(a)).map_err(|_| format!("unknown arm {a}")))
            .collect::<Result<_, _>>()?,
    };
    let bundle = Path::new(args.get("--bundle").ok_or("--bundle required")?);
    let output = Path::new(args.get("--out").ok_or("--out required")?);
    if output.exists() || !output.parent().is_some_and(Path::is_dir) {
        return Err("output must be new within an existing directory".into());
    }
    let pool_path = args.get("--map-pool").map_or(
        "D:/Projects/ti4-engine-rs/out/pools/full_np8_12_train.json",
        String::as_str,
    );
    let pool_bytes = ti4_sim::artifacts::read_and_verify_pool_role(
        Path::new(pool_path),
        &[ti4_sim::artifacts::ArtifactRole::Train],
    )
    .map_err(|e| e.to_string())?;
    let pool_hash = hash(&pool_bytes);
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(pool_bytes))
            .map_err(|e| e.to_string())?,
    );
    ti4_tensor::configure_deterministic(20_261_016).map_err(|e| e.to_string())?;
    let loaded = ti4_mlp::bundle::read(bundle).map_err(|e| e.to_string())?;
    let mut jobs = Vec::new();
    for origin in base..base + origins {
        for &arm in &arms {
            for d in 0..dice {
                jobs.push(Job {
                    origin,
                    arm,
                    dice: d,
                    repeat: false,
                });
            }
        }
        // Determinism control: control/dice0 played twice must give one state.
        jobs.push(Job {
            origin,
            arm: Arm::Control,
            dice: 0,
            repeat: true,
        });
    }
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build_global()
        .map_err(|e| e.to_string())?;
    let done = std::sync::atomic::AtomicUsize::new(0);
    let total = jobs.len();
    // One actor copy per worker; jobs are interleaved so slow origins spread across workers.
    let copies: Vec<_> = (0..workers)
        .map(|w| (w, loaded.actor.inference_copy()))
        .collect();
    let results: Vec<Value> = copies
        .into_par_iter()
        .map(|(w, actor)| {
            let actor = Rc::new(actor);
            jobs.iter()
                .skip(w)
                .step_by(workers)
                .map(|j| {
                    let r = play(&actor, &loaded.vocabulary, &pool, rounds, j);
                    let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    let status = if !r["error"].is_null() {
                        "FAILED"
                    } else if !r["unavailable"].is_null() {
                        "unavailable"
                    } else {
                        "complete"
                    };
                    let rep = if j.repeat { " repeat" } else { "" };
                    eprintln!(
                        "[{n}/{total}] origin{} {:?} dice{}{rep}: {status}",
                        j.origin, j.arm, j.dice
                    );
                    r
                })
                .collect::<Vec<_>>()
        })
        .flatten()
        .collect();
    let mut repeats = Vec::new();
    for r in results.iter().filter(|r| r["job"]["repeat"] == true) {
        let twin = results.iter().find(|o| {
            o["job"]["origin"] == r["job"]["origin"]
                && o["job"]["arm"] == "control"
                && o["job"]["dice"] == 0
                && o["job"]["repeat"] == false
        });
        let equal =
            r["error"].is_null() && twin.is_some_and(|t| t["state_hash"] == r["state_hash"]);
        repeats.push(json!({"origin":r["job"]["origin"],"equal":equal}));
    }
    let report = json!({"schema":1,"status":"sol-round1-pilot-diagnostic-not-training-labels",
        "seed_base":base,"origins":origins,"dice":dice,"rounds":rounds,"temperature":0.001,
        "diplomacy":true,"arms":arms,"lineup":FACTIONS,"pool":pool_path,"pool_sha256":pool_hash,
        "checkpoint":bundle,"source_sha256":hash(include_bytes!("sol_round1_pilot.rs")),
        "source_claims":["C13","C15","C17","C18","C19","C21"],"determinism":repeats,"results":results});
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|e| e.to_string())?;
    let mut file = std::io::BufWriter::new(file);
    serde_json::to_writer(&mut file, &report).map_err(|e| e.to_string())?;
    file.write_all(b"\n").map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())?;
    if repeats.iter().any(|r| r["equal"] != true) {
        return Err("determinism control failed; report retained".into());
    }
    Ok(())
}

fn main() {
    if let Err(e) = execute() {
        eprintln!("REFUSED: {e}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tech_seat_is_another_seat_and_spreads() {
        let seats: std::collections::BTreeSet<_> =
            (610_170_001..610_170_101).map(tech_seat).collect();
        assert!(seats.iter().all(|s| (1..=5).contains(s)));
        assert_eq!(seats.len(), 5);
    }

    #[test]
    fn card_names_resolve() {
        assert!(card_named("pok7technology", "Technology"));
        assert!(!card_named("pok1leadership", "Technology"));
    }

    #[test]
    fn doctrine_swaps_dreadnoughts_for_fitting_fighters_and_fills_on_stop() {
        let build = |unit: &str, excess: i64| {
            ChoiceOption::new(format!("build|{unit}|1"), "produce")
                .with("unit", unit)
                .with("capacity_excess_after", excess)
        };
        let c = Choice::new(
            PlayerId::new("seat0"),
            "produce",
            vec![build("dreadnought", 0), build("fighter", 0), build("sol_infantry", 0), ChoiceOption::new("done_producing", "decline")],
        );
        let dread = c.options[0].clone();
        let stop = c.options[3].clone();
        assert_eq!(doctrine(&c, "produce_unit", &dread).as_deref(), Some("build|fighter|1"));
        assert_eq!(doctrine(&c, "produce_unit", &stop).as_deref(), Some("build|fighter|1"));
        let full = Choice::new(
            PlayerId::new("seat0"),
            "produce",
            vec![build("dreadnought", 0), build("fighter", 1), build("sol_infantry", 0), ChoiceOption::decline()],
        );
        assert_eq!(doctrine(&full, "produce_unit", &dread).as_deref(), Some("build|sol_infantry|1"));
        assert_eq!(doctrine(&full, "produce_unit", &stop), None);
    }

    #[test]
    fn arms_parse_from_cli_names() {
        let arm: Arm = serde_json::from_value(json!("tech-gd")).unwrap();
        assert_eq!(arm, Arm::TechGd);
    }
}
