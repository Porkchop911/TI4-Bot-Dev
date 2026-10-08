//! One game with a local LLM in one seat: timed, scored, and compared with the policy in that seat.
//!
//! Operator, 2026-10-08: "have it play one game, timed and scored to see how [bad] it really is".
//! The same seed and rotation are played twice: first with the learned policy at every seat (the
//! baseline for the seat under test), then with an OpenAI-compatible chat endpoint answering that
//! seat's decisions. The other five seats are the policy both times, at the evaluation temperature.
//!
//! The LLM sees what the seat may see: its own public standing, planets, units by system, held
//! secrets and action cards, every rival's faction and score, the decision's prompt, and the legal
//! options by label. It answers with an option number. A decision with one option is not sent. An
//! answer that names no legal option takes the first option and is counted. Every call is logged
//! (`--log`, JSON lines) with its latency, token counts and answer.
//!
//! ```text
//! llm_seat_game --bundle <checkpoint> --seed 900000000 [--rotation 0] [--seat 0]
//!   [--endpoint http://127.0.0.1:8080/v1] [--model qwen3.8-flash-next-iq3_xxs] [--think]
//!   [--effort low|medium|high]
//!   [--rounds 4] [--temperature 0.001] [--map-pool out/pools/full_np8_12_holdout.json]
//!   [--log out/llm-game.jsonl] [--max-tokens 1024] [--roster six|wide|new]
//!   [--lineup a,b,c,d,e,f] [--llm-faction winnu] [--live out/llm-live]
//! ```

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::{Read as _, Write as _};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use serde_json::{Value, json};
use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TILE_SEED_OFFSET: u64 = 0;

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|arg| arg == name)
        .and_then(|at| args.get(at + 1))
        .cloned()
}

fn refuse(reason: &str) -> ! {
    eprintln!("REFUSED: {reason}");
    std::process::exit(2)
}

fn number<T: std::str::FromStr>(name: &str, fallback: T) -> T {
    argument(name).map_or(fallback, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse(&format!("{name} expects a number, got {value:?}")))
    })
}

/// A plain HTTP/1.1 POST to a local endpoint: no TLS, `Connection: close`, chunked or sized body.
fn post_json(url: &str, body: &Value) -> Result<Value, String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| format!("{url}: only http:// endpoints"))?;
    let (host, path) = rest.split_once('/').map_or((rest, "/".to_owned()), |(h, p)| {
        (h, format!("/{p}"))
    });
    let payload = body.to_string();
    let mut stream =
        std::net::TcpStream::connect(host).map_err(|error| format!("connecting {host}: {error}"))?;
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("sending: {error}"))?;
    let mut raw = Vec::new();
    stream
        .read_to_end(&mut raw)
        .map_err(|error| format!("reading: {error}"))?;
    let text = String::from_utf8_lossy(&raw);
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| "a response without headers".to_owned())?;
    if !head.starts_with("HTTP/1.1 200") && !head.starts_with("HTTP/1.0 200") {
        return Err(format!("{}: {}", head.lines().next().unwrap_or(""), body.trim()));
    }
    let body = if head.to_ascii_lowercase().contains("transfer-encoding: chunked") {
        let mut out = String::new();
        let mut rest = body;
        loop {
            let Some((size, after)) = rest.split_once("\r\n") else {
                break;
            };
            let size = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
            if size == 0 {
                break;
            }
            out.push_str(after.get(..size).unwrap_or(after));
            rest = after.get(size + 2..).unwrap_or("");
        }
        out
    } else {
        body.to_owned()
    };
    serde_json::from_str(&body).map_err(|error| format!("parsing the reply: {error}"))
}

