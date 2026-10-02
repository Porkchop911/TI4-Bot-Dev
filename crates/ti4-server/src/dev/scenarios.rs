//! Authoritative dev scenario presets and builder logic.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_model::content_types::POK;
use ti4_model::id::{
    ActionCardId, ObjectiveId, PlanetId, PlayerId, StrategyCardId, SystemId, UnitTypeId,
};
use ti4_model::units::Unit;

use std::thread;
use std::time::Duration;

use crate::protocol::server::ServerMessage;
use crate::protocol::status::ViewerRole;
use crate::session::{GameRegistry, GameSession, MockClient, SeatController, SessionConfig};
use crate::storage::{
    LobbySlotId, PLAYER_RECORD_VERSION, PersistedLobbyPhase, PlayerLobbyMember, PlayerLobbyRecord,
    PlayerLobbySlot, PlayerSession, generate_player_id,
};

/// Summary metadata for a dev scenario available to launch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioSummary {
    pub id: String,
    pub title: String,
    pub category: String,
    pub description: String,
    pub player_count: usize,
    pub human_faction: String,
    pub opponent_factions: Vec<String>,
}

/// Request to launch a specific dev scenario.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchScenarioRequest {
    pub scenario_id: String,
    pub seed: Option<u64>,
}

/// Response returned when a dev scenario is launched.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchScenarioResponse {
    pub game_id: String,
    pub player_session: String,
    pub player_id: String,
    pub scenario_id: String,
    /// Only returned by the dev-only four-view script launcher.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_seats: Option<BTreeMap<String, String>>,
}

/// List of all registered dev scenarios.
#[must_use]
pub fn available_scenarios() -> Vec<ScenarioSummary> {
    vec![
        ScenarioSummary {
            id: "research_tech_skips".to_owned(),
            title: "Technology Research & Skips".to_owned(),
            category: "Strategy".to_owned(),
            description: "Player 1 (Federation of Sol) has played the Technology strategy card. Choose technologies to research, toggle ready tech specialty planets for skips, and pay for a second technology.".to_owned(),
            player_count: 3,
            human_faction: "Federation of Sol".to_owned(),
            opponent_factions: vec!["Emirates of Hacan".to_owned(), "Barony of Letnev".to_owned()],
        },
        ScenarioSummary {
            id: "tactical_action".to_owned(),
            title: "Tactical Action & Movement".to_owned(),
            category: "Tactical".to_owned(),
            description: "Player 1 (Federation of Sol) is active in Round 1 Action Phase with full tactic pool and unexhausted fleet at Jord (Home #01). Activate an adjacent sector, move Carrier, Cruiser, Fighters, and Infantry, and resolve planetary exploration or landing.".to_owned(),
            player_count: 3,
            human_faction: "Federation of Sol".to_owned(),
            opponent_factions: vec!["Emirates of Hacan".to_owned(), "Barony of Letnev".to_owned()],
        },
        ScenarioSummary {
            id: "production_batch".to_owned(),
            title: "Production Batch".to_owned(),
            category: "Tactical".to_owned(),
            description: "Sol is ready to produce at Jord with a free production use and another controlled planet.".to_owned(),
            player_count: 3,
            human_faction: "Federation of Sol".to_owned(),
            opponent_factions: vec!["Emirates of Hacan".to_owned(), "Barony of Letnev".to_owned()],
        },
        ScenarioSummary {
            id: "production_payment_batch".to_owned(),
            title: "Production Payment Batch".to_owned(),
            category: "Tactical".to_owned(),
            description: "Sol is ready to produce at Jord and can pay using multiple controlled planets.".to_owned(),
            player_count: 3,
            human_faction: "Federation of Sol".to_owned(),
            opponent_factions: vec!["Emirates of Hacan".to_owned(), "Barony of Letnev".to_owned()],
        },
        ScenarioSummary {
            id: "production_payment_autospend".to_owned(),
            title: "Production Payment Auto-spend".to_owned(),
            category: "Tactical".to_owned(),
            description: "Sol can pay with Jord and Wren Terra; the last payment option is spent automatically.".to_owned(),
            player_count: 3,
            human_faction: "Federation of Sol".to_owned(),
            opponent_factions: vec!["Emirates of Hacan".to_owned(), "Barony of Letnev".to_owned()],
        },
        ScenarioSummary {
            id: "space_combat".to_owned(),
            title: "Space Combat Encounter".to_owned(),
            category: "Combat".to_owned(),
            description: "Hostile Barony of Letnev ships (Cruiser, Destroyer, 2 Fighters) are positioned in the adjacent border system. Activate the contested sector and move your fleet in to trigger the full space combat flow: anti-fighter barrage, combat rolls, sustain damage, hit assignment, and retreat choices against the bot.".to_owned(),
            player_count: 3,
            human_faction: "Federation of Sol".to_owned(),
            opponent_factions: vec!["Emirates of Hacan".to_owned(), "Barony of Letnev".to_owned()],
        },
        ScenarioSummary {
            id: "ongoing_combat".to_owned(),
            title: "Ongoing Space Combat".to_owned(),
            category: "Combat".to_owned(),
            description: "Space combat is already in progress between your Sol fleet and Letnev's armada in a contested sector. Play combat action cards such as Direct Hit and Shields Holding while resolving dice rolls, sustain damage, and casualties without setting up movement first.".to_owned(),
            player_count: 3,
            human_faction: "Federation of Sol".to_owned(),
            opponent_factions: vec!["Emirates of Hacan".to_owned(), "Barony of Letnev".to_owned()],
        },
        ScenarioSummary {
            id: "ongoing_combat_four_views".to_owned(),
            title: "Four-view Space Combat".to_owned(),
            category: "Combat".to_owned(),
            description: "Human-controlled Sol, Letnev and Hacan; Sol holds combat cards and Letnev holds Sabotage.".to_owned(),
            player_count: 3,
            human_faction: "Federation of Sol".to_owned(),
            opponent_factions: vec!["Emirates of Hacan".to_owned(), "Barony of Letnev".to_owned()],
        },
        ScenarioSummary {
            id: "ongoing_invasion_four_views".to_owned(),
            title: "Four-view Invasion".to_owned(),
            category: "Combat".to_owned(),
            description: "Three human seats and a spectator, starting before the invasion reaction on a single contested planet.".to_owned(),
            player_count: 3,
            human_faction: "Federation of Sol".to_owned(),
            opponent_factions: vec!["Emirates of Hacan".to_owned(), "Barony of Letnev".to_owned()],
        },
        ScenarioSummary {
            id: "ongoing_invasion_coexistence".to_owned(),
            title: "Multi-planet Invasion".to_owned(),
            category: "Combat".to_owned(),
            description: "Two contested planets with coexisting rival ground forces.".to_owned(),
            player_count: 3,
            human_faction: "Federation of Sol".to_owned(),
            opponent_factions: vec!["Emirates of Hacan".to_owned(), "Barony of Letnev".to_owned()],
        },
        ScenarioSummary {
            id: "ongoing_invasion_parley".to_owned(),
            title: "Interrupted Invasion Landing".to_owned(),
            category: "Combat".to_owned(),
            description: "Single-planet invasion with a human defender holding Parley to interrupt sequential landings.".to_owned(),
            player_count: 3,
            human_faction: "Federation of Sol".to_owned(),
            opponent_factions: vec!["Emirates of Hacan".to_owned(), "Barony of Letnev".to_owned()],
        },
        ScenarioSummary {
            id: "endgame".to_owned(),
            title: "Endgame to Victory".to_owned(),
            category: "Endgame".to_owned(),
            description: "Three seats near completion: bots pass final action turns, score status objectives, and reach Game Over.".to_owned(),
            player_count: 3,
            human_faction: "Federation of Sol".to_owned(),
            opponent_factions: vec!["Emirates of Hacan".to_owned(), "Barony of Letnev".to_owned()],
        },
        ScenarioSummary {
            id: "score_objective_status".to_owned(),
            title: "Score Objective (Status Phase)".to_owned(),
            category: "Status".to_owned(),
            description: "Player 1 (Federation of Sol) is in the Status Phase with multiple scoreable public objectives (Lead From the Front and Negotiate Trade Routes) that have not been scored yet. Choose which public objective to score.".to_owned(),
            player_count: 3,
            human_faction: "Federation of Sol".to_owned(),
            opponent_factions: vec!["Emirates of Hacan".to_owned(), "Barony of Letnev".to_owned()],
        },
        ScenarioSummary {
            id: "score_objective_imperial".to_owned(),
            title: "Score Objective (Imperial)".to_owned(),
            category: "Strategy".to_owned(),
            description: "Player 1 (Federation of Sol) plays the Imperial strategy card primary ability, allowing them to score a public objective immediately during the Action Phase. Choose which public objective to score.".to_owned(),
            player_count: 3,
            human_faction: "Federation of Sol".to_owned(),
            opponent_factions: vec!["Emirates of Hacan".to_owned(), "Barony of Letnev".to_owned()],
        },
        ScenarioSummary {
            id: "full_game".to_owned(),
            title: "Full Game Match".to_owned(),
            category: "Match".to_owned(),
            description: "Fresh 3-player match from Round 1 setup through to Game Over, with external agents or humans occupying all seats.".to_owned(),
            player_count: 3,
            human_faction: "Federation of Sol".to_owned(),
            opponent_factions: vec!["Emirates of Hacan".to_owned(), "Barony of Letnev".to_owned()],
        },
    ]
}