/// As [`post_json`] with `"stream": true`: each reasoning or answer delta goes to `on_delta` as it
/// arrives (`"reasoning"` or `"content"`), and the whole reply comes back shaped like a
/// non-streamed one. A server that answers with a JSON error instead of a stream returns it as is.
fn post_stream(
    url: &str,
    body: &Value,
    on_delta: &mut dyn FnMut(&str, &str),
) -> Result<Value, String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| format!("{url}: only http:// endpoints"))?;
    let (host, path) = rest.split_once('/').map_or((rest, "/".to_owned()), |(h, p)| {
        (h, format!("/{p}"))
    });
    let mut body = body.clone();
    body["stream"] = json!(true);
    let payload = body.to_string();
    let mut stream =
        std::net::TcpStream::connect(host).map_err(|error| format!("connecting {host}: {error}"))?;
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("sending: {error}"))?;
    let mut reader = std::io::BufReader::new(stream);
    let mut head = String::new();
    loop {
        let mut line = String::new();
        let read = std::io::BufRead::read_line(&mut reader, &mut line)
            .map_err(|error| format!("reading: {error}"))?;
        if read == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        head.push_str(&line);
    }
    let lower = head.to_ascii_lowercase();
    if !lower.contains("text/event-stream") {
        let mut rest = String::new();
        reader
            .read_to_string(&mut rest)
            .map_err(|error| format!("reading: {error}"))?;
        return serde_json::from_str(&rest).map_err(|error| format!("parsing the reply: {error}"));
    }
    let (mut content, mut reasoning) = (String::new(), String::new());
    let mut usage = Value::Null;
    let mut finish = Value::Null;
    loop {
        let mut line = String::new();
        let read = std::io::BufRead::read_line(&mut reader, &mut line)
            .map_err(|error| format!("reading: {error}"))?;
        if read == 0 {
            break;
        }
        let Some(data) = line.trim_end().strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data == "[DONE]" {
            break;
        }
        let Ok(event) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        if event.get("error").is_some() {
            return Ok(event);
        }
        if !event["usage"].is_null() {
            usage = event["usage"].clone();
        }
        let choice = &event["choices"][0];
        if !choice["finish_reason"].is_null() {
            finish = choice["finish_reason"].clone();
        }
        if let Some(text) = choice["delta"]["reasoning_content"].as_str() {
            reasoning.push_str(text);
            on_delta("reasoning", text);
        }
        if let Some(text) = choice["delta"]["content"].as_str() {
            content.push_str(text);
            on_delta("content", text);
        }
    }
    if finish == json!("length") {
        return Ok(json!({"error": {"code": "length", "message": "the budget ran out"}}));
    }
    Ok(json!({
        "choices": [{"message": {"content": content, "reasoning_content": reasoning}, "finish_reason": finish}],
        "usage": usage,
    }))
}

/// The live files a viewer polls (`--live <prefix>`): `<prefix>-state.json`, rewritten whole for
/// each decision, and `<prefix>-thinking.txt`, the current decision's thinking as it streams.
struct Live {
    state: String,
    thinking: String,
    history: RefCell<Vec<Value>>,
}

impl Live {
    fn write_state(&self, value: &Value) {
        let temporary = format!("{}.tmp", self.state);
        if std::fs::write(&temporary, value.to_string()).is_ok() {
            let _ = std::fs::rename(&temporary, &self.state);
        }
    }

    fn begin_thinking(&self, header: &str) {
        let _ = std::fs::write(&self.thinking, header);
    }

    fn append_thinking(&self, text: &str) {
        if let Ok(mut file) = std::fs::OpenOptions::new().append(true).open(&self.thinking) {
            let _ = file.write_all(text.as_bytes());
        }
    }
}

/// What one LLM call cost and answered.
#[derive(Default)]
struct Tally {
    calls: usize,
    unparsed: usize,
    errors: usize,
    /// Decisions whose thinking answer ran out of budget and were answered again without thinking.
    retried: usize,
    millis: u128,
    prompt_tokens: u64,
    completion_tokens: u64,
}

struct LlmSeat {
    seat: PlayerId,
    faction: String,
    endpoint: String,
    model: String,
    think: bool,
    effort: String,
    max_tokens: u64,
    tally: Rc<RefCell<Tally>>,
    log: Rc<RefCell<Option<std::fs::File>>>,
    live: Option<Rc<Live>>,
}

fn unit_name(content: &ContentStore, id: &str) -> String {
    ti4_content::units::unit_type(content, id, DEFAULT)
        .and_then(|unit| unit.record().text("name").map(str::to_owned))
        .unwrap_or_else(|| id.to_owned())
}

fn system_name(content: &ContentStore, id: &str) -> String {
    ti4_content::galaxy::system(content, id, DEFAULT)
        .and_then(|system| system.name().map(str::to_owned))
        .unwrap_or_else(|| id.to_owned())
}

impl LlmSeat {
    /// The seat's view, compactly: prompt tokens are the slow part of a local model.
    fn state(&self, seen: &SeatObservation<'_>) -> String {
        let observed = seen.observed();
        let content = observed.content();
        let mut out = String::new();
        let _ = std::fmt::Write::write_fmt(
            &mut out,
            format_args!(
                "Round {}, {:?} phase. You are seat {} ({}).\n",
                observed.round(),
                observed.phase(),
                self.seat,
                self.faction
            ),
        );
        if let Some(me) = observed.seat(&self.seat) {
            let _ = std::fmt::Write::write_fmt(
                &mut out,
                format_args!(
                    "You: {} VP, {} trade goods, {} commodities, command tokens tactic {} / fleet {} / strategy {}, strategy cards {:?}, technologies {:?}.\n",
                    me.victory_points,
                    me.trade_goods,
                    me.commodities,
                    me.tactic_tokens,
                    me.fleet_tokens,
                    me.strategic_tokens,
                    me.strategy_cards.iter().map(ToString::to_string).collect::<Vec<_>>(),
                    me.technologies.iter().map(ToString::to_string).collect::<Vec<_>>(),
                ),
            );
        }
        let planets: Vec<String> = observed
            .controlled_planets(&self.seat)
            .into_iter()
            .map(|(_, planet)| planet.to_string())
            .collect();
        let _ = std::fmt::Write::write_fmt(&mut out, format_args!("Planets: {}.\n", planets.join(", ")));
        let mut fleets = Vec::new();
        for system in observed.systems_with_units_of(&self.seat) {
            let here = observed.system(system);
            let mut counts: BTreeMap<String, usize> = BTreeMap::new();
            for unit in here
                .units
                .iter()
                .chain(here.planet_units.values().flatten())
                .filter(|unit| unit.owner == self.seat)
            {
                *counts.entry(unit_name(content, unit.type_id.as_str())).or_default() += 1;
            }
            let units: Vec<String> = counts.iter().map(|(name, n)| format!("{n} {name}")).collect();
            fleets.push(format!("{}: {}", system_name(content, system.as_str()), units.join(", ")));
        }
        let _ = std::fmt::Write::write_fmt(&mut out, format_args!("Your units: {}.\n", fleets.join("; ")));
        let secrets: Vec<String> = seen.held_secrets().iter().map(ToString::to_string).collect();
        let cards: Vec<String> = seen.held_action_cards().iter().map(ToString::to_string).collect();
        let _ = std::fmt::Write::write_fmt(
            &mut out,
            format_args!("Your secret objectives: {secrets:?}. Your action cards: {cards:?}.\n"),
        );
        let rivals: Vec<String> = observed
            .players()
            .into_iter()
            .filter(|player| **player != self.seat)
            .filter_map(|player| {
                observed
                    .seat(player)
                    .map(|seat| format!("{player} {} {} VP", seat.faction, seat.victory_points))
            })
            .collect();
        let _ = std::fmt::Write::write_fmt(&mut out, format_args!("Rivals: {}.\n", rivals.join("; ")));
        out
    }

    /// The public board for the viewer: every hex with its units and planet owners.
    fn board(seen: &SeatObservation<'_>) -> Value {
        let observed = seen.observed();
        let content = observed.content();
        let Some(galaxy) = observed.galaxy() else {
            return json!([]);
        };
        let tiles: Vec<Value> = galaxy
            .system_ids()
            .into_iter()
            .filter_map(|id| {
                let hex = galaxy.coord_of(id)?;
                let here = observed.system(&ti4_model::id::SystemId::new(id));
                let mut units: BTreeMap<(String, String), usize> = BTreeMap::new();
                for unit in here.units.iter().chain(here.planet_units.values().flatten()) {
                    *units
                        .entry((unit.owner.to_string(), unit_name(content, unit.type_id.as_str())))
                        .or_default() += 1;
                }
                let units: Vec<Value> = units
                    .into_iter()
                    .map(|((owner, unit), n)| json!({"owner": owner, "unit": unit, "n": n}))
                    .collect();
                let planets: Vec<Value> = here
                    .planet_control
                    .iter()
                    .map(|(planet, owner)| json!({"planet": planet.to_string(), "owner": owner.to_string()}))
                    .collect();
                Some(json!({
                    "system": id,
                    "name": system_name(content, id),
                    "q": hex.q,
                    "r": hex.r,
                    "units": units,
                    "planets": planets,
                }))
            })
            .collect();
        json!(tiles)
    }