/// Builds and registers a scenario session into the registry.
pub fn launch_scenario(
    registry: &Arc<GameRegistry>,
    scenario_id: &str,
    seed: Option<u64>,
) -> Result<LaunchScenarioResponse, String> {
    let seed = seed.unwrap_or_else(rand::random::<u64>);
    let (mut config, lobby_record, human_player, session_token, border_system) = match scenario_id {
        "tactical_action" => {
            let (c, l, p, t) = build_tactical_scenario(seed)?;
            (c, l, p, t, String::new())
        }
        "research_tech_skips" => {
            let (c, l, p, t) = build_research_scenario(seed)?;
            (c, l, p, t, String::new())
        }
        "score_objective_status" => {
            let (c, l, p, t) = build_score_objective_status_scenario(seed)?;
            (c, l, p, t, String::new())
        }
        "score_objective_imperial" => {
            let (c, l, p, t) = build_score_objective_imperial_scenario(seed)?;
            (c, l, p, t, String::new())
        }
        "production_batch" | "production_payment_batch" | "production_payment_autospend" => {
            let (c, l, p, t) = build_production_scenario(seed, scenario_id)?;
            (c, l, p, t, String::new())
        }
        "space_combat" | "ongoing_combat" | "ongoing_combat_four_views" => {
            build_space_combat_scenario(seed)?
        }
        "ongoing_invasion_four_views"
        | "ongoing_invasion_coexistence"
        | "ongoing_invasion_parley" => {
            build_invasion_scenario(seed, scenario_id == "ongoing_invasion_coexistence")?
        }
        "endgame" => {
            let (c, l, p, t) = build_endgame_scenario(seed)?;
            (c, l, p, t, String::new())
        }
        "full_game" => {
            let (c, l, p, t) = build_full_game_scenario(seed)?;
            (c, l, p, t, String::new())
        }
        other => return Err(format!("Unknown scenario '{other}'")),
    };

    let test_seats =
        if scenario_id == "ongoing_combat_four_views" {
            // Leave the winner with ships after the scripted return fire, so the
            // immediate SPACE_COMBAT_WON Salvage window is exercised as well.
            config
                .state
                .player_mut(&human_player)
                .expect("Sol is seated")
                .fleet_tokens = 8;
            config
                .state
                .player_mut(&human_player)
                .expect("Sol is seated")
                .action_cards
                .push(ActionCardId::new("sh2"));
            config.state.system_mut(&SystemId::new("01")).units.extend(
                (0..2).map(|_| Unit::new(UnitTypeId::new("cruiser"), human_player.clone())),
            );
            // The attacking fleet fires a first-round anti-fighter barrage before
            // retreat announcements and ordinary combat rolls.
            config
                .state
                .system_mut(&SystemId::new("01"))
                .units
                .push(Unit::new(
                    UnitTypeId::new("destroyer"),
                    human_player.clone(),
                ));
            let seats = config
                .seat_tokens
                .iter()
                .map(|(id, token)| (id.to_string(), token.clone()))
                .collect();
            for (seat, controller) in &mut config.seats {
                *controller = SeatController::Human;
                if seat != &human_player
                    && config
                        .state
                        .player(seat)
                        .is_some_and(|p| p.faction.as_str() == "letnev")
                {
                    config
                        .state
                        .player_mut(seat)
                        .expect("Letnev is seated")
                        .action_cards
                        .push(ActionCardId::new("sabo1"));
                }
            }
            Some(seats)
        } else if scenario_id.starts_with("ongoing_invasion_") {
            if scenario_id == "ongoing_invasion_parley" {
                let defender = config
                    .seats
                    .keys()
                    .find(|seat| {
                        config
                            .state
                            .player(seat)
                            .is_some_and(|player| player.faction.as_str() == "letnev")
                    })
                    .expect("Letnev seated")
                    .clone();
                config
                    .state
                    .player_mut(&defender)
                    .expect("defender seated")
                    .action_cards
                    .push(ActionCardId::new("parley"));
            }
            if scenario_id != "ongoing_invasion_coexistence" {
                for controller in config.seats.values_mut() {
                    *controller = SeatController::Human;
                }
            }
            Some(
                config
                    .seat_tokens
                    .iter()
                    .map(|(id, token)| (id.to_string(), token.clone()))
                    .collect(),
            )
        } else if scenario_id == "endgame" || scenario_id == "full_game" {
            Some(
                config
                    .seat_tokens
                    .iter()
                    .map(|(id, token)| (id.to_string(), token.clone()))
                    .collect(),
            )
        } else {
            None
        };
    let game_id = config.game_id.clone();
    let session = registry.launch_dev_scenario(config, lobby_record)?;

    // Scripted scenarios advance the engine through initial decisions before returning
    // the launch response. When persistence is enabled, these decisions are durably logged
    // using the normal session store. If scripting fails, the error is returned and no
    // launch response (URL/token) is advertised; the save remains recoverable at the last
    // committed decision.
    if scenario_id == "ongoing_combat" || scenario_id == "ongoing_combat_four_views" {
        advance_into_space_combat(
            &session,
            &human_player,
            &border_system,
            scenario_id == "ongoing_combat_four_views",
        )?;
    }
    if scenario_id.starts_with("ongoing_invasion_") {
        advance_into_invasion(&session, &human_player, &border_system)?;
    }
    if scenario_id.starts_with("production_") {
        advance_into_production(&session, &human_player)?;
    }
    if scenario_id == "research_tech_skips" {
        advance_into_research(&session, &human_player)?;
    }
    if scenario_id == "score_objective_status" {
        wait_for_decision_subtype(&session, &human_player, "score_objective")?;
    }
    if scenario_id == "score_objective_imperial" {
        advance_into_imperial_scoring(&session, &human_player)?;
    }

    Ok(LaunchScenarioResponse {
        game_id,
        player_session: session_token,
        player_id: human_player.to_string(),
        scenario_id: scenario_id.to_owned(),
        test_seats,
    })
}