    fn snapshot(&self, choice: &Choice, seen: &SeatObservation<'_>, chosen: Option<usize>) {
        let Some(live) = &self.live else { return };
        let observed = seen.observed();
        let players: Vec<Value> = observed
            .players()
            .into_iter()
            .filter_map(|player| {
                observed.seat(player).map(|seat| {
                    json!({
                        "player": player.to_string(),
                        "faction": seat.faction.to_string(),
                        "vp": seat.victory_points,
                        "llm": *player == self.seat,
                    })
                })
            })
            .collect();
        let tally = self.tally.borrow();
        live.write_state(&json!({
            "round": observed.round(),
            "phase": format!("{:?}", observed.phase()),
            "seat": self.seat.to_string(),
            "faction": self.faction,
            "prompt": choice.prompt,
            "options": choice.options.iter().map(|o| o.label.clone()).collect::<Vec<_>>(),
            "chosen": chosen,
            "players": players,
            "board": Self::board(seen),
            "history": *live.history.borrow(),
            "calls": tally.calls,
            "seconds": tally.millis / 1000,
        }));
    }

    fn ask(&self, choice: &Choice, seen: &SeatObservation<'_>) -> ChoiceOption {
        let options: Vec<String> = choice
            .options
            .iter()
            .enumerate()
            .map(|(index, option)| format!("{index}: {}", option.label))
            .collect();
        let user = format!(
            "{}\nDecision: {}\nOptions:\n{}\nChoose one option by its number.",
            self.state(seen),
            choice.prompt,
            options.join("\n")
        );
        // The legal choices as a constrained answer: the reply must be {"choice": n} with n one of
        // the offered option numbers.
        let legal: Vec<usize> = (0..choice.options.len()).collect();
        let request = |think: bool| {
            let mut body = json!({
                "model": self.model,
                "messages": [
                    {"role": "system", "content": "You are playing Twilight Imperium 4th edition to win: score victory points by the end of round 4. Every option offered is legal. Choose one of them."},
                    {"role": "user", "content": user},
                ],
                "max_tokens": self.max_tokens,
                "temperature": 0.2,
                "response_format": {"type": "json_schema", "json_schema": {"name": "choice", "strict": true, "schema": {
                    "type": "object",
                    "properties": {"choice": {"type": "integer", "enum": legal}},
                    "required": ["choice"],
                    "additionalProperties": false,
                }}},
            });
            if think {
                body["reasoning_effort"] = json!(self.effort);
            } else {
                body["chat_template_kwargs"] = json!({"enable_thinking": false});
            }
            let url = format!("{}/chat/completions", self.endpoint);
            match &self.live {
                Some(live) => post_stream(&url, &body, &mut |_, text| live.append_thinking(text)),
                None => post_json(&url, &body),
            }
        };
        if let Some(live) = &self.live {
            live.begin_thinking(&format!(
                "Round {} -- {}\n\n",
                seen.observed().round(),
                choice.prompt
            ));
        }
        self.snapshot(choice, seen, None);
        let started = Instant::now();
        let mut reply = request(self.think);
        let mut retried = false;
        if self.think && reply.as_ref().is_ok_and(|r| r.get("choices").is_none()) {
            // Thinking ran out of budget before the answer: answer once more without it.
            reply = request(false);
            retried = true;
        }
        let millis = started.elapsed().as_millis();
        let mut tally = self.tally.borrow_mut();
        tally.calls += 1;
        tally.millis += millis;
        if retried {
            tally.retried += 1;
        }
        let (answer, chosen) = match &reply {
            Ok(reply) => {
                tally.prompt_tokens += reply["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
                tally.completion_tokens +=
                    reply["usage"]["completion_tokens"].as_u64().unwrap_or(0);
                let content = reply["choices"][0]["message"]["content"]
                    .as_str()
                    .unwrap_or("")
                    .to_owned();
                let picked = serde_json::from_str::<Value>(&content)
                    .ok()
                    .and_then(|answer| answer["choice"].as_u64())
                    .and_then(|index| usize::try_from(index).ok())
                    .filter(|index| *index < choice.options.len());
                (content, picked)
            }
            Err(error) => {
                tally.errors += 1;
                (format!("ERROR {error}"), None)
            }
        };
        if chosen.is_none() {
            tally.unparsed += 1;
        }
        let index = chosen.unwrap_or(0);
        if let Some(live) = &self.live {
            let mut history = live.history.borrow_mut();
            history.push(json!({
                "round": seen.observed().round(),
                "prompt": choice.prompt,
                "label": choice.options[index].label,
                "seconds": millis / 1000,
                "parsed": chosen.is_some(),
            }));
            let excess = history.len().saturating_sub(40);
            history.drain(..excess);
        }
        if let Some(file) = self.log.borrow_mut().as_mut() {
            let _ = writeln!(
                file,
                "{}",
                json!({
                    "round": seen.observed().round(),
                    "prompt": choice.prompt,
                    "subtype": choice.context.as_ref().map(|c| c.subtype.clone()),
                    "options": choice.options.len(),
                    "answer": answer,
                    "chosen": index,
                    "label": choice.options[index].label,
                    "parsed": chosen.is_some(),
                    "retried_without_thinking": retried,
                    "ms": millis,
                    "reasoning": reply.as_ref().ok().and_then(|r| r["choices"][0]["message"]["reasoning_content"].as_str()).unwrap_or(""),
                })
            );
        }
        drop(tally);
        self.snapshot(choice, seen, Some(index));
        choice.options[index].clone()
    }
}

impl Decider for LlmSeat {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        // Every seat is asked through `choose_seeing`; a blind ask takes the first option.
        choice
            .options
            .first()
            .cloned()
            .ok_or_else(|| IllegalChoice::NoOptions {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
            })
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        if choice.options.len() <= 1 {
            return self.choose(choice);
        }
        Ok(self.ask(choice, seen))
    }
}

#[expect(clippy::too_many_lines, reason = "a linear script: set up, play twice, report")]
fn main() {
    let bundle_path = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    let pool_path = argument("--map-pool")
        .unwrap_or_else(|| "out/pools/full_np8_12_holdout.json".to_owned());
    let seed: u64 = number("--seed", 900_000_000);
    let rotation: usize = number("--rotation", 0);
    let llm_index: usize = number("--seat", 0);
    let rounds: u32 = number("--rounds", 4);
    let temperature: f64 = number("--temperature", 0.001);
    let max_tokens: u64 = number("--max-tokens", 1024);
    let endpoint =
        argument("--endpoint").unwrap_or_else(|| "http://127.0.0.1:8080/v1".to_owned());
    let model = argument("--model").unwrap_or_else(|| "qwen3.8-flash-next-iq3_xxs".to_owned());
    let think = std::env::args().any(|arg| arg == "--think");
    let effort = argument("--effort").unwrap_or_else(|| "low".to_owned());
    let log_path = argument("--log");
    // `--live <prefix>`: files a browser viewer polls while the game is played.
    let live: Option<Rc<Live>> = argument("--live").map(|prefix| {
        Rc::new(Live {
            state: format!("{prefix}-state.json"),
            thinking: format!("{prefix}-thinking.txt"),
            history: RefCell::new(Vec::new()),
        })
    });

    ti4_tensor::configure_deterministic(20_261_008)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let content = ContentStore::embedded();
    let bundle = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let vocabulary = bundle.vocabulary;
    let actor = Rc::new(bundle.actor);
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(
            std::fs::read(&pool_path)
                .unwrap_or_else(|error| refuse(&format!("reading {pool_path}: {error}"))),
        ))
        .unwrap_or_else(|error| refuse(&format!("parsing the pool: {error}"))),
    );
    let players: Vec<PlayerId> = (0..6)
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let mut llm_player = players
        .get(llm_index)
        .cloned()
        .unwrap_or_else(|| refuse("--seat must be 0..=5"));
    // `--roster new` seats six factions from outside the standard six, as the faction-row pilot
    // trained them; `wide` draws from every faction; `six` (the default) is the standard table.
    let roster = argument("--roster").map_or(ti4_engine::seating::FactionRoster::InScope, |value| {
        ti4_training::rollout::parse_roster(&value).unwrap_or_else(|error| refuse(&error))
    });
    // `--lineup a,b,c,d,e,f` names the table outright (the seeded scramble still seats them).
    let factions: Vec<FactionId> = match argument("--lineup") {
        Some(lineup) => {
            let names: Vec<String> = lineup.split(',').map(|f| f.trim().to_owned()).collect();
            if names.len() != 6 {
                refuse("--lineup names six factions");
            }
            names.iter().map(FactionId::new).collect()
        }
        None => ti4_training::rollout::game_factions(
            content, roster, &FACTIONS, &players, DEFAULT, seed,
        )
        .unwrap_or_else(|error| refuse(&error)),
    };
    let seated: BTreeMap<PlayerId, FactionId> = players
        .iter()
        .enumerate()
        .map(|(index, player)| {
            (
                player.clone(),
                ti4_training::rollout::seated_faction(&factions, seed, rotation, index),
            )
        })
        .collect();
    // `--llm-faction` puts the LLM in whichever seat that faction was seated at.
    if let Some(wanted) = argument("--llm-faction") {
        llm_player = seated
            .iter()
            .find(|(_, faction)| faction.as_str() == wanted)
            .map(|(player, _)| player.clone())
            .unwrap_or_else(|| refuse(&format!("{wanted} is not at this table")));
    }
    let llm_faction = seated[&llm_player].to_string();
    println!("LLM seat game: seed {seed} rotation {rotation}, {llm_player} ({llm_faction})");
    println!("  endpoint    {endpoint}  model {model}  thinking {think} (effort {effort})");
    println!("  others      {bundle_path} at temperature {temperature}");

    let tally = Rc::new(RefCell::new(Tally::default()));
    let log = Rc::new(RefCell::new(log_path.as_ref().map(|path| {
        std::fs::File::create(path).unwrap_or_else(|error| refuse(&format!("{path}: {error}")))
    })));

    let play = |with_llm: bool| {
        let started = Instant::now();
        let (rollout, _) =
            ti4_training::rollout::play_with_capabilities_and_decider_factory_digest(
                content,
                &players,
                &seated,
                DEFAULT,
                seed,
                ti4_training::rollout::Horizon {
                    rounds,
                    steps: 10_000,
                },
                ti4_engine::opening::DEFAULT_REQUIREMENT,
                &ti4_training::rollout::OpeningMap::PythonPool {
                    pool: Arc::clone(&pool),
                    tile_seed_offset: TILE_SEED_OFFSET,
                },
                ti4_training::rollout::SimulationCapabilities { diplomacy: true },
                false,
                |baselines| {
                    let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
                    for (index, player) in players.iter().enumerate() {
                        if with_llm && *player == llm_player {
                            deciders.insert(
                                player.clone(),
                                Box::new(LlmSeat {
                                    seat: player.clone(),
                                    faction: llm_faction.clone(),
                                    endpoint: endpoint.clone(),
                                    model: model.clone(),
                                    think,
                                    effort: effort.clone(),
                                    max_tokens,
                                    tally: Rc::clone(&tally),
                                    log: Rc::clone(&log),
                                    live: live.clone(),
                                }),
                            );
                            continue;
                        }
                        let row = ti4_mlp::FactionRow::of(seated[player].as_str())
                            .map_err(|error| format!("{player}: {error}"))?;
                        let baseline = baselines
                            .get(player)
                            .copied()
                            .ok_or_else(|| format!("{player} has no setup baseline"))?;
                        let stream = seed
                            .wrapping_mul(1_000_003)
                            .wrapping_add(u64::try_from(index).unwrap_or(0));
                        let (decider, _status) =
                            ti4_mlp::bot::MlpBot::sharing(&actor, vocabulary.clone(), row, stream)
                                .at_temperature(temperature)
                                .from_setup(baseline)
                                .seat();
                        deciders.insert(player.clone(), decider);
                    }
                    Ok(deciders)
                },
            );
        (rollout, started.elapsed().as_secs_f64())
    };

    let report = |label: &str, rollout: &ti4_training::rollout::Rollout, seconds: f64| {
        if let Some(error) = &rollout.error {
            println!("  {label}: game failed: {error}");
            return;
        }
        let line: Vec<String> = rollout
            .seats
            .iter()
            .map(|seat| {
                format!(
                    "{}{} {} VP{}",
                    if seat.player == llm_player { "*" } else { "" },
                    seat.faction,
                    seat.episode.final_progress.victory_points,
                    if seat.episode.cleared { " cleared" } else { "" }
                )
            })
            .collect();
        println!("  {label} ({seconds:.1}s): {}", line.join(" | "));
    };

    let (policy, policy_seconds) = play(false);
    report("policy at every seat", &policy, policy_seconds);
    let (llm, llm_seconds) = play(true);
    report("LLM in the * seat   ", &llm, llm_seconds);

    let tally = tally.borrow();
    #[expect(clippy::cast_precision_loss, reason = "small counts")]
    let mean = tally.millis as f64 / tally.calls.max(1) as f64 / 1000.0;
    println!(
        "  LLM calls   {} ({} unparsed -> first option, {} errors, {} answered again without thinking), {:.1}s mean, {:.1} min total, {} prompt + {} completion tokens",
        tally.calls,
        tally.unparsed,
        tally.errors,
        tally.retried,
        mean,
        tally.millis as f64 / 60_000.0,
        tally.prompt_tokens,
        tally.completion_tokens
    );
}