fn build_research_scenario(
    seed: u64,
) -> Result<(SessionConfig, PlayerLobbyRecord, PlayerId, String), String> {
    let (mut config, lobby, player, token, galaxy, _, _) = setup_base_3p_game(seed, "dev_tech")?;
    let content = ContentStore::embedded();

    let strat_tech = StrategyCardId::new("pok7technology");
    if let Some(sol) = config.state.player_mut(&player) {
        sol.strategy_cards = vec![strat_tech.clone()];
        sol.trade_goods = 6;
        sol.technologies = [
            ti4_model::id::TechnologyId::new("amd"),
            ti4_model::id::TechnologyId::new("nm"),
        ]
        .into_iter()
        .collect();
    }
    config
        .state
        .unclaimed_strategy_cards
        .retain(|c| c != &strat_tech);
    config
        .state
        .unclaimed_strategy_cards
        .push(StrategyCardId::new("leadership"));

    let all_planets = ti4_content::galaxy::all_planets(content, POK);
    let mut ready_assigned = false;
    let mut exhausted_assigned = false;

    for system_id in galaxy.system_ids() {
        if system_id == "01" {
            continue;
        }
        if let Some(tile) = ti4_content::galaxy::system(content, system_id, POK) {
            for planet in tile.planets() {
                if let Some(rec) = all_planets.get(planet) {
                    if !rec.tech_specialties().is_empty() {
                        let pid = PlanetId::new(planet);
                        config
                            .state
                            .system_mut(&SystemId::new(system_id))
                            .set_control(pid.clone(), player.clone());
                        if !ready_assigned {
                            ready_assigned = true;
                        } else if !exhausted_assigned {
                            config.state.exhaust_planet(pid);
                            exhausted_assigned = true;
                            break;
                        }
                    }
                }
            }
        }
        if ready_assigned && exhausted_assigned {
            break;
        }
    }

    Ok((config, lobby, player, token))
}

fn advance_into_research(session: &Arc<GameSession>, player: &PlayerId) -> Result<(), String> {
    let client = MockClient::connect(session.clone(), ViewerRole::Player(player.clone()));
    for _ in 0..20 {
        let mut offered = None;
        for _ in 0..150 {
            if let Ok(ServerMessage::PendingChoice(message)) = client.try_recv() {
                offered = Some(message.choice);
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let choice = offered
            .or_else(|| {
                client
                    .snapshot()
                    .pending_choice
                    .map(|envelope| envelope.choice)
            })
            .ok_or_else(|| {
                "timed out waiting for initial choice in advance_into_research".to_owned()
            })?;

        if choice
            .context
            .as_ref()
            .is_some_and(|ctx| ctx.subtype == "research_technology")
        {
            return Ok(());
        }

        let Some((_, nonce, version)) = session.current_pending_decision() else {
            return Err("no current pending decision in advance_into_research".to_owned());
        };

        if let Some(strat_opt) = choice
            .options
            .iter()
            .find(|o| o.id.starts_with("strategic|"))
        {
            client
                .submit(&nonce, version, &strat_opt.id)
                .map_err(|e| format!("submit strategic card failed: {e:?}"))?;
        } else if choice.options.iter().any(|o| o.id == "strategic") {
            client
                .submit(&nonce, version, "strategic")
                .map_err(|e| format!("submit strategic action failed: {e:?}"))?;
        } else if choice.options.iter().any(|o| o.id == "decline") {
            client
                .submit(&nonce, version, "decline")
                .map_err(|e| format!("submit decline failed: {e:?}"))?;
        } else {
            return Err(format!(
                "unexpected choice in advance_into_research: {:?}, options: {:?}",
                choice.prompt,
                choice.options.iter().map(|o| &o.id).collect::<Vec<_>>()
            ));
        }
    }
    Err("advance_into_research exceeded 20 iterations".to_owned())
}

fn build_score_objective_status_scenario(
    seed: u64,
) -> Result<(SessionConfig, PlayerLobbyRecord, PlayerId, String), String> {
    let (mut config, lobby, player, token, _galaxy, _, _) =
        setup_base_3p_game(seed, "dev_status_score")?;

    config.state.phase = ti4_model::state::Phase::Status;
    config.state.active = None;
    config.state.revealed_objectives = vec![
        ObjectiveId::new("lead"),
        ObjectiveId::new("trade_routes"),
        ObjectiveId::new("corner"),
    ];
    config.state.scored_objectives.clear();

    if let Some(sol) = config.state.player_mut(&player) {
        sol.tactic_tokens = 2;
        sol.strategic_tokens = 2; // total 4 tokens >= 3 for Lead From the Front
        sol.trade_goods = 6; // 6 trade goods >= 5 for Negotiate Trade Routes
        sol.victory_points = 2;
    }

    Ok((config, lobby, player, token))
}

fn build_score_objective_imperial_scenario(
    seed: u64,
) -> Result<(SessionConfig, PlayerLobbyRecord, PlayerId, String), String> {
    let (mut config, lobby, player, token, _galaxy, _, _) =
        setup_base_3p_game(seed, "dev_imperial_score")?;

    let strat_imperial = StrategyCardId::new("pok8imperial");
    if let Some(sol) = config.state.player_mut(&player) {
        sol.strategy_cards = vec![strat_imperial.clone()];
        sol.tactic_tokens = 2;
        sol.strategic_tokens = 2;
        sol.trade_goods = 6;
        sol.victory_points = 2;
    }
    config
        .state
        .unclaimed_strategy_cards
        .retain(|c| c != &strat_imperial);
    config
        .state
        .unclaimed_strategy_cards
        .push(StrategyCardId::new("leadership"));

    config.state.revealed_objectives = vec![
        ObjectiveId::new("lead"),
        ObjectiveId::new("trade_routes"),
        ObjectiveId::new("corner"),
    ];
    config.state.scored_objectives.clear();
    config.state.active = Some(player.clone());

    Ok((config, lobby, player, token))
}

fn advance_into_imperial_scoring(
    session: &Arc<GameSession>,
    player: &PlayerId,
) -> Result<(), String> {
    let client = MockClient::connect(session.clone(), ViewerRole::Player(player.clone()));
    for _ in 0..20 {
        let mut offered = None;
        for _ in 0..150 {
            if let Ok(ServerMessage::PendingChoice(message)) = client.try_recv() {
                offered = Some(message.choice);
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let choice = offered
            .or_else(|| {
                client
                    .snapshot()
                    .pending_choice
                    .map(|envelope| envelope.choice)
            })
            .ok_or_else(|| {
                "timed out waiting for initial choice in advance_into_imperial_scoring".to_owned()
            })?;

        if choice
            .context
            .as_ref()
            .is_some_and(|ctx| ctx.subtype == "imperial_score_objective")
        {
            return Ok(());
        }

        let Some((_, nonce, version)) = session.current_pending_decision() else {
            return Err("no current pending decision in advance_into_imperial_scoring".to_owned());
        };

        if let Some(strat_opt) = choice
            .options
            .iter()
            .find(|o| o.id.starts_with("strategic|") || o.id == "strategic")
        {
            client
                .submit(&nonce, version, &strat_opt.id)
                .map_err(|e| format!("submit imperial card failed: {e:?}"))?;
        } else if choice.options.iter().any(|o| o.id == "decline") {
            client
                .submit(&nonce, version, "decline")
                .map_err(|e| format!("submit decline failed: {e:?}"))?;
        } else {
            return Err(format!(
                "unexpected choice in advance_into_imperial_scoring: {:?}, options: {:?}",
                choice.prompt,
                choice.options.iter().map(|o| &o.id).collect::<Vec<_>>()
            ));
        }
    }
    Err("advance_into_imperial_scoring exceeded 20 iterations".to_owned())
}

fn wait_for_decision_subtype(
    session: &Arc<GameSession>,
    player: &PlayerId,
    wanted_subtype: &str,
) -> Result<(), String> {
    let client = MockClient::connect(session.clone(), ViewerRole::Player(player.clone()));
    for _ in 0..150 {
        if let Some(pending) = client.snapshot().pending_choice {
            if pending
                .choice
                .context
                .as_ref()
                .is_some_and(|ctx| ctx.subtype == wanted_subtype)
            {
                return Ok(());
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
    Err(format!("timed out waiting for {wanted_subtype} decision"))
}

fn build_production_scenario(
    seed: u64,
    scenario: &str,
) -> Result<(SessionConfig, PlayerLobbyRecord, PlayerId, String), String> {
    let (mut config, lobby, player, token, galaxy, _, _) =
        setup_base_3p_game(seed, "dev_production")?;
    let content = ContentStore::embedded();
    let planets: Vec<_> = galaxy
        .system_ids()
        .into_iter()
        .filter(|system| *system != "01")
        .filter_map(|system| {
            ti4_content::galaxy::system(content, system, POK)?
                .planets()
                .into_iter()
                .next()
                .map(|planet| (system, planet))
        })
        .take(if scenario == "production_payment_autospend" {
            0
        } else {
            2
        })
        .collect();
    if scenario != "production_payment_autospend" && planets.len() != 2 {
        return Err("production scenario needs two additional planets".to_owned());
    }
    for (system, planet) in planets {
        config
            .state
            .system_mut(&SystemId::new(system))
            .set_control(PlanetId::new(planet), player.clone());
    }
    if scenario == "production_payment_autospend" {
        if !galaxy.system_ids().contains(&"10") {
            return Err("production payment scenario needs Wren Terra's system".to_owned());
        }
        config
            .state
            .system_mut(&SystemId::new("10"))
            .set_control(PlanetId::new("wrenterra"), player.clone());
    }
    if scenario == "production_batch" {
        let sequence = config.state.production_seq;
        config
            .state
            .player_mut(&player)
            .expect("Sol seated")
            .free_production_use = Some(sequence + 1);
    }
    Ok((config, lobby, player, token))
}

fn advance_into_production(session: &Arc<GameSession>, player: &PlayerId) -> Result<(), String> {
    let client = MockClient::connect(session.clone(), ViewerRole::Player(player.clone()));
    for _ in 0..20 {
        let mut offered = None;
        for _ in 0..150 {
            if let Ok(ServerMessage::PendingChoice(message)) = client.try_recv() {
                offered = Some(message.choice);
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let choice = offered
            .or_else(|| {
                client
                    .snapshot()
                    .pending_choice
                    .map(|pending| pending.choice)
            })
            .ok_or("no production setup choice")?;
        if choice
            .context
            .as_ref()
            .is_some_and(|ctx| ctx.subtype == "produce_unit")
        {
            return Ok(());
        }
        let next = if choice.options.iter().any(|option| option.id == "tactical") {
            "tactical"
        } else if choice.options.iter().any(|option| option.id == "01") {
            "01"
        } else if choice
            .options
            .iter()
            .any(|option| option.id == "done_loading")
        {
            "done_loading"
        } else if choice
            .options
            .iter()
            .any(|option| option.id == "done_moving")
        {
            "done_moving"
        } else {
            return Err(format!(
                "unexpected production setup choice: {}",
                choice.prompt
            ));
        };
        let (_, nonce, version) = session
            .current_pending_decision()
            .ok_or("no pending production setup decision")?;
        client
            .submit(&nonce, version, next)
            .map_err(|err| format!("production setup submission failed: {err:?}"))?;
    }
    Err("production setup exceeded 20 decisions".to_owned())
}

fn build_endgame_scenario(
    seed: u64,
) -> Result<(SessionConfig, PlayerLobbyRecord, PlayerId, String), String> {
    let (mut config, lobby, p1, session1, _galaxy, p2, p3) =
        setup_base_3p_game(seed, "dev_endgame")?;

    config
        .state
        .player_mut(&p1)
        .expect("Sol seated")
        .victory_points = 9;
    config
        .state
        .player_mut(&p2)
        .expect("Hacan seated")
        .victory_points = 9;
    config
        .state
        .player_mut(&p3)
        .expect("Letnev seated")
        .victory_points = 8;

    config.state.objective_deck.clear();
    config.state.round = 5;

    for seat in [&p1, &p2, &p3] {
        if let Some(player) = config.state.player_mut(seat) {
            player.tactic_tokens = 0;
            player.strategic_tokens = 0;
            player.strategy_cards.clear();
        }
        config.seats.insert(seat.clone(), SeatController::Human);
    }

    Ok((config, lobby, p1, session1))
}

fn build_full_game_scenario(
    seed: u64,
) -> Result<(SessionConfig, PlayerLobbyRecord, PlayerId, String), String> {
    let (mut config, lobby, p1, session1, _galaxy, p2, p3) =
        setup_base_3p_game(seed, "dev_full_game")?;

    for seat in [&p1, &p2, &p3] {
        config.seats.insert(seat.clone(), SeatController::Human);
    }

    Ok((config, lobby, p1, session1))
}

fn build_invasion_scenario(
    seed: u64,
    coexistence: bool,
) -> Result<(SessionConfig, PlayerLobbyRecord, PlayerId, String, String), String> {
    let (mut config, lobby, invader, token, galaxy, third, defender) =
        setup_base_3p_game(seed, "dev_invasion")?;
    let content = ContentStore::embedded();
    let mut candidates: Vec<&str> = galaxy.adjacent("01").into_iter().collect();
    candidates.extend(galaxy.system_ids());
    let system = candidates
        .iter()
        .copied()
        .find(|id| {
            ti4_content::galaxy::system(content, id, POK).is_some_and(|tile| {
                !tile.is_supernova()
                    && !tile.is_asteroid_field()
                    && if coexistence {
                        tile.planets().len() >= 2
                    } else {
                        tile.planets().len() == 1
                    }
            })
        })
        .ok_or_else(|| {
            format!(
                "no suitable invasion system: coexistence={coexistence} candidates={candidates:?}"
            )
        })?;
    let planets: Vec<PlanetId> = ti4_content::galaxy::system(content, system, POK)
        .expect("selected system exists")
        .planets()
        .into_iter()
        .take(if coexistence { 2 } else { 1 })
        .map(PlanetId::new)
        .collect();
    // Keep the scripted carriers legal before the post-combat fleet check. Otherwise setup
    // removes carriers and then their passengers before the landing scenario can begin.
    config
        .state
        .player_mut(&invader)
        .expect("invader seated")
        .fleet_tokens = if coexistence { 4 } else { 3 };
    let board = config.state.system_mut(&SystemId::new(system));
    board.units.clear();
    board.command_tokens.clear();
    board.planet_units.clear();
    for _ in 0..(if coexistence { 4 } else { 3 }) {
        board
            .units
            .push(Unit::new(UnitTypeId::new("carrier"), invader.clone()));
    }
    for _ in 0..(if coexistence { 6 } else { 4 }) {
        board
            .units
            .push(Unit::new(UnitTypeId::new("infantry"), invader.clone()));
    }
    board
        .units
        .push(Unit::new(UnitTypeId::new("mech"), invader.clone()));
    for planet in &planets {
        board.set_control(planet.clone(), defender.clone());
        board.planet_units.insert(
            planet.clone(),
            vec![
                Unit::new(UnitTypeId::new("infantry"), defender.clone()),
                Unit::new(UnitTypeId::new("infantry"), defender.clone()),
                Unit::new(UnitTypeId::new("pds"), defender.clone()),
            ],
        );
    }
    if coexistence {
        board
            .planet_units
            .get_mut(&planets[0])
            .expect("first planet")
            .push(Unit::new(UnitTypeId::new("infantry"), third.clone()));
        board
            .coexisting
            .entry(planets[0].clone())
            .or_default()
            .insert(third);
    }
    // A real invasion-start card holds the primary scenario at the first public boundary.
    config
        .state
        .player_mut(&invader)
        .expect("invader seated")
        .action_cards
        .push(ActionCardId::new("blitz"));
    Ok((config, lobby, invader, token, system.to_owned()))
}

fn advance_into_invasion(
    session: &Arc<GameSession>,
    invader: &PlayerId,
    system: &str,
) -> Result<(), String> {
    let client = MockClient::connect(session.clone(), ViewerRole::Player(invader.clone()));
    for _ in 0..30 {
        let mut offer = None;
        for _ in 0..150 {
            if let Ok(ServerMessage::PendingChoice(message)) = client.try_recv() {
                offer = Some(message.choice);
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let choice = offer
            .or_else(|| client.snapshot().pending_choice.map(|p| p.choice))
            .ok_or("timed out entering invasion")?;
        if choice
            .context
            .as_ref()
            .is_some_and(|ctx| ctx.invasion_seq.is_some())
        {
            return Ok(());
        }
        let (_, nonce, version) = session
            .current_pending_decision()
            .ok_or("no pending invasion setup decision")?;
        let next = if choice.options.iter().any(|option| option.id == "tactical") {
            "tactical"
        } else if choice.options.iter().any(|option| option.id == system) {
            system
        } else if choice
            .options
            .iter()
            .any(|option| option.id == "done_loading")
        {
            "done_loading"
        } else if choice
            .options
            .iter()
            .any(|option| option.id == "done_moving")
        {
            "done_moving"
        } else {
            choice
                .options
                .first()
                .ok_or("empty invasion setup choice")?
                .id
                .as_str()
        };
        client
            .submit(&nonce, version, next)
            .map_err(|err| format!("invasion setup submission failed: {err:?}"))?;
    }
    Err("invasion setup exceeded 30 decisions".to_owned())
}

fn advance_into_space_combat(
    session: &Arc<GameSession>,
    human_player: &PlayerId,
    border_system: &str,
    stop_at_opening: bool,
) -> Result<(), String> {
    let client = MockClient::connect(session.clone(), ViewerRole::Player(human_player.clone()));
    let wait_for_choice = |client: &MockClient| {
        for _ in 0..150 {
            if let Ok(ServerMessage::PendingChoice(msg)) = client.try_recv() {
                return Some(msg.choice);
            }
            thread::sleep(Duration::from_millis(10));
        }
        None
    };

    let border_sys_id = border_system;

    // 1. Initial choice -> "tactical"
    let _ = wait_for_choice(&client)
        .or_else(|| client.snapshot().pending_choice.map(|p| p.choice))
        .ok_or_else(|| "timed out waiting for initial choice".to_owned())?;
    let (_, nonce_1, ver_1) = session
        .current_pending_decision()
        .ok_or_else(|| "no pending decision for tactical".to_owned())?;
    client
        .submit(&nonce_1, ver_1, "tactical")
        .map_err(|e| format!("submit tactical failed: {e:?}"))?;

    // 2. Activate border system
    let _ = wait_for_choice(&client)
        .ok_or_else(|| "timed out waiting for activate choice".to_owned())?;
    let (_, nonce_2, ver_2) = session
        .current_pending_decision()
        .ok_or_else(|| "no pending decision for activate".to_owned())?;
    client
        .submit(&nonce_2, ver_2, border_sys_id)
        .map_err(|e| format!("submit activate failed: {e:?}"))?;

    // 3. Movement and loading loop; stop at the first combat decision, before
    // rolling dice or opening a reaction to the bot's sustain damage.
    let mut choice = wait_for_choice(&client)
        .ok_or_else(|| "timed out waiting for movement choice".to_owned())?;
    while !(stop_at_opening
        && choice.context.as_ref().is_some_and(|ctx| {
            ctx.subtype.contains("SPACE_COMBAT_STARTED")
                || ctx.subtype.contains("COMBAT_ROUND_STARTED")
        }))
        && !choice
            .options
            .iter()
            .any(|o| o.kind == "retreat" || o.kind == "sustain" || o.kind == "casualty")
    {
        let (_, nonce, ver) = session
            .current_pending_decision()
            .ok_or_else(|| "no pending decision in move/combat loop".to_owned())?;
        if let Some(load_opt) = choice.options.iter().find(|o| o.id.starts_with("load|0")) {
            client
                .submit(&nonce, ver, &load_opt.id)
                .map_err(|e| format!("submit load failed: {e:?}"))?;
        } else if let Some(done_load) = choice.options.iter().find(|o| o.id == "done_loading") {
            client
                .submit(&nonce, ver, &done_load.id)
                .map_err(|e| format!("submit done_loading failed: {e:?}"))?;
        } else if let Some(move_opt) = choice.options.iter().find(|o| o.id.starts_with("move|")) {
            client
                .submit(&nonce, ver, &move_opt.id)
                .map_err(|e| format!("submit move failed: {e:?}"))?;
        } else if let Some(done_move) = choice.options.iter().find(|o| o.id == "done_moving") {
            client
                .submit(&nonce, ver, &done_move.id)
                .map_err(|e| format!("submit done_moving failed: {e:?}"))?;
        } else {
            return Err(format!("unexpected choice in loop: {:?}", choice.prompt));
        }
        choice = wait_for_choice(&client).ok_or_else(|| "timed out in loop".to_owned())?;
    }

    Ok(())
}

fn setup_base_3p_game(
    seed: u64,
    game_prefix: &str,
) -> Result<
    (
        SessionConfig,
        PlayerLobbyRecord,
        PlayerId,
        String,
        ti4_content::galaxy::Galaxy,
        PlayerId,
        PlayerId,
    ),
    String,
> {
    let content = ContentStore::embedded();

    let mut existing_players = BTreeMap::new();
    let p1 = generate_player_id(&existing_players);
    let session1 = PlayerSession::generate();
    existing_players.insert(
        p1.clone(),
        PlayerLobbyMember {
            ready: true,
            session: session1.clone(),
            nickname: "Player 1 (Sol)".to_owned(),
        },
    );

    let p2 = generate_player_id(&existing_players);
    let session2 = PlayerSession::generate();
    existing_players.insert(
        p2.clone(),
        PlayerLobbyMember {
            ready: true,
            session: session2.clone(),
            nickname: "Bot (Hacan)".to_owned(),
        },
    );

    let p3 = generate_player_id(&existing_players);
    let session3 = PlayerSession::generate();
    existing_players.insert(
        p3.clone(),
        PlayerLobbyMember {
            ready: true,
            session: session3.clone(),
            nickname: "Bot (Letnev)".to_owned(),
        },
    );

    let player_ids = vec![p1.clone(), p2.clone(), p3.clone()];
    let (mut state, galaxy) =
        crate::map::create_game_with_map(content, &player_ids, seed).map_err(|e| e.to_string())?;

    // Sol: Leadership (initiative 1), Hacan: Trade (5), Letnev: Warfare (6)
    let strat_sol = StrategyCardId::new("leadership");
    let strat_hacan = StrategyCardId::new("trade");
    let strat_letnev = StrategyCardId::new("warfare");

    state.player_mut(&p1).expect("p1").strategy_cards = vec![strat_sol.clone()];
    state.player_mut(&p2).expect("p2").strategy_cards = vec![strat_hacan.clone()];
    state.player_mut(&p3).expect("p3").strategy_cards = vec![strat_letnev.clone()];

    state
        .unclaimed_strategy_cards
        .retain(|c| c != &strat_sol && c != &strat_hacan && c != &strat_letnev);

    // Transition to Action Phase with p1 as active
    ti4_engine::phase::advance_phase(&mut state);
    ti4_engine::phase::begin_action_turn(&mut state, &p1);

    if let Some(sol) = state.player_mut(&p1) {
        sol.tactic_tokens = 3;
        sol.fleet_tokens = 3;
        sol.strategic_tokens = 2;
    }

    let map_tiles = crate::map::build_board_tiles(content, &galaxy);
    let game_id = format!("{game_prefix}_{:016x}", rand::random::<u64>());

    let mut config = SessionConfig::new(&game_id, state)
        .with_seed(seed)
        .with_player_ids(player_ids.clone())
        .with_galaxy(galaxy.clone(), map_tiles);
    config.seats.insert(p1.clone(), SeatController::Human);
    config
        .seats
        .insert(p2.clone(), SeatController::BotFirstOption);
    config
        .seats
        .insert(p3.clone(), SeatController::BotFirstOption);
    config
        .seat_tokens
        .insert(p1.clone(), session1.as_str().to_owned());
    config
        .seat_tokens
        .insert(p2.clone(), session2.as_str().to_owned());
    config
        .seat_tokens
        .insert(p3.clone(), session3.as_str().to_owned());

    let slots = player_ids
        .iter()
        .enumerate()
        .map(|(i, pid)| PlayerLobbySlot {
            slot_id: LobbySlotId(format!("slot_{}", i + 1)),
            occupant: Some(pid.clone()),
        })
        .collect();

    let lobby_record = PlayerLobbyRecord {
        schema_version: PLAYER_RECORD_VERSION,
        game_id: game_id.clone(),
        phase: PersistedLobbyPhase::Running,
        host_player_id: p1.clone(),
        slots,
        players: existing_players,
        seed,
        lobby_version: 1,
    };

    Ok((
        config,
        lobby_record,
        p1,
        session1.as_str().to_owned(),
        galaxy,
        p2,
        p3,
    ))
}

fn build_tactical_scenario(
    seed: u64,
) -> Result<(SessionConfig, PlayerLobbyRecord, PlayerId, String), String> {
    let (mut config, lobby_record, p1, token, galaxy, p2, _) =
        setup_base_3p_game(seed, "dev_tactical")?;
    let content = ContentStore::embedded();

    // 1. Find a passable adjacent system to Sol's home (01) to serve as Hacan's border system
    let adjacent_ids = galaxy.adjacent("01");
    let border_system = adjacent_ids
        .iter()
        .find(|sys_id| {
            if let Some(sys) = ti4_content::galaxy::system(content, sys_id, POK) {
                !sys.is_supernova() && !sys.is_asteroid_field()
            } else {
                false
            }
        })
        .copied()
        .unwrap_or("18");

    // 2. Find a second passable system adjacent to the border system (distinct from 01)
    let border_adjacents = galaxy.adjacent(border_system);
    let second_system = border_adjacents
        .iter()
        .find(|sys_id| {
            if **sys_id == "01" || **sys_id == border_system {
                return false;
            }
            if let Some(sys) = ti4_content::galaxy::system(content, sys_id, POK) {
                !sys.is_supernova() && !sys.is_asteroid_field()
            } else {
                false
            }
        })
        .copied()
        .unwrap_or("19");

    let border_sys_id = SystemId::new(border_system);
    let second_sys_id = SystemId::new(second_system);

    // Place Hacan's fleet in the border system
    let border_state = config.state.system_mut(&border_sys_id);
    border_state.command_tokens.clear();
    border_state.units.clear();
    border_state
        .units
        .push(Unit::new(UnitTypeId::new("cruiser"), p2.clone()));
    border_state
        .units
        .push(Unit::new(UnitTypeId::new("destroyer"), p2.clone()));
    border_state
        .units
        .push(Unit::new(UnitTypeId::new("fighter"), p2.clone()));
    border_state
        .units
        .push(Unit::new(UnitTypeId::new("fighter"), p2.clone()));

    // If there are planets in border_system, place 1 Hacan Infantry on the first planet
    if let Some(first_planet) = border_state.planet_units.keys().cloned().next() {
        border_state.land(
            &first_planet,
            &[Unit::new(UnitTypeId::new("infantry"), p2.clone())],
        );
        border_state.set_control(first_planet, p2.clone());
    }

    // Place Sol's second fleet in second_system (also adjacent to border system)
    let second_state = config.state.system_mut(&second_sys_id);
    second_state.command_tokens.clear();
    second_state.units.clear();
    second_state
        .units
        .push(Unit::new(UnitTypeId::new("cruiser"), p1.clone()));
    second_state
        .units
        .push(Unit::new(UnitTypeId::new("dreadnought"), p1.clone()));
    second_state
        .units
        .push(Unit::new(UnitTypeId::new("fighter"), p1.clone()));

    let sol_infantry = vec![
        Unit::new(UnitTypeId::new("infantry"), p1.clone()),
        Unit::new(UnitTypeId::new("infantry"), p1.clone()),
    ];

    if let Some(first_planet) = second_state.planet_units.keys().cloned().next() {
        second_state.land(&first_planet, &sol_infantry);
        second_state.set_control(first_planet, p1.clone());
    } else {
        second_state.units.extend(sol_infantry);
    }

    Ok((config, lobby_record, p1, token))
}

fn build_space_combat_scenario(
    seed: u64,
) -> Result<(SessionConfig, PlayerLobbyRecord, PlayerId, String, String), String> {
    let (mut config, lobby_record, p1, token, galaxy, _, p3) =
        setup_base_3p_game(seed, "dev_combat")?;
    let content = ContentStore::embedded();

    // Give Sol cards for the combat's different timing windows and enough fleet tokens.
    if let Some(sol) = config.state.player_mut(&p1) {
        sol.fleet_tokens = 4;
        sol.action_cards.extend(
            ["dh1", "sh1", "courageous", "salvage"]
                .into_iter()
                .map(ActionCardId::new),
        );
    }
    if let Some(letnev) = config.state.player_mut(&p3) {
        letnev.fleet_tokens = 4;
    }

    // Add Dreadnought to Sol's home system (01)
    let home_state = config.state.system_mut(&SystemId::new("01"));
    home_state
        .units
        .push(Unit::new(UnitTypeId::new("dreadnought"), p1.clone()));

    // Find an adjacent system to Sol's home (01) that is not an impassable anomaly
    let adjacent_ids = galaxy.adjacent("01");
    let border_system = adjacent_ids
        .into_iter()
        .find(|sys_id| {
            if let Some(sys) = ti4_content::galaxy::system(content, sys_id, POK) {
                !sys.is_supernova() && !sys.is_asteroid_field()
            } else {
                false
            }
        })
        .unwrap_or("18");

    let border_sys_id = SystemId::new(border_system);

    // Place Letnev's fleet in the border system (including Dreadnoughts)
    let sys_state = config.state.system_mut(&border_sys_id);
    sys_state.command_tokens.clear();
    sys_state.units.clear();
    sys_state
        .units
        .push(Unit::new(UnitTypeId::new("dreadnought"), p3.clone()));
    sys_state
        .units
        .push(Unit::new(UnitTypeId::new("dreadnought"), p3.clone()));
    sys_state
        .units
        .push(Unit::new(UnitTypeId::new("cruiser"), p3.clone()));
    sys_state
        .units
        .push(Unit::new(UnitTypeId::new("destroyer"), p3.clone()));
    sys_state
        .units
        .push(Unit::new(UnitTypeId::new("fighter"), p3.clone()));
    sys_state
        .units
        .push(Unit::new(UnitTypeId::new("fighter"), p3.clone()));

    Ok((config, lobby_record, p1, token, border_system.to_owned()))
}
